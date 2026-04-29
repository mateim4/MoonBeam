# M2 step 1 — uinput pen probe

Run: 2026-04-29

## What the probe confirms

`host/src/bin/probe-uinput-pen.rs` opens `/dev/uinput`, builds a virtual
pen device with the full set of axes and buttons documented in
`docs/MOONBEAM-APP-PLAN.md` §9.1, then (optionally) emits synthetic
strokes so the kernel actually carries pressure-modulated events.

The build is `input-linux 0.7.1`'s `UInputHandle` API: `set_evbit` /
`set_keybit` / `set_absbit` / `set_mscbit` / `set_propbit` to declare
capabilities, `create()` with an `[AbsoluteInfoSetup]` slice for the
five axes, then `write(&[InputEvent])` to push events.

```
=== MoonBeam M2 step 1 — uinput pen probe ===
device created: name=MoonBeam Pen vendor=0x04e8 product=0x4d42
  evdev path:    /dev/input/event279
  sysfs path:    /sys/devices/virtual/input/input65
  coord space:   2960x1848 (matches Tab S11 Ultra panel default)
  pressure max:  4095 (full S-Pen range is 4096)
```

## Capabilities that landed on the device

Read from `/sys/class/input/event279/device/capabilities/*` while the
probe was holding the device alive:

| What | sysfs hex | Decoded |
|---|---|---|
| `ev`         | `0x1b`        | EV_SYN, EV_KEY, EV_ABS, EV_MSC |
| `abs`        | `0xd000003`   | ABS_X, ABS_Y, ABS_PRESSURE, ABS_TILT_X, ABS_TILT_Y |
| `key` word 5 | `0x1c03`      | BTN_TOOL_PEN (0x140), BTN_TOOL_RUBBER (0x141), BTN_TOUCH (0x14a), BTN_STYLUS (0x14b), BTN_STYLUS2 (0x14c) |
| `msc`        | `0x1`         | MSC_SERIAL |
| `properties` | `2`           | INPUT_PROP_DIRECT |
| `id`         | bus=0006, vendor=04e8, product=4d42 | BUS_VIRTUAL, Samsung VID, "MB" PID |

This matches the M2 spec exactly. Eraser detection works because
`BTN_TOOL_RUBBER` is in the keymap; we toggle pen↔eraser by emitting
`BTN_TOOL_PEN=0` + `BTN_TOOL_RUBBER=1` (and vice versa) at proximity-
in time, which Krita / GIMP / Inkscape all already respect.

## Userspace classification

```
$ udevadm info /dev/input/event279
E: ID_INPUT=1
E: ID_INPUT_TABLET=1
E: ID_INPUT_WIDTH_MM=246
E: ID_INPUT_HEIGHT_MM=153
```

`ID_INPUT_TABLET=1` is what we wanted. udev's `60-input-id.rules`
ships the same heuristic libinput uses internally
(`EVDEV_INPUT_DEVICE_TABLET` derives from it), so Wayland compositors
will route our device through the tablet protocol rather than as a
generic touchscreen or pointer.

The 246×153 mm physical-size derives from `resolution=12` units/mm in
the `AbsoluteInfoSetup` for X and Y — close enough to the Tab S11
Ultra's actual panel (~254×165 mm) that clients won't apply a weird
scale factor when they look at `INPUT_PROP_DIRECT` devices.

## Wire format on the kernel side (locked)

Event sequence per stroke:

```
proximity-in:
  EV_MSC  MSC_SERIAL   <serial>
  EV_KEY  BTN_TOOL_PEN 1
  EV_SYN  SYN_REPORT

per sample (60×, default):
  EV_ABS  ABS_X        <x>
  EV_ABS  ABS_Y        <y>
  EV_ABS  ABS_PRESSURE <p>          # 0..4095, triangular ramp
  EV_ABS  ABS_TILT_X   <tx>         # placeholder 0 in the probe
  EV_ABS  ABS_TILT_Y   <ty>
  EV_KEY  BTN_TOUCH    0|1          # only emitted on transitions
  EV_SYN  SYN_REPORT

proximity-out:
  EV_KEY  BTN_TOUCH    0
  EV_KEY  BTN_TOOL_PEN 0
  EV_SYN  SYN_REPORT
```

`EventTime::new(0, 0)` is correct on every event — the kernel rewrites
the timestamp on receipt, and uinput clients (libinput etc.) only ever
see the kernel-stamped value. Emitting non-zero timestamps from
userspace is a footgun documented in `Documentation/input/uinput.rst`.

## Decisions captured

- **One uinput device per virtual display**, not a single device shared
  across multiple displays. When M3 ships and a tablet pairs to the
  host, we'll create one `MoonBeam Pen` + one `MoonBeam Touch` per
  paired tablet identity. The vendor/product/serial we use here
  (Samsung VID + ASCII "MB" PID + per-pair serial) gives KDE's tablet
  applet enough to remember per-device settings across reconnects.
- **Coordinate space matches the Android-side virtual display**, not
  the host's primary monitor. So when the Android client is showing a
  2960×1848 surface, an event at (2960, 1848) on the tablet maps 1:1
  to (2960, 1848) in the uinput device. Compositor-side mapping from
  uinput coords → screen output is handled by KDE's tablet
  configuration (`kcm_tablet`), the same way a Wacom would be
  configured.
- **Pressure max 4095**, not 8191 or 16383. The Tab S11 Ultra's S-Pen
  reports 0..4095, and there's no benefit to widening the range
  past what the source device has.
- **Resolution = 12 units/mm** for X and Y. Picked so width-mm × 12 ≈
  panel width, which keeps `ID_INPUT_WIDTH_MM` honest and avoids
  confusing libinput's tablet-tools auto-config.
- **Synthetic stroke is parameterised**, not hard-coded. Default is 3
  strokes × 60 samples × 1.5 s, with 5 s pre-roll so the developer
  can attach `evtest` or `libinput debug-events` first. The eraser
  toggle is not wired up yet — adding it is one keybit transition,
  scheduled with the multitouch probe (M2 step 2) so the two probes
  ship a single coherent input model.

## Things deferred (not blocking M2)

- **Krita visual test**: confirming pressure ramps actually paint a
  fading line in a real tablet-aware app. Static `udevadm` and sysfs
  checks already prove the device is correctly classified, and the
  stroke loop is in place; the visual confirmation is a polish step
  before M3.
- **Tilt is emitted but always 0** in the probe. The Android client
  is the source of truth for tilt; the probe just proves the axis
  exists on the kernel side. Real tilt values go in via the WS
  control channel in M2 step 3.
- **Eraser flip and S-Pen button events** (BTN_STYLUS / BTN_STYLUS2)
  are declared on the device but not exercised by the synthetic
  stroke. These come for free once the WS control channel ships,
  because the wire format is just a tagged union of these same event
  types — the probe is the same code path with a hard-coded source.
- **Serial uniqueness**: the probe uses a fixed `MSC_SERIAL` value.
  Per-pair serials live in M3 once we have a tablet identity to
  derive them from.

## System under test

- Kernel: 6.19.6-arch1-3-g14
- KWin: 6.6.4 (Plasma 6.6, Wayland)
- input-linux: 0.7.1
- udev: systemd 257 (`60-input-id.rules`)
- /dev/uinput permissions: ACL grants `mateim` rw (no group-input
  membership required to create the device, but reading the resulting
  `/dev/input/eventNN` still requires `input` group — only relevant
  for debugging probes, not for production where moonbeamd will own
  the FD it created)

## Follow-ups (not blocking M2)

- **M2 step 2 — multitouch probe**: same shape as this one but with
  `ABS_MT_*` axes for 10 fingers + `INPUT_PROP_DIRECT` on a separate
  device. Krita doesn't care about touch; the receiver is the
  compositor itself for two-finger scroll / pinch.
- **M2 step 3 — control-channel input**: WebSocket type=0x03, payload
  is a tagged union of `{down, move, up, tilt, button, pressure}`
  events. Server-side feeds them into the same uinput device this
  probe just proved out. Synthetic source first (an axum endpoint
  that takes JSON), then real Android events in M3.
- **M2 step 4 — single multiplexed WS**: roll video / input into one
  upgrade endpoint with the locked
  `[type:u8][flags:u8][payload]` framing from M1.
