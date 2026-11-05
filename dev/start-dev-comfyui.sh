#!/usr/bin/env bash
# Starts the development ComfyUI on port 8189, reachable from this machine only. The tablet reaches
# it over USB with `adb reverse tcp:8189 tcp:8189`. Pass `--listen 0.0.0.0` to open it to the LAN
# (anyone on the network can then use it: ComfyUI has no login). Sampling previews are on, for the
# results bar. `stop.sh` stops it.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")" && pwd)"
if [ -f "$ROOT/comfyui.pid" ] && kill -0 "$(cat "$ROOT/comfyui.pid")" 2>/dev/null; then
    echo "Already running (pid $(cat "$ROOT/comfyui.pid"))." >&2
    exit 1
fi
cd "$ROOT/ComfyUI"
# exec keeps this shell's pid, so the file names ComfyUI itself
echo $$ >"$ROOT/comfyui.pid"
exec "$ROOT/venv/bin/python" main.py --port 8189 --disable-auto-launch --preview-method auto "$@"
