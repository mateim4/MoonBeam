# M1 step 3 — h264_nvenc encode spike

Run: 2026-04-27

## What the probe confirms

`host/src/bin/probe-encode.rs` reuses the portal + PipeWire capture from
step 2, then routes each captured BGRx frame into `h264_nvenc` via
ffmpeg-next 8.1 and writes Annex-B H.264 to `/tmp/moonbeam-test.h264`.

Source picked: `Laptop screen` (eDP-2), declared 1707x1067 logical,
delivered as 2560x1600 BGRx by the producer.

```
got stream from portal: node_id=113
  declared size:    1707x1067
  source_type:      Monitor
negotiated format: VideoFormat::BGRx 2560x1600 @ 0/1 fps
h264_nvenc opened: 2560x1600 BGR0, 30000000 bps, output=/tmp/moonbeam-test.h264
stream state: Paused -> Streaming

=== captured 117 frames, encoded 117 packets, 6.26 MiB written ===
```

`ffprobe` on the resulting file:

```
codec_name=h264             profile=Main             level=51
width=2560                  height=1600              pix_fmt=yuv420p
mime_codec_string=avc1.4d4033
```

`ffmpeg -i ... -f null -` decodes cleanly with no errors. A single-frame
PNG export at 2560x1600 RGB shows the actual desktop intact (sharp,
correct color, no banding or chroma corruption).

## Findings

- **Format chain works end-to-end with no CPU color conversion**:
  PipeWire BGRx → ffmpeg `Pixel::BGRZ` (= `AV_PIX_FMT_BGR0`) → NVENC
  `NV_ENC_BUFFER_FORMAT_ARGB`. NVENC does the BGR→YUV420P conversion
  internally on the GPU.
- **NVENC settings used**: preset=`p1` (fastest), tune=`ull`
  (ultra-low-latency), rc=`cbr`, zerolatency=`1`, bitrate=30 Mbps,
  GOP=60, no B-frames. These are appropriate defaults for an
  interactive remote-display use case; the daemon will expose them as
  config later.
- **Output is valid Annex-B**: `is_avc=false`, `nal_length_size=0` in
  `ffprobe` confirms NVENC is emitting elementary-stream NAL units, not
  AVCC length-prefixed. That's exactly what we want to put on a
  WebSocket later — no demuxer / bitstream-filter needed on either end.
- **Frame rate dropped to ~25 fps in the spike** (vs ~136 fps on the
  same source in the no-encode probe). Cause: encode runs synchronously
  on the PipeWire `process` callback — `send_frame` + `receive_packet`
  block the capture loop, and the per-frame 16 MiB BGRx → frame-buffer
  memcpy is on the same thread. This is a known artifact of the
  spike's single-thread design and not a budget concern: production
  will run capture and encode on separate threads (ring buffer between
  them) and use dmabuf to skip the memcpy entirely.

## Decisions captured

- **Encoder**: `h264_nvenc` is sufficient for v1. AV1/HEVC NVENC remain
  available on RTX 5090 if we want better compression later, but H.264
  has the broadest browser MSE / Android MediaCodec support, which
  matters for the transport layer.
- **Output framing**: raw Annex-B, no muxer. The transport layer will
  packetize NAL units into WebSocket binary frames directly.
- **Pixel format pinned to BGRx for capture**: the encoder spike asks
  the producer for BGRx only (no fallback list), so format negotiation
  is unambiguous and the encoder can be created at param-changed time
  with a known input format.

## System under test

- GPU: NVIDIA GeForce RTX 5090 Laptop GPU, driver 595.58.03
- Kernel: 6.19.6-arch1-3-g14
- KWin: 6.6.4 (Plasma 6.6, Wayland)
- ffmpeg: n8.1 with `--enable-nvenc --enable-cuda-llvm`
- ffmpeg-next: 8.1.0; pipewire-rs: 0.9.2; ashpd: 0.10

## Follow-ups (not blocking M1)

- **Move encode off the PipeWire thread**: spawn an encode worker
  thread, connect via SPSC ring or `tokio::sync::mpsc`. The PW callback
  should only enqueue frames (or, post-dmabuf, fences). This removes
  the back-pressure that capped this spike at 25 fps.
- **dmabuf import path**: replace the BGRx memcpy with
  `av_hwdevice_ctx_create(AV_HWDEVICE_TYPE_CUDA)` + dmabuf import via
  `av_hwframe_ctx_init` so frames live on the GPU end-to-end. M4
  latency-tuning concern.
- **First-NAL latency measurement**: the spike doesn't measure
  frame-arrival → packet-emit latency; do this after moving encode
  off-thread, since the synchronous spike doesn't represent the
  production pipeline's latency profile.
- **Bitrate / GOP knobs in config**: surface preset/tune/bitrate/GOP
  in `config.toml` so headset-vs-desk usage can be tuned without
  recompiling.
