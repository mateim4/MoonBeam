#!/usr/bin/env bash
# M0 — empirically verify whether KWin (Plasma 6.6 Wayland) actually
# scans out at 120Hz on a vkms virtual output, or clamps to 60.
#
# Three checks; pass requires Check 1 + Check 3 agreement (Check 2 is sanity).
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RESULTS="$REPO_ROOT/docs/M0-RESULTS.md"
TARGET_W=2960
TARGET_H=1848
TARGET_MHZ=120000

# kscreen-doctor emits ANSI color codes that break awk anchors. Strip them.
ksd() { kscreen-doctor "$@" 2>&1 | sed -E 's/\x1b\[[0-9;]*m//g'; }

echo "==> Loading vkms"
"$REPO_ROOT/scripts/load-vkms.sh"
sleep 1

echo "==> Finding Virtual-* connector"
# Capture once and feed via here-string to avoid SIGPIPE-vs-pipefail issues
# when awk exits early after the first match.
KSD_OUT="$(ksd -o)"
CONNECTOR="$(awk '/^Output:/ {for (i=1;i<=NF;i++) if ($i ~ /^Virtual-/) {print $i; exit}}' <<<"$KSD_OUT")"
if [[ -z "$CONNECTOR" ]]; then
    echo "FAIL: no Virtual-* connector visible to kscreen-doctor" >&2
    echo "Current outputs:" >&2
    ksd -o >&2
    exit 1
fi
echo "    connector = $CONNECTOR"

echo "==> Adding custom mode ${TARGET_W}x${TARGET_H}@$((TARGET_MHZ/1000))Hz"
ksd "output.${CONNECTOR}.addCustomMode.${TARGET_W}.${TARGET_H}.${TARGET_MHZ}.full" || {
    echo "    addCustomMode returned non-zero (mode may already exist; continuing)"
}
sleep 1

echo "==> Locating mode ID for ${TARGET_W}x${TARGET_H}@~120Hz"
# kscreen-doctor rounds 120000mHz to "119.99" — match @11x or @12x to be tolerant.
KSD_OUT="$(ksd -o)"
MODE_ID="$(awk -v c="$CONNECTOR" -v w="$TARGET_W" -v h="$TARGET_H" '
    $0 ~ "Output:.*"c { in_block=1; next }
    in_block && /^Output:/ { in_block=0 }
    in_block {
        for (i=1;i<=NF;i++) {
            if ($i ~ ":"w"x"h"@1[12]") {
                n = split($i, parts, ":")
                print parts[1]
                exit
            }
        }
    }' <<<"$KSD_OUT")"
if [[ -z "$MODE_ID" ]]; then
    echo "FAIL: ${TARGET_W}x${TARGET_H}@120 mode not present after addCustomMode" >&2
    echo "    KWin likely rejected the mode at validation time." >&2
    echo "    Modes for $CONNECTOR:" >&2
    awk -v c="$CONNECTOR" '
        $0 ~ "Output:.*"c { found=1 }
        found && /^Output:/ && !($0 ~ "Output:.*"c) { found=0 }
        found && /Custom modes:|Modes:/' <<<"$KSD_OUT" >&2
    exit 1
fi
echo "    mode_id = $MODE_ID"

echo "==> Switching to mode $MODE_ID and positioning right of primary"
ksd \
    "output.${CONNECTOR}.mode.${MODE_ID}" \
    "output.${CONNECTOR}.position.3840,0" \
    "output.${CONNECTOR}.enable"
sleep 2

# ---- Check 1: kernel-side mode commit (authoritative) ----
echo "==> Check 1: kernel DRM state"
CHECK1="$(sudo bash -c 'cat /sys/kernel/debug/dri/*/state 2>/dev/null' \
    | awk '/connector|crtc/ {block=$0; getline x; block=block"\n"x} /Virtual-1|vkms/ {print block; for(i=0;i<8;i++){getline l; print l}}' \
    | head -60 || true)"
if [[ -z "$CHECK1" ]]; then
    CHECK1="(no debugfs DRM state matched Virtual-1/vkms; debugfs may be unmounted)"
fi

# ---- Check 2: KWin output sanity ----
echo "==> Check 2: KWin output report"
KSD_OUT="$(ksd -o)"
CHECK2="$(awk -v c="$CONNECTOR" '
    $0 ~ "Output:.*"c { found=1 }
    found && /^Output:/ && !($0 ~ "Output:.*"c) { found=0 }
    found {print; n++; if (n>=20) exit}' <<<"$KSD_OUT")"

# ---- Check 3: vblank cadence over 5s (the real test) ----
echo "==> Check 3: counting drm_vblank_event over 5s"
CHECK3=""
if command -v perf >/dev/null 2>&1 && sudo perf list 2>/dev/null | grep -q drm:drm_vblank_event; then
    CHECK3="$(sudo perf stat -e drm:drm_vblank_event -a sleep 5 2>&1 | tail -20)"
elif command -v bpftrace >/dev/null 2>&1; then
    echo "    perf unavailable; using bpftrace"
    CHECK3="$(sudo timeout 6 bpftrace -e \
        'tracepoint:drm:drm_vblank_event { @[args->crtc] = count(); } interval:s:5 { exit(); }' \
        2>&1 | tail -20)"
elif [[ -e /sys/kernel/tracing/events/drm/drm_vblank_event/enable \
     || -e /sys/kernel/debug/tracing/events/drm/drm_vblank_event/enable ]]; then
    echo "    perf/bpftrace unavailable; using tracefs directly"
    TRACE_SCRIPT="$(mktemp)"
    cat >"$TRACE_SCRIPT" <<'TRACE_EOF'
#!/usr/bin/env bash
set -e
if [[ -e /sys/kernel/tracing/trace ]]; then
    TF=/sys/kernel/tracing
else
    TF=/sys/kernel/debug/tracing
fi
echo 0 > "$TF/events/drm/drm_vblank_event/enable" 2>/dev/null || true
: > "$TF/trace"
echo 1 > "$TF/events/drm/drm_vblank_event/enable"
sleep 5
echo 0 > "$TF/events/drm/drm_vblank_event/enable"
echo "=== total drm_vblank_event lines in 5s ==="
grep -c drm_vblank_event "$TF/trace" || true
echo "=== per-crtc breakdown (count crtc=N) ==="
grep -oE 'crtc=[0-9]+' "$TF/trace" | sort | uniq -c | sort -rn
TRACE_EOF
    CHECK3="$(sudo bash "$TRACE_SCRIPT" 2>&1)"
    rm -f "$TRACE_SCRIPT"
else
    CHECK3="(no tracing facility available — install perf or bpftrace)"
fi

# ---- Build results doc ----
echo "==> Writing $RESULTS"
KERNEL="$(uname -r)"
KWIN_VER="$(kwin_wayland --version 2>&1 | head -1 || echo unknown)"
NV_VER="$(nvidia-smi --query-gpu=driver_version --format=csv,noheader 2>/dev/null | head -1 || echo unknown)"
PLASMA_VER="$(plasmashell --version 2>&1 | head -1 || echo unknown)"

cat >"$RESULTS" <<EOF
# M0 — KWin vkms 120Hz commit test

Run: $(date -Is)

## Environment
- Kernel: \`$KERNEL\`
- KWin: \`$KWIN_VER\`
- NVIDIA driver: \`$NV_VER\`
- Plasma: \`$PLASMA_VER\`

## Test parameters
- Connector: \`$CONNECTOR\`
- Mode: ${TARGET_W}x${TARGET_H} @ $((TARGET_MHZ/1000))Hz, kscreen mode id \`$MODE_ID\`

## Check 1 — kernel DRM state (authoritative)
\`\`\`
$CHECK1
\`\`\`

## Check 2 — KWin output report (sanity)
\`\`\`
$CHECK2
\`\`\`

## Check 3 — drm_vblank_event cadence over 5s (real scanout rate)

Pass criterion: ~600 vblanks (120Hz). Clamp-to-60: ~300.

\`\`\`
$CHECK3
\`\`\`

## Verdict

_To be filled in based on Check 3._

EOF

echo
echo "==> Done. Results:"
echo "------------------------------------------------------------"
cat "$RESULTS"
