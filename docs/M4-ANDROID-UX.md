# M4 Android UX — design specification

Status: **design, pre-implementation.** This doc is the source of
truth for the M4 UI/UX work. Implementation reads from this; code
PRs that diverge from this doc need to update it first.

Last revised: 2026-05-01

## 1. What this covers

The full v0 in-app UX after M3's "video surface + nothing else"
shipped. Specifically:

- The floating "MoonBeam puck" and its radial menu
- Connection lifecycle states and how the user perceives them
- The Settings panel
- The (now-relocated) latency stats overlay
- Theme tokens, motion language, accessibility minimums

What this **does not** cover:

- Pairing UX (6-digit code flow) — M4 phase 2, separate doc
- Audio routing UI — depends on audio path implementation
- LAN host discovery (mDNS) UI — depends on LAN transport
- S-Pen Air Actions integration — best-effort polish, not v0
- Localization — English only for v0

## 2. Design language

### 2.1 Foundation

**Material 3 Expressive** (Compose Material 3 1.4+, Compose BOM
2025.06.00 or later). We use Expressive specifically for:

- Shape morphing on the puck → radial menu transition
- Spring physics for puck drift
- Button group component for the radial menu items
- Expressive motion specs (`MaterialTheme.motionScheme.fast/default`)

We don't use:

- Top app bars (no app frame — the SurfaceView IS the screen)
- Navigation bars (single screen)
- Floating action buttons (the puck is our FAB)
- M3 Expressive's wavy progress (no progress indicators in this UX)

### 2.2 Color palette

> **⚠ Placeholder.** Final colorway will be supplied by the user in
> a follow-up. The values below are temporary so the rest of the
> spec has something concrete to reference; swap before
> implementation. The structural decisions (always-dark base,
> single accent, semi-transparent surface) are kept regardless of
> the final hues.

The app lives on top of arbitrary video, so colors must remain
legible against any background. Strategy: a **darkened tinted
glass** aesthetic — the puck and panels are semi-transparent dark
surfaces with bright accent colors.

```
Base scheme: dark (always, even in light system theme)
  surface          = #181A1F at 70% alpha    (puck background, panel bg)
  surface-bright   = #22252C at 85% alpha    (radial item background)
  on-surface       = #ECECEC                 (icons, primary text)
  on-surface-variant = #B0B5BC               (secondary text, stats)

Accent: vibrant green (matches the existing brand-ish #22CC55 we've
been using for status text)
  primary          = #22CC55
  primary-container = #0E5C25
  on-primary       = #001A06
  on-primary-container = #C8FFD8

Destructive (Disconnect, errors)
  error            = #FF5466
  error-container  = #5C0E18
  on-error         = #FFFFFF

Stroke / hairline
  outline-variant  = #ECECEC at 12% alpha    (item separators)
```

Rationale for "always dark": video content varies, but a dark
surface + bright accent reads cleanly on both bright (web, Krita)
and dark (terminal, IDE) host content. Light-mode glass would
disappear on white documents.

### 2.3 Typography

```
display    — none (no headline-scale text in this UX)
headline   — none
title      — Settings panel section headers
body-large — Settings panel content
body-medium — radial menu item labels (when shown)
body-small — stats overlay, status overlay
label-medium — toast messages
```

Use Compose Material 3 Expressive's default typography stack; we
don't override fonts. Roboto Flex (Expressive's default variable
font) handles weight/optical size variations natively.

### 2.4 Shape language

All custom components use **rounded** shapes — the puck is a circle,
the radial menu items are circles, the Settings panel uses 28dp top
corners (M3 large-shape). No square corners except the SurfaceView
(which is the full window).

For the puck → radial transition, we use **shape morphing** — the
puck's circle dilates and morphs into the radial container's
expanded circle.

### 2.5 Motion

Three motion families in this UX:

| Use | M3 Expressive token | Approx duration |
|---|---|---|
| Puck drift to edge (after inactivity) | `motionScheme.slowEffects` (spring, low stiffness) | ~600 ms |
| Puck → radial expansion | `motionScheme.defaultSpatial` (spring, medium) | ~300 ms |
| Radial item selection | `motionScheme.fastSpatial` (spring, high stiffness) | ~150 ms |
| Settings panel slide-up | `motionScheme.defaultSpatial` | ~300 ms |
| Toast appearance | `motionScheme.fastEffects` (fade) | ~150 ms |

Springs everywhere. No linear or eased curves except for opacity
fades (which use `fastEffects`). Motion is the most distinctive
part of the Expressive language — leaning into it pays off.

## 3. Layout architecture

```
+------------------------------------------------------+
| SurfaceView (fullscreen, MediaCodec output)          |
|                                                      |
|                                                      |
|  [puck]                                              |
|                                                      |
|                                                      |
|                       (status overlay - center, only |
|                        until first frame)            |
|                                                      |
|                                                      |
|                                              [stats] |
|                                                      |
+------------------------------------------------------+
        ↑                          ↑          ↑
   floating puck            settings       stats
   (drift to edge)          slides up      top-right
                            from bottom    when enabled
```

Compose hierarchy:

```kotlin
Box(Modifier.fillMaxSize()) {
    AndroidView { SurfaceView(...) }     // video, full screen

    if (status.framesDecoded == 0) {
        StatusOverlay(...)               // center, until first frame
    }

    if (settings.showStats) {
        StatsOverlay(...)                // top-right
    }

    Puck(...)                            // floating, owns its position
    RadialMenu(...)                      // anchored to puck position
                                         //   (visible only when expanded)

    SettingsSheet(...)                   // bottom-sheet, on demand

    Toaster(...)                         // bottom-center, ephemeral
}
```

## 4. The puck

### 4.1 Physical specs

- **Size at rest:** 48dp diameter (M3 minimum touch target rounded up)
- **Size when finger-down (pre-expand):** 56dp (anticipation)
- **Color:** `surface` (#181A1F at 70%) with a 1.5dp `primary`-tinted
  glow stroke at the perimeter
- **Inner glyph:** small "M" mark (placeholder until brand finalizes)
  in `primary` color, 16sp
- **Elevation:** 4dp shadow (M3 elevation level 2) — visible against
  bright video, gentle against dark video
- **Opacity at rest:** 40% (matches plan §7)
- **Opacity while moving / interacting:** 90%
- **Initial position:** bottom-right corner, 16dp inset

### 4.2 States

```
                +------------+
                |    Idle    |  (after 3s of no interaction)
                +-----+------+
                      |
              finger-down on puck
                      ↓
                +-----+------+
                |  PressIn   |  (size +8dp, opacity → 90%)
                +-----+------+
                      |
              release / drag start
                ↙             ↘
       tap (no drag)      drag started
              ↓                 ↓
       +-----+------+    +-----+------+
       |  Expanded  |    |  Dragging  |
       | (radial)   |    +-----+------+
       +-----+------+          |
              |          drag ended
       item selected            ↓
       or dismiss               +-----+------+
              ↓                 |  Resettling |
       (back to Idle)           |  (springing |
                                |  to nearest |
                                |   edge)     |
                                +-----+------+
                                      |
                                  (3s later)
                                      ↓
                                    Idle
```

### 4.3 Drift-to-edge

After 3s of no interaction, the puck animates to the nearest edge
of the screen — reducing its visible footprint. Behavior:

- Compute the closest edge (left/right/top/bottom) by Euclidean
  distance to the puck center
- Animate puck position to `(edge_x, current_y)` (or the analogous
  for top/bottom) with an inset of `-puck_radius * 0.5` — i.e.,
  half the puck is off-screen
- Spring motion (slowEffects)
- During drift, opacity → 25%
- Touch target stays full size (we expand the touchable bounds via
  Modifier.size beyond the visual size)

When user taps the half-hidden puck, it springs back fully on-screen
with a slight overshoot (Expressive's spring default), then enters
PressIn → Expanded.

### 4.4 Pen pass-through

Critical: **pen events never trigger the puck.** The user must be
able to draw on top of the puck without it interpreting strokes as
"start dragging me."

Implementation: in the puck's pointer input handler, check
`PointerInputChange.type` (`PointerType.Stylus` or
`PointerType.Eraser`) — if any pointer in the current event is a
stylus, return `consumed = false` and let the event pass through to
the SurfaceView underneath.

Edge case: hover events from the S-Pen near the puck. Same rule —
hover with stylus tool type is ignored by the puck.

### 4.5 Edge-swipe stow

Drag the puck off any edge by ≥75% of its radius → it snaps fully
off-screen with a small "tab" indicator (8dp wide, puck-height tall,
20% opacity) remaining at the edge. Tap the tab to bring the puck
back on-screen.

## 5. Radial menu

### 5.1 Items (in order, top-clockwise)

| Slot | Icon | Label (a11y) | Action |
|---|---|---|---|
| 1 (top) | `mode_extend` / `mode_mirror` | "Extend mode" / "Mirror mode" | Toggle and persist per host |
| 2 | `quality_drawing` / `quality_display` | "Drawing mode" / "Display mode" | Toggle, send `set_quality` control msg |
| 3 | `rotate_screen` | "Rotate host display" | Send rotate control msg, host applies |
| 4 (bottom) | `volume_up` / `volume_off` | "Audio on" / "Audio off" | Toggle audio stream subscription |
| 5 | `settings` | "Settings" | Open SettingsSheet |
| 6 | `power_off` | "Disconnect" | Show DisconnectConfirm dialog |

Six items in a perfect hexagon around the puck position. Item 1 is
straight up; rotation continues clockwise.

### 5.2 Layout — edge-aware

If the puck is near a screen edge, the radial would clip. Resolve
by **rotating the radial** so all items stay on-screen:

- Puck near right edge: rotate radial 30° counter-clockwise (items
  arc to the left)
- Puck near top edge: rotate so no item is above puck
- Compute the "forbidden" angle range based on edge proximity, then
  bias the item layout to use only the allowed arc

Each item is 56dp diameter, placed at `radius = 88dp` from the puck
center. So total radial diameter ≈ 232dp.

### 5.3 Expansion animation

Two-stage:

1. **Puck shape morph** (150 ms, fastSpatial): puck dilates from 48dp
   to 80dp, becomes a soft-edged disc that hosts the items.
2. **Item stagger** (180 ms total, items emerge with 30 ms stagger):
   each item scales from 0 to full size with a slight "pop" (spring
   overshoot).

Use M3 Expressive's `MorphAnimation` API where appropriate; the
stagger is a manual `LaunchedEffect` over a list of items.

### 5.4 Selection feedback

Tap an item:
- Item scales to 1.15× and back to 1.0 (50 ms)
- Brief `primary`-tinted halo expands and fades (M3 Expressive
  ripple)
- Action fires
- Radial collapses (reverse of expansion, 150 ms total)

Toggle items (Extend/Mirror, Drawing/Display, Audio) — the icon
crossfades to the new state without dismissing the radial. So the
user can flip multiple toggles without re-opening the menu. The
radial dismisses 4s after the last interaction.

Action items (Rotate, Settings, Disconnect) — radial collapses
immediately, action proceeds.

### 5.5 Dismiss

Three ways:

1. Tap outside the radial (anywhere on the SurfaceView).
2. Press the system back button.
3. 4s of inactivity after the most recent toggle/no-action.

Dismiss animation is the reverse of expansion, but slightly faster
(120 ms total).

### 5.6 S-Pen native variant (Variant B from plan §7.2)

When an S-Pen with a button is detected on the device:

- **Pen-button hold + hover within ~15mm of the screen** → radial
  expands at the pen tip position (not at the puck position).
- Same items, same layout (with edge-aware adjustments).
- Selection: hover over item + release pen button.
- Released without selection over an item → dismiss, no action.

The Variant A puck stays present and usable; Variant B is purely
additive.

## 6. Connection lifecycle UX

### 6.1 First launch (no host known yet)

Shown on the SurfaceView (which is black until first frame):

```
┌──────────────────────────────────┐
│                                  │
│         MoonBeam                 │
│         ─────────                │
│                                  │
│     Plug in your laptop's USB    │
│     cable and start moonbeamd    │
│                                  │
│     waiting for connection…      │
│     ⏵ retry in 1s                │
│                                  │
└──────────────────────────────────┘
```

This is `StatusOverlay` after expansion. Replaces the current
"M3 step 1 stub" placeholder. Simpler than a full pairing dialog
because pairing is M4 phase 2.

### 6.2 Connecting

```
┌──────────────────────────────────┐
│                                  │
│         MoonBeam                 │
│                                  │
│     Connecting…                  │
│                                  │
└──────────────────────────────────┘
```

Same overlay, simpler text.

### 6.3 Connected, waiting for keyframe

```
┌──────────────────────────────────┐
│                                  │
│         MoonBeam                 │
│                                  │
│     Connected. Waiting for       │
│     first frame…                 │
│                                  │
└──────────────────────────────────┘
```

### 6.4 Streaming

Status overlay disappears on the first decoded frame. Puck appears
(animating in from the bottom-right corner). Stats overlay appears
in top-right if enabled in Settings.

### 6.5 Disconnected by user

```
Toast (3s, bottom-center):
"Disconnected from <host>. Reconnecting…"
```

Status overlay reappears. Puck disappears. The connection retry
loop continues; user can also kill the app to stop reconnecting.

### 6.6 Lost connection (host disappeared)

Same as 6.5 but the toast text:

```
"Connection lost. Trying to reconnect…"
```

If 30s passes with no reconnect, the toast updates to:

```
"Host unreachable. Check the cable and that moonbeamd is running."
```

Reconnect attempts continue in the background; user can manually
kill the app.

### 6.7 Error (decode failure, malformed stream)

Rare in practice. Toast:

```
"Video error. Reconnecting…"
```

Decoder is torn down, recreated on next frame. If it happens
repeatedly within 10s, the toast becomes:

```
"Video decoder is misbehaving. Disconnect and try again?"
```

## 7. Settings panel

### 7.1 Trigger and dismiss

- Open from radial → Settings item.
- Slides up from bottom as an M3 modal bottom sheet.
- Half-height by default, drag to expand to 90%.
- Dismiss: drag down, system back, tap scrim, or radial Dismiss.

### 7.2 Content for v0

```
┌──────────────────────────────────────┐
│  ━━━                                  │  ← drag handle
│                                      │
│  HOST                                │
│  Connected to roglap (Linux)         │
│  ws://127.0.0.1:7878/ws              │
│  Connected since 14:32 (12 min)      │
│                                      │
│  ───────────────────────────────────  │
│                                      │
│  DISPLAY                             │
│  Mode             [ Extend ▾ ]       │
│  Quality          [ Drawing ▾ ]      │
│                                      │
│  ───────────────────────────────────  │
│                                      │
│  PEN                                 │
│  Pressure curve   [ Linear ▾ ]       │
│  Stylus button    [ Right click ▾ ]  │
│                                      │
│  ───────────────────────────────────  │
│                                      │
│  DEBUG                               │
│  Show latency stats     [ ON  ]      │
│  Show wire format       [ off ]      │
│  Verbose logging        [ off ]      │
│                                      │
│  ───────────────────────────────────  │
│                                      │
│  ABOUT                               │
│  MoonBeam 0.1.0-m4                   │
│  Source: github.com/mateim4/MoonBeam │
│                                      │
│  ───────────────────────────────────  │
│                                      │
│  [    Disconnect   ]                 │
│  [    Force reconnect    ]           │
│                                      │
└──────────────────────────────────────┘
```

The "DEBUG" section is hidden behind a long-press on "MoonBeam"
title (4 quick taps), Easter-egg style. Most users won't see it.

## 8. Stats overlay (relocation)

The current top-right stats overlay stays, but:

- **Off by default** (was always-on through M3). Settings → Debug →
  Show latency stats turns it on.
- **Position:** top-right, 12dp inset.
- **Auto-fade:** if user taps the overlay area, stats fade out for
  60s (in case they're covering important video content).
- **Same content** as M3: fps / decode / input / ws-rtt.

## 9. Accessibility

### 9.1 TalkBack

Every interactive element has a content description:

- Puck: "MoonBeam menu, double-tap to open"
- Each radial item: see §5.1 a11y column
- Settings panel sections: section header announced as heading
- Toggles: announce current state ("Audio: on. Double-tap to turn
  off.")

### 9.2 Touch targets

Min 48dp for all interactive elements. The puck is 48dp; radial
items are 56dp; settings rows are 56dp tall.

### 9.3 Contrast

All text/icons against `surface` (40% alpha dark) must clear WCAG
2.1 AA. Verified at design time:

- `on-surface` (#ECECEC) on `surface` over a worst-case white
  background: contrast ratio ~4.7:1 ✅
- `on-surface` over a worst-case black: ~13:1 ✅
- `primary` (#22CC55) on `surface` over white: ~3.2:1 — fails AA
  for body text but passes for icons and large text ✅ (we only use
  primary for icons and the puck glyph at 16sp)

### 9.4 Reduced motion

Respect Android's `Settings.Global.ANIMATOR_DURATION_SCALE` and
`Settings.Secure.ACCESSIBILITY_DISPLAY_INVERSION_ENABLED`. When
animations are disabled system-wide, replace spring transitions with
instant snaps; replace expansion stagger with a single fade.

## 10. Gesture handling

### 10.1 Finger vs pen disambiguation

The SurfaceView eats all events that hit it. The puck is in front
of the SurfaceView and gets first dibs. Rules:

| Pointer type | On puck | Off puck |
|---|---|---|
| Stylus / Eraser | Pass through to SurfaceView (always) | Pass through to SurfaceView |
| Finger | Capture if puck visible; otherwise pass through | Pass through |
| Hover (stylus) | Always pass through | Always pass through |

Implementation: in the puck Composable, use
`Modifier.pointerInput { awaitPointerEventScope { ... } }` and
return without consuming for any event whose pointers contain a
stylus.

### 10.2 Multi-touch on puck

If the user puts two fingers on the puck at once, treat as a single
press (use the first pointer). Don't attempt pinch-to-resize or
rotate the puck — keep gestures simple.

### 10.3 Drag-to-position

While dragging the puck, the radial menu does NOT open until release.
On release, if the puck has moved >10dp from press-down position,
treat as drag end (settle at new position, no menu). Otherwise treat
as tap (open radial).

### 10.4 Edge-swipe vs drag

If the user drags the puck past 75% of its radius off-screen,
trigger the edge-swipe stow behavior (§4.5). The drag→stow boundary
is sticky: once stowed, dragging the tab back triggers un-stow.

## 11. Toaster

A small bottom-center area for ephemeral feedback, slimmer than M3
SnackBar:

- Single line of text, 14sp
- `surface-bright` background, `on-surface` text
- 28dp pill shape
- Auto-dismiss after 3s (4s for actionable toasts with a button)
- Stack: at most 1 toast visible; new toast replaces with cross-fade
- Position: 24dp above the bottom edge

Used for connection state changes (§6.5–6.7), mode-switch
confirmations ("Switched to Drawing mode"), and audio toggles.

## 12. Implementation notes for Compose

### 12.1 Composable hierarchy

```
MoonBeamApp
└── MaterialExpressiveTheme(colorScheme = MoonBeamColors, typography = ...)
    └── MoonBeamScaffold
        ├── VideoSurface (existing AndroidView wrapping SurfaceView)
        ├── StatusOverlay (existing, restyled with new theme tokens)
        ├── StatsOverlay (existing, off-by-default toggle)
        ├── Puck
        │   └── PuckContent (handles drift, drag, expand)
        ├── RadialMenu (anchored to puck, visible when expanded)
        │   └── RadialItem × 6
        ├── SettingsSheet (ModalBottomSheet, conditional)
        └── Toaster (single-line, bottom-center)
```

### 12.2 State hoisting

```kotlin
// Top-level state, hoisted in MoonBeamViewModel
data class UiState(
    val connection: ConnectionState,        // existing
    val stats: Stats,                       // existing
    val puck: PuckState,                    // new
    val radialOpen: Boolean,                // new
    val settingsOpen: Boolean,              // new
    val toast: Toast?,                      // new
    val settings: AppSettings,              // new (persisted)
)

data class PuckState(
    val position: Offset,                   // current pixel position
    val anchored: Anchor,                   // edge it's docked to
    val opacity: Float,                     // 0.25 .. 0.9
    val sizeMultiplier: Float,              // 1.0 default, 1.17 pressed
)

data class AppSettings(
    val showStats: Boolean = false,
    val showWireDebug: Boolean = false,
    val verboseLogging: Boolean = false,
    val mode: ConnectionMode = Extended,
    val quality: QualityMode = Display,
    val audioEnabled: Boolean = false,
    val pressureCurve: PressureCurve = Linear,
    val stylusButton: StylusBinding = RightClick,
)
```

### 12.3 Persistence

`AppSettings` lives in **DataStore Preferences** (added as a new
dep). Per-host preferences (mode, quality per paired host) wait for
the pairing flow in M4 phase 2; for v0 there's only one set of
settings.

### 12.4 Theming entry point

```kotlin
@Composable
fun MoonBeamTheme(content: @Composable () -> Unit) {
    MaterialExpressiveTheme(
        colorScheme = darkColorScheme(/* tokens from §2.2 */),
        motionScheme = MotionScheme.expressive(),
        // typography = default expressive, no override
        // shapes = default expressive, no override
        content = content,
    )
}
```

### 12.5 New Compose deps

| Dep | Why |
|---|---|
| `androidx.compose.material3:material3:1.4.x+` (via BOM 2025.06+) | M3 Expressive APIs |
| `androidx.datastore:datastore-preferences:1.1.x` | persisting AppSettings |
| `androidx.compose.material:material-icons-extended` | radial menu icons |

No DI framework, no nav library — single screen, single ViewModel.

## 13. Phasing within M4

This is too large to land in one PR. Suggested order:

1. **Theme + base scaffolding** (one PR)
   - Bump Compose BOM, add MaterialExpressiveTheme wrapper
   - Define MoonBeamColors and the new tokens
   - Update existing StatusOverlay + StatsOverlay to use the tokens
   - DataStore Preferences for AppSettings
2. **Puck (resting + drift, no menu)** (one PR)
   - Floating Composable, drag, drift-to-edge, pen pass-through
   - Stats overlay relocates (still always-on temporarily)
3. **Radial menu (no real actions)** (one PR)
   - Expansion animation, six items as visual stubs, dismiss behavior
   - Edge-aware layout
   - Each item is a no-op except logging the click
4. **Settings sheet** (one PR)
   - Modal bottom sheet, sections, persistence wiring
   - Stats overlay toggle moves here (now off-by-default)
5. **Connection lifecycle UX** (one PR)
   - Toaster, redesigned StatusOverlay, error states
6. **Wire radial actions to real behavior** (one or two PRs)
   - Disconnect, force-reconnect, audio toggle (when audio ships),
     mode toggles (when host control channel ships)
7. **S-Pen Variant B** (last; depends on pen detection plumbing)

Each PR keeps the app shippable end of step.

## 14. What this doc deliberately leaves open

- **App icon & branding.** Placeholder "M" glyph. Real brand work
  later.
- **Specific shape morph curve** for the puck → radial transition.
  Will tune at implementation time.
- **Toast queue semantics** if multiple events fire in quick
  succession. v0 does "replace last," may want a queue if it feels
  wrong in use.
- **Settings panel scrolling vs. paging** for long sections. Default
  scrolling for v0; revisit if it feels awkward.
- **Pen pass-through edge cases** when the pen is hovering AND a
  finger is touching the puck. Probably finger wins, but verify in
  testing.
- **Sound effects.** No sounds in v0 (Wayland-side audio path is
  separate work).

## 15. Acceptance criteria for "M4 UX done"

The user can:

- [ ] See connection state without inspecting logs
- [ ] Disconnect cleanly via the puck → radial → Disconnect path
- [ ] Force reconnect from Settings
- [ ] Toggle the latency stats overlay
- [ ] Move the puck to a comfortable position; it stays there
  across foreground-background-foreground cycles
- [ ] Draw with the S-Pen on top of the puck without triggering it
- [ ] Get visible feedback (toast) when the connection state changes
- [ ] Open Settings and read the connected host info
- [ ] Use the app for 30 minutes without needing to think about its
  UI — meaning the puck genuinely fades into the background

When all of those are real, M4 phase 1 (the UX) is done. Phase 2
(pairing, mDNS, audio) starts after.
