# M1 step 4 — WebSocket transport probe

Run: 2026-04-29

## What the probe confirms

`host/src/bin/probe-stream.rs` reuses the portal + PipeWire + NVENC
pipeline from step 3, but instead of writing Annex-B to disk, each
encoded access unit is broadcast on a `tokio::sync::broadcast` channel
to subscribed WebSocket clients. axum 0.8 serves a tiny static page
at `/` (`browser/index.html`) and a `/ws` upgrade endpoint.

The browser page uses the WebCodecs `VideoDecoder` API to decode the
raw Annex-B stream live — no muxer, no MSE, no JS-side parsing of
SPS/PPS beyond identifying the keyframe boundary.

Source picked: laptop screen (2560×1600 BGRx), same as step 3.

```
HTTP+WS server listening on http://0.0.0.0:7878/
got stream from portal: node_id=...
negotiated format: VideoFormat::BGRx 2560x1600 @ 0/1 fps
h264_nvenc opened: 2560x1600 BGR0, 30000000 bps, GOP=30
stream state: Paused -> Streaming
packets_out=60   frames_in=62   subscribers=0
packets_out=120  frames_in=122  subscribers=0
ws client connected (subscribers=1)
packets_out=180  frames_in=182  subscribers=1
…
packets_out=2100 frames_in=2102 subscribers=1
```

Browser side: a single `<canvas>` rendered the live desktop. No
`Lagged` errors, no decoder errors, no dropped subscribers across the
~35-second observation window. First-keyframe-after-connect latency
was sub-second (well within the 0.5s GOP).

## Wire format (locked)

Each WebSocket binary message:

```
+--------+--------+----------------------+
| type   | flags  | payload              |
| u8     | u8     | variable             |
+--------+--------+----------------------+

  type  = 0x01 video, 0x02 audio (future), 0x03 input (future)
  flags = bit0 = keyframe (video only; meaningless for other types)
  payload = raw Annex-B H.264 access unit (for type=0x01)
```

This is the same framing the Android MediaCodec client will consume
when M3 ships. Server-side `packet.is_key()` from ffmpeg-next is the
authoritative source for the keyframe flag — clients get to set
`BUFFER_FLAG_KEY_FRAME` (Android) or `EncodedVideoChunk.type='key'`
(WebCodecs) without parsing NAL types themselves.

## Findings

- **The browser-side WebCodecs path works on raw Annex-B with no
  description** — `VideoDecoder.configure({ codec: 'avc1.4d4033' })`
  alone is enough; SPS+PPS arrive in-band on every IDR (NVENC
  prepends them automatically when `repeat_pps` is on, which it is by
  default). MSE would have required wrapping the stream in fMP4
  fragments, an extra ~100–300 ms of buffering, and a second wire
  format. We dodged all of that.

- **Capture+encode is faster with the broadcast sink than with the
  file sink in step 3** — roughly 60 fps vs ~25 fps for the same
  source. The file-write in `probe-encode` (`BufWriter::write_all` on
  every drained packet) was the dominant back-pressure on the
  PipeWire `process` thread. Replacing it with `broadcast::send` of a
  `bytes::Bytes` (essentially a refcount bump after the per-packet
  alloc) lets the encoder drain freely. Production will still split
  capture and encode onto separate threads with a ring buffer, but
  this run shows the synchronous design has more headroom than step
  3 suggested.

- **Multi-client fanout is free** — `broadcast::Sender::send` clones
  `Bytes` to N receivers without re-allocating the packet, so the
  cost of a second WS subscriber is just the per-client TCP write.
  Confirmed by reading the source; this run only had one subscriber.

- **GOP=30 is the right default for the probe.** A new browser tab
  starts decoding within 0.5 second on average. We can revisit
  smaller GOPs (or per-client force-IDR) when the Android client
  needs more aggressive reconnect handling.

## Decisions captured

- **Transport framing**: 2-byte header (`type`, `flags`) + raw
  Annex-B payload. Same wire format on USB-C (`adb reverse`) and LAN.
  Documented in `docs/MOONBEAM-APP-PLAN.md` §5.1.
- **Browser client is a debugging tool only.** `browser/index.html`
  exists to verify the host transport before any Android code is
  written. The native Android app (M3) is the product. The browser
  page can be deleted when M3 lands; nothing else depends on it.
- **WS slow-client policy**: drop on `RecvError::Lagged` and let the
  client reconnect. Current capacity is 64 access units (~1 s at 60
  fps). For production we'll likely add a `force_idr` round-trip on
  reconnect so the server can issue a keyframe immediately rather
  than wait up to one GOP boundary.

## System under test

- GPU: NVIDIA GeForce RTX 5090 Laptop GPU, driver 595.58.03
- Kernel: 6.19.6-arch1-3-g14
- KWin: 6.6.4 (Plasma 6.6, Wayland)
- ffmpeg: n8.1 with `--enable-nvenc --enable-cuda-llvm`
- ffmpeg-next: 8.1.0; pipewire-rs: 0.9.2; ashpd: 0.10
- axum: 0.8; tower-http: 0.6; tokio: 1.40
- Browser: Firefox (current stable on this host)

## Follow-ups (not blocking M1)

- **Server-side keyframe replay cache**: keep the most recent SPS +
  PPS + IDR sequence in `AppState` so a freshly-connected client
  decodes immediately instead of waiting up to 1 GOP. Pairs naturally
  with a `force_idr` opcode on the WS control side.
- **Rate-limit the `packets_out=` log line** to once per second
  rather than every 60 packets, so the line stays at 1 Hz independent
  of capture rate. Cosmetic only.
- **Move encode off the PipeWire thread** (same item carried over
  from step 3). Less urgent now that we know the synchronous design
  hits 60 fps, but still required before we attempt 120 fps end-to-end.
- **Audio path on the same WS** with `type=0x02`. M3-ish.
- **TLS option** for LAN deployments. WS-over-TLS is one axum
  `tls_rustls` line; Android picks it up via `wss://`.
