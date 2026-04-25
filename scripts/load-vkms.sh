#!/usr/bin/env bash
# Load vkms with writeback enabled.
# Idempotent: safe to re-run.
set -euo pipefail

if lsmod | grep -q '^vkms '; then
    echo "vkms already loaded"
else
    sudo modprobe vkms enable_writeback=1
    echo "vkms loaded"
fi

echo
echo "Connectors visible to KDE:"
kscreen-doctor -o | grep -E '^Output:'
