# M3 step 1 — Android project scaffold

Run: 2026-04-30

## What this step is

The scaffolding step that proves we can build a debug APK from CLI for
the M3 Android client. No video, no input, no networking yet — just a
minimal Compose shell over a black surface, plus the `:protocol`
module containing the Kotlin twin of the Rust-side `InputMsg`.

This is the first milestone where Android code exists at all.

## Layout

```
android/
  settings.gradle.kts      # :app + :protocol
  build.gradle.kts          # root, plugins declared "apply false"
  gradle.properties
  gradle/libs.versions.toml # version catalog (single source of truth)
  gradle/wrapper/           # pinned Gradle 8.11.1
  gradlew, gradlew.bat
  local.properties          # gitignored, points at the SDK
  .gitignore
  app/
    build.gradle.kts        # com.android.application + Compose
    src/main/AndroidManifest.xml
    src/main/java/com/m151/moonbeam/MainActivity.kt
    src/main/res/values/{strings,themes}.xml
  protocol/
    build.gradle.kts        # plain Kotlin JVM library + serialization
    src/main/kotlin/com/m151/moonbeam/protocol/InputMsg.kt
    src/main/kotlin/com/m151/moonbeam/protocol/Wire.kt
    src/test/kotlin/com/m151/moonbeam/protocol/WireTest.kt
```

## Tech stack (locked, per the architectural review)

| Layer | Choice | Vendor |
|---|---|---|
| Language | Kotlin 2.1.0 | JetBrains |
| Build | Gradle 8.11.1 (wrapper) + AGP 8.7.3 | Gradle Inc + Google |
| UI | Jetpack Compose (Compose BOM 2024.12.01) | Google |
| Video surface (planned) | `SurfaceView` wrapped in `AndroidView` | Google |
| Video decoder (planned) | `MediaCodec` direct | Google |
| WebSocket | OkHttp 4.12.0 | Square |
| JSON | kotlinx.serialization 1.7.3 | JetBrains |
| Async | kotlinx.coroutines 1.9.0 | JetBrains |

Identifiers:

- `applicationId` and `namespace`: `com.m151.moonbeam`
- `minSdk`: 30 (Android 11) — backwards-compat for older tablets
- `targetSdk` / `compileSdk`: 34 (Android 14) — Tab S11 Ultra ships
  with this
- `versionCode` / `versionName`: `1` / `0.1.0-m3`

## Decisions captured

- **Two modules from day one**: `:app` (Android) and `:protocol` (plain
  Kotlin JVM). Reasoning: the protocol code has no Android dependency
  (no `android.*` imports), so it can be unit-tested with plain JUnit
  on the JVM without an Android emulator. Also positions us for a
  future iPad client via Kotlin Multiplatform — `:protocol` becomes
  multiplatform if/when that ships.
- **Version catalog (`gradle/libs.versions.toml`)** as the single
  source of truth for dependency versions. Bumps live in one file.
- **`encodeDefaults = true` on the JSON instance.** Rust's `serde`
  always emits every field; Kotlin's `kotlinx.serialization` omits
  default-valued fields by default. We force-emit defaults so the
  Kotlin → Rust wire bytes are byte-identical to what
  probe-input-test-client produces.
- **`namespace == applicationId`.** No build-flavour split for now
  (debug/release/staging). One identity, sideloaded only.
- **Fullscreen landscape, system rotation disabled.** Per
  `MOONBEAM-APP-PLAN.md` §4: orientation is host-driven, not
  tablet-driven. The activity locks landscape and sets
  `configChanges` so KWin re-declaring the virtual output's transform
  doesn't recreate the activity.
- **`mediaPlayback` foreground service type** (declared in dependencies
  but not yet used — the M3 step 2 service will pick it up).
- **No launcher icon shipped yet.** Manifest references removed; the
  app will use Android's default icon. Real icon is M4 polish.

## Things that compile and pass right now

- `:protocol:test` — all 4 round-trip tests pass:
  - `pen_down` encodes with snake_case fields and `"type":"pen_down"`
  - `pen_up` encodes as just the discriminator
  - `touch_down` round-trips defaults (catches the `encodeDefaults`
    drift between Kotlin and Rust)
  - `pen_button` uses lowercase enum values (`"button":"stylus"`)
- `:app:assembleDebug` — produces a 24 MB `app-debug.apk`

## Things that don't exist yet (M3 steps 2+)

- WebSocket client (OkHttp) connecting to host
- MediaCodec H.264 decoder hooked to a `SurfaceView`
- Foreground service holding the connection alive
- Touch / pen `MotionEvent` capture and forwarding via `Wire.encodeInput`
- `adb reverse tcp:7878 tcp:7878` workflow doc
- Latency measurement

## Build cheatsheet

```sh
cd android
JAVA_HOME=/usr/lib/jvm/java-17-openjdk ./gradlew :protocol:test
JAVA_HOME=/usr/lib/jvm/java-17-openjdk ./gradlew :app:assembleDebug
# Install to a connected device:
~/Android/Sdk/platform-tools/adb install -r app/build/outputs/apk/debug/app-debug.apk
```

## System under test

- Kernel: 6.19.6-arch1-3-g14
- JDK: openjdk 17.0.18 (`/usr/lib/jvm/java-17-openjdk`)
- Gradle: 8.11.1 via wrapper (system Gradle is 9.4.1, ignored)
- Android Gradle Plugin: 8.7.3
- Android SDK: at `~/Android/Sdk`, platform-tools 37, build-tools 34.0.0,
  platforms;android-34
