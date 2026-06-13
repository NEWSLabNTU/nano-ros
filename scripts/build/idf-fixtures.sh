#!/usr/bin/env bash
# Build-stage esp-idf fixtures (issue 0041 — No compilation inside tests).
# Stages an esp-idf example/fixture, sources the IDF env, and runs
# `idf.py set-target <t> && build` into build/idf-fixtures/<id>/, stamping
# `.compile-ok`. Tests resolve the produced ELF instead of running idf.py.
#
# Gated: skips cleanly (no stamp → test skips per tier) when IDF_PATH/idf.py or
# the env shim are unavailable. `-DNANO_ROS_SKIP_BOOTSTRAP=ON` is required — the
# bootstrap re-runs tools/setup.sh which fails offline despite populated
# submodules; NANO_ROS_ROOT must be the repo root so the staged copy resolves
# `integrations/nano-ros`.
set -u

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/../.." && pwd)"
cd "$repo_root"

out_root="$repo_root/build/idf-fixtures"
mkdir -p "$out_root"

# Activate the IDF env (export.sh from IDF_PATH, or the workspace env shim).
if [ -n "${IDF_PATH:-}" ] && [ -f "$IDF_PATH/export.sh" ]; then
    # shellcheck disable=SC1091
    source "$IDF_PATH/export.sh" >/dev/null 2>&1 || true
elif [ -n "${NROS_ESP_IDF_ENV_SHIM:-}" ] && [ -f "$NROS_ESP_IDF_ENV_SHIM" ]; then
    # shellcheck disable=SC1091
    source "$NROS_ESP_IDF_ENV_SHIM" >/dev/null 2>&1 || true
fi

if ! command -v idf.py >/dev/null 2>&1; then
    echo "idf-fixtures: idf.py unavailable (source \$IDF_PATH/export.sh) — skipping" >&2
    exit 0
fi

# id : src-rel : subdir (idf.py runs here, '.' = staged root) : elf-name : target
IDF_FIXTURES=(
    "esp_idf_talker:examples/esp32/rust/talker:.:esp32_bsp_talker:esp32c3"
    "esp_idf_listener:examples/esp32/rust/listener:.:esp32_bsp_listener:esp32c3"
    "esp_idf_bringup:packages/testing/nros-tests/fixtures/multi_pkg_workspace_esp_idf:esp_idf_app:multi_pkg_workspace_esp_idf:esp32c3"
)

n=0
for entry in "${IDF_FIXTURES[@]}"; do
    IFS=':' read -r id src subdir elf target <<< "$entry"
    staged="$out_root/$id"
    [ -d "$repo_root/$src" ] || { echo "idf-fixtures: src missing: $src" >&2; continue; }
    echo "== idf-fixture: $id ($elf @ $target) =="
    rm -rf "$staged"
    mkdir -p "$staged"
    cp -r "$repo_root/$src/." "$staged/"
    # Rewrite @NANO_ROS_ROOT@ (h5's esp_idf_app/CMakeLists uses it; examples don't).
    find "$staged" -type f -exec grep -lZ '@NANO_ROS_ROOT@' {} + 2>/dev/null \
        | xargs -0 -r sed -i "s#@NANO_ROS_ROOT@#$repo_root#g"
    # Issue #44 follow-up — the esp *examples* reference the in-tree workspace
    # crates with parent-relative path deps (`path = "../../../../packages/..."`)
    # sized for their real location under `examples/`. Copied to a different depth
    # under `build/idf-fixtures/<id>/`, those `../` chains escape the repo and
    # cargo fails (`failed to read /home/aeon/repos/packages/...`). Rewrite any
    # `path = "(../)+packages/..."` to the absolute repo root so the staged tree's
    # location no longer matters. Internal relative deps (e.g. a generated crate's
    # `path = "../builtin_interfaces"`) lack the `packages/` anchor and are left
    # untouched, as are the `@NANO_ROS_ROOT@` fixture templates.
    find "$staged" -name Cargo.toml -print0 2>/dev/null \
        | xargs -0 -r sed -E -i "s#path = \"(\.\./)+packages/#path = \"$repo_root/packages/#g"
    rm -f "$staged/.compile-ok"
    (
        cd "$staged/$subdir"
        NANO_ROS_ROOT="$repo_root" idf.py -B build -DNANO_ROS_SKIP_BOOTSTRAP=ON set-target "$target"
        NANO_ROS_ROOT="$repo_root" idf.py -B build build
    )
    if [ -f "$staged/$subdir/build/$elf.elf" ]; then
        date -u +%Y-%m-%dT%H:%M:%SZ > "$staged/.compile-ok"
        echo "   built $staged/$subdir/build/$elf.elf"
        n=$((n + 1))
    else
        echo "   idf build produced no $elf.elf (no stamp; test will report)" >&2
    fi
done
echo "idf fixtures built ($n/${#IDF_FIXTURES[@]})."
