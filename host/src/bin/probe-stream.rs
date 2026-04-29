// MoonBeam M1 step 4 — WebSocket transport probe.
//
// Same portal+PipeWire+NVENC pipeline as probe-encode, but instead of
// writing Annex-B to a file, each encoded access unit is broadcast on a
// tokio channel. axum serves a small static dir at / (browser/index.html)
// and a /ws WebSocket that forwards every packet to subscribed clients
// using the wire format defined in docs/MOONBEAM-APP-PLAN.md §5.1:
//
//   [type:u8][flags:u8][payload...]
//
//   type  = 0x01 video, 0x02 audio (future), 0x03 input (future)
//   flags = bit0 = keyframe (video only)
//   payload = raw Annex-B H.264 access unit
//
// This is the same framing the Android MediaCodec client will consume,
// so the browser probe doubles as the wire-format reference impl.
//
// Run on host: cargo run --bin probe-stream
// Open in a WebCodecs-capable browser: http://localhost:7878/

use std::net::SocketAddr;
use std::os::fd::{AsRawFd, OwnedFd};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use ashpd::desktop::screencast::{CursorMode, Screencast, SourceType};
use ashpd::desktop::PersistMode;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use bytes::Bytes;
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
use tokio::sync::broadcast;
use tower_http::services::ServeDir;

const TYPE_VIDEO: u8 = 0x01;
const FLAG_KEYFRAME: u8 = 0x01;

// 64 access units ≈ 1s at 60fps; a slow client gets one second of
// grace before they're dropped (RecvError::Lagged).
const BROADCAST_CAPACITY: usize = 64;

#[derive(Parser)]
#[command(about = "Capture+NVENC pipeline served over WebSocket for browser-side WebCodecs decoding")]
struct Cli {
    /// Bind address for the HTTP+WS server
    #[arg(short, long, default_value = "0.0.0.0:7878")]
    bind: String,
    /// Target encoded bitrate (bits/sec). Default 30 Mbps.
    #[arg(long, default_value_t = 30_000_000)]
    bitrate: usize,
    /// Static-file directory served at /
    #[arg(long, default_value = "browser")]
    static_dir: String,
}

#[derive(Clone)]
struct AppState {
    tx: broadcast::Sender<Bytes>,
}

struct EncoderState {
    encoder: encoder::Video,
    width: u32,
    height: u32,
    tx: broadcast::Sender<Bytes>,
    frames_in: u64,
    packets_out: u64,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Channel created up-front so the PW thread always has a sink even
    // when no WS clients are connected. send() is non-blocking and
    // simply drops packets when there are no subscribers.
    let (tx, _) = broadcast::channel::<Bytes>(BROADCAST_CAPACITY);

    println!("=== MoonBeam M1 step 4 — WebSocket transport probe ===");
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

    let bitrate = cli.bitrate;
    let tx_for_pw = tx.clone();
    let _pw_thread = std::thread::spawn(move || -> anyhow::Result<()> {
        run_pipewire_capture(pw_fd, node_id, bitrate, tx_for_pw)
    });

    let bind: SocketAddr = cli.bind.parse().context("parse --bind")?;
    let app_state = AppState { tx: tx.clone() };
    let app = Router::new()
        .route("/ws", get(ws_handler))
        .fallback_service(ServeDir::new(&cli.static_dir))
        .with_state(app_state);

    println!("HTTP+WS server listening on http://{bind}/");
    println!("  open the URL above in a WebCodecs-capable browser.");
    println!("  Ctrl+C to stop.\n");

    let listener = tokio::net::TcpListener::bind(&bind)
        .await
        .context("TCP bind")?;
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            tokio::signal::ctrl_c().await.ok();
            println!("\nshutting down (ctrl+c)…");
        })
        .await
        .context("axum::serve")?;

    Ok(())
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(mut socket: WebSocket, state: AppState) {
    let mut rx = state.tx.subscribe();
    println!("ws client connected (subscribers={})", state.tx.receiver_count());

    loop {
        tokio::select! {
            res = rx.recv() => match res {
                Ok(payload) => {
                    if socket.send(Message::Binary(payload.to_vec().into())).await.is_err() {
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    eprintln!("ws client lagged {n} packets, dropping connection");
                    break;
                }
                Err(broadcast::error::RecvError::Closed) => break,
            },
            msg = socket.recv() => match msg {
                Some(Ok(Message::Close(_))) | None => break,
                Some(Err(_)) => break,
                _ => {}
            },
        }
    }
    println!("ws client disconnected");
}

fn run_pipewire_capture(
    fd: OwnedFd,
    node_id: u32,
    bitrate: usize,
    tx: broadcast::Sender<Bytes>,
) -> anyhow::Result<()> {
    pw::init();
    let main_loop = pw::main_loop::MainLoopRc::new(None)?;
    let context = pw::context::ContextRc::new(&main_loop, None)?;
    let core = context.connect_fd_rc(fd, None)?;

    let stream = pw::stream::StreamRc::new(
        core,
        "moonbeam-probe-stream",
        pw::properties::properties! {
            *pw::keys::MEDIA_TYPE => "Video",
            *pw::keys::MEDIA_CATEGORY => "Capture",
            *pw::keys::MEDIA_ROLE => "Screen",
        },
    )?;

    let state: Arc<Mutex<Option<EncoderState>>> = Arc::new(Mutex::new(None));
    let state_for_format = state.clone();
    let state_for_process = state.clone();
    let tx_for_format = tx.clone();

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
            let mut guard = state_for_format.lock().unwrap();
            if guard.is_some() {
                return;
            }
            if info.format() != VideoFormat::BGRx {
                eprintln!(
                    "warning: producer picked {:?}, expected BGRx; bailing",
                    info.format()
                );
                return;
            }
            match build_encoder(s.width, s.height, bitrate, tx_for_format.clone()) {
                Ok(es) => {
                    println!(
                        "h264_nvenc opened: {}x{} BGR0, {} bps, GOP=30",
                        es.width, es.height, bitrate
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

    main_loop.run();
    Ok(())
}

fn drain_packets(es: &mut EncoderState) {
    let mut packet = Packet::empty();
    while es.encoder.receive_packet(&mut packet).is_ok() {
        let is_key = packet.is_key();
        if let Some(payload) = packet.data() {
            let mut buf = Vec::with_capacity(payload.len() + 2);
            buf.push(TYPE_VIDEO);
            buf.push(if is_key { FLAG_KEYFRAME } else { 0 });
            buf.extend_from_slice(payload);
            // send() returns Err only when there are no subscribers; that's
            // the steady state when nobody is watching, so we ignore it.
            let _ = es.tx.send(Bytes::from(buf));
            es.packets_out += 1;
            if es.packets_out % 60 == 0 {
                println!(
                    "packets_out={} frames_in={} subscribers={}",
                    es.packets_out,
                    es.frames_in,
                    es.tx.receiver_count()
                );
            }
        }
    }
}

fn build_encoder(
    width: u32,
    height: u32,
    bitrate: usize,
    tx: broadcast::Sender<Bytes>,
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
    // Smaller GOP than the file probe (30 vs 60) so a freshly-connected
    // browser client only waits 0.5–1.2s for the first keyframe before
    // it can start decoding.
    video.set_gop(30);
    video.set_max_b_frames(0);

    let mut opts = Dictionary::new();
    opts.set("preset", "p1");
    opts.set("tune", "ull");
    opts.set("rc", "cbr");
    opts.set("zerolatency", "1");

    let opened = video.open_with(opts).context("open h264_nvenc")?;
    Ok(EncoderState {
        encoder: opened,
        width,
        height,
        tx,
        frames_in: 0,
        packets_out: 0,
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
