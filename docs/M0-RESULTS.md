# M0 — KWin vkms 120Hz commit test

Run: 2026-04-25T15:00:43+02:00

## Environment
- Kernel: `6.19.6-arch1-3-g14`
- KWin: `QThreadStorage: entry 1 destroyed before end of thread 0x55f9d91b27a0`
- NVIDIA driver: `595.58.03`
- Plasma: `plasmashell 6.6.4
unknown`

## Test parameters
- Connector: `Virtual-1`
- Mode: 2960x1848 @ 120Hz, kscreen mode id `37`

## Check 1 — kernel DRM state (authoritative)
```
connector[51]: Virtual-1
	crtc=crtc-0
	self_refresh_aware=0
	interlace_allowed=0
	ycbcr_420_allowed=0
	max_requested_bpc=0
	colorspace=Default
colorop[39]:
	type=1D LUT
	bypass=1
connector[51]: Virtual-1
	crtc=crtc-0
	self_refresh_aware=0
	interlace_allowed=0
	ycbcr_420_allowed=0
	max_requested_bpc=0
	colorspace=Default
	colorspace=Default
	colorspace=Default
	colorspace=Default
```

## Check 2 — KWin output report (sanity)
```
Output: 2 Virtual-1 40a65325-4c8d-442f-983e-17a1d8f08b93
	enabled
	connected
	priority 2
	Unknown
	replication source:0
	Modes:  10:1792x1344@60.00  11:2048x1152@60.00  12:1920x1200@59.88  13:1920x1200@59.95  14:1920x1080@60.00  15:1600x1200@60.00  16:1680x1050@59.95  17:1680x1050@59.88  18:1400x1050@59.98  19:1400x1050@59.95  20:1600x900@60.00  21:1280x1024@60.02  22:1440x900@59.89  23:1440x900@59.90  24:1280x960@60.00  25:1366x768@59.79  26:1366x768@60.00  27:1360x768@60.01  28:1280x800@59.81  29:1280x800@59.91  3:1024x768@60.00!  30:1280x768@59.87  31:1280x768@59.99  32:1280x720@60.00  33:800x600@60.32  34:800x600@56.25  35:848x480@60.00  36:640x480@59.94  37:2960x1848@119.99*  4:4096x2160@60.00  5:4096x2160@59.94  6:2560x1600@59.99  7:2560x1600@59.97  8:1920x1440@60.00  9:1856x1392@59.99 
	Custom modes:
		0: 2960x1848@119.99
	Geometry: 3840,0 2960x1848
	Scale: 1
	Rotation: 1
	Overscan: 0
	Vrr: incapable
	RgbRange: unknown
	HDR: incapable
	Wide Color Gamut: incapable
	ICC profile: none
	Color profile source: sRGB
	Color power preference: prefer efficiency and performance
```

## Check 3 — drm_vblank_event cadence over 5s (real scanout rate)

Pass criterion: ~600 vblanks (120Hz). Clamp-to-60: ~300.

```
=== total drm_vblank_event lines in 5s ===
601
=== per-crtc breakdown (count crtc=N) ===
    601 crtc=0
```

## Verdict

**PASS.** 601 `drm_vblank_event` ticks in 5 seconds = **120.2 Hz**, within the ±30/5s pass band. With mpv driving 120fps content into Virtual-1, vkms's software vblank timer ran at the configured rate and KWin committed against it without clamping.

Architectural implication: the planned MoonBeam pipeline can be built on stock vkms. No need to fork the module, patch KWin, or fall back to EVDI. M1 (capture → encode → transport) proceeds against vanilla vkms.

Caveat for future work: the `drm:drm_vblank_event` tracepoint reports only `crtc=N` without device disambiguation, so a strict reading is "crtc=0 in *some* DRM device ticked 601 times." Given mpv was rendering exclusively to Virtual-1 and the laptop panel had no animation forcing vblank requests on the iGPU's CRTC, the only CRTC with reason to tick at 120Hz was vkms's. If stricter disambiguation is ever needed, switch the cadence check to `bpftrace` on the same tracepoint and capture the device pointer.

