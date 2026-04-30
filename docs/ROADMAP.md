# MoonBeam Roadmap

Milestones are ordered for maximum learning per unit of effort: each one answers an empirical question that gates the next.

## M0 — Does vkms + KWin respect a 120Hz custom mode? ✅

The single most important unknown. If this fails, the whole architecture pivots to a custom kernel module like EVDI.

**Tasks:**
- Load vkms with writeback enabled
- Probe whether the connector appears in `kscreen-doctor -o`
- Add a custom 2960×1848@120Hz mode and try to set it
- Measure with a frame-timestamping tool whether KWin actually commits at 120Hz

**Exit criteria:** binary yes/no on whether vkms is a viable foundation on KDE Wayland 6.6.

**Result:** PASS. Documented in [`M0-RESULTS.md`](M0-RESULTS.md). KWin commits at 120Hz on a vkms virtual output; the entire architecture rests on vanilla in-tree vkms, no kernel patching needed.

## M1 — End-to-end video, no input ✅

Capture vkms framebuffer, encode via NVENC, ship over TCP, decode and display in a minimal client.

**Steps shipped:**
- Step 1 — DRM writeback exploration ([`M1-WRITEBACK-PROBE.md`](M1-WRITEBACK-PROBE.md))
- Step 2 — xdg-desktop-portal screencast frames ([`M1-PORTAL-PROBE.md`](M1-PORTAL-PROBE.md))
- Step 3 — NVENC h264 encode of portal frames ([`M1-ENCODE-PROBE.md`](M1-ENCODE-PROBE.md))
- Step 4 — WebSocket transport + browser WebCodecs decode ([`M1-TRANSPORT-PROBE.md`](M1-TRANSPORT-PROBE.md))

**Exit criteria:** an Android app shows what's drawn on the virtual display at native resolution and ≥60fps.

**Result:** met for the browser case. Browser WebCodecs decodes ~60fps cleanly on Annex-B-over-WS. The wire format is locked: `[type:u8][flags:u8][payload]`. Native Android client is deferred to M3 per the [app plan](MOONBEAM-APP-PLAN.md) §14 — the browser is a debugging tool, not the product.

## M2 — Touch + pen passthrough ✅

Add `uinput` device, wire WebSocket control channel, send touch events from a synthetic source, verify they hit the right tablet/touchscreen subsystem in the kernel.

**Steps shipped:**
- Step 1 — uinput pen device, S-Pen capability set, `ID_INPUT_TABLET=1` ([`M2-UINPUT-PEN-PROBE.md`](M2-UINPUT-PEN-PROBE.md))
- Step 2 — uinput multitouch device, 10-slot MT-B, `ID_INPUT_TOUCHSCREEN=1` ([`M2-UINPUT-TOUCH-PROBE.md`](M2-UINPUT-TOUCH-PROBE.md))
- Step 3 — WebSocket input return, JSON-in-binary-frame, end-to-end verified ([`M2-INPUT-WS-PROBE.md`](M2-INPUT-WS-PROBE.md))
- Step 4 — single multiplexed WS for both directions ([`M2-MUX-PROBE.md`](M2-MUX-PROBE.md))

**Original exit criteria:** drawing in Krita on the tablet draws into the host's Krita window with pen pressure.

**Result:** structural + protocol exit met (a paired pen+touch uinput device pair, JSON wire format locked, end-to-end JSON→WS→uinput verified with synthetic events). The Krita visual confirmation is deferred to M3 — without an Android client, the synthetic pen stroke would drag the cursor across the active desktop, which we explicitly chose to avoid. Krita test runs the first time real Android events arrive.

## M3 — USB-C wired transport + minimum Android client

`adb reverse` tunnel, both channels over USB. The Android app first appears here as the smallest viable client that opens the WS, decodes via `MediaCodec`, and forwards touch/pen events back. Latency is the headline metric, but secondary to the binary "does anything work end-to-end with a real tablet" question.

**Tasks (planned):**
- Step 1 — Android project skeleton (Kotlin, single Activity, fullscreen surface)
- Step 2 — `adb reverse tcp:7878 tcp:7878` connection, MediaCodec H.264 decode of WS video frames
- Step 3 — Android touch + S-Pen `MotionEvent` → JSON over WS, mapped to host coordinate space
- Step 4 — measure round-trip latency (capture → encode → wire → decode → present, plus touch → wire → uinput → compositor)

**Exit criteria:** round-trip touch latency under 30ms wired; pressure-sensitive drawing in host-side Krita using the real S-Pen.

## M4 — Tuning

Encoder presets, frame pacing, EDID fine-tuning. Try to actually hit 120fps end-to-end. Android-app polish (pairing UX, floating widget, audio, per-host saved modes, S-Pen feature integration) lives across this milestone, checked against [`MOONBEAM-APP-PLAN.md`](MOONBEAM-APP-PLAN.md).

**Exit criteria:** sustained 120fps on visually-static content; ≥90fps under load. Drawing latency feels indistinguishable from a wired Wacom Cintiq.

## M5+ (future)

- Mutter / wlroots compositor support
- iPad client
- Audio (planned in app plan §8 — `type=0x02` on the same WS, Opus 48kHz stereo, host PipeWire null-sink)
- Multi-client (multiple tablets to one host)
- Cloud relay / hole punching
