# M1 step 2 — xdg-desktop-portal screencast probe

Run: 2026-04-26

## What the probe confirms

`host/src/bin/probe-portal.rs` requests a screencast from
xdg-desktop-portal-kde via `ashpd`, opens the resulting PipeWire stream,
and counts frames over a fixed window. Source picked in the dialog: the
laptop panel (`Laptop screen` / eDP-2).

```
got stream from portal: node_id=98
  declared size:    1707x1067    (logical, KDE scaled)
  declared position: 0,0
  source_type:      Monitor
  pipewire fd:      11

stream state: Unconnected -> Connecting
stream state: Connecting -> Paused
negotiated format: VideoFormat::BGRx 2560x1600 @ 0/1 fps
stream state: Paused -> Streaming

=== captured 681 frames in ~5 seconds (136.2 fps avg) ===
```

## Findings

- **Pipeline works end-to-end**: portal → PipeWire → our process, frames
  flow without further intervention after the user grants access.
- **Format negotiated as BGRx** = `DRM_FORMAT_XRGB8888` = ffmpeg pixfmt
  `bgr0` = `NV_ENC_BUFFER_FORMAT_ARGB`. **NVENC accepts this directly** —
  no CPU-side color conversion needed in the M1 pipeline.
- **Variable framerate** (negotiated `0/1`): KWin streams at whatever
  rate the source is updating, not a fixed cadence. For our use case
  (presenting Virtual-1 to apps that render at the configured refresh)
  this means we receive the natural update rate of the source.
- **136 fps avg on the laptop panel**: the SCAR 16's panel runs above
  60Hz natively, and KWin's screencast hands us frames at that rate.
  This is the rate-floor we'd see with a real high-refresh source — for
  Virtual-1 at 120Hz we expect ~600 frames in 5s.
- **Portal dialog exposes Virtual-1** as a selectable source with the
  KDE wallpaper rendered in its preview, confirming KWin treats the
  vkms output as a first-class display for screencast.

## Decisions captured

- Capture backend for M1 = portal screencast (decision recorded in
  `M1-WRITEBACK-PROBE.md`).
- Default capture pixel format = **BGRx** (the format the producer
  picked from our enum; matches NVENC native input).

## Follow-ups (not blocking M1)

- **dmabuf negotiation**: the probe uses `MAP_BUFFERS` so PipeWire
  delivers SHM-mapped buffers (CPU memory). For zero-copy to the GPU
  encoder we'll add an `SPA_PARAM_Buffers` param requesting
  `SPA_DATA_DmaBuf`, and pass the dmabuf fd to ffmpeg-next as an AV
  hardware frame. This is an M4 latency-tuning concern.
- **Restore tokens**: the probe uses `PersistMode::DoNot`, so the user
  is prompted on every connect. For the daemon, we'll switch to
  `Persistent` and persist the restore token in the config file so the
  daemon reconnects without prompting after first auth.
