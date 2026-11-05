#!/usr/bin/env bash
# Stops the development ComfyUI that start.sh started: asks it to finish (Ctrl+C), waits, and only
# then insists. Only ever touches the ComfyUI running from this folder's own Python.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")" && pwd)"
PY="$ROOT/venv/bin/python"

# The dev ComfyUI's pid: from start.sh's file if it still names it, otherwise found by its command
# line (an instance started before there was a pid file).
pid=""
if [ -f "$ROOT/comfyui.pid" ]; then
    pid="$(cat "$ROOT/comfyui.pid")"
    if ! tr '\0' ' ' <"/proc/$pid/cmdline" 2>/dev/null | grep -qF "$PY main.py"; then
        pid=""
    fi
fi
[ -n "$pid" ] || pid="$(pgrep -f -x "$PY main.py .*" | head -n 1 || true)"

if [ -z "$pid" ]; then
    echo "Not running."
    rm -f "$ROOT/comfyui.pid"
    exit 0
fi

# Waits up to $1 seconds for the process to end; succeeds once it has.
gone_within() {
    for _ in $(seq "$(($1 * 4))"); do
        kill -0 "$pid" 2>/dev/null || return 0
        sleep 0.25
    done
    return 1
}

echo "Stopping ComfyUI (pid $pid)..."
kill -INT "$pid" 2>/dev/null || true
if ! gone_within 20; then
    echo "Still running; sending TERM."
    kill -TERM "$pid" 2>/dev/null || true
    if ! gone_within 10; then
        echo "Still running; sending KILL."
        kill -KILL "$pid" 2>/dev/null || true
        gone_within 5 || { echo "Could not stop pid $pid." >&2; exit 1; }
    fi
fi
rm -f "$ROOT/comfyui.pid"
echo "Stopped."
