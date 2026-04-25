#!/usr/bin/env bash
# Load vkms with writeback enabled.
# Idempotent: safe to re-run.
set -euo pipefail

if lsmod | grep '^vkms ' >/dev/null; then
    echo "vkms already loaded"
else
    sudo modprobe vkms enable_writeback=1
    echo "vkms loaded"
fi

echo
echo "Connectors visible to KDE:"
# Strip ANSI color escapes; tolerate no matches.
kscreen-doctor -o 2>/dev/null | sed -E 's/\x1b\[[0-9;]*m//g' | grep -E '^Output:' || true
