// MoonBeam M1 step 3 — NVENC encode spike.
//
// Same portal+PipeWire capture as probe-portal, but each captured BGRx frame
// is fed to ffmpeg-next's h264_nvenc encoder and the resulting Annex-B NAL
// units are written to /tmp/moonbeam-test.h264. Verifies that:
//   1. The BGRx byte order from the portal feeds NVENC without conversion.
//   2. h264_nvenc actually picks up our RTX 5090 and produces a valid stream.
//   3. End-to-end latency from frame arrival to encoded packet is reasonable.
//
// Run with: cargo run --bin probe-encode -- --duration 5
// Verify  : ffplay -fflags +genpts /tmp/moonbeam-test.h264

use std::fs::File;
use std::io::{BufWriter, Write};
use std::os::fd::{AsRawFd, OwnedFd};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result};
use ashpd::desktop::screencast::{CursorMode, Screencast, SourceType};
use ashpd::desktop::PersistMode;
use clap::Parser;
use ffmpeg_next as ffmpeg;
use ffmpeg::codec;
use ffmpeg::format::Pixel;
use ffmpeg::{encoder, frame, Dictionary, Packet};
use pipewire as pw;
use pw::spa::param::format::{FormatProperties, MediaSubtype, MediaType};
use pw::spa::param::video::{VideoFormat, VideoInfoRaw};
use pw::spa::param::ParamType;
use pw::spa::pod::{ChoiceValue, Object, Pod, Property, PropertyFlags, Value};
use pw::spa::utils::{Choice, ChoiceEnum, ChoiceFlags, Fraction, Id, Rectangle, SpaTypes};
use pw::stream::StreamFlags;

#[derive(Parser)]
#[command(about = "Capture via portal + encode with h264_nvenc to /tmp/moonbeam-test.h264")]
struct Cli {
    /// Seconds of video to encode after the stream starts producing
    #[arg(short, long, default_value = "5")]
    duration: u64,
    /// Output file (raw Annex-B H.264)
    #[arg(short, long, default_value = "/tmp/moonbeam-test.h264")]
    output: String,
    /// Target encoded bitrate (bits/sec). Default 30 Mbps.
    #[arg(short, long, default_value_t = 30_000_000)]
    bitrate: usize,
}

struct EncoderState {
    encoder: encoder::Video,
    width: u32,
    height: u32,
    file: BufWriter<File>,
    frames_in: u64,
    packets_out: u64,
    bytes_out: u64,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    println!("=== MoonBeam M1 step 3 — NVENC encode spike ===");
    println!("Requesting screencast session via xdg-desktop-portal...");
    println!("(KDE will pop up a dialog — pick Virtual-1 or any monitor.)\n");

    let proxy = Screencast::new().await.context("Screencast::new")?;
    let session = proxy.create_session().await.context("create_session")?;

    proxy
        .select_sources(
            &session,
            CursorMode::Embedded,
            SourceType::Monitor | SourceType::Virtual,
            false,
            None,
            PersistMode::DoNot,
        )
        .await
        .context("select_sources")?;

    let response = proxy.start(&session, None).await.context("start")?;
    let streams_resp = response.response().context("start response")?;
    let stream_list = streams_resp.streams();

    if stream_list.is_empty() {
        anyhow::bail!("portal returned no streams (user cancelled?)");
    }

    let s = &stream_list[0];
    let node_id = s.pipe_wire_node_id();
    println!("got stream from portal: node_id={node_id}");
    if let Some((w, h)) = s.size() {
        println!("  declared size:    {w}x{h}");
    }
    if let Some(t) = s.source_type() {
        println!("  source_type:      {t:?}");
    }

    let pw_fd: OwnedFd = proxy
        .open_pipe_wire_remote(&session)
        .await
        .context("open_pipe_wire_remote")?;
    println!("  pipewire fd:      {}\n", pw_fd.as_raw_fd());

    ffmpeg::init().context("ffmpeg::init")?;

    let output_path = cli.output.clone();
    let duration = cli.duration;
    let bitrate = cli.bitrate;

    let pw_thread = std::thread::spawn(move || -> anyhow::Result<()> {
        run_pipewire_capture(pw_fd, node_id, output_path, duration, bitrate)
    });

    pw_thread.join().expect("pipewire thread panicked")?;
    Ok(())
}

fn run_pipewire_capture(
    fd: OwnedFd,
    node_id: u32,
    output_path: String,
    duration_secs: u64,
    bitrate: usize,
) -> anyhow::Result<()> {
    pw::init();
    let main_loop = pw::main_loop::MainLoopRc::new(None)?;
    let context = pw::context::ContextRc::new(&main_loop, None)?;
    let core = context.connect_fd_rc(fd, None)?;

    let stream = pw::stream::StreamRc::new(
        core,
        "moonbeam-probe-encode",
        pw::properties::properties! {
            *pw::keys::MEDIA_TYPE => "Video",
            *pw::keys::MEDIA_CATEGORY => "Capture",
            *pw::keys::MEDIA_ROLE => "Screen",
        },
    )?;

    let state: Arc<Mutex<Option<EncoderState>>> = Arc::new(Mutex::new(None));
    let state_for_format = state.clone();
    let state_for_process = state.clone();
    let output_path_for_format = output_path.clone();

    let _listener = stream
        .add_local_listener_with_user_data(())
        .state_changed(|_, _, old, new| {
            println!("stream state: {old:?} -> {new:?}");
        })
        .param_changed(move |_, _, id, param| {
            let Some(param) = param else {
                return;
            };
            if id != ParamType::Format.as_raw() {
                return;
            }
            let mut info = VideoInfoRaw::new();
            if info.parse(param).is_err() {
                return;
            }
            let s = info.size();
            let f = info.framerate();
            println!(
                "negotiated format: {:?} {}x{} @ {}/{} fps",
                info.format(),
                s.width,
                s.height,
                f.num,
                f.denom
            );

            // Only construct the encoder once. KWin can re-emit Format params
            // (e.g. after a re-negotiation) but width/height should be stable
            // for a given source.
            let mut guard = state_for_format.lock().unwrap();
            if guard.is_some() {
                return;
            }

            // We only handle BGRx for the spike — that's what KWin negotiates
            // by default and what NVENC takes natively as ARGB.
            if info.format() != VideoFormat::BGRx {
                eprintln!(
                    "warning: producer picked {:?}, expected BGRx; bailing",
                    info.format()
                );
                return;
            }

            match build_encoder(s.width, s.height, bitrate, &output_path_for_format) {
                Ok(es) => {
                    println!(
                        "h264_nvenc opened: {}x{} BGR0, {} bps, output={}",
                        es.width, es.height, bitrate, output_path_for_format
                    );
                    *guard = Some(es);
                }
                Err(e) => eprintln!("encoder open failed: {e:?}"),
            }
        })
        .process(move |stream, _| {
            let Some(mut buffer) = stream.dequeue_buffer() else {
                return;
            };
            let datas = buffer.datas_mut();
            if datas.is_empty() {
                return;
            }
            let data = &mut datas[0];
            let chunk = data.chunk();
            let stride = chunk.stride() as usize;
            let size = chunk.size() as usize;
            let Some(bytes) = data.data() else {
                return;
            };

            let mut guard = state_for_process.lock().unwrap();
            let Some(es) = guard.as_mut() else {
                return;
            };

            let row_bytes = (es.width as usize) * 4;
            let height = es.height as usize;
            if stride < row_bytes || size < stride * height {
                eprintln!(
                    "buffer too small: stride={} size={} need {}x{}",
                    stride, size, row_bytes, height
                );
                return;
            }

            let mut frame = frame::Video::new(Pixel::BGRZ, es.width, es.height);
            let dst_stride = frame.stride(0);
            {
                let dst = frame.data_mut(0);
                for y in 0..height {
                    let s_off = y * stride;
                    let d_off = y * dst_stride;
                    dst[d_off..d_off + row_bytes]
                        .copy_from_slice(&bytes[s_off..s_off + row_bytes]);
                }
            }

            // PTS in encoder time_base units (1/60). This advances the clock
            // monotonically per captured frame; playback timing therefore
            // reflects the capture cadence the source is producing at. Good
            // enough for a verification spike.
            frame.set_pts(Some(es.frames_in as i64));
            es.frames_in += 1;

            if let Err(e) = es.encoder.send_frame(&frame) {
                eprintln!("send_frame failed: {e}");
                return;
            }
            drain_packets(es);
        })
        .register()?;

    let format_obj = build_enum_format();
    let bytes: Vec<u8> = pw::spa::pod::serialize::PodSerializer::serialize(
        std::io::Cursor::new(Vec::new()),
        &Value::Object(format_obj),
    )
    .map_err(|e| anyhow::anyhow!("PodSerializer failed: {e:?}"))?
    .0
    .into_inner();

    let pod = Pod::from_bytes(&bytes).context("Pod::from_bytes for format")?;
    let mut params = [pod];

    stream.connect(
        pw::spa::utils::Direction::Input,
        Some(node_id),
        StreamFlags::AUTOCONNECT | StreamFlags::MAP_BUFFERS,
        &mut params,
    )?;

    let main_loop_for_quit = main_loop.clone();
    let timer = main_loop.loop_().add_timer(move |_| {
        main_loop_for_quit.quit();
    });
    timer
        .update_timer(Some(Duration::from_secs(duration_secs)), None)
        .into_result()
        .map_err(|e| anyhow::anyhow!("timer arm failed: {e:?}"))?;

    main_loop.run();

    // Flush the encoder: send EOF so any frames stuck inside NVENC's
    // delayed-output queue get spat out, then drain remaining packets.
    let mut guard = state.lock().unwrap();
    if let Some(es) = guard.as_mut() {
        if let Err(e) = es.encoder.send_eof() {
            eprintln!("send_eof failed: {e}");
        }
        drain_packets(es);
        es.file.flush().ok();
        println!(
            "\n=== captured {} frames, encoded {} packets, {:.2} MiB written to {} ===",
            es.frames_in,
            es.packets_out,
            es.bytes_out as f64 / (1024.0 * 1024.0),
            output_path
        );
    } else {
        eprintln!("encoder was never initialised — no format negotiated?");
    }

    Ok(())
}

fn drain_packets(es: &mut EncoderState) {
    let mut packet = Packet::empty();
    while es.encoder.receive_packet(&mut packet).is_ok() {
        if let Some(payload) = packet.data() {
            if let Err(e) = es.file.write_all(payload) {
                eprintln!("file write failed: {e}");
                break;
            }
            es.packets_out += 1;
            es.bytes_out += payload.len() as u64;
        }
    }
}

fn build_encoder(
    width: u32,
    height: u32,
    bitrate: usize,
    output_path: &str,
) -> anyhow::Result<EncoderState> {
    let codec = encoder::find_by_name("h264_nvenc")
        .ok_or_else(|| anyhow::anyhow!("h264_nvenc not available in this ffmpeg build"))?;

    let mut video = codec::context::Context::new_with_codec(codec)
        .encoder()
        .video()?;
    video.set_width(width);
    video.set_height(height);
    video.set_format(Pixel::BGRZ);
    video.set_time_base((1, 60));
    video.set_frame_rate(Some((60, 1)));
    video.set_bit_rate(bitrate);
    video.set_max_bit_rate(bitrate);
    video.set_gop(60);
    video.set_max_b_frames(0);

    // h264_nvenc-specific knobs for low-latency, RGB-input encode.
    let mut opts = Dictionary::new();
    opts.set("preset", "p1"); // p1 = fastest
    opts.set("tune", "ull"); // ultra-low-latency
    opts.set("rc", "cbr");
    opts.set("zerolatency", "1");

    let opened = video
        .open_with(opts)
        .context("open h264_nvenc (driver/build mismatch?)")?;

    let file = BufWriter::new(File::create(output_path).context("create output file")?);

    Ok(EncoderState {
        encoder: opened,
        width,
        height,
        file,
        frames_in: 0,
        packets_out: 0,
        bytes_out: 0,
    })
}

fn build_enum_format() -> Object {
    Object {
        type_: SpaTypes::ObjectParamFormat.as_raw(),
        id: ParamType::EnumFormat.as_raw(),
        properties: vec![
            Property {
                key: FormatProperties::MediaType.as_raw(),
                flags: PropertyFlags::empty(),
                value: Value::Id(Id(MediaType::Video.as_raw())),
            },
            Property {
                key: FormatProperties::MediaSubtype.as_raw(),
                flags: PropertyFlags::empty(),
                value: Value::Id(Id(MediaSubtype::Raw.as_raw())),
            },
            Property {
                key: FormatProperties::VideoFormat.as_raw(),
                flags: PropertyFlags::empty(),
                value: Value::Choice(ChoiceValue::Id(Choice(
                    ChoiceFlags::empty(),
                    ChoiceEnum::Enum {
                        default: Id(VideoFormat::BGRx.as_raw()),
                        alternatives: vec![Id(VideoFormat::BGRx.as_raw())],
                    },
                ))),
            },
            Property {
                key: FormatProperties::VideoSize.as_raw(),
                flags: PropertyFlags::empty(),
                value: Value::Choice(ChoiceValue::Rectangle(Choice(
                    ChoiceFlags::empty(),
                    ChoiceEnum::Range {
                        default: Rectangle {
                            width: 1920,
                            height: 1080,
                        },
                        min: Rectangle {
                            width: 1,
                            height: 1,
                        },
                        max: Rectangle {
                            width: 8192,
                            height: 8192,
                        },
                    },
                ))),
            },
            Property {
                key: FormatProperties::VideoFramerate.as_raw(),
                flags: PropertyFlags::empty(),
                value: Value::Choice(ChoiceValue::Fraction(Choice(
                    ChoiceFlags::empty(),
                    ChoiceEnum::Range {
                        default: Fraction { num: 60, denom: 1 },
                        min: Fraction { num: 0, denom: 1 },
                        max: Fraction {
                            num: 1000,
                            denom: 1,
                        },
                    },
                ))),
            },
        ],
    }
}
