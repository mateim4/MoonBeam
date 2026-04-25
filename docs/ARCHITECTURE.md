# MoonBeam Architecture

## Goals

1. **High refresh rate**: drive a virtual display at the client's native refresh (target 120Hz on Tab S11 Ultra).
2. **Native resolution**: match the client's panel pixel-for-pixel (target 2960×1848).
3. **Low input latency**: touch and pen events from the client appear at the host within ~16ms (one 60Hz frame), ideally <10ms.
4. **Pen pressure + tilt**: full S-Pen-equivalent input fidelity.
5. **Wired-first**: USB-C via `adb reverse` is the primary transport. Wi-Fi is a fallback.
6. **Compositor-agnostic where possible**: prototype targets KDE Plasma Wayland, but the architecture should not preclude Mutter / wlroots support.

## Non-goals (for v0)

- Multi-client (one tablet at a time)
- Audio routing
- HDR
- DRM-protected content streaming
- Arbitrary OS clients (Android-first; iOS later)

## Component breakdown

### 1. Virtual display (`scripts/`, `host/display/`)

**Mechanism:** [vkms](https://docs.kernel.org/gpu/vkms.html) (Virtual KMS), in mainline Linux. Loaded with parameters describing the desired output:

```
modprobe vkms enable_writeback=1
```

The connector's mode is set via `kscreen-doctor` once the compositor picks it up, with a custom mode declaring the desired refresh:

```
kscreen-doctor output.Virtual-1.addCustomMode.2960.1848.120000.full
kscreen-doctor output.Virtual-1.mode.<id> output.Virtual-1.position.<x>,0
```

**Open question:** whether KWin will respect a 120Hz mode on a vkms output, or quietly clamp to 60Hz like it does with `krfb-virtualmonitor`'s wlr-virtual-output. To be tested empirically as milestone M0.

**Fallback if M0 fails:** patch vkms to expose a writeback connector with a forced refresh-rate timing in EDID; or write a small kernel module on top of vkms that exposes a configurable EDID. EVDI is the existing precedent.

### 2. Capture (`host/capture/`)

Two paths:

- **Preferred:** DRM writeback connector (`drm/writeback.c`). Zero-copy on supported drivers, lowest latency. Captures the rendered framebuffer of the virtual output before scanout.
- **Fallback:** PipeWire screencast via `xdg-desktop-portal-kde`. Higher latency, requires user to grant the share dialog once per session, but works without root and on every modern Wayland compositor.

### 3. Encode (`host/encode/`)

`ffmpeg` libav, NVENC backend (`h264_nvenc` or `hevc_nvenc`). Tuning targets:

- `preset=p1` (lowest latency)
- `tune=ull` (ultra-low-latency)
- `zerolatency=1`
- `rc=cbr` with bitrate sized to USB 3.x bandwidth (~50–100 Mbit/s plenty for visually-lossless at 120fps)

VAAPI fallback for non-NVIDIA hosts (Intel/AMD).

### 4. Transport (`host/transport/`, `android/transport/`)

Two-channel design over a single TCP-or-Unix-socket multiplex:

- **Video channel:** raw H.264/HEVC NALUs framed by length prefix. No RTP overhead in v0.
- **Control channel:** length-prefixed JSON for input events, mode changes, heartbeat.

Wired transport: `adb reverse tcp:<port> tcp:<port>` so the tablet client connects to `localhost:<port>` which forwards to the host. Wi-Fi transport: same protocol, just direct TCP.

### 5. Input return (`host/input/`)

`uinput` (`/dev/uinput`) virtual devices:
- One absolute-position pen device with pressure (BTN_TOOL_PEN, BTN_TOUCH, ABS_PRESSURE, ABS_TILT_X/Y)
- One multitouch device (ABS_MT_SLOT, ABS_MT_TRACKING_ID, ABS_MT_POSITION_X/Y, ABS_MT_PRESSURE)

Coordinates from the client are scaled to the vkms output geometry. Weylus' implementation is a well-tested reference.

### 6. Android client (`android/`)

- `MediaCodec` + `Surface` for low-latency H.264/HEVC decode → SurfaceView.
- `View.onTouchEvent` / `MotionEvent` for touch and S-Pen capture, including pressure (`getPressure()`) and tilt (`getAxisValue(AXIS_TILT)`).
- WebSocket or raw TCP for transport — match host side.

## Roadmap milestones

See [ROADMAP.md](ROADMAP.md).
