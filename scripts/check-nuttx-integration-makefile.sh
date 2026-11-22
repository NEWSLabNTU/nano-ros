#!/usr/bin/env bash
# The NuttX integration shell's Makefile, read statically.
#
# Until phase-550 W1 it was on a path NOTHING in CI executed.
#
# `just nuttx build` builds the KERNEL. The nano-ros app only compiles when
# `CONFIG_NROS=y`, and no shipped defconfig sets it — `git grep CONFIG_NROS=y
# packages/boards` returns nothing for NuttX. So the flow the book documents
# (set CONFIG_NROS, `cd $NUTTX_DIR && make`) was not exercised by any lane, and
# in the two weeks before 2026-08-13 it accumulated THREE independent breakages,
# each of which failed the user's very first build:
#
#   1. `--manifest-path $(NROS_REPO_DIR)/packages/core/nros-c` — nros-c and
#      nros-cpp moved to `packages/api/` on 2026-07-31 (phase-321 W2.e).
#   2. `CFLAGS += ${INCDIR_PREFIX}$(NROS_REPO_DIR)/…` placed NINE LINES ABOVE
#      the `NROS_REPO_DIR :=` that defines it. NuttX's CFLAGS is
#      simply-expanded, so `+=` expanded an empty variable.
#   3. an absolute `CSRCS` under a `PREFIX` whose parent nothing creates.
#
# This gate covers (1) and (2), which are the two that a static reading can
# settle: every `$(NROS_REPO_DIR)/<path>` must exist, and every make variable
# used in a `+=` must be defined before that line.
#
# It is deliberately NOT a substitute for running the flow, and (3) needs a
# real compile: `just nuttx build-make-road` builds the road on both boards in
# its own trees, daily (`nightly.yml`, job `nuttx-make-road`; phase-550 W1).
set -uo pipefail
cd "$(dirname "$0")/.."

fail=0
for mk in integrations/nuttx/Makefile; do
    [ -f "$mk" ] || continue

    # --- (1) every repo path the Makefile names must exist ------------------
    # Two shapes, and the SECOND is the one that actually broke. A bare
    # `$(NROS_REPO_DIR)/<literal>` is checkable directly; but the cargo call
    # sites compose the manifest path from a macro's TWO arguments —
    #   $(call NROS_CARGO_BUILD,<crate>,$(NROS_REPO_DIR)/<dir>,<features>)
    #     -> --manifest-path <dir>/<crate>/Cargo.toml
    # so checking `<dir>` alone passes while `<dir>/<crate>` does not exist.
    # That is exactly how `packages/core` survived here after nros-c moved to
    # `packages/api`: `packages/core` is still a real directory.
    # A path inside `$(wildcard …)` is OPTIONAL BY CONSTRUCTION — the Makefile
    # names it precisely because it may be absent and a fallback follows (the
    # CLI resolution chain: in-tree build → ~/.nros/bin → PATH). Demanding it
    # exist fails every pristine checkout, i.e. exactly the tree `check-fast`
    # documents itself green on — which is how this gate held pr-checks red
    # from dd33eef35 until 2026-08-20 while every dev machine passed.
    # Space-joined so membership below is a bash `case` rather than a `grep -q`
    # forked once per path: issue 0726 is about a checker drawing a verdict from
    # a grep that failed to START, and not forking is a better answer than
    # handling it. Exact-match is preserved — the charset matched above excludes
    # spaces, so a space-delimited test cannot straddle two entries.
    exempt="$(grep -oE '\$\(wildcard \$\(NROS_REPO_DIR\)/[A-Za-z0-9._/-]+' "$mk" \
             | sed 's|^\$(wildcard \$(NROS_REPO_DIR)/||' | sort -u | tr '\n' ' ')"
    while IFS= read -r rel; do
        [ -n "$rel" ] || continue
        case " $exempt " in
            *" $rel "*) continue ;;
        esac
        if [ ! -e "$rel" ]; then
            echo "  $mk names \$(NROS_REPO_DIR)/$rel — which does not exist" >&2
            fail=1
        fi
    done < <(grep -oE '\$\((NROS_REPO_DIR|NANO_ROS_ROOT_DEFS)\)/[A-Za-z0-9._/-]+' "$mk" \
             | sed -E 's#^\$\((NROS_REPO_DIR|NANO_ROS_ROOT_DEFS)\)/##' | sort -u)

    while IFS= read -r composed; do
        [ -n "$composed" ] || continue
        if [ ! -f "$composed/Cargo.toml" ]; then
            echo "  $mk builds \`$composed\` but $composed/Cargo.toml does not exist" >&2
            fail=1
        fi
    done < <(grep -oE '\$\(call [A-Z_]*CARGO_BUILD,[a-z0-9_-]+,\$\(NROS_REPO_DIR\)/[A-Za-z0-9._/-]+' "$mk" \
             | sed -E 's|^\$\(call [A-Z_]*CARGO_BUILD,([a-z0-9_-]+),\$\(NROS_REPO_DIR\)/(.*)$|\2/\1|' | sort -u)

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

    # --- (3) a clean goal parses without the ROS edition (issue 2807) -------
    # The apps tree's clean/distclean parse this file under whatever .config
    # the tree holds; on an arch switch that may not source our Kconfig, and a
    # refusal there stopped the kernel distclean part-way, leaving the other
    # arch's objects to be archived. Evaluated, not grepped: the edition block
    # (from its assignment through the `endif`s closing its `$(error)`) runs
    # under GNU make with an EMPTY edition for each goal shape.
    block="$(awk '/^NROS_ROS_EDITION :=/{on=1} on{print} on&&/CONFIG_NROS_ROS_EDITION is unset/{err=1; next}
                  err&&!/^endif$/{exit}' "$mk")"
    if [ -z "$block" ] || [[ "$block" != *'CONFIG_NROS_ROS_EDITION is unset'* ]]; then
        echo "  $mk: cannot find the ROS-edition refusal block to evaluate" >&2
        fail=1
    else
        probe="$(mktemp)"
        printf 'CONFIG_NROS_ROS_EDITION ?=\n%s\nall clean distclean context:\n\t@:\n' \
            "$block" > "$probe"
        for goal in clean distclean; do
            if ! make -s -f "$probe" "$goal" >/dev/null 2>&1; then
                echo "  $mk: '$goal' with no CONFIG_NROS_ROS_EDITION refuses; a clean" \
                     "goal must parse without it (issue 2807)" >&2
                fail=1
            fi
        done
        for goal in "" context "clean context"; do
            # shellcheck disable=SC2086 # word-split goals on purpose
            if make -s -f "$probe" $goal >/dev/null 2>&1; then
                echo "  $mk: '${goal:-<default>}' with no CONFIG_NROS_ROS_EDITION builds;" \
                     "a building goal must refuse (RFC-0056 wire mismatch)" >&2
                fail=1
            fi
        done
        if ! make -s -f "$probe" CONFIG_NROS_ROS_EDITION='"humble"' >/dev/null 2>&1; then
            echo "  $mk: a set CONFIG_NROS_ROS_EDITION still refuses" >&2
            fail=1
        fi
        rm -f "$probe"
    fi
done

if [ "$fail" -ne 0 ]; then
    cat >&2 <<'EOF'

The NuttX integration Makefile is broken for the flow in
book/src/guide/platforms/nuttx.md.

  paths:  a repo layout move (packages/core -> packages/api) does not
          reach this file automatically — it names paths as text.
  order:  NuttX's CFLAGS is simply-expanded, so `+=` reads the variable's
          value AT THAT LINE. Define before use.

Verify the whole flow with:  just nuttx build-make-road
EOF
    exit 1
fi
echo "check-nuttx-integration-makefile: OK (paths exist; no use-before-define in '+='; a clean goal needs no edition)"
