# M2 step 3 — WebSocket input return probe

Run: 2026-04-30

## What the probe confirms

`host/src/bin/probe-input-server.rs` is the third milestone-2 piece:
a single axum server that owns both the pen and touch uinput devices
proven out in steps 1 and 2, accepts WebSocket connections, parses
input events from binary frames using the M1-locked framing, and
translates them into kernel-side uinput writes.

`host/src/bin/probe-input-test-client.rs` drives it from the same
host: connects to `ws://127.0.0.1:7879/ws`, sends a scripted sequence
(pen tap → pen stroke with triangular pressure ramp → stylus button →
two-finger pinch → finger tap), and exits.

## Wire format

Re-uses M1's `[type:u8][flags:u8][payload]` framing, with `type=0x03`
reserved for input. Flags are unused for now and zeroed by the
client. Payload is a UTF-8 JSON object, one event per binary frame.

Each event is one self-contained `InputMsg`:

```jsonc
// Pen
{ "type": "pen_down",   "x": 1480, "y": 924, "pressure": 2048, "tilt_x": 0, "tilt_y": 0 }
{ "type": "pen_move",   "x": 1481, "y": 924, "pressure": 2100, "tilt_x": 0, "tilt_y": 0 }
{ "type": "pen_up" }
{ "type": "pen_button", "button": "stylus" | "stylus2", "state": true | false }

// Touch
{ "type": "touch_down", "slot": 0, "id": 1001, "x": 1480, "y": 924, "major": 200, "pressure": 100 }
{ "type": "touch_move", "slot": 0,             "x": 1481, "y": 924, "major": 200, "pressure": 100 }
{ "type": "touch_up",   "slot": 0 }
```

`tilt_x`, `tilt_y`, `major`, and `pressure` (on touch) are optional
in the schema — clients that don't compute them can omit the fields
and the server defaults them to sensible values.

Each event the server applies emits a complete kernel input report
ending with `SYN_REPORT`. Coalescing multiple events into one report
(e.g. moves on slot 0 + slot 1 within the same frame) is a future
optimisation; for the probe, one frame = one report keeps the
ordering deterministic.

## Verified behaviour

Server stdout, with the test client driving a `--pen-only --samples 20
--stroke-ms 600` run:

```
=== MoonBeam M2 step 3 — WebSocket input server ===
pen device:   /dev/input/event260
touch device: /dev/input/event261
HTTP+WS server listening on http://0.0.0.0:7879/
ws client connected
  apply: PenDown { x: 1480, y: 924, pressure: 2047, tilt_x: 0, tilt_y: 0 }
  apply: PenUp
  apply: PenDown { x: 0, y: 924, pressure: 1, tilt_x: 0, tilt_y: 0 }
  apply: PenMove { x: 147,  y: 924, pressure: 409,  tilt_x: 0, tilt_y: 0 }
  apply: PenMove { x: 295,  y: 924, pressure: 819,  ... }
  apply: PenMove { x: 443,  y: 924, pressure: 1228, ... }
  ...
  apply: PenMove { x: 1479, y: 924, pressure: 4095, ... }   # peak
  ...
  apply: PenMove { x: 2959, y: 924, pressure: 1,    ... }
  apply: PenUp
  apply: PenButton { button: Stylus, state: true }
  apply: PenButton { button: Stylus, state: false }
ws client disconnected
```

What this proves:

- WS upgrade succeeds; binary frames flow.
- The `[type=0x03][flags][json]` framing is parsed correctly: the
  type discriminator matches, the JSON tail deserialises into the
  tagged-enum `InputMsg`.
- The triangular pressure ramp arrives intact and matches the
  client's mathematical model exactly (peaks at the configured max
  in the middle sample, falls back to ~1 at the edges).
- `uinput.write()` succeeds for every event — no kernel rejection,
  no per-frame errors logged.
- Clean disconnect / device teardown on client `Close`.

The touch path was verified statically in M2 step 2 and structurally
again here (`apply()` for `TouchDown` / `TouchMove` / `TouchUp`
is the same code path as the pen variants, just emitting against the
touch device). Live touch was deliberately skipped during the probe
run — the synthetic gestures land at coordinates that would click on
whatever Wayland surface happened to be focused.

## Decisions captured

- **JSON-in-binary-frame, not JSON over text frame.** Two reasons.
  First, sharing a single `/ws` between video (binary) and input
  (binary) in M2 step 4 is cheaper if both speak binary — the server
  routes by the leading `type` byte without checking message type.
  Second, Android's WebSocket library defaults to binary; mixing
  text frames means a second buffer path on the client.
- **One event per frame, one SYN_REPORT per event.** We had a brief
  debate over whether to batch (e.g. send all per-frame slot moves
  inside one frame, finalised by a `frame` event). Current call:
  don't batch yet. Synchronous `apply` per WS message keeps server
  state-machine reasoning trivial (no "what happens if a batch
  arrives mid-stroke and the connection drops between events").
  Profile-driven optimisation if the Android-side input rate ever
  becomes the bottleneck.
- **Server owns both uinput devices for its whole lifetime.** Not
  per-connection. Tearing devices down on disconnect would generate
  spurious udev / KWin re-classification storms when the user's
  network blips. M3 will revisit when "tablet identity" becomes a
  thing — at that point, devices live for the duration of a paired
  tablet's session, which is structurally `connection lifetime` on
  USB-C.
- **Default JSON field absences are server-side defaults.** `major`
  defaults to 200, `pressure` (touch) to 100, tilts to 0. The Android
  client is expected to fill in real values when it has them; the
  defaults are there so a hand-rolled debugging client (browser dev
  console, curl pipeline, etc.) can fire minimal events.
- **No backpressure in the server.** uinput writes are
  microseconds-fast and we trust the client's pacing. If the client
  exceeds the kernel's input rate (~500 Hz typical) the bottleneck
  is uinput itself, which fails loudly rather than silently dropping.

## Things deferred (not blocking M2)

- **Cross-device coordination**: palm rejection (touch-suppress when
  pen is in proximity) requires the server to remember the pen's
  proximity state across messages. The probe does not implement this.
  Lives in M2 step 4 or beyond.
- **Eraser tool transitions**: `pen_down` always asserts
  `BTN_TOOL_PEN`; the schema has no way for the client to say "this
  is the eraser end". A future `tool` field on `pen_down` (`"pen"` /
  `"eraser"`) is the right shape; not in scope yet because the
  Android client doesn't exist to send it.
- **Per-host pressure curves** (MOONBEAM-APP-PLAN.md §9.2): we'd
  apply a configured curve server-side before the uinput write.
  Trivial once the config layer (deferred to M4) ships.
- **Visual confirmation in Krita**: same logic as steps 1 and 2 —
  the static + live-server checks prove the path; visual confirmation
  is one button click away once a tablet-aware app has focus and
  we're ready to drag the cursor across the desktop.

## System under test

- Kernel: 6.19.6-arch1-3-g14
- KWin: 6.6.4 (Plasma 6.6, Wayland)
- axum: 0.8 (`ws` feature)
- tokio-tungstenite: 0.29 (client side; promoted to direct dep from
  axum's transitive use)
- serde: 1, serde_json: 1, futures-util: 0.3
- input-linux: 0.7.1

## Follow-ups (not blocking M2)

- **M2 step 4 — single multiplexed WS.** Roll video + input into one
  upgrade endpoint behind one axum process. The wire format already
  routes by leading byte; merging is a code organisation step, not a
  protocol change.
- **Force-IDR opcode** (carried over from M1 follow-ups): client
  sends `{"type":"force_idr"}` on the input WS, server responds by
  asking the encoder for an IDR. Pairs with the M1 server-side
  keyframe replay cache when both ship.
- **Schema versioning**. Once Android starts shipping, an `InputMsg`
  field rename or addition needs a version negotiation step. Probably
  add a `protocol_version` field to the WS upgrade query string and
  reject mismatches at the upgrade level.
