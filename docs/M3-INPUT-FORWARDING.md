# M3 step 3 — Android input forwarding

Run: 2026-04-30

## What this step is

The other half of the M3 round-trip. Step 2 made laptop video appear
on the tablet; step 3 makes tablet finger and pen events appear on
the laptop, lighting up the uinput devices we built in M2.

Touching this off in code:

- `TouchHandler.kt` (in `:app/com.m151.moonbeam.input`) — translates
  Android `MotionEvent`s into our locked `InputMsg` JSON. Dispatches
  on per-pointer `getToolType`: `TOOL_TYPE_STYLUS`/`TOOL_TYPE_ERASER`
  goes to the pen path with pressure + tilt + button-state diff;
  everything else goes to the multitouch path. Slot allocation reuses
  Android's `pointerId` directly as the uinput MT slot; tracking ids
  are issued per-stroke and never reused.
- `MoonBeamViewModel.sendInput(InputMsg)` — the bridge. Calls
  `WsClient.send`, which encodes via `Wire.encodeInput` and pushes a
  binary frame.
- `MainActivity` SurfaceView setup — attaches both
  `setOnTouchListener` (touch + pen-on-glass) and `setOnHoverListener`
  (pen near glass, not touching) so we capture the full pen
  interaction range. Both feed the same `TouchHandler.handle`.

The host side is unchanged — `probe-mux` already had `apply()` from
M2 step 4. We added a `println!("  input: {msg:?}")` log line to
surface the events for live debugging.

## What works (verified live, 2026-04-30)

**Touch path → host touchscreen:**
- Single tap, single drag, multi-finger gesture all flow
- `TouchDown {slot, id, x, y, ...} → TouchMove (×N) → TouchUp` with
  monotonically-incrementing tracking ids per stroke
- KDE recognises the device as `ID_INPUT_TOUCHSCREEN=1` (per M2 step 2)
  and routes through `wl_touch` to the focused surface
- Confirmed by **scrolling a terminal on the host using a finger
  gesture on the tablet** — the host treats the tablet as a real
  touchscreen for any touch-aware app

**Pen path → host tablet:**
- Pen tip on glass: smooth `PenDown → PenMove (varying pressure) → PenUp`
- Real pressure values (e.g. 86–99 across a stroke), real tilt
  decomposition into `tilt_x`/`tilt_y` (e.g. -9 → -6 → -4 across a
  stroke as the angle changes)
- KDE recognises the device as `ID_INPUT_TABLET=1` (per M2 step 1)
  and routes through `wp_tablet_v2`
- Confirmed by **drawing pressure-modulated strokes in Inkscape on
  the host** with the Tab S11 Ultra's S-Pen Creator Edition

**Pen ≠ touch routing nuance:**
- Touch events go to *every* surface (terminal, browser, anything
  on screen)
- Pen events only go to apps that *opt into* the tablet protocol
  (Inkscape, Krita, GIMP). Terminals/text editors don't, which is
  why a finger drag scrolled the terminal but a pen stroke didn't.
  This is correct Wayland behaviour, not a bug.

## Wire format used

Reuses the M2-step-3 JSON schema verbatim (locked in
`docs/M2-INPUT-WS-PROBE.md`):

```jsonc
{ "type": "pen_down",   "x": 1179, "y": 1061, "pressure": 98, "tilt_x": -9, "tilt_y": 0 }
{ "type": "pen_move",   "x": 1179, "y": 1060, "pressure": 97, "tilt_x": -9, "tilt_y": 0 }
{ "type": "pen_up" }
{ "type": "pen_button", "button": "stylus", "state": true }
{ "type": "touch_down", "slot": 0, "id": 1011, "x": 339, "y": 1475, "major": 200, "pressure": 100 }
{ "type": "touch_move", "slot": 0, "x": 343, "y": 1445, "major": 200, "pressure": 100 }
{ "type": "touch_up",   "slot": 0 }
```

Each frame is `[0x03][0x00][json]` — same `[type:u8][flags:u8][payload]`
framing as video, just type 0x03 instead of 0x01. The Rust host
discriminates on the leading byte.

## Decisions captured

- **Coordinate pass-through 1:1.** No transform between Android pixel
  coords and uinput device coord space. Works because the tablet
  runs fullscreen at native resolution (2960×1848) and the uinput
  devices are configured at the same resolution. M3 step 4 may
  revisit if the host's virtual display ends up at a different
  geometry than the tablet panel.
- **Pressure pass-through with `coerceAtLeast(1)`.** Android's
  `getPressure()` returns 0..1 (sometimes slightly more); we
  multiply by `pressure_max=4095` and clamp. Forcing a minimum of 1
  on `pen_down`/`pen_move` because the host treats `pressure=0` as
  "not touching" via internal logic, which causes brief stroke
  drop-outs at very light touches.
- **Tilt computed from polar form.** Android gives `AXIS_TILT`
  (magnitude in radians, 0=perpendicular) and `getOrientation`
  (which way the tilt points, -π..+π). We decompose into Cartesian
  `tilt_x = sin(tilt) × cos(orientation) × 90`, same for y, clamped
  to ±90 to match the uinput device's range. Verified live —
  stroking the pen at different angles produces distinct
  tilt_x/tilt_y in the host log.
- **`pointerId` used directly as MT slot.** Android keeps pointer
  ids stable within a gesture (which is what matters for slot state
  on the host). Across gestures they're reused, but a fresh
  tracking id (`nextTrackingId++`) per `TouchDown` keeps libinput
  from misinterpreting two touches as one.
- **Tracking ids never reused.** Per the M2 step 2 decision, this
  is what prevents libinput dedup. Counter starts at 1000 to make
  test-vs-prod ids visually distinct in logs.
- **Hover is captured but not yet sent on the wire.** The
  `OnHoverListener` is wired so the events reach `TouchHandler`,
  but the current schema has no `pen_hover` message — we just see
  the events for debugging. Adding hover support means a small
  schema bump (new message type, host side asserts BTN_TOOL_PEN
  without BTN_TOUCH); deferred to M3 step 4 or M4.
- **No foreground service yet.** When the user switches apps the
  WS dies and reconnects when they come back. Acceptable for M3
  exit; lands properly when polishing in M4.

## What's left for M3

Step 4 — latency measurement, with the exit criterion of <30 ms
round-trip wired. We have all the plumbing; we just haven't
instrumented timestamps end-to-end yet.

## Open issues / follow-ups (not blocking M3)

- **Hover events.** `OnHoverListener` is wired but unused. Adding
  `pen_hover` to the schema lets a tablet-aware app see proximity
  before contact (matters for some Krita brush previews). Schema
  bump + host-side handler.
- **Eraser tool.** Android distinguishes `TOOL_TYPE_ERASER` from
  `TOOL_TYPE_STYLUS`; we currently route both into `pen_*`
  messages with the same tool. Adding a `tool` field on `pen_down`
  (`"pen"` / `"eraser"`) lets the host emit
  `BTN_TOOL_RUBBER` instead of `BTN_TOOL_PEN` — Krita/Inkscape
  treat that as the eraser end of a Wacom-class pen.
- **Coordinate transform when display geometry differs.** If the
  host's virtual display isn't 2960×1848 (e.g. a 4K monitor mirrored
  at 3840×2160), our 1:1 pass-through scales wrong. Fix: capture
  the surface size from `surfaceChanged()` and rescale coords by
  surface-size / virtual-display-size ratio.
- **Foreground service** for resilience — see plan §10.
- **USB-C wired transport.** The `adb reverse` tunnel currently
  requires the USB-A side; USB-C↔USB-C role swap on the laptop is
  blocked by ASUS firmware (documented in M3-VIDEO-DECODE.md).
  Investigation continues separately; for daily use the USB-A
  cable works.

## System under test

Same as M3 step 2:
- Host: kernel 6.19.6, KWin 6.6.4 (Plasma 6.6 Wayland)
- Tablet: Galaxy Tab S11 Ultra (Android 14), S-Pen Creator Edition
- Transport: `adb reverse tcp:7878 tcp:7878` over USB-C-to-USB-A
