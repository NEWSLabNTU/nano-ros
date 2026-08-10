#!/usr/bin/env bash
# phase-344 W7 — RFC-0070 R1, as AMENDED 2026-08-10: scoped by context.
#
# R1 originally said "Nothing writes build output inside a source directory",
# globally. That is wrong for the copy-out examples, and acting on the unscoped
# rule broke the copy-out contract once already (391 per-leaf `.gitignore` files
# were consolidated to the root, after which a copied-out leaf had no ignore at
# all). So this gate enforces the AMENDED rule and nothing wider:
#
#   examples/**   copy-out leaves. Cargo/CMake convention — `target/` and
#                 `build/` beside the source, per-leaf `.gitignore`. NOT FLAGGED.
#                 A user copies the leaf out and it must behave like a normal
#                 Cargo/CMake project.
#
#   workspace     the nano-ros workspace duplicates the COLCON experience
#                 (`build/`, `install/`, `log/`). Build output belongs under
#                 $NROS_BUILD_ROOT, and a stray cache dir beside workspace
#                 SOURCE is what this gate fails on.
#
# Deliberately NOT a byte-saving gate. phase-344 §1.5 measured that relocation
# frees ZERO bytes (83.2 % of the cmake dirs' bytes is corrosion's own cargo
# tree). This exists for R1's stated value — one root, one vocabulary,
# verifiable — and to stop the amended scope drifting back.
set -uo pipefail
cd "$(dirname "$0")/.."

# The workspace SOURCE trees. `examples/workspaces/*/src` holds authored
# packages; their build output belongs under the build root, not beside them.
# `nros sync` output is EXEMPT and owned elsewhere. A synced package carries
# `<pkg>/build/{nros,nros-metadata}/` — SystemModels and the cmake metadata
# probe. It is build output beside source and so is genuinely R1-shaped, but
# phase-330's `model_location` owns where it lands, not phase-344 W7. Flagging
# it here would make this gate red for another phase's debt, which is how a gate
# gets an `|| true` bolted on. Named rather than hidden: this is the same class
# that inflated phase-344's first census by 93 dirs.
mapfile -t hits < <(
    find examples/workspaces/*/src -maxdepth 3 -type d \
        \( -name 'build' -o -name 'build-*' -o -name 'target' -o -name 'target-*' \) \
        2>/dev/null \
    | while read -r d; do
        # a `build/` holding ONLY sync output is not W7's to flag
        if [ "$(basename "$d")" = "build" ]; then
            rest="$(ls -A "$d" 2>/dev/null | grep -vxE 'nros|nros-metadata' | head -1)"
            [ -z "$rest" ] && continue
        fi
        printf '%s\n' "$d"
      done | sort
)

# `third-party/` is out of scope by the RFC's own Open section — vendored trees
# cannot follow R1. Nothing under it is reachable from the find above, but the
# exemption is stated so the next reader does not widen the find and then
# rediscover it.

if [ "${#hits[@]}" -gt 0 ]; then
    echo "[FAIL] build output inside a WORKSPACE source dir (RFC-0070 R1, workspace scope):" >&2
    printf '  %s\n' "${hits[@]}" >&2
    cat >&2 <<'EOF'

The nano-ros workspace follows the colcon shape: build output under
$NROS_BUILD_ROOT (default <repo>/build/), not beside the source.

  derive the path:  scripts/build/build-root.sh -> nros_build_dir <kind> <coord>
  NOT a literal, and NOT a new suffix (R2: "a new ad-hoc suffix is a bug")

If this is a copy-out EXAMPLE rather than a workspace package, it does not
belong under examples/workspaces/*/src — examples/** is deliberately exempt.
EOF
    exit 1
fi

echo "check-workspace-build-output: OK (no build output beside workspace source; examples/** exempt by design)"
