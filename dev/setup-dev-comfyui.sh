#!/usr/bin/env bash
# Sets up the development ComfyUI for procraft-diffusion: a separate install that shares nothing
# with any other ComfyUI on the machine. Re-running it is safe; it brings an existing install back
# to the pinned versions. Models are not downloaded: copy them into $ROOT/models/ by hand
# (docs/generation/01-brainstorm.md §4a lists them).
set -euo pipefail

ROOT="${1:-<dev ComfyUI folder>}"
PYTHON_VERSION=3.12
TORCH_INDEX=https://download.pytorch.org/whl/cu130

# ComfyUI and the add-ons at commits known to work together (the same set krita-ai-diffusion 1.54.0
# was tested with)
COMFY_URL=https://github.com/comfyanonymous/ComfyUI
COMFY_COMMIT=e9027f2b30f37bb3052714eb08fcf479542f4fc0
NODES=(
    "comfyui-tooling-nodes https://github.com/Acly/comfyui-tooling-nodes b3ae4aa2d98f6ac4284ddbe261e3559c94bd652b"
    "comfyui-inpaint-nodes https://github.com/Acly/comfyui-inpaint-nodes bd6d8fd5104d539187dfb172d07352215c09fcaa"
    "comfyui_controlnet_aux https://github.com/Fannovel16/comfyui_controlnet_aux e8b689a513c3e6b63edc44066560ca5919c0576e"
)

# keeps uv's cache on the same drive, so packages are hard-linked rather than copied
export UV_CACHE_DIR="$ROOT/.uv-cache"

# Clones (or fetches) `url` into `dir` and checks out `commit`.
checkout() {
    local dir="$1" url="$2" commit="$3"
    if [ ! -d "$dir/.git" ]; then
        git init -q "$dir"
        git -C "$dir" remote add origin "$url"
    fi
    git -C "$dir" fetch -q --depth 1 origin "$commit"
    git -C "$dir" checkout -q --force FETCH_HEAD
}

mkdir -p "$ROOT"
echo "== Python $PYTHON_VERSION"
[ -x "$ROOT/venv/bin/python" ] || uv venv -q --python "$PYTHON_VERSION" "$ROOT/venv"
PY="$ROOT/venv/bin/python"

echo "== ComfyUI $COMFY_COMMIT"
checkout "$ROOT/ComfyUI" "$COMFY_URL" "$COMFY_COMMIT"

echo "== PyTorch (CUDA, $TORCH_INDEX)"
uv pip install -q --python "$PY" torch torchvision torchaudio --index-url "$TORCH_INDEX"

echo "== ComfyUI requirements"
uv pip install -q --python "$PY" -r "$ROOT/ComfyUI/requirements.txt"

for node in "${NODES[@]}"; do
    read -r name url commit <<<"$node"
    echo "== $name $commit"
    checkout "$ROOT/ComfyUI/custom_nodes/$name" "$url" "$commit"
    req="$ROOT/ComfyUI/custom_nodes/$name/requirements.txt"
    [ -f "$req" ] && uv pip install -q --python "$PY" -r "$req"
done

# the model folders the copied files go into
mkdir -p "$ROOT"/ComfyUI/models/{diffusion_models,text_encoders,vae,controlnet,loras}

cp "$(dirname "$0")/start-dev-comfyui.sh" "$ROOT/start.sh"
cp "$(dirname "$0")/stop-dev-comfyui.sh" "$ROOT/stop.sh"
echo "== Done. Start it with $ROOT/start.sh, stop it with $ROOT/stop.sh"
