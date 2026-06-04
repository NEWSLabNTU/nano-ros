#!/usr/bin/env bash
# Phase 212.M-F.8 — render a PX4 SITL board overlay enabling every
# `nros_<name>/` module that `nros codegen-system --ahead-of-vendor
# --target px4` has emitted under `$PX4_AUTOPILOT_DIR/src/modules/`.
#
# Usage:
#   integrations/px4/sitl-overlay/render-overlay.sh \
#       [--px4-dir <path>] \
#       [--output <path>]
#
# Defaults:
#   --px4-dir : $PX4_AUTOPILOT_DIR (or $PX4_DIR)
#   --output  : stdout
#
# The script scans `<px4-dir>/src/modules/nros_*/` for emitted module
# directories and renders the
# `integrations/px4/sitl-overlay/nros.px4board.in` template, expanding
# `@NROS_MODULES_ENABLE@` to one `CONFIG_MODULES_NROS_<UPPER>=y` line
# per discovered module.
#
# The rendered fragment is meant to be **appended** to one of the SITL
# board files PX4 ships, e.g.:
#
#     ./render-overlay.sh >> $PX4_AUTOPILOT_DIR/boards/px4/sitl/default.px4board
#
# or saved to a sibling `nros.px4board` board variant.
#
# Stays out of the vendored PX4 tree by design — the operator drives
# the concatenation step. See `integrations/px4/README.md`.
#
# Future improvement (TODO Phase 212.M-F.8 → in-tree CLI):
# move this rendering into `nros codegen-system --target px4
# --board-overlay <path>`. That would let one `nros` invocation do
# both the module-dir emit AND the overlay write, without the
# operator needing to chain a second tool. The CLI lives in-tree at
# `packages/cli/` since Phase 218 (the standalone
# https://github.com/NEWSLabNTU/nros-cli is archived / read-only).

set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
template="${script_dir}/nros.px4board.in"

px4_dir=""
output=""

while (($#)); do
    case "$1" in
        --px4-dir)
            px4_dir="$2"
            shift 2
            ;;
        --output)
            output="$2"
            shift 2
            ;;
        -h|--help)
            sed -n '2,/^set -euo/p' "$0" | sed 's/^# \{0,1\}//;/^set -euo/d'
            exit 0
            ;;
        *)
            echo "render-overlay.sh: unknown argument: $1" >&2
            exit 2
            ;;
    esac
done

if [[ -z "$px4_dir" ]]; then
    px4_dir="${PX4_AUTOPILOT_DIR:-${PX4_DIR:-}}"
fi
if [[ -z "$px4_dir" ]]; then
    echo "render-overlay.sh: --px4-dir not given and \$PX4_AUTOPILOT_DIR / \$PX4_DIR unset" >&2
    exit 2
fi

modules_dir="${px4_dir}/src/modules"
if [[ ! -d "$modules_dir" ]]; then
    echo "render-overlay.sh: ${modules_dir} not found — not a PX4 checkout?" >&2
    exit 2
fi

# Collect every `nros_<name>/` directory the codegen has emitted.
# `compgen -G` returns 1 (and an empty stdout) when no glob matches;
# tolerate that and render an empty fragment.
shopt -s nullglob
declare -a nros_modules=()
for d in "${modules_dir}"/nros_*/; do
    name="$(basename "${d%/}")"
    # Skip the side-car plan dir/file the codegen drops alongside the
    # modules — only emitted-as-PX4-module dirs need a Kconfig switch.
    case "$name" in
        nros-system|nros-plan.json) continue ;;
    esac
    # Strip leading `nros_` -> uppercase remainder for the Kconfig sym.
    sym_lower="${name#nros_}"
    if [[ -z "$sym_lower" ]]; then
        continue
    fi
    sym_upper="$(echo "$sym_lower" | tr '[:lower:]' '[:upper:]')"
    nros_modules+=("CONFIG_MODULES_NROS_${sym_upper}=y")
done
shopt -u nullglob

# Build the replacement block. When no modules are present, leave a
# comment behind so the operator can see the fragment was rendered
# but found nothing to enable.
if ((${#nros_modules[@]} == 0)); then
    enable_block="# (no nros_<name>/ module dirs found under ${modules_dir})"
else
    enable_block="$(printf '%s\n' "${nros_modules[@]}")"
fi

# Inline-render the template, expanding @NROS_MODULES_ENABLE@. Using
# awk keeps the substitution literal-safe (sed would mis-handle `&`
# / `/` if a module name ever contained one — defensive).
rendered="$(awk -v block="$enable_block" '
    {
        if (index($0, "@NROS_MODULES_ENABLE@") > 0) {
            sub(/@NROS_MODULES_ENABLE@/, block)
        }
        print
    }
' "$template")"

if [[ -n "$output" ]]; then
    printf '%s\n' "$rendered" > "$output"
else
    printf '%s\n' "$rendered"
fi
