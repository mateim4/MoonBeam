#!/usr/bin/env bash
# M0 cleanup: disable the vkms output and unload the module.
set -euo pipefail

ksd() { kscreen-doctor "$@" 2>&1 | sed -E 's/\x1b\[[0-9;]*m//g'; }

CONNECTOR="$(ksd -o \
    | awk '/^Output:/ {for (i=1;i<=NF;i++) if ($i ~ /^Virtual-/) {print $i; exit}}' || true)"

if [[ -n "$CONNECTOR" ]]; then
    echo "==> Disabling $CONNECTOR"
    ksd "output.${CONNECTOR}.disable" || true
    sleep 1
fi

if lsmod | grep '^vkms ' >/dev/null; then
    echo "==> Unloading vkms"
    sudo modprobe -r vkms 2>&1 || {
        echo "    failed to unload vkms (still in use); reboot may be needed" >&2
        exit 1
    }
else
    echo "vkms not loaded"
fi

echo "==> Done"
