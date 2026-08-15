#!/usr/bin/env bash
# scripts/zephyr/cargo-features-patch.sh
#
# Phase 168.1 — patch zephyr-lang-rust's
# `modules/lang/rust/CMakeLists.txt` to honor a CMake-set
# `EXTRA_CARGO_ARGS` variable, so per-example CMakeLists.txt can
# inject `--no-default-features --features rmw-<x>` based on the
# Kconfig RMW choice (CONFIG_NROS_RMW_<X>=y).
#
# Upstream has TODOs noting the missing pass-through — this patch fills the gap.
#
# Idempotent: detects each injected block via grep and only adds missing
# blocks.
#
# ONE INJECTION SITE, and hunk 3 enforces it (issue 0544).
#
# The pass-through goes INSIDE `add_cargo_target_with_zephyr_env`, which every
# caller routes through, so one injection covers `cargo build` and `cargo doc`
# both. An earlier revision of this comment claimed the awk matched "two such
# lines: cargo build (~199) and cargo doc (~243)" — that described an upstream
# layout that has since been refactored into the shared function, so the awk
# matches ONE line and always did the whole job.
#
# Someone reading that stale comment concluded the pass-through was only half
# applied and hand-added `${EXTRA_CARGO_ARGS}` to BOTH call sites
# (`CARGO_ARGS build ${EXTRA_CARGO_ARGS}` / `CARGO_ARGS doc ...`). Those edits
# are in no tracked producer, and hunk 2's guard greps only its OWN marker, so
# it could not see them. Function-level injection PLUS caller-level injection
# put the flag on the command twice, and cargo 1.97.1 rejects that outright:
#
#     error: the argument '--no-default-features' cannot be used multiple times
#
# Every Zephyr Rust leaf failed at `cargo build`, taking the zephyr fixture
# module — and `ci-matrix` — down, while the C/C++ lanes stayed green because
# they never see EXTRA_CARGO_ARGS.
#
# So this script now REPAIRS as well as applies: hunk 3 strips caller-level
# copies. A workspace that already carries them is fixed by re-running setup,
# rather than staying broken until someone re-derives the diagnosis.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
NANO_ROS_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
IN_TREE_WORKSPACE="$NANO_ROS_ROOT/zephyr-workspace"
LEGACY_WORKSPACE="$(cd "$NANO_ROS_ROOT/.." && pwd)/nano-ros-workspace"

if [ -n "${1:-}" ]; then
    WORKSPACE="$1"
elif [ -n "${NROS_ZEPHYR_WORKSPACE:-}" ]; then
    WORKSPACE="$NROS_ZEPHYR_WORKSPACE"
elif [ -d "$IN_TREE_WORKSPACE/zephyr" ]; then
    WORKSPACE="$IN_TREE_WORKSPACE"
else
    WORKSPACE="$LEGACY_WORKSPACE"
fi

CMAKE_FILE="$WORKSPACE/modules/lang/rust/CMakeLists.txt"
if [ ! -f "$CMAKE_FILE" ]; then
    echo "ERROR: $CMAKE_FILE missing" >&2
    exit 1
fi

changed=0

if ! grep -q "nano-ros: NROS_CARGO_PROFILE override" "$CMAKE_FILE"; then
    # Override zephyr-lang-rust's CONFIG_DEBUG-derived cargo profile
    # with the repository-wide profile knob. This keeps the output path
    # (`RUST_BUILD_TYPE`) and cargo args (`rust_build_type_arg`) in sync.
    TMP="$(mktemp)"
    awk '
    {
        print
        if ($0 ~ /^[[:space:]]+set\(rust_build_type_arg "--release"\)[[:space:]]*$/) {
            seen_release=1
        }
        if (seen_release && $0 ~ /^[[:space:]]+endif\(\)[[:space:]]*$/ && !done) {
            print "  # nano-ros: NROS_CARGO_PROFILE override."
            print "  # Empty/unset preserves upstream CONFIG_DEBUG behavior."
            print "  set(_nros_cargo_profile \"$ENV{NROS_CARGO_PROFILE}\")"
            print "  if(NOT _nros_cargo_profile STREQUAL \"\")"
            print "    if(_nros_cargo_profile STREQUAL \"dev\")"
            print "      set(RUST_BUILD_TYPE \"debug\")"
            print "      set(rust_build_type_arg \"\")"
            print "    elseif(_nros_cargo_profile STREQUAL \"release\")"
            print "      set(RUST_BUILD_TYPE \"release\")"
            print "      set(rust_build_type_arg \"--release\")"
            print "    else()"
            print "      set(RUST_BUILD_TYPE \"${_nros_cargo_profile}\")"
            print "      set(rust_build_type_arg --profile ${_nros_cargo_profile})"
            print "    endif()"
            print "  endif()"
            done=1
            seen_release=0
        }
    }
    ' "$CMAKE_FILE" > "$TMP"
    mv "$TMP" "$CMAKE_FILE"
    changed=1
fi

if ! grep -q "nano-ros: EXTRA_CARGO_ARGS pass-through" "$CMAKE_FILE"; then
    # Inject ${EXTRA_CARGO_ARGS} immediately after the line containing only
    # `${rust_build_type_arg}`, INSIDE `add_cargo_target_with_zephyr_env`.
    # That function builds the command every caller uses, so one injection
    # serves `cargo build` and `cargo doc` both — do NOT also add it at a call
    # site, or the flag lands on the command twice (issue 0544; hunk 3 removes
    # such copies).
    TMP="$(mktemp)"
    awk '
    {
        print
        if ($0 ~ /^[[:space:]]+\$\{rust_build_type_arg\}[[:space:]]*$/) {
            print ""
            print "      # nano-ros: EXTRA_CARGO_ARGS pass-through (Phase 168.1)."
            print "      # Honors CMakeLists.txt `set(EXTRA_CARGO_ARGS ...)` set"
            print "      # before `rust_cargo_application()`."
            print "      ${EXTRA_CARGO_ARGS}"
        }
    }
    ' "$CMAKE_FILE" > "$TMP"

    mv "$TMP" "$CMAKE_FILE"
    changed=1
fi

# --- 3. strip caller-level copies of the pass-through (issue 0544) -----------
#
# `CARGO_ARGS build ${EXTRA_CARGO_ARGS}` / `CARGO_ARGS doc ${EXTRA_CARGO_ARGS}`
# duplicate what hunk 2 already injects inside the function. Upstream's own
# lines are bare (`CARGO_ARGS build`), so removing the variable restores the
# upstream text exactly — this cannot damage a clean checkout, and it repairs a
# workspace that a stale reading of hunk 2's old comment left broken.
#
# Unconditional rather than guarded by a marker: the copies it removes carry no
# marker of their own, which is precisely why hunk 2's guard could not see them.
if grep -qE '^[[:space:]]*CARGO_ARGS (build|doc) \$\{EXTRA_CARGO_ARGS\}' "$CMAKE_FILE"; then
    TMP="$(mktemp)"
    sed -E 's/^([[:space:]]*CARGO_ARGS (build|doc)) \$\{EXTRA_CARGO_ARGS\}[[:space:]]*$/\1/' \
        "$CMAKE_FILE" > "$TMP"
    mv "$TMP" "$CMAKE_FILE"
    changed=1
    echo "[cargo-features-patch] removed caller-level EXTRA_CARGO_ARGS copies (issue 0544)"
fi

# The invariant, asserted rather than assumed: the pass-through reaches the
# cargo command EXACTLY once. A second occurrence is a duplicated flag, and
# cargo rejects `--no-default-features` twice — a failure that reads as a cargo
# usage error, several layers from the file that caused it.
# Count CODE occurrences only. The injected block and the call sites both carry
# explanatory comments that name the variable, and counting those would make
# this fire on a correct file — the same prose-counting mistake that makes a
# grep-based gate useless.
occurrences="$(grep -v '^[[:space:]]*#' "$CMAKE_FILE" | grep -c '\${EXTRA_CARGO_ARGS}' || true)"
if [ "$occurrences" -ne 1 ]; then
    echo "ERROR: \${EXTRA_CARGO_ARGS} appears $occurrences time(s) in" >&2
    echo "       $CMAKE_FILE — expected exactly 1 (issue 0544)." >&2
    echo "       More than one puts the flag on the cargo command twice:" >&2
    echo "         error: the argument '--no-default-features' cannot be used multiple times" >&2
    grep -n '\${EXTRA_CARGO_ARGS}' "$CMAKE_FILE" >&2 || true
    exit 1
fi

# --- 4. board FACTS + SITE config onto the cargo env (phase-351 W5 / issue 0605)
#
# The Zephyr RUST entry's cargo is spawned HERE, by zephyr-lang-rust — not by
# `nros_cargo_build()` — so the hook W5 put in that function never ran for these
# cells and the arm shipped INERT: a full configure printed no delivery line and
# no reason, which is the silence the wave exists to remove (issue 0529).
#
# Injected BEFORE `cargo`, because `cmake -E env` treats every KEY=VALUE up to
# the first non-assignment as the environment and everything after as the
# command. Placing it with the other pass-throughs (after `${rust_build_type_arg}`,
# which is already an ARGUMENT) would make cargo see `NROS_BOARD=…` as a
# subcommand.
#
# `NROS_BOARD_FACTS_ENV` is the CACHE INTERNAL list `nros_resolve_board_facts()`
# fills; empty (a host build, an unmapped board) expands to nothing and the
# command is byte-identical to before.
if ! grep -q "nano-ros: board facts" "$CMAKE_FILE"; then
    TMP="$(mktemp)"
    awk '
    {
        if ($0 ~ /^[[:space:]]+cargo \$\{cargo_command\}[[:space:]]*$/) {
            print "      # nano-ros: board facts + site config (phase-351 W5, issue 0605)."
            print "      # MUST precede `cargo` — `cmake -E env` ends the env at the"
            print "      # first non-KEY=VALUE argument."
            print "      ${NROS_BOARD_FACTS_ENV}"
        }
        print
    }
    ' "$CMAKE_FILE" > "$TMP"

    if ! grep -q "NROS_BOARD_FACTS_ENV" "$TMP"; then
        rm -f "$TMP"
        echo "[cargo-features-patch] ERROR: no `cargo \${cargo_command}` line to inject before" >&2
        echo "       in $CMAKE_FILE — upstream layout changed; fix this patch rather" >&2
        echo "       than leaving the Zephyr rust lane without its board rung (issue 0605)." >&2
        exit 1
    fi
    mv "$TMP" "$CMAKE_FILE"
    changed=1
fi

if [ "$changed" -eq 0 ]; then
    echo "[cargo-features-patch] already applied to $CMAKE_FILE"
else
    echo "[cargo-features-patch] patched $CMAKE_FILE"
fi
