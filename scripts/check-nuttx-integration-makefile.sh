#!/usr/bin/env bash
# The NuttX integration shell's Makefile is on a path NOTHING in CI executes.
#
# `just nuttx build` builds the KERNEL. The nano-ros app only compiles when
# `CONFIG_NROS=y`, and no shipped defconfig sets it — `git grep CONFIG_NROS=y
# packages/boards` returns nothing for NuttX. So the flow the book documents
# (set CONFIG_NROS, `cd $NUTTX_DIR && make`) is not exercised by any lane, and
# in the two weeks before 2026-08-13 it accumulated THREE independent breakages,
# each of which failed the user's very first build:
#
#   1. `--manifest-path $(NANO_ROS_ROOT)/packages/core/nros-c` — nros-c and
#      nros-cpp moved to `packages/api/` on 2026-07-31 (phase-321 W2.e).
#   2. `CFLAGS += ${INCDIR_PREFIX}$(NANO_ROS_ROOT)/…` placed NINE LINES ABOVE
#      the `NANO_ROS_ROOT :=` that defines it. NuttX's CFLAGS is
#      simply-expanded, so `+=` expanded an empty variable.
#   3. an absolute `CSRCS` under a `PREFIX` whose parent nothing creates.
#
# This gate covers (1) and (2), which are the two that a static reading can
# settle: every `$(NANO_ROS_ROOT)/<path>` must exist, and every make variable
# used in a `+=` must be defined before that line. (3) needs a real compile and
# is covered by the build recipe below, not here.
#
# It is deliberately NOT a substitute for running the flow. The honest fix is a
# lane that sets CONFIG_NROS and builds; `just nuttx build-integration-app` is
# that build, kept out of `check-fast` because it needs the NuttX toolchain and
# mutates the shared tree's `.config` (issue 0525), which a fast gate must not
# do. Run it before releasing anything that touches these Makefiles.
set -uo pipefail
cd "$(dirname "$0")/.."

fail=0
for mk in integrations/nuttx/Makefile integrations/nuttx/apps-external-template/Makefile; do
    [ -f "$mk" ] || continue

    # --- (1) every repo path the Makefile names must exist ------------------
    # Two shapes, and the SECOND is the one that actually broke. A bare
    # `$(NANO_ROS_ROOT)/<literal>` is checkable directly; but the cargo call
    # sites compose the manifest path from a macro's TWO arguments —
    #   $(call NROS_CARGO_BUILD,<crate>,$(NANO_ROS_ROOT)/<dir>,<features>)
    #     -> --manifest-path <dir>/<crate>/Cargo.toml
    # so checking `<dir>` alone passes while `<dir>/<crate>` does not exist.
    # That is exactly how `packages/core` survived here after nros-c moved to
    # `packages/api`: `packages/core` is still a real directory.
    while IFS= read -r rel; do
        [ -n "$rel" ] || continue
        if [ ! -e "$rel" ]; then
            echo "  $mk names \$(NANO_ROS_ROOT)/$rel — which does not exist" >&2
            fail=1
        fi
    done < <(grep -oE '\$\(NANO_ROS_ROOT\)/[A-Za-z0-9._/-]+' "$mk" \
             | sed 's|^\$(NANO_ROS_ROOT)/||' | sort -u)

    while IFS= read -r composed; do
        [ -n "$composed" ] || continue
        if [ ! -f "$composed/Cargo.toml" ]; then
            echo "  $mk builds \`$composed\` but $composed/Cargo.toml does not exist" >&2
            fail=1
        fi
    done < <(grep -oE '\$\(call [A-Z_]*CARGO_BUILD,[a-z0-9_-]+,\$\(NANO_ROS_ROOT\)/[A-Za-z0-9._/-]+' "$mk" \
             | sed -E 's|^\$\(call [A-Z_]*CARGO_BUILD,([a-z0-9_-]+),\$\(NANO_ROS_ROOT\)/(.*)$|\2/\1|' | sort -u)

    # --- (2) no `+=` may use a variable this file defines LATER -------------
    # Simply-expanded `+=` takes the value at that line, so a use-before-define
    # silently appends an empty string rather than failing.
    while IFS= read -r line; do
        n="${line%%:*}"
        for var in $(printf '%s' "${line#*:}" | grep -oE '\$\(([A-Z_][A-Z0-9_]*)\)' \
                     | tr -d '$()' | sort -u); do
            def="$(grep -nE "^${var}[[:space:]]*[:?]?=" "$mk" | head -1 | cut -d: -f1)"
            [ -n "$def" ] || continue
            if [ "$def" -gt "$n" ]; then
                echo "  $mk:$n uses \$($var) in a '+=', but it is defined later at line $def" >&2
                fail=1
            fi
        done
    done < <(grep -nE '^[A-Za-z_][A-Za-z0-9_]*[[:space:]]*\+=' "$mk")
done

if [ "$fail" -ne 0 ]; then
    cat >&2 <<'EOF'

The NuttX integration Makefile is broken for the flow in
book/src/getting-started/integration-nuttx.md, and no lane builds it.

  paths:  a repo layout move (packages/core -> packages/api) does not
          reach this file automatically — it names paths as text.
  order:  NuttX's CFLAGS is simply-expanded, so `+=` reads the variable's
          value AT THAT LINE. Define before use.

Verify the whole flow with:  just nuttx build-integration-app
EOF
    exit 1
fi
echo "check-nuttx-integration-makefile: OK (paths exist; no use-before-define in '+=')"
