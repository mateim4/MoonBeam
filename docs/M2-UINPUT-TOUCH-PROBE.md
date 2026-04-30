# M2 step 2 — uinput multitouch probe

Run: 2026-04-30

## What the probe confirms

`host/src/bin/probe-uinput-touch.rs` is the finger-touch counterpart
to the M2 step 1 pen probe. Same shape, different capability set: 10
slots of MT protocol type B, single-touch fallback axes for legacy
clients, and contact-count buttons (BTN_TOOL_FINGER through QUINTTAP)
so libinput can count active fingers without parsing slot state itself.

```
=== MoonBeam M2 step 2 — uinput multitouch probe ===
device created: name=MoonBeam Touch vendor=0x04e8 product=0x4d54
  evdev path:    /dev/input/event261
  sysfs path:    /sys/devices/virtual/input/input46
  coord space:   2960x1848 (matches Tab S11 Ultra panel default)
  slots:         10 (Tab S11 Ultra reports 10)
```

## Capabilities that landed on the device

Read from `/sys/class/input/event261/device/capabilities/*`:

| What | sysfs hex | Decoded |
|---|---|---|
| `ev`         | `0xb`             | EV_SYN, EV_KEY, EV_ABS |
| `abs`        | `0x661800000000003` | ABS_X, ABS_Y, ABS_MT_SLOT, ABS_MT_TOUCH_MAJOR, ABS_MT_POSITION_X, ABS_MT_POSITION_Y, ABS_MT_TRACKING_ID, ABS_MT_PRESSURE |
| `key` word 5 | `0xe520`          | BTN_TOOL_FINGER (0x145), BTN_TOOL_QUINTTAP (0x148), BTN_TOUCH (0x14a), BTN_TOOL_DOUBLETAP (0x14d), BTN_TOOL_TRIPLETAP (0x14e), BTN_TOOL_QUADTAP (0x14f) |
| `properties` | `2`               | INPUT_PROP_DIRECT |
| `id`         | bus=0006, vendor=04e8, product=4d54 | BUS_VIRTUAL, Samsung VID, "MT" PID |

All 8 declared axes and all 6 declared buttons are present, with no
extras leaking in. The pen probe (M2 step 1) and the touch probe both
share `vendor=0x04e8`, but their distinct PIDs (`0x4d42`="MB" for pen,
`0x4d54`="MT" for touch) keep them as separate devices in udev / KDE's
input KCM.

## Userspace classification

```
$ udevadm info /dev/input/event261
E: ID_INPUT=1
E: ID_INPUT_TOUCHSCREEN=1
E: ID_INPUT_WIDTH_MM=246
E: ID_INPUT_HEIGHT_MM=153
```

`ID_INPUT_TOUCHSCREEN=1` (not `ID_INPUT_TABLET`, not `ID_INPUT_TOUCHPAD`)
is the exact classification we wanted. The decision tree in udev's
`60-input-id.rules`:

- `ABS_MT_SLOT` + `INPUT_PROP_DIRECT` + no `BTN_TOOL_PEN` → **touchscreen**
- `ABS_MT_SLOT` + no `INPUT_PROP_DIRECT` → touchpad
- `BTN_TOOL_PEN` + `INPUT_PROP_DIRECT` → tablet (the pen probe)

So the same heuristic that gave the pen probe a tablet classification
gave us a touchscreen classification — both of which Wayland
compositors route through the right protocols.

## Wire format on the kernel side (locked)

Two-finger pinch-out, one frame at `t=0`:

```
ABS_MT_SLOT          0
ABS_MT_TRACKING_ID   <id0>           # fresh per-stroke ID, never reused
ABS_MT_POSITION_X    1480
ABS_MT_POSITION_Y    924
ABS_MT_TOUCH_MAJOR   200
ABS_MT_PRESSURE      100
ABS_MT_SLOT          1
ABS_MT_TRACKING_ID   <id1>
ABS_MT_POSITION_X    1480
ABS_MT_POSITION_Y    924
ABS_MT_TOUCH_MAJOR   200
ABS_MT_PRESSURE      100
BTN_TOUCH            1
BTN_TOOL_DOUBLETAP   1
ABS_X                1480              # single-touch fallback follows slot 0
ABS_Y                924
SYN_REPORT
```

Per-frame thereafter (only the things that changed):

```
ABS_MT_SLOT          0
ABS_MT_POSITION_X    <x0>
ABS_MT_SLOT          1
ABS_MT_POSITION_X    <x1>
ABS_X                <x0>
SYN_REPORT
```

Release:

```
ABS_MT_SLOT          0
ABS_MT_TRACKING_ID   -1                # MT-B "this slot is gone"
ABS_MT_SLOT          1
ABS_MT_TRACKING_ID   -1
BTN_TOUCH            0
BTN_TOOL_DOUBLETAP   0
SYN_REPORT
```

Two MT-B subtleties worth pinning down here:

1. **`ABS_MT_TRACKING_ID` is per-contact, not per-slot.** Slot 0 might
   carry a different ID across two consecutive strokes; what matters
   is that within a single down→up sequence, the ID stays stable, and
   that the value `-1` is reserved for "released". The probe bumps
   `next_tracking_id` by 2 per stroke so we never alias.

2. **`BTN_TOUCH` and the contact-count buttons (FINGER / DOUBLETAP /
   TRIPLETAP / QUADTAP / QUINTTAP)** are *redundant* with the slot
   state in a strict reading of MT-B — but in practice every kernel
   touchscreen driver and every userspace consumer relies on them.
   We emit `BTN_TOUCH` once at down and once at up; we toggle exactly
   one of the contact-count buttons depending on how many slots are
   active. For two fingers: `BTN_TOOL_DOUBLETAP=1` at down,
   `BTN_TOOL_DOUBLETAP=0` at up.

## Decisions captured

- **One uinput touch device per virtual display**, distinct from the
  pen device. Sharing one device for both pen and touch is allowed by
  the spec but in practice breaks libinput's classification because
  it has to pick one of {tablet, touchscreen} per device.
- **10 slots advertised**, matching the Tab S11 Ultra. Higher slot
  counts (kernel max 60) are pure waste; lower would silently drop
  contacts when the user actually uses 6+ fingers (some drawing apps
  use multi-finger gestures for canvas manipulation).
- **MT_PRESSURE 0..255**, not 0..4095. Capacitive panels expose tiny
  capacitance variations as "pressure", and 8 bits is more than
  enough resolution. The S-Pen's 12-bit pressure is on the *pen*
  device, not the touch device.
- **MT_TOUCH_MAJOR 0..1024**, units = millimetres × resolution. With
  res=12, the maximum is ~85mm — fingertips are 8-15mm, palms can be
  40-60mm. Big enough that palm rejection (the user's thumb laid
  across the bezel) is representable.
- **No MT_TOUCH_MINOR or MT_ORIENTATION axes.** The Android touch
  callbacks expose major and minor axes of the contact ellipse, but
  the orientation is rarely meaningful at our event rate. We can add
  them later if a gesture (e.g., palm rejection by ellipse aspect
  ratio) needs them.
- **Single-touch fallback (ABS_X / ABS_Y) follows slot 0.** Required
  for any non-MT-aware client that still wants pointer events;
  Xwayland gets these.
- **Tracking IDs are bumped per-stroke, not reused.** The kernel
  doesn't enforce uniqueness — it would happily accept the same ID
  twice — but libinput uses ID equality to detect "same finger, lift
  and re-touch within debounce" vs "different fingers", and the wrong
  call can dedupe a real tap.

## Things deferred (not blocking M2 step 2)

- **Visual confirmation in a touch-aware app.** Same logic as the pen
  probe: the static classification is what determines whether the
  device exists in the right input category, and the synthetic
  gesture path is in place behind `--repeats N`. Visual confirmation
  is one Krita-or-system-pinch test away when M3 is ready to send
  real events.
- **Palm rejection** when pen is in proximity. Spec'd in
  MOONBEAM-APP-PLAN.md §9.1; implementation is a cross-device
  coordination problem (the touch device needs to know the pen
  device's proximity state) and lives in the WS control channel
  layer, not the probe.
- **Coordination with the pen device's coordinate space.** Both
  probes default to 2960×1848. Once the WS control channel exists
  the host will treat pen+touch as a single virtual tablet pair
  bound to the same virtual display.

## System under test

Same as M2 step 1 (kernel 6.19.6-arch1-3-g14, KWin 6.6.4 on Plasma 6.6
Wayland, input-linux 0.7.1).

## Follow-ups (not blocking M2)

- **M2 step 3 — control-channel input over WebSocket.** Wire format:
  `[type:0x03][flags][payload]` from the M1-locked framing. Payload
  is a tagged union of `{down, move, up, tilt, pressure, button}`
  events for pen, plus `{slot_down, slot_move, slot_up}` events for
  touch. Server side feeds them into the two uinput devices these
  probes just proved out. Synthetic source first (an axum endpoint
  that accepts JSON and translates into uinput writes), then real
  Android events in M3.
- **M2 step 4 — single multiplexed WS.** Roll video + input into one
  upgrade endpoint, same code path the browser probe demonstrated
  for video.
- **Per-tablet identity binding.** Once M3 ships, the host needs to
  associate a paired tablet with its pair of uinput devices and
  destroy them on disconnect. Out of scope for the probes.
