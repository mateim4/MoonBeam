# AGENTS.md — guidance for AI coding agents

This repo is built with AI agent collaboration as a first-class workflow.
This file is the durable policy that issue briefs reference. Read it
before starting any task.

## Roles

- **Jules** (Google Labs Jules, `google-labs-jules[bot]`) — the
  primary delegated implementer. Triggered by adding the `jules`
  label to an issue. Works in an Ubuntu VM **without an Android SDK**.
- **Claude** — the architect / reviewer / human-side build verifier.
  Drives planning, files issues, reviews PRs, runs local builds Jules
  cannot, ships fixes on top of Jules' branches when the gap is small.
- **Human** (the repo owner) — final approver, merger, hardware tester.

## Build environments — what each role can run

| Command | Jules | Claude (local) | Human |
|---|---|---|---|
| `./gradlew :protocol:test` (pure JVM, multiplatform protocol layer) | ✅ | ✅ | ✅ |
| `./gradlew :app:assembleDebug` (Android app, requires SDK) | ❌ | ✅ | ✅ |
| Real-device testing on Galaxy Tab S11 Ultra | ❌ | ❌ | ✅ |
| Linux host stack (`moonbeamd`, vkms, NVENC) | ❌ | partial | ✅ |

Implications:

- **Jules cannot detect Android-side build breaks.** The phase 1 PR
  shipped two latent breaks (compileSdk + Expressive API gating)
  because file review can't catch what only the AGP manifest merger
  + kotlin compiler can.
- **Claude cannot detect on-device behavioral regressions.** Compile
  green ≠ feature works. Anything pen-related, gesture-related, or
  multi-touch needs the human + tablet.

## PR conventions

### `[BUILD-UNVERIFIED]` title prefix

Any agent that authored a PR without running `:app:assembleDebug` (or
the equivalent for whatever module changed) **must** prefix the PR
title with `[BUILD-UNVERIFIED]`. The prefix is removed by whoever
verifies the build green.

This is load-bearing: it's the first signal during review.

### Dependency-change disclosure

If a PR introduces, removes, or version-bumps any artifact in
`gradle/libs.versions.toml` or any module's `build.gradle.kts`, the
PR description must state:

- New `compileSdk` requirement (if the change forces a bump)
- New `minSdk` requirement (if the change forces a bump)
- Whether the change pulls in a transitive that uses unstable /
  internal APIs

Example: "BOM 2025.06.00 brings Compose UI 1.8.x → requires
compileSdk ≥ 35. MaterialExpressiveTheme is `internal` in this BOM."

A PR that bumps a dep without this disclosure block will be asked
to add it before review proceeds.

### Verification line

End every PR description with what was actually run:

```
Verified:
- ./gradlew :protocol:test → BUILD SUCCESSFUL
- :app:assembleDebug → not run (no Android SDK in this environment)
```

Lying about verification is the worst possible failure mode. "Did
not run" is always a valid answer.

## Branch and commit conventions

- Feature branches: `feat/<scope>-<short-desc>` (Jules appends a
  numeric task id automatically; that's fine).
- Commit messages: imperative mood, body explains *why* not *what*,
  Co-Authored-By trailer for collaboration.
- One PR = one phase of work per `docs/M4-ANDROID-UX.md` §13 phasing
  plan. Do not bundle phases.

## Where the spec lives

- **`docs/ROADMAP.md`** — milestone state, source of truth for
  "what's done."
- **`docs/MOONBEAM-APP-PLAN.md`** — Android app v0 scope and
  product principles.
- **`docs/M4-ANDROID-UX.md`** — current milestone's UX spec, §13
  has the PR-by-PR phasing plan that drives issue creation.
- **`docs/ARCHITECTURE.md`** — host-side stack and data flow.
- **`docs/M*-*.md`** — per-step implementation notes from finished
  milestones; useful for "what did we actually do and why."

A code change that diverges from the spec must update the spec in
the same PR. The spec is not aspirational — it tracks reality.

## Issue brief expectations

Issues filed for delegation should include:

1. **Build-environment caveat** (link or paste the relevant rules
   above) so the agent doesn't claim verification it can't perform.
2. **Goal** — one paragraph, references spec section.
3. **Scope (in)** — explicit list, sized for one PR.
4. **Scope (out)** — what's deliberately excluded so the agent
   doesn't drift into adjacent phases.
5. **Acceptance** — checkboxes the human + reviewer can tick.
6. **Files you'll touch** — anchors the agent to the right
   directory layout.

## Out of scope for agents (without explicit human approval)

- Changes to `LICENSE`, `README.md`, `AGENTS.md` itself, or any
  `docs/*` file beyond updating the spec the PR is implementing.
- Any change to the host-side Rust crates (`host/`, `protocol/`'s
  Rust bindings if added) unless the issue explicitly says so —
  Android UX work should not silently rewrite the wire protocol.
- Adding new top-level Gradle modules.
- Adding DI frameworks (Hilt, Koin) — single-screen app, not needed.
- Adding navigation libraries — single-screen app, not needed.
