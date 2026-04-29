// MoonBeam M2 step 1 — uinput pen device probe.
//
// The empirical question for M2 is: can a uinput-created tablet device
// be classified as a tablet by libinput, routed by KWin to the focused
// Wayland surface, and produce pressure-sensitive input in apps like
// Krita? If yes, M2 step 2 (WebSocket control channel) is unblocked.
//
// This probe creates a uinput device with the full pen capability set
// (pressure, tilt, button, eraser, hover) and sends a synthetic stroke:
// a horizontal line across the device's coordinate space with a
// triangular pressure ramp (0 → max → 0). It then keeps the device
// alive for 30 seconds so the user can inspect it with libinput
// debug-events / xdotool / Krita.
//
// Prerequisites:
//   - /dev/uinput must be writable by the current user. On Arch + KDE
//     the systemd-uaccess rule grants this to the active session
//     automatically (verify with: `getfacl /dev/uinput`). If it isn't,
//     a one-line udev rule will fix it; not in scope for this probe.
//
// Run with:
//   cargo run --bin probe-uinput-pen
//
// Verify (in a separate terminal, before running the probe):
//   sudo libinput debug-events --show-keycodes
//   # or
//   sudo evtest /dev/input/eventXX   # path is printed at startup
//
// Expected libinput output (excerpt):
//   -event-tablet-tool TABLET_TOOL_PROXIMITY  pen     ...
//   -event-tablet-tool TABLET_TOOL_TIP        pen     pressure: 0..1
//   -event-tablet-tool TABLET_TOOL_AXIS       pen     ...

use std::fs::OpenOptions;
use std::thread::sleep;
use std::time::Duration;

use anyhow::{Context, Result};
use clap::Parser;
use input_linux::{
    AbsoluteAxis, AbsoluteEvent, AbsoluteInfo, AbsoluteInfoSetup, EventKind, EventTime, InputId,
    InputProperty, Key, KeyEvent, KeyState, MiscEvent, MiscKind, SynchronizeEvent, UInputHandle,
};

#[derive(Parser)]
#[command(about = "Create a uinput pen device, send a synthetic stroke, hold the device open")]
struct Cli {
    /// Coordinate-space width (matches the virtual display we'll
    /// associate this with later). Default is the Tab S11 Ultra panel.
    #[arg(long, default_value_t = 2960)]
    width: i32,
    /// Coordinate-space height.
    #[arg(long, default_value_t = 1848)]
    height: i32,
    /// Maximum pressure value reported.
    #[arg(long, default_value_t = 4095)]
    pressure_max: i32,
    /// How long to keep the device alive after the stroke (seconds).
    /// Long enough to inspect with libinput debug-events / Krita.
    #[arg(long, default_value_t = 30)]
    hold_secs: u64,
    /// Number of intermediate samples in the synthetic stroke.
    #[arg(long, default_value_t = 60)]
    stroke_samples: u32,
    /// Total stroke duration in milliseconds.
    #[arg(long, default_value_t = 1500)]
    stroke_ms: u64,
    /// Seconds to wait between device creation and the first stroke,
    /// so the user can attach evtest / libinput debug-events.
    #[arg(long, default_value_t = 5)]
    start_delay: u64,
    /// Number of times to repeat the stroke (with a 1s gap between).
    /// More strokes = more chances to see them in evtest / Krita.
    #[arg(long, default_value_t = 3)]
    repeats: u32,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    println!("=== MoonBeam M2 step 1 — uinput pen probe ===");

    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/uinput")
        .context("open /dev/uinput (check ACL: `getfacl /dev/uinput`)")?;
    let uinput = UInputHandle::new(file);

    // Capability bits — every event kind we'll emit must be enabled,
    // every key/axis/property must be enabled, before the device is
    // created. After UI_DEV_CREATE these are immutable.

    // Event kinds.
    uinput.set_evbit(EventKind::Key)?;
    uinput.set_evbit(EventKind::Absolute)?;
    uinput.set_evbit(EventKind::Synchronize)?;
    uinput.set_evbit(EventKind::Misc)?;

    // Pen-specific keys / buttons. The presence of BTN_TOOL_PEN +
    // INPUT_PROP_DIRECT is what tells libinput "this is a tablet, not
    // a mouse". Without these libinput would expose it as a generic
    // pointer and Krita would not see pressure.
    uinput.set_keybit(Key::ButtonToolPen)?;
    uinput.set_keybit(Key::ButtonToolRubber)?; // eraser end of the S-Pen
    uinput.set_keybit(Key::ButtonTouch)?; // pen tip on glass
    uinput.set_keybit(Key::ButtonStylus)?; // primary pen button
    uinput.set_keybit(Key::ButtonStylus2)?; // (S-Pen has only one, but we
                                            // declare both so we can map
                                            // double-click later)

    // Axes.
    uinput.set_absbit(AbsoluteAxis::X)?;
    uinput.set_absbit(AbsoluteAxis::Y)?;
    uinput.set_absbit(AbsoluteAxis::Pressure)?;
    uinput.set_absbit(AbsoluteAxis::TiltX)?;
    uinput.set_absbit(AbsoluteAxis::TiltY)?;

    // Misc events. MSC_SERIAL is conventionally used by Wacom-class
    // tablets to identify the specific tool (pen vs eraser vs other
    // pen). libinput uses it to track tool transitions.
    uinput.set_mscbit(MiscKind::Serial)?;

    // Properties — INPUT_PROP_DIRECT means the device is co-located
    // with a screen (Wacom Cintiq style), not an indirect tablet
    // (Wacom Intuos style). This is what we want for a tablet that
    // *is* the screen.
    uinput.set_propbit(InputProperty::Direct)?;

    let id = InputId {
        bustype: input_linux::sys::BUS_VIRTUAL as u16,
        // Use a Samsung-ish vendor ID and a unique product ID so
        // libinput's quirks file doesn't match us against a real
        // device. 0x04e8 is Samsung's USB vendor ID. The product ID
        // 0x4d42 spells "MB" — MoonBeam.
        vendor: 0x04e8,
        product: 0x4d42,
        version: 1,
    };

    let abs_setup = [
        AbsoluteInfoSetup {
            axis: AbsoluteAxis::X,
            info: AbsoluteInfo {
                value: 0,
                minimum: 0,
                maximum: cli.width - 1,
                fuzz: 0,
                flat: 0,
                // Resolution is units/mm; 11.62 inches diagonal Tab S11 Ultra
                // at 2960x1848 ≈ 11.7 units/mm horizontal. Approximate; not
                // critical for libinput classification.
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
            axis: AbsoluteAxis::Pressure,
            info: AbsoluteInfo {
                value: 0,
                minimum: 0,
                maximum: cli.pressure_max,
                fuzz: 0,
                flat: 0,
                resolution: 0,
            },
        },
        AbsoluteInfoSetup {
            axis: AbsoluteAxis::TiltX,
            info: AbsoluteInfo {
                value: 0,
                minimum: -90,
                maximum: 90,
                fuzz: 0,
                flat: 0,
                resolution: 0,
            },
        },
        AbsoluteInfoSetup {
            axis: AbsoluteAxis::TiltY,
            info: AbsoluteInfo {
                value: 0,
                minimum: -90,
                maximum: 90,
                fuzz: 0,
                flat: 0,
                resolution: 0,
            },
        },
    ];

    uinput
        .create(&id, b"MoonBeam Pen", 0, &abs_setup)
        .context("UI_DEV_CREATE — uinput device creation failed")?;

    // udev needs a beat to wire up /dev/input/eventN, /sys/.../, and
    // for libinput's session daemon to notice the new device.
    sleep(Duration::from_millis(200));

    let evdev = uinput
        .evdev_path()
        .context("could not look up evdev path for new device")?;
    let sys = uinput.sys_path().ok();
    println!(
        "device created: name=MoonBeam Pen vendor=0x{:04x} product=0x{:04x}",
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
    println!(
        "  pressure max:  {} (full S-Pen range is 4096)",
        cli.pressure_max
    );
    println!();
    println!("Hint:  sudo libinput debug-events --show-keycodes");
    println!("       sudo evtest {}", evdev.display());
    println!();
    println!(
        "Waiting {} s before first stroke — attach evtest / debug-events now if you want to watch.",
        cli.start_delay
    );
    sleep(Duration::from_secs(cli.start_delay));

    let serial: i32 = 0xC0FFEE;
    let y_mid = cli.height / 2;

    for stroke in 1..=cli.repeats {
        println!(
            "stroke {}/{}: {} samples over {} ms",
            stroke, cli.repeats, cli.stroke_samples, cli.stroke_ms
        );

        // Proximity in: BTN_TOOL_PEN=1, MSC_SERIAL.
        emit(
            &uinput,
            &[
                MiscEvent::new(zero_time(), MiscKind::Serial, serial).into(),
                KeyEvent::new(zero_time(), Key::ButtonToolPen, KeyState::PRESSED).into(),
                SynchronizeEvent::report(zero_time()).into(),
            ],
        )?;

        let frame_dt = Duration::from_millis(cli.stroke_ms / cli.stroke_samples as u64);
        let n = cli.stroke_samples as i32;
        for i in 0..=n {
            let t = i as f32 / n as f32;
            let x = (t * (cli.width - 1) as f32) as i32;
            // Triangular pressure: 0 -> max at midpoint -> 0.
            let pressure = if t <= 0.5 {
                (t / 0.5 * cli.pressure_max as f32) as i32
            } else {
                ((1.0 - t) / 0.5 * cli.pressure_max as f32) as i32
            };
            let touching = pressure > 0;

            let mut events = vec![
                AbsoluteEvent::new(zero_time(), AbsoluteAxis::X, x).into(),
                AbsoluteEvent::new(zero_time(), AbsoluteAxis::Y, y_mid).into(),
                AbsoluteEvent::new(zero_time(), AbsoluteAxis::Pressure, pressure).into(),
                AbsoluteEvent::new(zero_time(), AbsoluteAxis::TiltX, 0).into(),
                AbsoluteEvent::new(zero_time(), AbsoluteAxis::TiltY, 0).into(),
            ];
            // BTN_TOUCH transitions only on the boundary of pressure>0.
            if i == 1 && touching {
                events.push(
                    KeyEvent::new(zero_time(), Key::ButtonTouch, KeyState::PRESSED).into(),
                );
            } else if i == n && !touching {
                events.push(
                    KeyEvent::new(zero_time(), Key::ButtonTouch, KeyState::RELEASED).into(),
                );
            }
            events.push(SynchronizeEvent::report(zero_time()).into());
            emit(&uinput, &events)?;
            sleep(frame_dt);
        }

        // Proximity out.
        emit(
            &uinput,
            &[
                KeyEvent::new(zero_time(), Key::ButtonToolPen, KeyState::RELEASED).into(),
                SynchronizeEvent::report(zero_time()).into(),
            ],
        )?;

        if stroke < cli.repeats {
            sleep(Duration::from_secs(1));
        }
    }

    println!(
        "all strokes done; holding device alive for {} s",
        cli.hold_secs
    );
    sleep(Duration::from_secs(cli.hold_secs));

    uinput.dev_destroy().context("UI_DEV_DESTROY")?;
    println!("device destroyed; clean exit");
    Ok(())
}

fn zero_time() -> EventTime {
    // The kernel ignores user-supplied timestamps on uinput writes and
    // stamps them on receipt, so a zero EventTime is correct here.
    EventTime::new(0, 0)
}

fn emit(uinput: &UInputHandle<std::fs::File>, events: &[input_linux::InputEvent]) -> Result<()> {
    let raw: Vec<input_linux::sys::input_event> =
        events.iter().copied().map(Into::into).collect();
    uinput.write(&raw).context("uinput write")?;
    Ok(())
}
