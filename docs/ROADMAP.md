# MoonBeam Roadmap

Milestones are ordered for maximum learning per unit of effort: each one answers an empirical question that gates the next.

## M0 — Does vkms + KWin respect a 120Hz custom mode?

The single most important unknown. If this fails, the whole architecture pivots to a custom kernel module like EVDI.

**Tasks:**
- Load vkms with writeback enabled
- Probe whether the connector appears in `kscreen-doctor -o`
- Add a custom 2960×1848@120Hz mode and try to set it
- Measure with a frame-timestamping tool whether KWin actually commits at 120Hz

**Exit criteria:** binary yes/no on whether vkms is a viable foundation on KDE Wayland 6.6.

## M1 — End-to-end video, no input

Capture vkms framebuffer, encode via NVENC, ship over TCP, decode and display in a minimal Android client (or repurpose Moonlight).

**Exit criteria:** an Android app shows what's drawn on the virtual display at native resolution and ≥60fps.

## M2 — Touch + pen passthrough

Add `uinput` device, wire WebSocket control channel, send touch events from tablet, verify they hit the right virtual display in KDE.

**Exit criteria:** drawing in Krita on the tablet draws into the host's Krita window with pen pressure.

## M3 — USB-C wired transport

`adb reverse` tunnel, both channels over USB. Measure latency vs Wi-Fi.

**Exit criteria:** round-trip touch latency under 30ms wired.

## M4 — Tuning

Encoder presets, frame pacing, EDID fine-tuning. Try to actually hit 120fps end-to-end.

**Exit criteria:** sustained 120fps on visually-static content; ≥90fps under load.

## M5+ (future)

- Mutter / wlroots compositor support
- iPad client
- Audio
- Multi-client
