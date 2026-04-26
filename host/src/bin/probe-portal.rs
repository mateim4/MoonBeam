// MoonBeam M1 step 2 — xdg-desktop-portal screencast probe.
//
// Uses ashpd to request a screencast session from xdg-desktop-portal-kde,
// receives a PipeWire node id + remote fd, opens a video capture stream,
// and counts frames over a fixed duration. Confirms the portal pathway
// actually delivers frames and at what rate / format.
//
// User must select a source (Virtual-1 or any monitor) in the portal
// dialog that pops up.
//
// Run with: cargo run --bin probe-portal -- --duration 5

use std::os::fd::{AsRawFd, OwnedFd};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result};
use ashpd::desktop::screencast::{CursorMode, Screencast, SourceType};
use ashpd::desktop::PersistMode;
use clap::Parser;
use pipewire as pw;
use pw::spa::param::format::{FormatProperties, MediaSubtype, MediaType};
use pw::spa::param::video::{VideoFormat, VideoInfoRaw};
use pw::spa::param::ParamType;
use pw::spa::pod::{ChoiceValue, Object, Pod, Property, PropertyFlags, Value};
use pw::spa::utils::{Choice, ChoiceEnum, ChoiceFlags, Fraction, Id, Rectangle, SpaTypes};
use pw::stream::StreamFlags;

#[derive(Parser)]
#[command(about = "Request a screencast via xdg-desktop-portal and count frames over PipeWire")]
struct Cli {
    /// Seconds to count frames after the stream starts producing
    #[arg(short, long, default_value = "5")]
    duration: u64,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    println!("=== MoonBeam M1 step 2 — portal screencast probe ===");
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
    if let Some((x, y)) = s.position() {
        println!("  declared position: {x},{y}");
    }
    if let Some(t) = s.source_type() {
        println!("  source_type:      {t:?}");
    }

    let pw_fd: OwnedFd = proxy
        .open_pipe_wire_remote(&session)
        .await
        .context("open_pipe_wire_remote")?;
    println!("  pipewire fd:      {}\n", pw_fd.as_raw_fd());

    let frames = Arc::new(AtomicU64::new(0));
    let frames_for_thread = frames.clone();
    let duration = cli.duration;

    // pipewire's MainLoop uses Rc internally and is !Send, so the entire
    // capture loop runs on a dedicated std thread.
    let pw_thread = std::thread::spawn(move || -> anyhow::Result<()> {
        run_pipewire_capture(pw_fd, node_id, frames_for_thread, duration)
    });

    let join_result = pw_thread.join().expect("pipewire thread panicked");
    join_result?;

    let n = frames.load(Ordering::Relaxed);
    println!(
        "\n=== captured {} frames in ~{} seconds ({:.1} fps avg) ===",
        n,
        duration,
        n as f64 / duration as f64
    );

    Ok(())
}

fn run_pipewire_capture(
    fd: OwnedFd,
    node_id: u32,
    frames: Arc<AtomicU64>,
    duration_secs: u64,
) -> anyhow::Result<()> {
    pw::init();
    let main_loop = pw::main_loop::MainLoopRc::new(None)?;
    let context = pw::context::ContextRc::new(&main_loop, None)?;
    let core = context.connect_fd_rc(fd, None)?;

    let stream = pw::stream::StreamRc::new(
        core,
        "moonbeam-probe-portal",
        pw::properties::properties! {
            *pw::keys::MEDIA_TYPE => "Video",
            *pw::keys::MEDIA_CATEGORY => "Capture",
            *pw::keys::MEDIA_ROLE => "Screen",
        },
    )?;

    let frames_inner = frames.clone();
    // Track the negotiated format size so we can sanity-check buffer sizes.
    let negotiated = Arc::new(Mutex::new(None::<(u32, u32, u32, u32)>));
    let negotiated_inner = negotiated.clone();

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
            if info.parse(param).is_ok() {
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
                *negotiated_inner.lock().unwrap() = Some((s.width, s.height, f.num, f.denom));
            }
        })
        .process(move |stream, _| {
            if let Some(_buffer) = stream.dequeue_buffer() {
                frames_inner.fetch_add(1, Ordering::Relaxed);
            }
        })
        .register()?;

    // Build the EnumFormat pod by hand. We accept a permissive set of common
    // RGB/RGBA formats since vkms emits XR24/AR24/AB24 and KWin will pick
    // the best match. Size and framerate are wide ranges — let the producer
    // (KWin) pick the natural size/rate of the chosen output.
    let format_obj = Object {
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
                        alternatives: vec![
                            Id(VideoFormat::BGRx.as_raw()),
                            Id(VideoFormat::RGBx.as_raw()),
                            Id(VideoFormat::BGRA.as_raw()),
                            Id(VideoFormat::RGBA.as_raw()),
                            Id(VideoFormat::xBGR.as_raw()),
                            Id(VideoFormat::xRGB.as_raw()),
                            Id(VideoFormat::ABGR.as_raw()),
                            Id(VideoFormat::ARGB.as_raw()),
                        ],
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
    };

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

    // Schedule shutdown via the loop's own timer so we run on the loop's thread.
    let main_loop_for_quit = main_loop.clone();
    let timer = main_loop.loop_().add_timer(move |_| {
        main_loop_for_quit.quit();
    });
    timer
        .update_timer(Some(Duration::from_secs(duration_secs)), None)
        .into_result()
        .map_err(|e| anyhow::anyhow!("timer arm failed: {e:?}"))?;

    main_loop.run();
    Ok(())
}
