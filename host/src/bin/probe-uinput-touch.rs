// MoonBeam M2 step 2 — uinput multitouch device probe.
//
// Companion to probe-uinput-pen. Where the pen probe proves a single
// pressure-sensitive tool, this probe proves the finger-touch path:
// up to 10 simultaneous contacts, no pressure / tilt, but with the
// MT protocol type B slot machinery that real touchscreens use.
//
// The empirical question for M2 step 2 is: does udev classify our
// multitouch device as `ID_INPUT_TOUCHSCREEN=1` (the heuristic Wayland
// compositors use to route through the touch protocol), and does the
// kernel accept a multi-finger gesture as a coherent event sequence?
//
// What the probe creates:
//   - One uinput device named "MoonBeam Touch"
//   - INPUT_PROP_DIRECT (touchscreen, not touchpad)
//   - Single-touch fallback axes ABS_X / ABS_Y for legacy clients
//   - MT axes: SLOT, TRACKING_ID, POSITION_X/Y, TOUCH_MAJOR, PRESSURE
//   - Buttons: BTN_TOUCH, BTN_TOOL_FINGER..QUINTTAP for libinput's
//     contact-count classification
//
// What the probe sends (when --repeats > 0):
//   A two-finger pinch-out gesture. Both contacts start at the screen
//   center and animate outward to the left and right edges over the
//   stroke duration, then release. Repeats with a 1s gap.
//
// Run with:
//   cargo run --bin probe-uinput-touch
//
// Verify (in a separate terminal, before running):
//   sudo libinput debug-events --show-keycodes
//   sudo evtest /dev/input/eventXX
//
// Expected libinput output (excerpt):
//   -event-touch  TOUCH_DOWN   slot 0  (1480, 924)
//   -event-touch  TOUCH_DOWN   slot 1  (1480, 924)
//   -event-touch  TOUCH_MOTION slot 0  ...
//   -event-touch  TOUCH_MOTION slot 1  ...
//   -event-touch  TOUCH_UP     slot 0
//   -event-touch  TOUCH_UP     slot 1

use std::fs::OpenOptions;
use std::thread::sleep;
use std::time::Duration;

use anyhow::{Context, Result};
use clap::Parser;
use input_linux::{
    AbsoluteAxis, AbsoluteEvent, AbsoluteInfo, AbsoluteInfoSetup, EventKind, EventTime, InputId,
    InputProperty, Key, KeyEvent, KeyState, SynchronizeEvent, UInputHandle,
};

#[derive(Parser)]
#[command(about = "Create a uinput multitouch device, send a synthetic 2-finger pinch, hold open")]
struct Cli {
    /// Coordinate-space width. Default is the Tab S11 Ultra panel.
    #[arg(long, default_value_t = 2960)]
    width: i32,
    /// Coordinate-space height.
    #[arg(long, default_value_t = 1848)]
    height: i32,
    /// Number of MT slots to advertise. The Tab S11 Ultra reports 10.
    #[arg(long, default_value_t = 10)]
    slots: i32,
    /// How long to keep the device alive after the gesture (seconds).
    #[arg(long, default_value_t = 30)]
    hold_secs: u64,
    /// Number of intermediate samples in the synthetic gesture.
    #[arg(long, default_value_t = 60)]
    gesture_samples: u32,
    /// Total gesture duration in milliseconds.
    #[arg(long, default_value_t = 1500)]
    gesture_ms: u64,
    /// Seconds to wait between device creation and the first gesture,
    /// so the user can attach evtest / libinput debug-events.
    #[arg(long, default_value_t = 5)]
    start_delay: u64,
    /// Number of times to repeat the gesture (with a 1s gap between).
    #[arg(long, default_value_t = 3)]
    repeats: u32,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    println!("=== MoonBeam M2 step 2 — uinput multitouch probe ===");

    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/uinput")
        .context("open /dev/uinput (check ACL: `getfacl /dev/uinput`)")?;
    let uinput = UInputHandle::new(file);

    // Capability bits — must be enabled before UI_DEV_CREATE.
    uinput.set_evbit(EventKind::Key)?;
    uinput.set_evbit(EventKind::Absolute)?;
    uinput.set_evbit(EventKind::Synchronize)?;

    // Touch buttons. BTN_TOUCH is the single-bit "any contact down"
    // signal; BTN_TOOL_FINGER..QUINTTAP let libinput count active
    // contacts (1..5+ fingers) without parsing slot state itself.
    uinput.set_keybit(Key::ButtonTouch)?;
    uinput.set_keybit(Key::ButtonToolFinger)?;
    uinput.set_keybit(Key::ButtonToolDoubleTap)?;
    uinput.set_keybit(Key::ButtonToolTripleTap)?;
    uinput.set_keybit(Key::ButtonToolQuadtap)?;
    uinput.set_keybit(Key::ButtonToolQuintTap)?;

    // Single-touch fallback axes (mirrored from slot 0 each frame so
    // legacy / non-MT clients still see something coherent).
    uinput.set_absbit(AbsoluteAxis::X)?;
    uinput.set_absbit(AbsoluteAxis::Y)?;

    // MT axes (protocol type B — slot-based).
    uinput.set_absbit(AbsoluteAxis::MultitouchSlot)?;
    uinput.set_absbit(AbsoluteAxis::MultitouchTrackingId)?;
    uinput.set_absbit(AbsoluteAxis::MultitouchPositionX)?;
    uinput.set_absbit(AbsoluteAxis::MultitouchPositionY)?;
    uinput.set_absbit(AbsoluteAxis::MultitouchTouchMajor)?;
    uinput.set_absbit(AbsoluteAxis::MultitouchPressure)?;

    // INPUT_PROP_DIRECT: touchscreen, co-located with display.
    // Without it, libinput would expose this as a touchpad (indirect
    // device) and Wayland would pipe it through the pointer protocol.
    uinput.set_propbit(InputProperty::Direct)?;

    let id = InputId {
        bustype: input_linux::sys::BUS_VIRTUAL as u16,
        // Same Samsung VID as the pen probe; product 0x4d54 spells
        // "MT" (MoonBeam Touch). Distinct from the pen's 0x4d42 ("MB")
        // so udev / libinput / KDE's tablet KCM treat them as separate
        // devices that just happen to share a vendor.
        vendor: 0x04e8,
        product: 0x4d54,
        version: 1,
    };

    let abs_setup = vec![
        AbsoluteInfoSetup {
            axis: AbsoluteAxis::X,
            info: AbsoluteInfo {
                value: 0,
                minimum: 0,
                maximum: cli.width - 1,
                fuzz: 0,
                flat: 0,
                resolution: 12,
            },
        },
        AbsoluteInfoSetup {
            axis: AbsoluteAxis::Y,
            info: AbsoluteInfo {
                value: 0,
                minimum: 0,
                maximum: cli.height - 1,
                fuzz: 0,
                flat: 0,
                resolution: 12,
            },
        },
        AbsoluteInfoSetup {
            axis: AbsoluteAxis::MultitouchSlot,
            info: AbsoluteInfo {
                value: 0,
                minimum: 0,
                maximum: cli.slots - 1,
                fuzz: 0,
                flat: 0,
                resolution: 0,
            },
        },
        AbsoluteInfoSetup {
            axis: AbsoluteAxis::MultitouchTrackingId,
            info: AbsoluteInfo {
                value: 0,
                minimum: 0,
                // Kernel-recommended cap; -1 is reserved for "released".
                maximum: 65535,
                fuzz: 0,
                flat: 0,
                resolution: 0,
            },
        },
        AbsoluteInfoSetup {
            axis: AbsoluteAxis::MultitouchPositionX,
            info: AbsoluteInfo {
                value: 0,
                minimum: 0,
                maximum: cli.width - 1,
                fuzz: 0,
                flat: 0,
                resolution: 12,
            },
        },
        AbsoluteInfoSetup {
            axis: AbsoluteAxis::MultitouchPositionY,
            info: AbsoluteInfo {
                value: 0,
                minimum: 0,
                maximum: cli.height - 1,
                fuzz: 0,
                flat: 0,
                resolution: 12,
            },
        },
        AbsoluteInfoSetup {
            axis: AbsoluteAxis::MultitouchTouchMajor,
            info: AbsoluteInfo {
                value: 0,
                minimum: 0,
                // Major-axis size in the same units as POSITION (so
                // millimetres × resolution). 200 ≈ 16mm at res=12, a
                // realistic fingertip contact patch.
                maximum: 1024,
                fuzz: 0,
                flat: 0,
                resolution: 12,
            },
        },
        AbsoluteInfoSetup {
            axis: AbsoluteAxis::MultitouchPressure,
            info: AbsoluteInfo {
                value: 0,
                minimum: 0,
                // Capacitive panels typically expose 0..255 here. Many
                // panels just hard-code 1 when in contact and 0 when
                // not; clients (libinput) treat this axis as advisory.
                maximum: 255,
                fuzz: 0,
                flat: 0,
                resolution: 0,
            },
        },
    ];

    uinput
        .create(&id, b"MoonBeam Touch", 0, &abs_setup)
        .context("UI_DEV_CREATE — uinput multitouch device creation failed")?;

    sleep(Duration::from_millis(200));

    let evdev = uinput
        .evdev_path()
        .context("could not look up evdev path for new device")?;
    let sys = uinput.sys_path().ok();
    println!(
        "device created: name=MoonBeam Touch vendor=0x{:04x} product=0x{:04x}",
        id.vendor, id.product
    );
    println!("  evdev path:    {}", evdev.display());
    if let Some(s) = sys {
        println!("  sysfs path:    {}", s.display());
    }
    println!(
        "  coord space:   {}x{} (matches Tab S11 Ultra panel default)",
        cli.width, cli.height
    );
    println!("  slots:         {} (Tab S11 Ultra reports 10)", cli.slots);
    println!();
    println!("Hint:  sudo libinput debug-events --show-keycodes");
    println!("       sudo evtest {}", evdev.display());
    println!();
    println!(
        "Waiting {} s before first gesture — attach evtest / debug-events now if you want to watch.",
        cli.start_delay
    );
    sleep(Duration::from_secs(cli.start_delay));

    let center_x = cli.width / 2;
    let y_mid = cli.height / 2;
    // Pinch-out endpoints: from center to ~1/4 and ~3/4 of the width.
    let left_end = cli.width / 4;
    let right_end = cli.width - left_end;

    // Tracking IDs are per-contact, not per-slot. They must be unique
    // within a stroke and bumped on every fresh down. We allocate a
    // fresh pair per stroke iteration.
    let mut next_tracking_id: i32 = 100;

    for stroke in 1..=cli.repeats {
        println!(
            "gesture {}/{}: 2-finger pinch-out, {} samples over {} ms",
            stroke, cli.repeats, cli.gesture_samples, cli.gesture_ms
        );

        let id0 = next_tracking_id;
        let id1 = next_tracking_id + 1;
        next_tracking_id += 2;

        // Frame 0: both contacts down at the center.
        emit(
            &uinput,
            &[
                AbsoluteEvent::new(zero_time(), AbsoluteAxis::MultitouchSlot, 0).into(),
                AbsoluteEvent::new(zero_time(), AbsoluteAxis::MultitouchTrackingId, id0).into(),
                AbsoluteEvent::new(zero_time(), AbsoluteAxis::MultitouchPositionX, center_x)
                    .into(),
                AbsoluteEvent::new(zero_time(), AbsoluteAxis::MultitouchPositionY, y_mid).into(),
                AbsoluteEvent::new(zero_time(), AbsoluteAxis::MultitouchTouchMajor, 200).into(),
                AbsoluteEvent::new(zero_time(), AbsoluteAxis::MultitouchPressure, 100).into(),
                AbsoluteEvent::new(zero_time(), AbsoluteAxis::MultitouchSlot, 1).into(),
                AbsoluteEvent::new(zero_time(), AbsoluteAxis::MultitouchTrackingId, id1).into(),
                AbsoluteEvent::new(zero_time(), AbsoluteAxis::MultitouchPositionX, center_x)
                    .into(),
                AbsoluteEvent::new(zero_time(), AbsoluteAxis::MultitouchPositionY, y_mid).into(),
                AbsoluteEvent::new(zero_time(), AbsoluteAxis::MultitouchTouchMajor, 200).into(),
                AbsoluteEvent::new(zero_time(), AbsoluteAxis::MultitouchPressure, 100).into(),
                KeyEvent::new(zero_time(), Key::ButtonTouch, KeyState::PRESSED).into(),
                KeyEvent::new(zero_time(), Key::ButtonToolDoubleTap, KeyState::PRESSED).into(),
                AbsoluteEvent::new(zero_time(), AbsoluteAxis::X, center_x).into(),
                AbsoluteEvent::new(zero_time(), AbsoluteAxis::Y, y_mid).into(),
                SynchronizeEvent::report(zero_time()).into(),
            ],
        )?;

        let frame_dt = Duration::from_millis(cli.gesture_ms / cli.gesture_samples as u64);
        let n = cli.gesture_samples as i32;
        for i in 1..=n {
            let t = i as f32 / n as f32;
            let x0 = center_x + ((left_end - center_x) as f32 * t) as i32;
            let x1 = center_x + ((right_end - center_x) as f32 * t) as i32;

            emit(
                &uinput,
                &[
                    AbsoluteEvent::new(zero_time(), AbsoluteAxis::MultitouchSlot, 0).into(),
                    AbsoluteEvent::new(zero_time(), AbsoluteAxis::MultitouchPositionX, x0).into(),
                    AbsoluteEvent::new(zero_time(), AbsoluteAxis::MultitouchSlot, 1).into(),
                    AbsoluteEvent::new(zero_time(), AbsoluteAxis::MultitouchPositionX, x1).into(),
                    // Single-touch fallback follows slot 0.
                    AbsoluteEvent::new(zero_time(), AbsoluteAxis::X, x0).into(),
                    SynchronizeEvent::report(zero_time()).into(),
                ],
            )?;
            sleep(frame_dt);
        }

        // Release both contacts. Per MT protocol type B: emit
        // tracking_id = -1 in each slot to signal release.
        emit(
            &uinput,
            &[
                AbsoluteEvent::new(zero_time(), AbsoluteAxis::MultitouchSlot, 0).into(),
                AbsoluteEvent::new(zero_time(), AbsoluteAxis::MultitouchTrackingId, -1).into(),
                AbsoluteEvent::new(zero_time(), AbsoluteAxis::MultitouchSlot, 1).into(),
                AbsoluteEvent::new(zero_time(), AbsoluteAxis::MultitouchTrackingId, -1).into(),
                KeyEvent::new(zero_time(), Key::ButtonTouch, KeyState::RELEASED).into(),
                KeyEvent::new(zero_time(), Key::ButtonToolDoubleTap, KeyState::RELEASED).into(),
                SynchronizeEvent::report(zero_time()).into(),
            ],
        )?;

        if stroke < cli.repeats {
            sleep(Duration::from_secs(1));
        }
    }

    println!(
        "all gestures done; holding device alive for {} s",
        cli.hold_secs
    );
    sleep(Duration::from_secs(cli.hold_secs));

    uinput.dev_destroy().context("UI_DEV_DESTROY")?;
    println!("device destroyed; clean exit");
    Ok(())
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
