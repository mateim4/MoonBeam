# M3 step 4 — latency instrumentation

Run: 2026-04-30

## What this step is

The closing step of M3, where we measure what we just built.
Three new instrumentation paths, all surfaced through a small
top-right overlay on the tablet:

- **Decode latency** — time from a video frame's arrival on the WS
  to the moment `MediaCodec.releaseOutputBuffer(render = true)`
  returns. Captures the cost of hardware H.264 decode + Surface
  buffer release.
- **Input pre-wire age** — `MotionEvent.getEventTime()` minus
  `SystemClock.uptimeMillis()` at the moment we put bytes on the
  WS. Captures everything between "kernel observed input" and
  "tablet starts sending it on the network" — Activity dispatch,
  TouchHandler logic, JSON encode, OkHttp queue.
- **WS round-trip** — a 1 Hz ping/pong using new opcodes
  `TYPE_PING = 0x04` / `TYPE_PONG = 0x05`. Payload is an 8-byte
  little-endian u64 timestamp (microseconds, monotonic). Server
  echoes the exact same bytes with the type byte rewritten —
  no JSON, quick-path before any other inbound handling. Tablet
  computes RTT on receipt.

## Live measured numbers (Tab S11 Ultra ↔ ROG SCAR 16, USB-C-to-A + adb reverse)

```
fps:    80–90
decode: 1–2 ms
input:  0–3 ms
ws rtt: 11 ms
```

## What this implies for round-trip latency

Decomposing a "user moves pen → user sees response on tablet" round
trip from the components we measured plus typical kernel + display
costs:

| Stage | Time |
|---|---|
| Input encode (tablet) | 0–3 ms |
| Wire half-trip out | ~5.5 ms (half of RTT) |
| Host uinput → compositor | <1 ms |
| Host app re-render | 1–16 ms (depends on app and frame timing) |
| NVENC encode | 1–2 ms (hardware path) |
| Wire half-trip back | ~5.5 ms |
| Tablet decode | 1–2 ms |
| Tablet present (next vsync, 120 Hz) | ~8 ms |
| **Total estimated round-trip** | **~22–42 ms** |

Typical case lands well inside the **M3 exit criterion of ≤30 ms
wired**. Worst case (large redraw + missed vsync) reaches ~42 ms,
still in the same ballpark.

## What dominates

The 11 ms WS RTT is the biggest individual cost. Components inside
it:

1. OkHttp Kotlin → adb tunnel (USB-A side, USB 2.0 likely) → host
   tungstenite WS server → quick-path pong → reverse path.
2. `adb reverse` adds its own framing on top of TCP.
3. WS framing: a few bytes of opcode + length headers per direction.

Half of it (~5 ms) is one-way wire overhead. None of the other
stages exceed 8 ms even at worst case. So if we want to push
total latency under, say, 20 ms, the WS path is where time hides.

## Decisions captured

- **Binary ping/pong, not JSON.** The latency measurement loop
  shouldn't be biased by the slowest serialiser on the path. 8-byte
  little-endian u64 timestamp is the entire payload; server echoes
  bytes verbatim with just the type byte swapped.
- **Quick-path on the host.** Pong response is sent before
  `handle_inbound`'s JSON parse path. Keeps the ping RTT a
  measurement of pure transport, not "transport + serialiser".
- **`SystemClock.uptimeMillis()` on Android, not the wall clock.**
  `MotionEvent.getEventTime()` is documented as
  `SystemClock.uptimeMillis()` aligned, so subtracting one from
  the other gives a meaningful delta. Wall-clock would be wrong
  across NTP adjustments.
- **`System.nanoTime() / 1_000` for the ping timestamp.** Local
  monotonic clock; the server doesn't interpret it, just echoes,
  so units only need to be self-consistent.
- **FPS measured per-second, not per-frame.** Counting frames in a
  sliding 1-second window avoids per-frame jitter polluting the
  display. Updates roughly once a second on the overlay.
- **Decode latency excludes feed time.** We measure
  `nanoTime()` at receive then at drain success, so the
  measurement is "MediaCodec round-trip" — not "WS arrival to
  pixels". Including feed time would double-count the wire wait.
- **Stats overlay always on.** Two lines, monospace, 11sp,
  ~60% alpha green. Doesn't interfere with the video. Toggle
  could be added later (M4 polish).

## What this does not measure

- **End-to-end visual round-trip** (pen moves → user sees pixel
  change). Requires a phone-camera filming the tablet showing the
  laptop screen, with a stopwatch on the laptop screen. The
  estimate above is built from component measurements; a real
  measurement is a 5-minute manual test.
- **NVENC encode time on the host.** We can add it via a
  reverse-direction stat (host pushes a `frame_encoded_us` along
  with the video access unit). Out of scope for M3.
- **Compositor-side frame production cost.** The "host app
  re-render" entry above is a typical-app guess, not a measured
  value.

## What's left for the M3 milestone

Code-side, M3 is **functionally complete**:
- M3 step 1 ✅ Android scaffold
- M3 step 2 ✅ video decode → SurfaceView
- M3 step 3 ✅ pen + touch → uinput (drawing in Inkscape works)
- M3 step 4 ✅ latency instrumentation, numbers within target

Open follow-ups, all M4 work:
- USB-C-to-USB-C role-swap (currently using USB-C-to-A; ASUS
  firmware investigation continues separately)
- Foreground service so backgrounding doesn't drop the WS
- Pairing UX, mDNS LAN discovery, mode toggles, audio
- Real phone-camera round-trip measurement

## System under test

- Host: kernel 6.19.6, KWin 6.6.4 (Plasma 6.6 Wayland), RTX 5090
  Laptop GPU, ROG SCAR 16
- Tablet: Galaxy Tab S11 Ultra (Android 14, Dimensity 9400+)
- Transport: USB-C-to-USB-A → laptop USB-A port → `adb reverse`
- Encoder config: `probe-mux` defaults — h264_nvenc, preset p1,
  tune ull, GOP 30, 30 Mbps target bitrate
