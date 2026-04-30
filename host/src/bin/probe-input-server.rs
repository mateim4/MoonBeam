// MoonBeam M2 step 3 — WebSocket input return probe.
//
// The empirical question for this step is: can input events arrive
// over the same WebSocket framing the M1 video probe locked, get
// translated into kernel-side uinput writes against the pen + touch
// devices the M2 step 1 / step 2 probes proved out, and reach a
// real Linux drawing app (Krita) end-to-end?
//
// Wire format (matches docs/MOONBEAM-APP-PLAN.md §5.1):
//   [type:u8][flags:u8][payload]
//   type  = 0x03 (input)
//   flags = unused for now (reserved for future use, e.g. "this frame
//           is a synthesized down-up burst, treat as a tap")
//   payload = UTF-8 JSON, one InputMsg per binary frame
//
// JSON shapes (serde tagged enum, "type" discriminator):
//   {"type":"pen_down","x":100,"y":200,"pressure":2048,"tilt_x":0,"tilt_y":0}
//   {"type":"pen_move","x":110,"y":205,"pressure":2100,"tilt_x":-3,"tilt_y":1}
//   {"type":"pen_up"}
//   {"type":"pen_button","button":"stylus"|"stylus2","state":true|false}
//   {"type":"touch_down","slot":0,"id":42,"x":100,"y":200,"major":15,"pressure":80}
//   {"type":"touch_move","slot":0,"x":110,"y":205,"major":15,"pressure":85}
//   {"type":"touch_up","slot":0}
//
// Each WS frame is one self-contained input event that the server
// translates into a complete kernel input report ending with
// SYN_REPORT. Coalescing multiple events into one report (e.g. a
// move on slot 0 + slot 1 in the same frame) is a future
// optimisation; for the probe, one frame = one report keeps the
// ordering intuitive.
//
// Run with:
//   cargo run --bin probe-input-server
// Then point a browser at http://127.0.0.1:7879/ and click the
// scripted-stroke buttons.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use clap::Parser;
use futures_util::StreamExt;
use input_linux::{
    AbsoluteAxis, AbsoluteEvent, AbsoluteInfo, AbsoluteInfoSetup, EventKind, EventTime, InputId,
    InputProperty, Key, KeyEvent, KeyState, MiscEvent, MiscKind, SynchronizeEvent, UInputHandle,
};
use serde::Deserialize;
use std::fs::OpenOptions;
use tokio::sync::Mutex;
use tower_http::services::ServeDir;

const TYPE_INPUT: u8 = 0x03;

#[derive(Parser)]
#[command(about = "WebSocket server that owns pen+touch uinput devices and routes incoming input events")]
struct Cli {
    #[arg(long, default_value = "0.0.0.0:7879")]
    bind: SocketAddr,
    #[arg(long, default_value = "browser")]
    static_dir: PathBuf,
    /// Coordinate space (matches the pen+touch probes).
    #[arg(long, default_value_t = 2960)]
    width: i32,
    #[arg(long, default_value_t = 1848)]
    height: i32,
    #[arg(long, default_value_t = 4095)]
    pressure_max: i32,
    #[arg(long, default_value_t = 10)]
    slots: i32,
}

type SharedUInput = Arc<Mutex<UInputHandle<std::fs::File>>>;

#[derive(Clone)]
struct AppState {
    pen: SharedUInput,
    touch: SharedUInput,
    pen_serial: i32,
}

#[derive(Deserialize, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
enum InputMsg {
    PenDown {
        x: i32,
        y: i32,
        pressure: i32,
        #[serde(default)]
        tilt_x: i32,
        #[serde(default)]
        tilt_y: i32,
    },
    PenMove {
        x: i32,
        y: i32,
        pressure: i32,
        #[serde(default)]
        tilt_x: i32,
        #[serde(default)]
        tilt_y: i32,
    },
    PenUp,
    PenButton {
        button: PenButton,
        state: bool,
    },
    TouchDown {
        slot: i32,
        id: i32,
        x: i32,
        y: i32,
        #[serde(default = "default_major")]
        major: i32,
        #[serde(default = "default_pressure")]
        pressure: i32,
    },
    TouchMove {
        slot: i32,
        x: i32,
        y: i32,
        #[serde(default = "default_major")]
        major: i32,
        #[serde(default = "default_pressure")]
        pressure: i32,
    },
    TouchUp {
        slot: i32,
    },
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "snake_case")]
enum PenButton {
    Stylus,
    Stylus2,
}

fn default_major() -> i32 {
    200
}
fn default_pressure() -> i32 {
    100
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    println!("=== MoonBeam M2 step 3 — WebSocket input server ===");

    // Build the pen device. Capability set must exactly match the
    // M2 step 1 probe — drift would mean a JSON event we accept here
    // wouldn't reach a tablet-aware app the same way.
    let pen = build_pen_device(&cli).context("build pen device")?;
    let touch = build_touch_device(&cli).context("build touch device")?;

    let state = AppState {
        pen: Arc::new(Mutex::new(pen)),
        touch: Arc::new(Mutex::new(touch)),
        pen_serial: 0xC0FFEE,
    };

    let app = Router::new()
        .route("/ws", get(ws_handler))
        .fallback_service(ServeDir::new(&cli.static_dir))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(cli.bind)
        .await
        .with_context(|| format!("bind {}", cli.bind))?;
    println!("HTTP+WS server listening on http://{}/", cli.bind);
    println!("    open browser at http://127.0.0.1:{}/input.html", cli.bind.port());

    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
            println!("\nshutting down (SIGINT)");
        })
        .await
        .context("axum::serve")?;

    Ok(())
}

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: AppState) {
    println!("ws client connected");
    let (_, mut receiver) = socket.split();
    while let Some(msg) = receiver.next().await {
        match msg {
            Ok(Message::Binary(bytes)) => {
                if let Err(e) = handle_binary(&bytes, &state).await {
                    eprintln!("input frame error: {e:#}");
                }
            }
            Ok(Message::Text(_)) => {
                // Reserved — not a probe error, just unused.
            }
            Ok(Message::Close(_)) => break,
            Ok(_) => {}
            Err(e) => {
                eprintln!("ws receive error: {e:#}");
                break;
            }
        }
    }
    println!("ws client disconnected");
}

async fn handle_binary(bytes: &[u8], state: &AppState) -> Result<()> {
    if bytes.len() < 2 {
        anyhow::bail!("frame too short ({} bytes)", bytes.len());
    }
    let frame_type = bytes[0];
    let _flags = bytes[1];
    if frame_type != TYPE_INPUT {
        // Silently ignore non-input frames so this server can later
        // share a /ws with the video probe without rejecting frames
        // that aren't ours.
        return Ok(());
    }
    let payload = &bytes[2..];
    let msg: InputMsg =
        serde_json::from_slice(payload).context("parse JSON input message")?;
    println!("  apply: {msg:?}");
    apply(state, msg).await
}

async fn apply(state: &AppState, msg: InputMsg) -> Result<()> {
    match msg {
        InputMsg::PenDown {
            x,
            y,
            pressure,
            tilt_x,
            tilt_y,
        } => {
            let pen = state.pen.lock().await;
            emit(
                &pen,
                &[
                    MiscEvent::new(zero_time(), MiscKind::Serial, state.pen_serial).into(),
                    KeyEvent::new(zero_time(), Key::ButtonToolPen, KeyState::PRESSED).into(),
                    AbsoluteEvent::new(zero_time(), AbsoluteAxis::X, x).into(),
                    AbsoluteEvent::new(zero_time(), AbsoluteAxis::Y, y).into(),
                    AbsoluteEvent::new(zero_time(), AbsoluteAxis::Pressure, pressure).into(),
                    AbsoluteEvent::new(zero_time(), AbsoluteAxis::TiltX, tilt_x).into(),
                    AbsoluteEvent::new(zero_time(), AbsoluteAxis::TiltY, tilt_y).into(),
                    KeyEvent::new(zero_time(), Key::ButtonTouch, KeyState::PRESSED).into(),
                    SynchronizeEvent::report(zero_time()).into(),
                ],
            )?;
        }
        InputMsg::PenMove {
            x,
            y,
            pressure,
            tilt_x,
            tilt_y,
        } => {
            let pen = state.pen.lock().await;
            emit(
                &pen,
                &[
                    AbsoluteEvent::new(zero_time(), AbsoluteAxis::X, x).into(),
                    AbsoluteEvent::new(zero_time(), AbsoluteAxis::Y, y).into(),
                    AbsoluteEvent::new(zero_time(), AbsoluteAxis::Pressure, pressure).into(),
                    AbsoluteEvent::new(zero_time(), AbsoluteAxis::TiltX, tilt_x).into(),
                    AbsoluteEvent::new(zero_time(), AbsoluteAxis::TiltY, tilt_y).into(),
                    SynchronizeEvent::report(zero_time()).into(),
                ],
            )?;
        }
        InputMsg::PenUp => {
            let pen = state.pen.lock().await;
            emit(
                &pen,
                &[
                    KeyEvent::new(zero_time(), Key::ButtonTouch, KeyState::RELEASED).into(),
                    AbsoluteEvent::new(zero_time(), AbsoluteAxis::Pressure, 0).into(),
                    KeyEvent::new(zero_time(), Key::ButtonToolPen, KeyState::RELEASED).into(),
                    SynchronizeEvent::report(zero_time()).into(),
                ],
            )?;
        }
        InputMsg::PenButton { button, state: on } => {
            let pen = state.pen.lock().await;
            let key = match button {
                PenButton::Stylus => Key::ButtonStylus,
                PenButton::Stylus2 => Key::ButtonStylus2,
            };
            let key_state = if on { KeyState::PRESSED } else { KeyState::RELEASED };
            emit(
                &pen,
                &[
                    KeyEvent::new(zero_time(), key, key_state).into(),
                    SynchronizeEvent::report(zero_time()).into(),
                ],
            )?;
        }
        InputMsg::TouchDown {
            slot,
            id,
            x,
            y,
            major,
            pressure,
        } => {
            let touch = state.touch.lock().await;
            emit(
                &touch,
                &[
                    AbsoluteEvent::new(zero_time(), AbsoluteAxis::MultitouchSlot, slot).into(),
                    AbsoluteEvent::new(zero_time(), AbsoluteAxis::MultitouchTrackingId, id).into(),
                    AbsoluteEvent::new(zero_time(), AbsoluteAxis::MultitouchPositionX, x).into(),
                    AbsoluteEvent::new(zero_time(), AbsoluteAxis::MultitouchPositionY, y).into(),
                    AbsoluteEvent::new(zero_time(), AbsoluteAxis::MultitouchTouchMajor, major)
                        .into(),
                    AbsoluteEvent::new(zero_time(), AbsoluteAxis::MultitouchPressure, pressure)
                        .into(),
                    KeyEvent::new(zero_time(), Key::ButtonTouch, KeyState::PRESSED).into(),
                    AbsoluteEvent::new(zero_time(), AbsoluteAxis::X, x).into(),
                    AbsoluteEvent::new(zero_time(), AbsoluteAxis::Y, y).into(),
                    SynchronizeEvent::report(zero_time()).into(),
                ],
            )?;
        }
        InputMsg::TouchMove {
            slot,
            x,
            y,
            major,
            pressure,
        } => {
            let touch = state.touch.lock().await;
            emit(
                &touch,
                &[
                    AbsoluteEvent::new(zero_time(), AbsoluteAxis::MultitouchSlot, slot).into(),
                    AbsoluteEvent::new(zero_time(), AbsoluteAxis::MultitouchPositionX, x).into(),
                    AbsoluteEvent::new(zero_time(), AbsoluteAxis::MultitouchPositionY, y).into(),
                    AbsoluteEvent::new(zero_time(), AbsoluteAxis::MultitouchTouchMajor, major)
                        .into(),
                    AbsoluteEvent::new(zero_time(), AbsoluteAxis::MultitouchPressure, pressure)
                        .into(),
                    AbsoluteEvent::new(zero_time(), AbsoluteAxis::X, x).into(),
                    AbsoluteEvent::new(zero_time(), AbsoluteAxis::Y, y).into(),
                    SynchronizeEvent::report(zero_time()).into(),
                ],
            )?;
        }
        InputMsg::TouchUp { slot } => {
            let touch = state.touch.lock().await;
            emit(
                &touch,
                &[
                    AbsoluteEvent::new(zero_time(), AbsoluteAxis::MultitouchSlot, slot).into(),
                    AbsoluteEvent::new(zero_time(), AbsoluteAxis::MultitouchTrackingId, -1).into(),
                    KeyEvent::new(zero_time(), Key::ButtonTouch, KeyState::RELEASED).into(),
                    SynchronizeEvent::report(zero_time()).into(),
                ],
            )?;
        }
    }
    Ok(())
}

fn build_pen_device(cli: &Cli) -> Result<UInputHandle<std::fs::File>> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/uinput")
        .context("open /dev/uinput")?;
    let uinput = UInputHandle::new(file);

    uinput.set_evbit(EventKind::Key)?;
    uinput.set_evbit(EventKind::Absolute)?;
    uinput.set_evbit(EventKind::Synchronize)?;
    uinput.set_evbit(EventKind::Misc)?;
    uinput.set_keybit(Key::ButtonToolPen)?;
    uinput.set_keybit(Key::ButtonToolRubber)?;
    uinput.set_keybit(Key::ButtonTouch)?;
    uinput.set_keybit(Key::ButtonStylus)?;
    uinput.set_keybit(Key::ButtonStylus2)?;
    uinput.set_absbit(AbsoluteAxis::X)?;
    uinput.set_absbit(AbsoluteAxis::Y)?;
    uinput.set_absbit(AbsoluteAxis::Pressure)?;
    uinput.set_absbit(AbsoluteAxis::TiltX)?;
    uinput.set_absbit(AbsoluteAxis::TiltY)?;
    uinput.set_mscbit(MiscKind::Serial)?;
    uinput.set_propbit(InputProperty::Direct)?;

    let id = InputId {
        bustype: input_linux::sys::BUS_VIRTUAL as u16,
        vendor: 0x04e8,
        product: 0x4d42,
        version: 1,
    };

    let abs = abs_pen(cli);
    uinput.create(&id, b"MoonBeam Pen", 0, &abs)?;
    std::thread::sleep(std::time::Duration::from_millis(200));
    let path = uinput.evdev_path()?;
    println!("pen device:   {}", path.display());
    Ok(uinput)
}

fn build_touch_device(cli: &Cli) -> Result<UInputHandle<std::fs::File>> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/uinput")
        .context("open /dev/uinput")?;
    let uinput = UInputHandle::new(file);

    uinput.set_evbit(EventKind::Key)?;
    uinput.set_evbit(EventKind::Absolute)?;
    uinput.set_evbit(EventKind::Synchronize)?;
    uinput.set_keybit(Key::ButtonTouch)?;
    uinput.set_keybit(Key::ButtonToolFinger)?;
    uinput.set_keybit(Key::ButtonToolDoubleTap)?;
    uinput.set_keybit(Key::ButtonToolTripleTap)?;
    uinput.set_keybit(Key::ButtonToolQuadtap)?;
    uinput.set_keybit(Key::ButtonToolQuintTap)?;
    uinput.set_absbit(AbsoluteAxis::X)?;
    uinput.set_absbit(AbsoluteAxis::Y)?;
    uinput.set_absbit(AbsoluteAxis::MultitouchSlot)?;
    uinput.set_absbit(AbsoluteAxis::MultitouchTrackingId)?;
    uinput.set_absbit(AbsoluteAxis::MultitouchPositionX)?;
    uinput.set_absbit(AbsoluteAxis::MultitouchPositionY)?;
    uinput.set_absbit(AbsoluteAxis::MultitouchTouchMajor)?;
    uinput.set_absbit(AbsoluteAxis::MultitouchPressure)?;
    uinput.set_propbit(InputProperty::Direct)?;

    let id = InputId {
        bustype: input_linux::sys::BUS_VIRTUAL as u16,
        vendor: 0x04e8,
        product: 0x4d54,
        version: 1,
    };

    let abs = abs_touch(cli);
    uinput.create(&id, b"MoonBeam Touch", 0, &abs)?;
    std::thread::sleep(std::time::Duration::from_millis(200));
    let path = uinput.evdev_path()?;
    println!("touch device: {}", path.display());
    Ok(uinput)
}

fn abs_pen(cli: &Cli) -> Vec<AbsoluteInfoSetup> {
    vec![
        abs(AbsoluteAxis::X, 0, cli.width - 1, 12),
        abs(AbsoluteAxis::Y, 0, cli.height - 1, 12),
        abs(AbsoluteAxis::Pressure, 0, cli.pressure_max, 0),
        abs(AbsoluteAxis::TiltX, -90, 90, 0),
        abs(AbsoluteAxis::TiltY, -90, 90, 0),
    ]
}

fn abs_touch(cli: &Cli) -> Vec<AbsoluteInfoSetup> {
    vec![
        abs(AbsoluteAxis::X, 0, cli.width - 1, 12),
        abs(AbsoluteAxis::Y, 0, cli.height - 1, 12),
        abs(AbsoluteAxis::MultitouchSlot, 0, cli.slots - 1, 0),
        abs(AbsoluteAxis::MultitouchTrackingId, 0, 65535, 0),
        abs(AbsoluteAxis::MultitouchPositionX, 0, cli.width - 1, 12),
        abs(AbsoluteAxis::MultitouchPositionY, 0, cli.height - 1, 12),
        abs(AbsoluteAxis::MultitouchTouchMajor, 0, 1024, 12),
        abs(AbsoluteAxis::MultitouchPressure, 0, 255, 0),
    ]
}

fn abs(axis: AbsoluteAxis, minimum: i32, maximum: i32, resolution: i32) -> AbsoluteInfoSetup {
    AbsoluteInfoSetup {
        axis,
        info: AbsoluteInfo {
            value: 0,
            minimum,
            maximum,
            fuzz: 0,
            flat: 0,
            resolution,
        },
    }
}

fn zero_time() -> EventTime {
    EventTime::new(0, 0)
}

fn emit(uinput: &UInputHandle<std::fs::File>, events: &[input_linux::InputEvent]) -> Result<()> {
    let raw: Vec<input_linux::sys::input_event> =
        events.iter().copied().map(Into::into).collect();
    uinput.write(&raw).context("uinput write")?;
    Ok(())
}
