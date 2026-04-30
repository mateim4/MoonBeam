# M2 step 4 — multiplexed video + input WebSocket

Run: 2026-04-30 (compile only; live test deferred — see below)

## What this step is

The integration step that fuses M1 step 4 (video out over WS) and M2
step 3 (input in over WS) into a single axum process behind one
`/ws` upgrade. From this point on, a single TCP connection carries
both directions of the protocol — one WS, two responsibilities, no
extra port and no second TLS context.

`host/src/bin/probe-mux.rs` is the merged binary. It is structurally
the union of probe-stream and probe-input-server with the WS handler
upgraded to dispatch on the leading byte:

- Outbound on the WS: video access units from the
  portal+PipeWire+NVENC pipeline, framed `[0x01][flags][annex_b]`.
- Inbound on the WS: input messages, framed `[0x03][flags][json]`,
  routed through `apply()` to the pen + touch uinput devices.

The wire format itself didn't change — it was already designed for
this in M1 step 4, with `type` bytes reserved for future channels
(`0x02` audio, room for `0x04`+ control opcodes). What changed is
that the *server* now uses the discriminator instead of treating
non-video frames as a no-op.

## Code shape

```rust
// handle_socket — one tokio::select! per connection
loop {
    select! {
        // Outbound: video broadcast → WS
        res = rx.recv() => match res {
            Ok(payload) => socket.send(Message::Binary(payload.into())).await?,
            Err(Lagged(n)) => break,    // drop lagging client
            Err(Closed) => break,
        },
        // Inbound: WS → uinput
        msg = socket.recv() => match msg {
            Some(Ok(Message::Binary(bytes))) => handle_inbound(&bytes, &state).await,
            Some(Ok(Message::Close(_))) | None => break,
            ...
        },
    }
}
```

`handle_inbound` is two lines of routing on `bytes[0]` (the type
byte), then the same JSON-parse → `apply()` path probe-input-server
already exercised. The `apply()` body is verbatim from M2 step 3 — no
behavioral changes to input handling.

## What landed

| File | Change |
|---|---|
| `host/src/bin/probe-mux.rs` | New binary; merged probe-stream + probe-input-server. |
| `host/Cargo.toml` | No new deps (probe-mux uses only crates already pulled in by the two source probes). |

`probe-stream` and `probe-input-server` are kept around — they're
smaller, more focused probes for testing video or input in isolation.
probe-mux is the "this is what moonbeamd will look like" proof.

## Verification status

- **Compile**: ✅ clean build, first try.
- **Static review**: ✅ the only new logic is `handle_inbound`'s
  type-byte dispatch (3 lines) and the `apply()` body (verbatim
  copy from probe-input-server's verified path). The video out path
  is byte-identical to probe-stream's verified path.
- **Live end-to-end run**: ⏸ deferred. Running probe-mux requires
  dismissing the KDE xdg-desktop-portal screencast dialog (same as
  probe-stream). Will be exercised the next time the user is ready
  to drive the portal interactively. The structural argument above
  is strong enough that we don't expect surprises — but if probe-mux
  misbehaves at runtime, the obvious suspects are:
  - WebSocket frame ordering when video and input arrive
    interleaved (single `select!` arm at a time, so this is mostly
    a matter of fairness — tokio's `select!` documents that it
    polls arms in pseudo-random order, which is fine for us)
  - uinput device construction failing if `/dev/uinput` ACL
    differs at the time probe-mux runs vs probe-input-server runs
    (unlikely; both are run as the same user, same systemd session)

## Decisions captured

- **One probe replaces two, but both source probes stay.** The merge
  isn't a deletion. probe-stream and probe-input-server are still
  the right tool for isolated debugging — if video stutters, you
  want to bisect by running it standalone; if input dispatch
  breaks, same logic.
- **uinput devices are built before the portal dialog opens.** The
  rationale: a misconfigured `/dev/uinput` should fail fast, not
  after the user has clicked through the KDE consent dialog. This
  also keeps the failure modes diagnosable in order — kernel-side
  failures appear before any compositor interaction.
- **No shared library extraction yet.** probe-mux duplicates ~150
  lines of uinput device-building and InputMsg deserialisation from
  probe-input-server. The pattern only exists in two binaries;
  per "no abstractions beyond what's needed", a `lib.rs` refactor
  waits until moonbeamd proper begins (when the same code will be
  imported by the daemon main + tests).
- **Single `select!` per connection, no spawn.** The other shape
  worth considering is `socket.split()` + two spawned tasks (one
  for send, one for recv) joined at the end. Today's single-loop
  design is simpler to reason about, has no cross-task locking
  concerns (we hold `state.pen.lock()` only for the duration of one
  uinput write), and one connection at a time is the v0 design
  anyway. We'll revisit if the input arm starves the video arm or
  vice versa under load.

## Things deferred (not blocking M2)

- **Live cosmetic verification**: open browser to `index.html` (video
  arrives), open browser to `input.html` (clicks fire events), watch
  one connection carry both. ~30 seconds of click-through, scheduled
  for the next interactive session.
- **Server-side keyframe replay cache** (carried over from M1
  follow-ups): when a fresh client connects mid-GOP, we make them
  wait up to 1 GOP for the next IDR. The M2-step-3 follow-up about
  a `force_idr` opcode pairs with this — same WS, type `0x04` say,
  flags 0, no payload, server bumps the encoder.
- **Per-tablet identity**: when the Android client lands in M3,
  probe-mux's "single shared pen + touch device pair" model will
  need to grow into "one pair per paired tablet identity". Out of
  scope for the probe.

## What this unblocks

M2 is now done in code. The integrated host-side daemon can:

1. Set up a virtual display (M0).
2. Capture it through xdg-desktop-portal screencast (M1 step 1-2).
3. Encode with NVENC (M1 step 3).
4. Broadcast video access units over a single WebSocket (M1 step 4).
5. Receive input events on the same WebSocket and route them to
   uinput devices that Linux apps see as a real S-Pen and a real
   10-finger touchscreen (M2 steps 1-3).
6. Fuse the two directions on one connection (M2 step 4 — this).

The next milestone is **M3: Android client**. That's where the
tablet stops running Spacedesk + Windows and starts running
something that connects to this daemon over `adb reverse` USB-C and
speaks the wire format we just locked.

## System under test

Same as the rest of M2 (kernel 6.19.6, KWin 6.6.4 on Plasma 6.6
Wayland, ffmpeg-next 8.1, axum 0.8, input-linux 0.7.1).
