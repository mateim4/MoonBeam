# MoonBeam Android App — v0 Plan

This document captures the product and UX decisions made before any
Android code is written. It is the source of truth for what the app
should do; implementation choices (Kotlin/Compose, MediaCodec, etc.)
are deferred until M3 begins.

Status: **planning complete, awaiting implementation milestone**.

## 1. Scope

The native Android app is the product. The browser client we will
build during M1 step 4 is a debugging tool only — it exists to verify
the host's WebSocket transport before we write any Android code.

Target device: Samsung Galaxy Tab S11 Ultra (2960×1848, 120Hz, S-Pen).
Other Android tablets are out of scope for v0.

## 2. Product principles

- **Extended screen is the primary use case.** Mirror is supported but
  secondary. The whole point of MoonBeam is "Linux Spacedesk-IDD",
  i.e. arbitrary virtual second monitor at native refresh.
- **Friction-free reconnection.** Once a tablet has been paired with a
  host, plugging the USB-C cable in should bring up the second screen
  with zero taps. Spacedesk's auto-activate-on-plug behavior is the
  model.
- **Native fullscreen, no browser.** The browser was rejected during
  planning because the image would not be fullscreen and would be
  awkward to manage.
- **Wired-first, LAN as fallback.** USB-C is the default path; LAN is
  available but explicit. Wi-Fi-Direct is out of scope for v0.

## 3. Modes

### 3.1 Connection mode (per host, persisted)

| Mode | Use |
|---|---|
| **Extended** (default) | Tablet acts as a second monitor to the host's existing display config. Primary use case. |
| **Mirror** | Tablet shows a copy of one of the host's existing outputs. |

The chosen mode is saved per paired host. Switching mode mid-session
updates the saved default for that host. Different hosts can have
different defaults — the same tablet might extend Laptop A and mirror
Laptop B, and both preferences persist across plug-cycles.

### 3.2 Quality/latency mode (toggleable mid-session)

| Mode | Encoder profile | Use |
|---|---|---|
| **Display** (default) | Higher bitrate, higher buffering, sharper image | Watching, reading, normal desktop work |
| **Drawing** | Lowest-latency NVENC profile, smaller GOP, willing to drop a bit of quality | Pen-on-screen work where lag is the felt problem |

Per-host saved default. Switchable from the floating widget without
disconnecting.

## 4. Tablet display behavior

- **Native resolution + refresh by default.** App requests
  2960×1848@120Hz on the host side as the virtual display geometry.
  The user can override per host if they want a lower resolution or
  refresh.
- **Orientation is host-driven, not tablet-driven.** This is a
  deliberate design choice: rotation is a Linux display-config
  concern (`kscreen-doctor` rotation, etc.), not the tablet's
  accelerometer. The Android app must lock its surface orientation
  and ignore device rotation; orientation changes happen because the
  host re-declared the virtual output's transform, not because the
  user turned the device.

  Implication: the floating widget should expose a "rotate" action
  that sends a control message to the host to flip its virtual-output
  transform. The host applies it; the tablet receives the new
  declared geometry on the next param-change.

## 5. Connectivity

### 5.1 USB-C (primary)

- First connection: tablet-led pairing flow. User installs the
  MoonBeam app on the tablet, plugs into the host, app guides them
  through Android permission grants and the host-side trust prompt.
- Subsequent connections: auto-trust on plug-in. The host stores the
  tablet's identity (serial-derived) and matches it against udev
  events; the tablet stores the host's identity and matches the
  USB-host vendor/product/serial. On a match, both sides silently
  bring up the saved connection mode. No taps.
- Transport: `adb reverse` tunnel for the WebSocket. Single WS
  multiplexes video / audio / input via a 1-byte type discriminator
  (0x01 video, 0x02 audio, 0x03 input).

### 5.2 LAN (secondary)

- Host advertises via mDNS (`_moonbeam._tcp` on port 7000 or similar).
- Tablet shows a manual pick-from-list of discovered hosts.
- No auto-extend on shared Wi-Fi: the user explicitly selects "connect
  to host X" each session. Rationale: being on the same network as a
  laptop does not mean the user wants their tablet to start
  extending into it.
- Same single-WS-with-discriminator framing as USB.

### 5.3 Out of scope for v0

- Wi-Fi Direct
- Bluetooth control fallback
- Cloud relay / hole punching

## 6. Pairing & device identity

- **Per-pair persisted state** (stored on host, indexed by tablet
  identity):
  - Tablet identity (USB serial-derived hash + LAN-side public key)
  - Last-used connection mode (extended | mirror)
  - Last-used quality mode (display | drawing)
  - Per-host pen pressure curve override (if set)
  - Per-host audio routing preference (on/off)
- **Per-pair persisted state** (stored on tablet, indexed by host
  identity):
  - Host identity (USB v/p/s + LAN host key)
  - Trusted: yes/no
- First-pair UX: tablet shows a 6-digit code; user types it on the
  host (or accepts a prompt). Both sides record the trust on success.

## 7. Floating widget UX

Two variants ship together. Both are implemented as a system
foreground-service-owned overlay; they are visible only inside the
MoonBeam app's surface (not over other apps — no
`SYSTEM_ALERT_WINDOW`).

### 7.1 Variant A — "MoonBeam puck" (default)

- 40dp circular widget at 40% opacity at rest, drifts to nearest edge
  after 3s of inactivity.
- Finger-tap → expands into an Air-Command-style radial menu:
  - **Extend / Mirror** (primary toggle, position varies by current
    mode)
  - **Drawing / Display** (quality toggle)
  - **Rotate** (sends rotate-host-output message)
  - **Audio on/off**
  - **Settings**
  - **Disconnect**
- Pen always passes through (pen events never trigger the widget) so
  drawing on top of the puck works.
- Edge-swipe stows the puck off-screen; a small tab on the screen
  edge un-stows it.

### 7.2 Variant B — S-Pen native (power-user shortcut)

- Pen-button hold + hover within ~15mm of the screen → radial menu
  appears at pen tip.
- Same menu items as Variant A.
- Released without selection → menu dismisses, no action.
- Optional: tied to S-Pen Air Actions if those are exposed by the
  Samsung SDK on the device.

Variant A is always available. Variant B activates automatically when
an S-Pen with a button is detected.

## 8. Audio

- **Direction:** host → tablet only. (Mic-from-tablet is out of scope
  for v0.)
- **Host side:** PipeWire null-sink named "MoonBeam Tablet"; user
  selects it as system output the same way they pick any other audio
  device.
- **Codec:** Opus, 48kHz, stereo, 96 kbps (CBR). Same WS as video,
  type discriminator 0x02.
- **Latency budget:** 40–80ms lipsync tolerance. On Drawing mode,
  audio packets are de-prioritized vs video to keep video latency
  flat.
- **Toggleable per host** from the floating widget.

## 9. Pen + touch input

### 9.1 Required (v0 ship-blocker)

- Multi-touch (10 contacts), absolute coordinates mapped to host
  virtual output.
- S-Pen pressure (full 4096-level range Samsung exposes).
- S-Pen tilt X/Y.
- S-Pen hover events (X/Y while pen is near but not on glass).
- S-Pen primary button (default mapping: right-click, configurable
  per host).
- S-Pen eraser detection (S-Pen reports tool=eraser when the back of
  the pen is used; this maps to `BTN_TOOL_RUBBER` on the host's
  uinput device, which Krita / GIMP / Inkscape all respect).
- Palm rejection: when pen is in proximity (hover detected), reject
  finger touches. Samsung's own algorithm if exposed via the SDK,
  otherwise implement on the app side before the events leave the
  tablet.

### 9.2 Configurable per host

- Pressure response curve (linear / soft / hard / custom).
- Pen-button mapping.
- Hover-while-disconnected does what (nothing / wakes screen).

### 9.3 Samsung-specific features to support if available

- Air Actions (gesture detection via the pen IMU).
- Pen-button single-click vs double-click vs hold differentiation.
- S-Pen settings sync (so the user's existing Samsung pen
  preferences carry over).

These are quality-of-life additions, not ship-blockers.

## 10. Resilience (Spacedesk-equivalent behavior)

| Event | App response |
|---|---|
| Cable unplugged | Graceful disconnect; second screen ends on host. On replug, auto-reconnect to the saved mode without user input. |
| App backgrounded | Foreground service keeps the WS alive and the surface receiving. The video surface pauses rendering when not visible but the connection survives. |
| Host laptop sleep | Connection drops cleanly. On host wake + replug (or LAN host coming back online), auto-reconnect. |
| Wi-Fi flake (1–5s) | Small jitter buffer absorbs short drops; reconnect with same session ID without re-pairing. Longer outages return to the discovery screen. |
| Android device sleep | Foreground service holds a partial wake-lock while connected. User can manually disconnect to release it. |

## 11. Android permissions

Required (declared in manifest, prompted on first launch / first use):

- `FOREGROUND_SERVICE` + `FOREGROUND_SERVICE_DATA_SYNC` — keep the WS
  + decode loop running while the app is backgrounded.
- `POST_NOTIFICATIONS` — show the persistent foreground-service
  notification.
- `WAKE_LOCK` — partial wake-lock while connected so audio/video
  don't stall.
- `NEARBY_WIFI_DEVICES` (Android 13+) — required for mDNS discovery
  on LAN; replaces the old coarse-location requirement.
- USB device-attached intent + per-device user grant for the host's
  vendor/product (no manifest permission, but a per-plug-in user
  prompt the first time).

Explicitly **not** needed:

- Storage / media access
- Accessibility services
- `SYSTEM_ALERT_WINDOW` (the widget lives only inside our app)
- Camera, microphone, contacts, location

## 12. Out of scope for v0

- Multiple tablets to one host
- Multiple hosts to one tablet simultaneously
- iPad / iOS client
- Mic-from-tablet → host
- HDR / wide-gamut color
- DRM-protected content
- Cloud relay

## 13. Open follow-ups (not blocking)

- Identity scheme details: how exactly we derive the stable
  per-tablet identity (USB serial may not be world-unique; consider
  hashing it with a host-generated salt at first pair).
- Pen-IMU access: does Samsung expose it without a privileged SDK
  partner agreement? If not, Air Actions becomes "best effort, not
  guaranteed".
- Audio-on-LAN path latency under congested networks — may need to
  drop priority below video automatically.
- Mode-default rotation behavior when the host has multiple physical
  monitors and the tablet's "extend" position has to pick which side
  to attach to.

## 14. Implementation milestones (this doc unblocks)

This plan does not start any implementation. Implementation order
remains as in `ROADMAP.md`:

- **M1 step 4** (next): browser-side WebSocket+WebCodecs probe to
  validate the host's transport. Browser code is throwaway.
- **M2**: input return path (uinput).
- **M3**: native Android app, using this plan as the spec.
