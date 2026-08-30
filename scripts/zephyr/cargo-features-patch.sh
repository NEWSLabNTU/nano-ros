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
# KEYED ON THE COMMAND, NOT ON THE FUNCTION — because upstream's layout
# oscillates and this patch has now been wrong in both directions.
#
# 3.7 (`404fcef`) routes every caller through `add_cargo_target_with_zephyr_env`,
# so its ONE `${rust_build_type_arg}` line follows a variable command,
# `cargo ${cargo_command}`, and a single injection covers build and doc both.
# 4.4 (`a763400`) DELETED that function and inlined it: `rust_cargo_application`
# now carries TWO `${rust_build_type_arg}` lines, after a literal `cargo build`
# and a literal `cargo doc`.
#
# An earlier revision of this comment described exactly that two-site layout and
# was dismissed as stale when 3.7 refactored it away. It was not stale; it was
# early. Anchoring on the function name meant the awk injected at BOTH 4.4 sites
# and hunk 4 rejected the file, taking every 4.4 cell down at `Set up Zephyr 4.4
# workspace` — 12 cells producing no verdict at all.
#
# So the anchor is the cargo COMMAND: inject after the `${rust_build_type_arg}`
# whose command is `cargo build` or `cargo ${cargo_command}`, never after
# `cargo doc`. That is exactly one site under both layouts, and it stays one if
# upstream moves the code between functions again.
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

# --- self-test -------------------------------------------------------------
#
# Both upstream layouts, run end to end, asserting the invariant this script
# exists to hold: EXACTLY ONE of each injection, on the BUILD command, never on
# `cargo doc`, and unchanged by a second run.
#
# It exists because anchoring on `add_cargo_target_with_zephyr_env` was correct
# for 3.7 and silently wrong for 4.4, and nothing in the tree could tell —
# the workspace is fetched by west at setup time, so the only place the two
# layouts meet is here.
#
#   bash scripts/zephyr/cargo-features-patch.sh --self-test
if [ "${1:-}" = "--self-test" ]; then
    _st_tmp="$(mktemp -d)"
    trap 'rm -rf "$_st_tmp"' EXIT
    _st_fail=0

    # 3.7: one shared function, the command spelled as a variable.
    # 4.4: the function inlined, two literal commands.
    for _layout in shared inlined; do
        _d="$_st_tmp/$_layout/modules/lang/rust"
        mkdir -p "$_d"
        {
            printf 'function(rust_cargo_application)\n'
            printf '  set(rust_build_type_arg --release)\n'
            printf '  add_custom_command(\n'
            printf '      DT_AUGMENTS="${DT_AUGMENTS}"\n'
            if [ "$_layout" = shared ]; then
                printf '      cargo ${cargo_command}\n'
            else
                printf '      cargo build\n'
            fi
            printf '      ${rust_build_type_arg}\n'
            printf '      ${command_paths}\n'
            if [ "$_layout" = inlined ]; then
                printf '      DT_AUGMENTS="${DT_AUGMENTS}"\n'
                printf '      cargo doc\n'
                printf '      ${rust_build_type_arg}\n'
            fi
            printf 'endfunction()\n'
        } > "$_d/CMakeLists.txt"

        for _run in 1 2; do
            if ! bash "${BASH_SOURCE[0]}" "$_st_tmp/$_layout" >/dev/null 2>&1; then
                echo "self-test: $_layout layout: patch FAILED on run $_run" >&2
                _st_fail=1
                continue 2
            fi
        done

        _code() { grep -v '"'"'^[[:space:]]*#'"'"' "$_d/CMakeLists.txt" | grep -c "$1" || true; }
        _e="$(_code '\${EXTRA_CARGO_ARGS}')"
        _f="$(_code '\${NROS_BOARD_FACTS_ENV}')"
        if [ "$_e" != 1 ] || [ "$_f" != 1 ]; then
            echo "self-test: $_layout layout: EXTRA_CARGO_ARGS=$_e NROS_BOARD_FACTS_ENV=$_f (want 1/1)" >&2
            _st_fail=1
        fi
        # `cargo doc` must be left alone: a second copy of the flag is the
        # failure this whole file is about.
        if [ "$_layout" = inlined ] && \
           awk '/cargo doc/{d=1} d && /\$\{EXTRA_CARGO_ARGS\}/{print; exit}' \
               "$_d/CMakeLists.txt" | grep -q .; then
            echo "self-test: inlined layout: EXTRA_CARGO_ARGS reached cargo doc" >&2
            _st_fail=1
        fi
    done

    if [ "$_st_fail" -eq 0 ]; then
        echo "cargo-features-patch self-test: OK (shared + inlined layouts, 1 injection each, idempotent)"
        exit 0
    fi
    exit 1
fi

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
    # Remember the last line that carried a command, so the injection can be
    # keyed on WHICH cargo invocation this `${rust_build_type_arg}` belongs to.
    # `cargo doc` gets nothing: doubling the flag is what cargo rejects.
    /^[[:space:]]*cargo[[:space:]]/ { cargo_line = $0 }
    {
        print
        if ($0 ~ /^[[:space:]]+\$\{rust_build_type_arg\}[[:space:]]*$/ &&
            cargo_line ~ /^[[:space:]]*cargo[[:space:]]+(build|\$\{cargo_command\})[[:space:]]*$/) {
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
        # Keyed on the cargo COMMAND for the same reason as hunk 2: 3.7 spells
        # it `cargo ${cargo_command}` inside the shared function, 4.4 inlined it
        # to a literal `cargo build` (and a `cargo doc` that must NOT get this).
        if ($0 ~ /^[[:space:]]+cargo[[:space:]]+(build|\$\{cargo_command\})[[:space:]]*$/) {
            print "      # nano-ros: board facts + site config (phase-351 W5, issue 0605)."
            print "      # MUST precede `cargo` — `cmake -E env` ends the env at the"
            print "      # first non-KEY=VALUE argument."
            print "      ${NROS_BOARD_FACTS_ENV}"
        }
        print
    }
    ' "$CMAKE_FILE" > "$TMP"

    facts_n="$(grep -v '^[[:space:]]*#' "$TMP" | grep -c '\${NROS_BOARD_FACTS_ENV}' || true)"
    if [ "$facts_n" -ne 1 ]; then
        rm -f "$TMP"
        echo '[cargo-features-patch] ERROR: no cargo build line to inject before' >&2
        echo "       in $CMAKE_FILE — upstream layout changed; fix this patch rather" >&2
        echo "       than leaving the Zephyr rust lane without its board rung (issue 0605)." >&2
        echo "       (found $facts_n injection site(s); expected exactly 1)" >&2
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
