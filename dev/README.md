# Development setup

## The dev ComfyUI

A separate ComfyUI for testing, in a folder of its own: `setup-dev-comfyui.sh <folder>` (or
`PROCRAFT_COMFY_ROOT=<folder>`). Personal details (paths, machine, devices) go in `dev/local.md`, which
git ignores; never in tracked files, as the repository is public. It shares nothing with
the Krita ComfyUI (`~/.local/share/krita/ai_diffusion/server`), which is never touched.

- `setup-dev-comfyui.sh [root]` installs it (or brings it back to the pinned versions): Python 3.12 in
  a `venv`, ComfyUI and the add-ons at pinned commits, PyTorch for CUDA 13.0, and `start.sh`.
  uv's cache lives in `<root>/.uv-cache`, on the same drive.
- `<root>/start.sh` starts it on **port 8189**, on this machine only, in the foreground (Ctrl+C
  stops it). Extra arguments go to ComfyUI (`--listen 0.0.0.0` opens it to the LAN; ComfyUI has no
  login). It writes `comfyui.pid` and refuses to start a second copy. In the background:
  `nohup <root>/start.sh > <root>/comfyui.log 2>&1 &`.
- `<root>/stop.sh` stops it cleanly from any terminal: Ctrl+C first, then TERM after 20 s, KILL
  after 10 s more. It only matches the ComfyUI run by this folder's own Python, never another one.
- Models are copied in by hand to `<root>/ComfyUI/models/` (list in `docs/generation/01-brainstorm.md` §4a).
- The drive it's on must be mounted before starting.

Add-ons installed: comfyui-tooling-nodes, comfyui-inpaint-nodes, comfyui_controlnet_aux.

As installed on 2026-11-05: ComfyUI 0.38.0, Python 3.12, PyTorch 2.14.1+cu130, 17 GB before models.
`setup.log` and `comfyui.log` sit next to `start.sh`.

## The tablet

The test tablet: Android 16 / API 36, arm64-v8a, 2800 × 2000 at
400 dpi (1120 × 800 dp in landscape), MediaTek Dimensity 9000.

- `adb` is Google's platform-tools in `~/.local/share/android-platform-tools/`, linked from
  `~/.local/bin/adb`.
- Reaching the dev ComfyUI over USB: `adb reverse tcp:8189 tcp:8189`, then `http://127.0.0.1:8189`
  on the tablet is the PC's ComfyUI. The forward lasts until the cable is unplugged or adb restarts.
- Checking it from the tablet's shell (its `nc` hangs up when its input ends, so keep it open):
  `adb shell '(printf "GET /system_stats HTTP/1.0\r\n\r\n"; sleep 3) | nc 127.0.0.1 8189'`
