# MoonBeam

A Linux-native equivalent of [Spacedesk](https://www.spacedesk.net/) — turn an Android tablet into a high-refresh-rate, touch-and-pen-capable extended display for a Linux host, over USB-C or Wi‑Fi.

## Status

**Alpha / proof-of-concept.** Targeting first working prototype on:
- **Host:** Arch Linux + KDE Plasma 6.6 Wayland, NVIDIA RTX 5090 (NVENC)
- **Client:** Samsung Galaxy Tab S11 Ultra (2960×1848 @ 120Hz, S-Pen)
- **Transport:** USB-C via `adb reverse` tunnel; Wi‑Fi as fallback

## Why

Linux already has streaming pieces (Sunshine, Moonlight) and input-passthrough pieces (Weylus). Nothing combines them with a real software-defined virtual display the way Windows IDDs allow Spacedesk to. The result: Linux users either get high-refresh streaming *without* touch/pen passthrough, or touch/pen passthrough capped at 60Hz on a virtual output. MoonBeam closes that gap.

## Architecture

```
┌──────────────────────── Linux host ─────────────────────────┐
│                                                              │
│  ┌──────────┐     ┌──────────────┐     ┌─────────────────┐  │
│  │   vkms   │────▶│ DRM writeback│────▶│ NVENC encoder   │──┼──▶ video stream
│  │ (kernel) │     │   capture    │     │  (h264/hevc/av1)│  │       (USB / WiFi)
│  └──────────┘     └──────────────┘     └─────────────────┘  │
│       ▲                                                      │
│       │ EDID declares 2960×1848@120Hz                       │
│       │                                                      │
│  ┌──────────┐                          ┌─────────────────┐  │
│  │  uinput  │◀─────────────────────────│ input listener  │◀─┼──── touch/pen events
│  │ (kernel) │   absolute touch + pen   │   (websocket)   │  │       (USB / WiFi)
│  └──────────┘   pressure + tilt        └─────────────────┘  │
│                                                              │
└──────────────────────────────────────────────────────────────┘
                              │ ▲
                              ▼ │
┌─────────────────── Android tablet client ───────────────────┐
│  Low-latency H.264 decoder + touch/pen capture + websocket  │
└──────────────────────────────────────────────────────────────┘
```

The novel piece is the integration: **a vkms-backed virtual display** that the compositor sees as a real 120Hz monitor, plus **touch/pen passthrough** that Sunshine/Moonlight don't provide.

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for component-level design.

## Components

- `host/` — userspace daemon: vkms orchestration, capture, encode, transport, input return
- `android/` — Android client: video decode, render, input capture, transport
- `scripts/` — vkms loader, EDID generator, helper utilities
- `docs/` — design docs and protocol specs

## Building / running

Not yet runnable — see [docs/ROADMAP.md](docs/ROADMAP.md) for milestone tracking.

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).
