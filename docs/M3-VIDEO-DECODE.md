# M3 step 2 — Android video decode

Run: 2026-04-30 (compile only; live test pending — see "Open issues" below)

## What this step is

The first step where the Android client actually does something. The
:app module gains:

- `WsClient` — OkHttp-backed WebSocket talking the locked
  `[type:u8][flags:u8][payload]` framing. Inbound video frames are
  emitted as a Flow; the input-side `send(InputMsg)` is wired but not
  exercised yet (M3 step 3 lands there).
- `VideoDecoder` — `MediaCodec` H.264 hardware decoder configured for
  low-latency mode (`KEY_LOW_LATENCY=1`), in-stream SPS/PPS discovery
  (NVENC's `repeat_pps` puts them on every IDR), and direct-to-Surface
  output.
- `MainActivity` + `MoonBeamRoot` — fullscreen black surface with a
  `SurfaceView` (wrapped in `AndroidView`). When the surface is
  created, a fresh `VideoDecoder` is started against it; when the
  surface goes away, the decoder is torn down.
- `MoonBeamViewModel` — owns the WS connection, drives feed/drain
  against whichever decoder the Activity has currently attached, and
  exposes a `StateFlow<Status>` for the status overlay (visible only
  until the first frame decodes).

## Wire format reuse

Same `[0x01][flags][annex_b]` framing the M1 browser probe consumed.
On the Android side this maps to:

```kotlin
Wire.decodeInbound(frame) -> Wire.Inbound.Video(annexB, isKeyframe)
decoder.feed(annexB, presentationTimeUs = monotonic, isKeyframe)
decoder.drain()  // releases buffers to the SurfaceView's surface
```

`presentationTimeUs` is the local monotonic clock at receive time
(System.nanoTime/1000), not the host's encode time. M3 step 4 may
revise this once we measure end-to-end latency and decide where to
inject capture timestamps for accuracy.

## Decisions captured

- **No pre-configured SPS/PPS.** `MediaCodec.configure()` is called
  with placeholder dimensions (1920×1080); the real geometry is
  extracted from the in-stream SPS that NVENC's `repeat_pps`
  prepends to every IDR. Mirrors what the browser-side WebCodecs
  decoder does — and avoids a bespoke "wait for first IDR, parse
  SPS, then configure" code path on the Android side.
- **Single coroutine for feed + drain.** `viewModelScope.launch {
  ws.connect().collect { ... } }` runs feed → drain inline per
  message. Simpler than two coroutines for an M3 minimum; if buffer
  pressure shows up under load, switch to `setCallback()` async mode
  (added in API 21, well-supported on the Tab S11 Ultra).
- **`KEY_LOW_LATENCY=1` on the format.** On Samsung and Pixel
  devices this maps to a fast-path that disables reorder buffers.
  Costs us frame B-pictures, which we don't have anyway because
  NVENC is configured `tune=ull, preset=p1, GOP=30, no B-frames`.
- **Decoder owned by Activity, WS owned by ViewModel.** The
  `VideoDecoder` is short-lived (tied to surface lifetime); the
  `WsClient` survives configuration changes. ViewModel "attaches"
  the decoder when surface is ready, "detaches" when it goes away.
  The connect-once behaviour means surface recreation (e.g. after
  background → foreground) doesn't drop the WS.
- **Fullscreen landscape, host-driven orientation.** Activity is
  locked to landscape and `configChanges` lists every dimension we
  could plausibly receive — so when KWin re-declares the virtual
  output's transform (per MOONBEAM-APP-PLAN.md §4), the Activity
  doesn't restart.
- **No foreground service yet.** A standalone Activity-bound flow is
  enough for "does video appear?" verification. The foreground
  service that keeps the connection alive across app-switch lands in
  M3 step 3 (alongside touch capture) or M3 step 4.

## What's verified

- Compile: `./gradlew :app:assembleDebug` succeeds.
- Tests: `./gradlew :protocol:test` still 4/4 green.
- Static review of the decoder + WS wiring: matches the host-side
  protocol (verified end-to-end during M2 step 3).

## Open issues — live test blocked on USB-C role negotiation

Live testing requires `adb` to see the tablet. Hit a wall during
this session:

- The Tab S11 Ultra is plugged into the laptop via USB-C; the laptop
  charges the tablet (correct power direction), but `adb devices`
  is empty and `lsusb` shows no Samsung device on the bus.
- The tablet's "USB controlled by" toggle (under the
  "Charging connected device via USB" notification) offers
  "This Device" / "Connected Device". Tapping "Connected Device"
  fails with "Couldn't switch" after a few seconds — the USB-C
  Dual Role Port (DRP) negotiation isn't completing.
- USB-C ↔ USB-C DRP swap requires CC pin support on both ends and
  the cable. The user already swapped two high-end cables and both
  laptop ports with no change, suggesting the bottleneck is in the
  laptop's USB-C controller's role-swap support (or the firmware
  thereof).

Two unblock paths, in order of preference for first verification:

1. **USB-C-to-USB-A cable into a USB-A port** on the laptop. With
   USB-A on the host side, there's no role to negotiate; the tablet
   sees a Type-A host and acts as device.
2. **Wireless ADB** (Developer options → Wireless debugging →
   pair with code). `adb reverse tcp:7878 tcp:7878` works the same
   over Wi-Fi as USB. Bandwidth is fine for 30 Mbps video on 5GHz.

The product target is wired USB-C (per the app plan), but for
development the wireless path is enough to validate the pipeline
end-to-end. Wired USB-C investigation continues separately.

## Live-test workflow (once adb sees the tablet)

```sh
# Laptop:
cd /home/mateim/DevApps/MoonBeam
cargo run --manifest-path host/Cargo.toml --bin probe-mux
# Wait for KWin's screencast portal dialog and pick a monitor.

# Other terminal, on laptop:
adb reverse tcp:7878 tcp:7878
adb install -r android/app/build/outputs/apk/debug/app-debug.apk
adb shell am start -n com.m151.moonbeam/.MainActivity

# Tablet should show:
#   1. "MoonBeam — connecting" overlay (~200 ms)
#   2. "MoonBeam — connected, waiting for first keyframe" (until next IDR, ≤500 ms with GOP=30 at 60fps)
#   3. The selected monitor's contents, fullscreen.
```

## System under test (build host)

Same as M3 step 1: kernel 6.19.6, JDK 17, Gradle 8.11.1, AGP 8.7.3,
Android SDK 34, target Tab S11 Ultra (Android 14, API 34).

## Follow-ups

- M3 step 3: capture Android touch + S-Pen `MotionEvent`s, encode
  via `Wire.encodeInput`, send via the existing `WsClient.send`.
  Foreground service to keep the connection alive across app-switch.
- M3 step 4: end-to-end latency measurement (capture → encode → wire
  → decode → present, plus the input direction). Hit ≤30 ms wired.
- USB-C role-swap diagnosis: laptop USB-C controller advertise
  capability, cable CC-line continuity, alt-mode negotiation. May
  end up filing this as a hardware-side note rather than a fix —
  wireless ADB is sufficient for development.
