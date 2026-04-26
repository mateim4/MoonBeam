# M1 step 1 — DRM writeback connector probe

Run: 2026-04-26

## What the probe confirms

`host/src/bin/probe-writeback.rs` opens `/dev/dri/card0` (vkms), enables
`DRM_CLIENT_CAP_ATOMIC` + `DRM_CLIENT_CAP_WRITEBACK_CONNECTORS`, and
enumerates connectors. Without the writeback client cap, the writeback
connector is hidden — this matches `modetest -M vkms -c` only showing
Virtual-1.

With the cap set:

```
connectors found: 2
  connector handle=connector::Handle(45) kind=Writeback state=Unknown modes=0
  connector handle=connector::Handle(51) kind=Virtual   state=Connected modes=34
```

## Writeback connector properties

```
DPMS, link-status, non-desktop, TILE
CRTC_ID
WRITEBACK_OUT_FENCE_PTR
WRITEBACK_FB_ID
WRITEBACK_PIXEL_FORMATS  (blob)
```

The three writeback-specific atomic properties (`WRITEBACK_FB_ID`,
`WRITEBACK_OUT_FENCE_PTR`, `WRITEBACK_PIXEL_FORMATS`) are present.

## Pixel formats supported by vkms writeback

```
0x34325241  AR24   DRM_FORMAT_ARGB8888
0x34325258  XR24   DRM_FORMAT_XRGB8888
0x34324241  AB24   DRM_FORMAT_ABGR8888
0x38345258  XR48   DRM_FORMAT_XRGB16161616
0x38345241  AR48   DRM_FORMAT_ARGB16161616
0x36314752  RG16   DRM_FORMAT_RGB565
```

**vkms emits RGB only — no NV12, no YUV.** This is fine: NVENC accepts
RGB input directly (NV_ENC_BUFFER_FORMAT_ARGB / `bgr0` in ffmpeg) and
performs RGB→YUV on the GPU during encode. No CPU conversion needed.

Target format for M1: **XR24 / DRM_FORMAT_XRGB8888** — 4 bytes/pixel, no
alpha (NVENC ignores it anyway), widest support, ffmpeg pix_fmt `bgr0`.

## Architectural finding

Direct writeback capture from our daemon is **not straightforward** while
KWin holds DRM_MASTER on `card0`. The writeback connector shares its
CRTC with Virtual-1; atomic commits that touch the writeback connector
also affect KWin's CRTC state, which we can't do without being master
or having a DRM lease.

Two viable paths from here:

| Path | Pros | Cons |
|---|---|---|
| PipeWire screencast via `xdg-desktop-portal-kde` | Zero-copy dmabufs from KWin; no master/lease drama; well-tested. KWin internally uses writeback to populate the dmabuf, so latency parity with direct writeback is roughly preserved. | Extra IPC step; portal permission prompt on first connect. |
| DRM lease coordination with KWin | True direct writeback, cuts out portal IPC. | Lease semantics for shared-CRTC writeback are unclear; KWin would need to lease both the CRTC and the writeback connector to us, which is unusual. High implementation risk. |

**Recommendation for M1:** PipeWire screencast (portal). Defer direct
writeback to a M4 latency-tuning experiment if the portal path turns
out to be the bottleneck.
