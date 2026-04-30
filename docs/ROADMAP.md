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

## M3 — USB-C wired transport + minimum Android client ✅

`adb reverse` tunnel, both channels over USB. The Android app first appears here as the smallest viable client that opens the WS, decodes via `MediaCodec`, and forwards touch/pen events back.

**Steps shipped:**
- Step 1 — Android project skeleton, two modules (`:app` + `:protocol`) ([`M3-ANDROID-SCAFFOLD.md`](M3-ANDROID-SCAFFOLD.md))
- Step 2 — MediaCodec H.264 decode of WS video frames into a SurfaceView ([`M3-VIDEO-DECODE.md`](M3-VIDEO-DECODE.md))
- Step 3 — pen + touch `MotionEvent` → uinput, verified live in Inkscape with pressure ([`M3-INPUT-FORWARDING.md`](M3-INPUT-FORWARDING.md))
- Step 4 — latency instrumentation, fps/decode/input/RTT overlay ([`M3-LATENCY.md`](M3-LATENCY.md))

**Original exit criteria:** round-trip touch latency under 30ms wired; pressure-sensitive drawing in host-side Krita using the real S-Pen.

**Result:** **met.** Live numbers (Tab S11 Ultra, S-Pen Creator Edition, USB-C-to-A + adb reverse): fps 80–90, decode 1–2 ms, input 0–3 ms, ws RTT 11 ms (median). End-to-end round-trip estimate: ~22–42 ms typical, well within target. Inkscape draws pressure-modulated strokes from the S-Pen.

**Known caveats:**
- USB-C-to-USB-C role swap fails on the ROG SCAR 16 (Meteor Lake / TB5 silicon, ASUS firmware controls role decisions out-of-band). Daily use is via USB-C-to-USB-A cable; investigation continues separately.
- No foreground service yet — the WS dies on app-switch.

## M4 — Tuning

Encoder presets, frame pacing, EDID fine-tuning. Try to actually hit 120fps end-to-end. Android-app polish (pairing UX, floating widget, audio, per-host saved modes, S-Pen feature integration) lives across this milestone, checked against [`MOONBEAM-APP-PLAN.md`](MOONBEAM-APP-PLAN.md).

**Latency optimizations (all non-architectural, drop-in swaps):**
- **Direct DRM writeback capture** instead of xdg-desktop-portal screencast. Saves the ~16 ms portal frame buffer. Slots into the `Capture` trait placeholder. Loses portal consent dialog (security tradeoff).
- **Slice-based NVENC encoding**. Output one horizontal band of each frame as soon as encoded; tablet starts decoding before frame is complete. Saves 5–10 ms. Wire-format-additive (new flag bit, forward-compatible).
- **MediaCodec async mode** (`setCallback` instead of polling) on the tablet. 1–3 ms.
- **Drop OkHttp auto-ping** (we have our own); reduces RTT tail spikes.
- **Force-IDR-on-input**: when input arrives, host requests fresh keyframe so first response frame is fast. Pairs with the `force_idr` opcode follow-up from M1.

**Riskier optimizations to consider only if needed:**
- **Custom USB transport** (skip `adb reverse`). New transport module alongside WS. Wire format unchanged. ~3–5 ms saved.
- **Replace WS with raw TCP**. Loses browser debug client compat; saves ~1–2 ms.

**Module extraction (housekeeping, not optimization):** lift probe-mux internals into the existing `host/src/{capture,encode,transport,input,proto}/` placeholder modules. Probes become thin orchestration shells. Required before declaring `moonbeamd` itself shippable.

**Exit criteria:** sustained 120fps on visually-static content; ≥90fps under load. Drawing latency feels indistinguishable from a wired Wacom Cintiq.

## M5+ (future)

- Mutter / wlroots compositor support
- iPad client
- Audio (planned in app plan §8 — `type=0x02` on the same WS, Opus 48kHz stereo, host PipeWire null-sink)
- Multi-client (multiple tablets to one host)
- Cloud relay / hole punching
