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

echo "==> Loading vkms"
"$REPO_ROOT/scripts/load-vkms.sh"
sleep 1

echo "==> Finding Virtual-* connector"
CONNECTOR="$(kscreen-doctor -o 2>/dev/null \
    | awk '/^Output:/ {for (i=1;i<=NF;i++) if ($i ~ /^Virtual-/) {print $i; exit}}')"
if [[ -z "$CONNECTOR" ]]; then
    echo "FAIL: no Virtual-* connector visible to kscreen-doctor" >&2
    echo "Current outputs:" >&2
    kscreen-doctor -o >&2
    exit 1
fi
echo "    connector = $CONNECTOR"

echo "==> Adding custom mode ${TARGET_W}x${TARGET_H}@$((TARGET_MHZ/1000))Hz"
kscreen-doctor "output.${CONNECTOR}.addCustomMode.${TARGET_W}.${TARGET_H}.${TARGET_MHZ}.full" 2>&1 || {
    echo "    addCustomMode returned non-zero (mode may already exist; continuing)"
}
sleep 1

echo "==> Locating mode ID for ${TARGET_W}x${TARGET_H}@120"
MODE_ID="$(kscreen-doctor -o 2>/dev/null \
    | awk -v c="$CONNECTOR" -v w="$TARGET_W" -v h="$TARGET_H" '
        $0 ~ "Output:.*"c { in_block=1; next }
        in_block && /^Output:/ { in_block=0 }
        in_block {
            for (i=1;i<=NF;i++) {
                if ($i ~ ":"w"x"h"@120") {
                    n = split($i, parts, ":")
                    print parts[1]
                    exit
                }
            }
        }')"
if [[ -z "$MODE_ID" ]]; then
    echo "FAIL: ${TARGET_W}x${TARGET_H}@120 mode not present after addCustomMode" >&2
    echo "    KWin likely rejected the mode at validation time." >&2
    kscreen-doctor -o >&2
    exit 1
fi
echo "    mode_id = $MODE_ID"

echo "==> Switching to mode $MODE_ID and positioning right of primary"
kscreen-doctor \
    "output.${CONNECTOR}.mode.${MODE_ID}" \
    "output.${CONNECTOR}.position.3840,0" \
    "output.${CONNECTOR}.enable" 2>&1
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
CHECK2="$(kscreen-doctor -o 2>&1 | awk -v c="$CONNECTOR" '
    $0 ~ "Output:.*"c { found=1 }
    found && /^Output:/ && !($0 ~ "Output:.*"c) { found=0 }
    found' | head -20)"

# ---- Check 3: vblank cadence over 5s (the real test) ----
echo "==> Check 3: counting drm_vblank_event over 5s"
CHECK3=""
if sudo perf list 2>/dev/null | grep -q drm:drm_vblank_event; then
    CHECK3="$(sudo perf stat -e drm:drm_vblank_event -a sleep 5 2>&1 | tail -20)"
elif command -v bpftrace >/dev/null 2>&1; then
    echo "    perf tracepoint unavailable; falling back to bpftrace"
    CHECK3="$(sudo timeout 6 bpftrace -e \
        'tracepoint:drm:drm_vblank_event { @[args->crtc] = count(); } interval:s:5 { exit(); }' \
        2>&1 | tail -20)"
else
    CHECK3="(neither perf nor bpftrace can read drm:drm_vblank_event tracepoint)"
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
