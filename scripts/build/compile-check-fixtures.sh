#!/usr/bin/env bash
# Build-stage "compile-check" fixtures (issue 0034 — No compilation inside tests).
#
# Some tests only need to prove that a small generated/template crate *compiles*
# (e.g. a macro re-export path resolves). Running `cargo check` inside the test
# makes the test wall-clock dominated by compile time → spurious nextest
# timeouts. Instead, this script does the compile in the BUILD stage: it stages
# each template into a gitignored build dir, rewrites `@NANO_ROS_ROOT@`
# placeholders to absolute `path =` deps, runs `cargo check`, and on success
# writes a `.compile-ok` stamp the test asserts (via
# `nros_tests::fixtures::require_compile_check`).
#
# Add a `[[compile_check_fixture]]` row to `examples/fixtures.toml` (phase-319 W2).
set -Eeuo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/../.." && pwd)"
cd "$repo_root"

# shellcheck source=scripts/build/cargo.sh
source "$repo_root/scripts/build/cargo.sh"

# issue 0493 — the ONE CMAKE_PREFIX_PATH derivation (SDK Corrosion). This script
# is where the wiring used to live INLINE, and being the only builder that had
# it is what made one host produce two different cargo target-dir topologies.
# shellcheck source=scripts/build/cmake-prefix.sh
source "$repo_root/scripts/build/cmake-prefix.sh"
nros_cmake_export_prefix_path

# RFC-0070 R1/R3 (phase-334 W2.b step 2) — the compile-check family's roots come
# from the ONE derivation, not from a literal. `NROS_REPO_ROOT` is pinned to THIS
# script's own repo root so the emitted path is byte-identical to the
# `<repo>/build/<kind>` literal it replaces even when an inherited
# `NROS_REPO_ROOT`/`NROS_REPO_DIR` names a different checkout (worktrees). Not
# exported: the pool re-invokes this script as a fresh bash, which pins its own.
NROS_REPO_ROOT="$repo_root"
# shellcheck source=scripts/build/build-root.sh
source "$repo_root/scripts/build/build-root.sh"

# issue 1454 — the identity of the launch-resolution toolchain a staged build
# consumed. One spelling, shared with `compile-check-signature.sh` and (through
# the script's runnable form) with the Rust fixture resolver.
# shellcheck source=scripts/build/launch-resolver-identity.sh
source "$repo_root/scripts/build/launch-resolver-identity.sh"

out_root="$(nros_build_dir "$NROS_KIND_COMPILE_CHECK")"
mkdir -p "$out_root"

# Write a fixture's `.compile-ok` stamp — issue 1454.
#
# The stamp used to be a bare date, i.e. "a build succeeded at some point",
# which is true of a museum artifact too. `--resolver` additionally records
# WHICH launch-resolution toolchain the build consumed, so
# `nros_tests::fixtures::require_compile_check` can compare it against the one
# on disk now and fail loud instead of asserting real codegen evidence produced
# by a parser nobody is running any more.
#
# `--resolver` is passed by the call sites whose build actually ran `nros sync`
# — never blanket, because this line is an ASSERTION: recording a tool a row
# never used would turn the next parser bump into a hard failure for a `cxx-syntax`
# snippet that a C++ compiler alone produced. (The `.inputsig` signature takes
# the opposite trade for the same fact; see the note there.)
#
# A stamp with NO resolver line is "nothing recorded", which the reader treats
# as unjudgeable rather than stale: every stamp on disk predating this change is
# that, and the `.inputsig` edge is what calls those stale.
nros_write_compile_ok() {
    local dir="$1" want_resolver="${2:-}"
    {
        date -u +%Y-%m-%dT%H:%M:%SZ
        if [ "$want_resolver" = "--resolver" ]; then
            printf 'tool:nros-launch-resolve=%s\n' \
                "$(nros_launch_resolver_identity "$repo_root" || echo absent)"
        fi
    } > "$dir/.compile-ok"
}

# Per-row OVERLAYS are FILES, never text in this script — issue 1656.
#
# A row that is "the template with a different X" keeps X under the template's
# own `cases/<id>/`, laid out like the tree it overlays
# (`cases/main_macro_form1/src/demo_entry/src/main.rs.case`). Staging copies it over
# the template and then drops `cases/`, so the staged build never sees the
# other rows' overlays.
#
# The reason is the signature, not tidiness: `.inputsig` hashes the row's `dir`
# (`compile-check-signature.sh`) and never this script, so the `post_stage`
# `printf`/`sed` rewrites that lived here were invisible to it — editing one
# left its row reading FRESH. Under `cases/` the overlay is inside `dir`, so it
# is an input like every other file there. (The cmake verdict rows already kept
# their cases as files, `cases/<id>.cmake`; those are flat FILES the project
# itself reads, so a `cases/` holding no subdirectory is left alone.)
#
# Every overlay file is stored as `<path>.case` and staged as `<path>`. The
# suffix keeps the tree's by-NAME scanners off them: a misuse row's
# `system.toml` names a board that does not exist ON PURPOSE, and a drift row's
# `nros-platform.toml` names a source path that does not exist on purpose —
# neither is a real system or descriptor, and gates that find every
# `system.toml` / `nros-platform.toml` in the index must not read them as one.
# A file under `cases/<id>/` WITHOUT the suffix is refused, so there is one
# spelling, not two.
apply_case_overlay() {
    local id="$1" staged="$2" src rel
    [ -d "$staged/cases" ] || return 0
    if [ -d "$staged/cases/$id" ]; then
        while IFS= read -r -d '' src; do
            rel="${src#"$staged/cases/$id/"}"
            case "$rel" in
                *.case) ;;
                *) echo "compile-check: $id: overlay file cases/$id/$rel lacks the .case suffix" >&2
                   return 2 ;;
            esac
            mkdir -p "$(dirname "$staged/${rel%.case}")"
            cp "$src" "$staged/${rel%.case}"
        done < <(find "$staged/cases/$id" -type f -print0)
    fi
    # Any overlay subdirectory means this `cases/` is the overlay table, not a
    # file set the project reads: drop it so no row builds a sibling's case.
    if compgen -G "$staged/cases/*/" >/dev/null; then
        rm -rf "$staged/cases"
    fi
}

# issue 1656 / 0501 — the staged rows of ONE template share one cargo target
# dir, so their dependency graph (every nros crate, every registry crate) is
# built ONCE instead of once per row (measured: five `n9_workspace` verdict rows
# were 729 MB each, 3.6 GB for one graph).
#
# Issue 0501 is why sharing needs the version stamp below: cargo identifies a
# workspace member by name + version + its path RELATIVE to the workspace root,
# which is identical in every staged copy, so without it the first row to check
# `demo_entry` successfully made every later row's check fresh — a misuse
# "compiled". Each member gets `+<row>` build metadata: distinct units per row,
# the external graph (reached through absolute `path =` deps) shared.
#
# Only rows whose stamp is the verdict/stamp itself share (`cargo-check`,
# `cargo-clippy`, `cargo-check-verdict`). `cargo-build` rows keep a private
# `target/` because the tests run the binary at `<row>/target/debug/…`.
#
# The stamp makes the MEMBERS distinct and nothing else, so a template whose
# rows differ in an input of an EXTERNAL crate must not share: its overlay
# changes what a dependency's build script reads, and a shared unit could carry
# one row's build-script result into another's verdict (0501's shape, one crate
# down). Such a template says so with a `.private-target-dir` file, and each of
# its rows gets a target dir of its own (`zpico_drift_gate` does — its rows
# differ only in the descriptor `zpico-sys/build.rs` reads).
#
# Both kinds live OUTSIDE the row dir, which `stage_tree` deletes on every
# build: a target dir inside it made every rebuild a cold one.
_cc_target_dir() {
    local dir="$1" staged="$2"
    local key="${dir//\//_}"
    if [ -e "$repo_root/$dir/.private-target-dir" ]; then
        key="$key@${staged##*/}"
    fi
    printf '%s/.shared-target/%s\n' "$out_root" "$key"
}

_cc_stamp_member_versions() {
    local id="$1" staged="$2" meta manifest
    meta="${id//_/-}"
    while IFS= read -r -d '' manifest; do
        python3 - "$manifest" "$meta" <<'PYSTAMP'
import re, sys
path, meta = sys.argv[1], sys.argv[2]
text = open(path, encoding="utf-8").read()
out, in_pkg, done = [], False, False
for line in text.splitlines(keepends=True):
    head = line.strip()
    if head.startswith("["):
        in_pkg = head == "[package]"
    m = re.match(r'^(\s*version\s*=\s*")([^"+]+)(")', line) if in_pkg and not done else None
    if m:
        line = f"{m.group(1)}{m.group(2)}+{meta}{m.group(3)}{line[m.end():]}"
        done = True
    out.append(line)
open(path, "w", encoding="utf-8").writelines(out)
PYSTAMP
    done < <(find "$staged" -name Cargo.toml -not -path '*/target/*' -print0)
}

# Concatenate the shared target dir's dep-info into the row's own dir, so the
# `.inputsig` closure (`dep-closure.py` reads `*.d` under the ROW dir) still
# sees what the build read. A UNION over every row sharing the dir — wider than
# this row's own closure, never narrower, which is the safe direction.
_cc_copy_shared_depinfo() {
    local shared="$1" staged="$2" f
    : > "$staged/shared-target.d"
    while IFS= read -r -d '' f; do
        cat "$f" >> "$staged/shared-target.d"
        printf '\n' >> "$staged/shared-target.d"
    done < <(find "$shared" -name '*.d' -print0 2>/dev/null)
}

# Build fixtures (id : src): same staging, but `cargo build -p demo_entry`
# producing a runnable binary at build/compile-check/<id>/target/debug/demo_entry
# that the test executes (e.g. boot/run-tier assertions). The compile is still
# the build stage; the test runs the prebuilt binary.

# Set by `stage_tree` to `--resolver` when this row's staging resolved a
# bringup, and to the empty string when it did not. Read by the `.compile-ok`
# write sites below (issue 1454). Cleared on every entry, so one row's answer
# can never be recorded against the next.
_cc_resolver_arg=""

# Rows whose build must NOT run `nros sync` — issue 1620.
#
# `stage_tree` syncs any staged tree with a `package.xml`, because that is what a
# user build does. Two kinds of row ask a different question and would be
# answered wrongly by a sync:
#
#   * `main_macro_resolves_from_inputs` asserts the macro resolves the model
#     from `system.toml` + the launch file ITSELF when no build system produced
#     one (issue 0414). A sync is exactly the build system it must not have.
#   * the `main_macro_misuse_*` verdicts reproduce a plain `cargo check` of a
#     misused entry. `unknown_board` would make the SYNC refuse first, so the
#     row would record nros's refusal instead of the macro's diagnostic.
_cc_row_skips_sync() {
    case "$1" in
        main_macro_resolves_from_inputs | main_macro_misuse_*) return 0 ;;
        *) return 1 ;;
    esac
}

# Rows whose build runs `nros-launch-resolve` OUTSIDE `nros sync` — issue 1454's
# stamp question, asked of a row that skips the sync. The macro resolves the
# bringup itself here, so the stamp must name the resolver all the same.
_cc_row_resolves_in_macro() {
    case "$1" in
        main_macro_resolves_from_inputs) return 0 ;;
        *) return 1 ;;
    esac
}

stage_tree() {
    local id="$1" src="$2" staged="$3"
    _cc_resolver_arg=""
    [ -d "$repo_root/$src" ] || {
        echo "compile-check: source template missing: $src" >&2
        return 2
    }
    rm -rf "$staged"
    mkdir -p "$staged"
    cp -r "$repo_root/$src/." "$staged/"
    # Rewrite the placeholder to the absolute repo root so the staged tree's
    # `path =` deps resolve (mirrors the staging the test used to do inline).
    # NOTE the `|| true`: under `set -euo pipefail`, `find -exec grep +` exits
    # nonzero when NO staged file contains the placeholder (grep's no-match exit
    # propagates through find), which would abort the whole run for any fixture
    # that doesn't use that placeholder. The rewrite is best-effort — a missing
    # placeholder is a no-op, not an error.
    find "$staged" -type f -exec grep -lZ '@NANO_ROS_ROOT@' {} + 2>/dev/null \
        | xargs -0 -r sed -i "s#@NANO_ROS_ROOT@#$repo_root#g" || true
    apply_case_overlay "$id" "$staged"
    # phase-330 W7.g — templates no longer carry committed SystemModels
    # (W4.a); resolve them into the staged workspace's build dir the same way
    # a user build does. AFTER the overlay, so form/tier rewrites of the
    # INPUTS (main.rs, system.toml) are what gets resolved.
    # Any staged pkg (package.xml) can be a bringup — system.toml is OPTIONAL
    # to the resolver (o4's bringup is launch/ + package.xml only).
    if _cc_row_skips_sync "$id"; then
        # An `if`, not `&&`: as the last command of this branch a false `&&`
        # list would become stage_tree's return status and trip errexit.
        if _cc_row_resolves_in_macro "$id"; then
            _cc_resolver_arg="--resolver"
        fi
    elif find "$staged" -maxdepth 3 -name package.xml -print -quit 2>/dev/null | grep -q .; then
        local _sync_cli="${NROS_CLI_BIN:-${NROS_CLI:-$(command -v nros || true)}}"
        if [ -z "$_sync_cli" ]; then
            echo "compile-check: nros CLI not found — cannot resolve staged models (just setup-cli)" >&2
            return 2
        fi
        ( cd "$staged" && "$_sync_cli" sync >/dev/null )
        # issue 1454 — this sync is where `nros-launch-resolve` ran, so THIS row
        # is a function of the launch-resolution toolchain and its stamp must
        # say so. Set here rather than guessed at the write site: the condition
        # is "did a bringup get resolved", which only this branch knows.
        _cc_resolver_arg="--resolver"
    fi
}

stage_and_check() {
    local id="$1" src="$2"
    local staged="$out_root/$id"
    echo "== compile-check: $id =="
    stage_tree "$id" "$src" "$staged"
    rm -f "$staged/.compile-ok"
    _cc_stamp_member_versions "$id" "$staged"
    local shared; shared="$(_cc_target_dir "$src" "$staged")"
    ( cd "$staged" && CARGO_TARGET_DIR="$shared" cargo check --manifest-path Cargo.toml )
    _cc_copy_shared_depinfo "$shared" "$staged"
    nros_write_compile_ok "$staged" "$_cc_resolver_arg"
    echo "   stamped $staged/.compile-ok"
}

# cargo-clippy (issue 1230). Same staging as `cargo-check`, `cargo clippy`
# instead of `cargo check`, same `.compile-ok` stamp.
#
# A separate builder rather than a flag on `cargo-check`, because the two ask
# different questions and a row should say which one it is: `cargo check`
# answers "does this type-check", clippy answers "is it clean under the lints".
# A crate whose lint verdict is the point (`generated_message_crate`: the
# emitted message code under `#![deny(warnings)]` + `#![deny(clippy::all)]`)
# would otherwise have to run its lints through a `post_stage` hook, which is
# where a check goes to be forgotten.
#
# Missing clippy is FATAL here, not a lane skip. It is a rustup component
# (`rustup component add clippy`), the whole `just check` line already requires
# it, and the test this replaces asserted exactly that (issue 1160: a green over
# a lint run that never happened is not a trade worth making).
stage_and_clippy() {
    local id="$1" src="$2"
    local staged="$out_root/$id"
    echo "== compile-clippy: $id =="
    cargo clippy --version >/dev/null 2>&1 || {
        echo "compile-clippy: \`cargo clippy\` is not available — install it with \`rustup component add clippy\` (row $id exists to report a LINT verdict; skipping it would report one nobody measured)" >&2
        exit 2
    }
    stage_tree "$id" "$src" "$staged"
    rm -f "$staged/.compile-ok"
    _cc_stamp_member_versions "$id" "$staged"
    local shared; shared="$(_cc_target_dir "$src" "$staged")"
    ( cd "$staged" && CARGO_TARGET_DIR="$shared" cargo clippy --manifest-path Cargo.toml )
    _cc_copy_shared_depinfo "$shared" "$staged"
    nros_write_compile_ok "$staged" "$_cc_resolver_arg"
    echo "   stamped $staged/.compile-ok"
}

stage_and_build() {
    local id="$1" src="$2" manifest_dir="${3:-.}" pkg="${4:-demo_entry}"
    local staged="$out_root/$id"
    echo "== build-fixture: $id =="
    stage_tree "$id" "$src" "$staged"
    rm -f "$staged/.compile-ok"
    # `manifest_dir` (3rd `id:src:dir` field) builds a member that lives in a
    # subdir excluded from the root workspace (e.g. O.5's `demo_entry/`, O.3's
    # `posix_entry/`). `pkg` (4th field) names the package when it isn't the
    # default `demo_entry` (O.3 builds `posix_entry`).
    ( cd "$staged" && cargo build -p "$pkg" --manifest-path "$manifest_dir/Cargo.toml" )
    nros_write_compile_ok "$staged" "$_cc_resolver_arg"
    # profile-literal-ok: dir vocabulary: echoes the manifest's target-directory name
    echo "   built $staged/$manifest_dir/target/debug/$pkg"
}

# --- VERDICT builders (issue 1620) ------------------------------------------
#
# "No compilation inside tests" had one standing exception: a FAIL-path
# diagnostic, on the grounds that "a build-stage fixture cannot express a
# compile that must fail — a fixture whose configure fails fails the BUILD".
# That conflated two things. What a must-fail test needs from the build stage is
# not an artifact of a SUCCESSFUL compile; it is the compile's VERDICT — its exit
# status and its diagnostics — and a verdict is an artifact like any other.
#
# So these builders run the compile, record what it said, and succeed whatever
# it said. The test asserts the verdict: "it failed, with THIS diagnostic", or,
# for the positive rows, "it succeeded" (plus whatever else the stderr shows).
# The build stage never decides pass/fail for a verdict row — that would put the
# assertion in the wrong place and turn an expected diagnostic into a red build.
#
# Recorded under the row's compile-check dir:
#   .verdict          the STAMP: date, the resolver line (issue 1454) and
#                     `exit=<status>`. Written LAST, so a crashed builder leaves
#                     no stamp and the resolver reports the row as not built.
#   verdict.stdout    / verdict.stderr — the compile's own output, verbatim.
#   verdict.prelude.* — for a row that compiles twice (`*_rebuilds_on_*`), the
#                     FIRST compile's verdict; the files above are the second's.
#
# Infrastructure failures (no source tree, a sync that refuses, cmake absent)
# still fail the BUILD — those are not verdicts about the code under test.

# Write the `.verdict` stamp. $1 staged dir, $2 the recorded compile's exit
# status; `_cc_capture` has already put its stdout/stderr/exit in place.
_cc_write_verdict_stamp() {
    local staged="$1" status="$2"
    {
        date -u +%Y-%m-%dT%H:%M:%SZ
        if [ "$_cc_resolver_arg" = "--resolver" ]; then
            printf 'tool:nros-launch-resolve=%s\n' \
                "$(nros_launch_resolver_identity "$repo_root" || echo absent)"
        fi
        printf 'exit=%s\n' "$status"
    } > "$staged/.verdict"
}

# Run one compile, capturing its verdict under $2 (a file prefix). Never fails on
# the compile's status — returns 0 and records it.
_cc_capture() {
    local staged="$1" prefix="$2"; shift 2
    local rc=0
    ( cd "$staged" && "$@" ) > "$staged/$prefix.stdout" 2> "$staged/$prefix.stderr" || rc=$?
    printf '%s\n' "$rc" > "$staged/$prefix.exit"
    _cc_last_rc="$rc"
}

# Per-row step BEFORE the recorded compile. Only the rebuild-tracking row has
# one: it compiles once (recorded as the prelude), then touches the model the
# sync produced, so the recorded compile is the one that must RE-check.
_cc_verdict_prelude() {
    local id="$1" staged="$2"; shift 2
    case "$id" in
        main_macro_rebuilds_on_model_touch)
            local model="$staged/build/nros/models/demo_bringup/system_model.yaml"
            [ -f "$model" ] || {
                echo "compile-check: $id: the sync wrote no model at $model" >&2
                return 2
            }
            _cc_capture "$staged" verdict.prelude "$@"
            # Past cargo's mtime resolution, then rewrite the model in place —
            # the same bytes, a new mtime: what `nros sync` does on a re-run.
            sleep 1.1
            cp "$model" "$model.tmp" && mv "$model.tmp" "$model"
            ;;
        *) : ;;
    esac
}

stage_and_check_verdict() {
    local id="$1" src="$2"
    local staged="$out_root/$id"
    echo "== compile-verdict: $id =="
    stage_tree "$id" "$src" "$staged"
    rm -f "$staged/.verdict" "$staged"/verdict.* "$staged/shared-target.d"
    _cc_stamp_member_versions "$id" "$staged"
    local shared; shared="$(_cc_target_dir "$src" "$staged")"
    # Hermetic: an inherited NROS_MODEL_DIR would answer the model question for
    # the macro, which is the very question `main_macro_resolves_from_inputs`
    # asks (issue 0414) — and would point the others at someone else's model.
    local cmd=(env -u NROS_MODEL_DIR CARGO_TARGET_DIR="$shared"
        cargo check --color never --manifest-path Cargo.toml)
    _cc_verdict_prelude "$id" "$staged" "${cmd[@]}"
    _cc_capture "$staged" verdict "${cmd[@]}"
    _cc_copy_shared_depinfo "$shared" "$staged"
    _cc_write_verdict_stamp "$staged" "$_cc_last_rc"
    echo "   recorded $staged/.verdict (exit=$_cc_last_rc)"
}

# cmake configure verdict. A FAILED configure writes neither
# `CMakeFiles/Makefile.cmake` nor a `build.ninja`, so the dep closure
# `compile-check-signature.sh` reads (CMAKE_MAKEFILE_DEPENDS / the RERUN_CMAKE
# edge) does not exist for exactly these rows — and the files they exist to
# test (`cmake/NanoRosNodeRegister.cmake`, the root `CMakeLists.txt`) are outside
# the row's own dir. Without a measured closure an edit to the module under test
# would leave a museum verdict looking fresh (issue 0196). CMake's own trace
# records every listfile it EXECUTED, failure or not, so the closure comes from
# there, as a Make-syntax `verdict.d` the signature's dep-info reader already
# understands.
stage_and_configure_verdict() {
    local id="$1" src="$2"
    local staged="$out_root/$id"
    echo "== configure-verdict: $id =="
    command -v cmake >/dev/null 2>&1 || {
        echo "configure-verdict: cmake absent — row $id records a cmake verdict and cannot be built without it" >&2
        exit 2
    }
    stage_tree "$id" "$src" "$staged"
    rm -f "$staged/.verdict" "$staged"/verdict.*
    rm -rf "$staged/build"
    local cmd=(cmake -S . -B build
        --trace-format=json-v1 --trace-redirect="$staged/verdict.trace.json")
    # A fixture holding per-case bodies (`cases/<id>.cmake`) is told which one.
    [ -d "$staged/cases" ] && cmd+=("-DNROS_VERDICT_CASE=$id")
    _cc_capture "$staged" verdict "${cmd[@]}"
    python3 "$repo_root/scripts/build/cmake-trace-deps.py" \
        "$staged/verdict.trace.json" "$staged/verdict.d"
    rm -f "$staged/verdict.trace.json"
    _cc_write_verdict_stamp "$staged" "$_cc_last_rc"
    echo "   recorded $staged/.verdict (exit=$_cc_last_rc)"
}

# cmake fixtures (id : template-dir relative to repo). Configure + build a C/C++
# template into a PERSISTENT build dir (build/cmake-fixtures/<id>) so the test
# can inspect generated TUs / link sidecars / depfiles AND run/`nm` the produced
# executable — instead of running cmake at test time (issue 0034). The codegen
# step shells the `nros` CLI; the build is skipped (no stamp → test skips/fails
# per tier) when cmake or a `codegen entry`-capable `nros` is unavailable.
cmake_out="$(nros_build_dir "$NROS_KIND_CMAKE_FIXTURES")"

# Issue 0695 — these four prereqs used to answer to ONE verdict (print, return 1,
# skip every cmake fixture, run on green), and they do not deserve the same one.
#
#   cmake absent            a host that cannot build C at all. A real skip — but
#                           a RECORDED one, because `cmake=0` in the summary read
#                           identically for "skipped them all" and "there were
#                           none", which is how a partial fixture set came out
#                           looking complete.
#   nros / codegen entry /  the SWEEP CONTRACT. CLAUDE.md requires
#   play_launch_parser      `source ./activate.sh` before any build; that is what
#                           puts all three on PATH. Missing one is operator
#                           error, and `stage_and_check` below ALREADY takes the
#                           whole build down for the very same missing binary
#                           ("compile-check: nros CLI not found"). One condition
#                           answered two ways in one script is the defect; the
#                           fatal half is the correct half.
#
# Skips are reported through `_note_lane_skip` so the final summary names them
# instead of printing a zero that means two different things.
lane_skips=()

# LANE FILTER — phase-395. Empty (the default) means every lane, which is what
# `build-test-fixtures` wants.
#
# A caller that needs only ONE lane can now say so, and `check-source-gates`
# does: `platform_header_compile` asserts the `platform_hdr_*` snippets, which
# are `cargo-check` records, and nothing else. Building every lane to get them
# dragged in `freertos_firmware` — a `cargo-build` record that needs the
# FreeRTOS KERNEL SUBMODULE, which CI does not provision — and the gate died on
# `missing include ... third-party/freertos/kernel`.
#
# That was a real regression from making the gate build its own fixtures: before
# it built nothing and silently depended on someone else having done so, and
# after it built far more than it needed. Neither is right; asking for the lane
# you assert is.
CC_LANES="${NROS_COMPILE_CHECK_LANES:-}"
# Comma OR space separated. `check-lane-contracts` reads the value as a comma
# list (`[A-Za-z0-9,_-]+`, unquoted), and this matched space-separated only, so
# the one spelling the gate could read selected NO lane here (issue 1656).
CC_LANES="${CC_LANES//,/ }"
_lane_on() {
    [ -z "$CC_LANES" ] && return 0
    case " $CC_LANES " in *" $1 "*) return 0 ;; *) return 1 ;; esac
}

_note_lane_skip() {
    lane_skips+=("$1")
    echo "$1 — skipping (recorded in the summary)" >&2
}

# The count a lane reports: its number, or SKIPPED(<why>) when the lane never ran.
_lane_count() {
    if [ -n "$2" ]; then printf 'SKIPPED(%s)' "$2"; else printf '%s' "$1"; fi
}

cmake_skipped=""
cmake_fixture_prereqs_ok() {
    # A lane the caller filtered OUT must not run its prerequisite check either.
    # The `nros` CLI check below is deliberately FATAL (a stale CLI is a defect,
    # not a host capability), and leaving it reachable meant a caller asking for
    # only `cargo-check` still died on a cmake prerequisite it had opted out of.
    _lane_on cmake-configure || { cmake_skipped="not in NROS_COMPILE_CHECK_LANES"; return 1; }
    command -v cmake >/dev/null 2>&1 || {
        cmake_skipped="cmake absent"
        _note_lane_skip "cmake-fixtures: cmake absent"
        return 1
    }
    local nb="${NROS_CLI:-$(command -v nros || true)}"
    [ -n "$nb" ] || {
        echo "cmake-fixtures: nros CLI not found — cannot codegen entries (source ./activate.sh, or just setup-cli)" >&2
        exit 2
    }
    "$nb" codegen entry --help >/dev/null 2>&1 || {
        echo "cmake-fixtures: '$nb' lacks 'codegen entry' — stale CLI (just setup-cli)" >&2
        exit 2
    }
    # "The C/mixed Entry templates parse launch XML via play_launch_parser."
    #
    # MEASURED FALSE, 2026-09-24 (issue 1454), and left in place deliberately:
    # `strace -f -e trace=execve` over a `pure_c_workspace` cmake-configure
    # build — 37,244 calls — spawns `nros-launch-resolve` twice and this binary
    # ZERO times. The templates' launch XML is resolved by `nros sync` through
    # the resolver, which statically links the parser crate from the
    # `packages/cli/third-party/play_launch` submodule; the SDK-store binary is
    # a standalone CLI nothing here runs. So on a host with a provisioned
    # resolver and no store binary this skips every cmake fixture for a reason
    # that is not true.
    #
    # NOT changed with 1454's fix: dropping it makes this lane RUN where it used
    # to skip, which is a behaviour change that wants its own measurement rather
    # than a ride on a fix about freshness. Recorded in the issue's Residue.
    #
    # A LANE SKIP, not `exit 2`, and the distinction is the point: a missing
    # play_launch_parser is a HOST CAPABILITY question, exactly like the
    # `cmake absent` case a few lines up, which has always skipped. Killing the
    # whole script for it meant a caller that needs only the compile-check
    # SNIPPETS could not get them — `check-source-gates` builds its own stamps
    # for `platform_header_compile`, and on a CI runner that never sources
    # `activate.sh` this turned into a required status check that could not
    # pass. A stale or absent `nros` CLI stays hard below: that is a defect,
    # not a capability.
    command -v play_launch_parser >/dev/null 2>&1 || {
        cmake_skipped="play_launch_parser absent"
        _note_lane_skip "cmake-fixtures: play_launch_parser not found (source ./activate.sh)"
        return 1
    }
    NROS_CLI_BIN="$nb"
    return 0
}

build_cmake_fixture() {
    local id="$1" src="$2"
    local bld="$cmake_out/$id"
    [ -d "$repo_root/$src" ] || { echo "cmake-fixtures: template missing: $src" >&2; return 2; }
    echo "== cmake-fixture: $id =="
    rm -rf "$bld"
    mkdir -p "$bld"
    # phase-445 W5 (RFC-0098 D9) — a WORKSPACE template has no root build file,
    # so there is nothing to `cmake -S`. It builds the way its README says and a
    # user does: `nros sync` + `nros build`, in a STAGED copy so neither the
    # generated roots under `build/` nor sync's output touch the source tree.
    # `nros build` with no image picks the bringup's one image, or builds every
    # package when there is no bringup (package mode); its outputs land under
    # `<id>/build/<coord-or-pkg>/cmake/`, which is what the rows' `output` name.
    # The RMW is the image's (`zenoh` in every template), so the old
    # `-DNROS_RMW=zenoh` pin below has nothing to override here.
    if [ ! -f "$repo_root/$src/CMakeLists.txt" ] && [ -f "$repo_root/$src/.colcon_workspace" ]; then
        cp -r "$repo_root/$src/." "$bld/"
        rm -rf "$bld/build" "$bld/generated"
        ( cd "$bld" \
            && NROS_REPO_DIR="$repo_root" "$NROS_CLI_BIN" sync >/dev/null \
            && NROS_REPO_DIR="$repo_root" "$NROS_CLI_BIN" build --workspace . --offline )
        # issue 1454 — this branch ran `nros sync`, so this fixture is baked
        # from a resolved SystemModel and is a function of the parser that
        # resolved it, exactly like the cargo rows. `require_cmake_fixture`
        # compares the stamp. The OTHER branch below does not sync, and gets no
        # stamp: an assertion about a tool a build never used is the thing this
        # issue is about.
        nros_write_compile_ok "$bld" --resolver
        echo "   built $bld (nros build)"
        return 0
    fi
    # The SDK-Corrosion prefix was derived HERE, inline, and in no other builder
    # — see `scripts/build/cmake-prefix.sh` for what that cost (issue 0493). It
    # is exported once at script scope now; this configure inherits it.
    #
    # Pass both nros cmake vars — different templates name it differently
    # (NROS_CLI_BIN vs NROS_BIN); the unused one is harmless.
    # phase-368 W9 — pin the backend the FIXTURE coordinate means. The template
    # roots now default to cyclonedds for a copied-out USER (no router needed),
    # but a compile-check row's coordinate rmw defaults to zenoh like every
    # manifest row (`row_coord()`), and the E2E tests that consume these
    # artifacts (cpp_multi_node_entry.rs) run them against a zenoh router.
    # Passing it explicitly keeps the fixture at the coordinate the tests
    # expect while the committed template serves users the daemonless default.
    cmake -S "$repo_root/$src" -B "$bld" "-DNROS_CLI_BIN=$NROS_CLI_BIN" "-DNROS_BIN=$NROS_CLI_BIN" \
        "-DNROS_RMW=zenoh"
    # Issue 0466 — how a cmake fixture gets its parallelism.
    #
    # These templates configure with the DEFAULT generator ("Unix Makefiles"),
    # so `cmake --build` runs a sub-make — and GNU make is the jobserver
    # protocol's native client. Under `nros_pool_run` the unit already carries
    # `MAKEFLAGS=-j<N> --jobserver-auth=fifo:/tmp/GMfifoNNN`, and make 4.4's FIFO
    # style is openable by any descendant (the pipe-FD style is what historically
    # forced projects to unset MAKEFLAGS around cmake). So the sub-make joins the
    # pool and the WHOLE fixture sweep shares one token budget.
    #
    # Passing `-j` here breaks exactly that. Measured under the pool:
    #
    #   cmake --build <dir>        -> silent; joins the jobserver
    #   cmake --build <dir> -j     -> "warning: -j0 forced in submake:
    #                                  resetting jobserver mode"
    #
    # i.e. the bare `-j` (unlimited) evicted the build from the pool and let it
    # run unbounded — the oversubscription the pool exists to prevent, caused by
    # the flag meant to make it fast.
    #
    # Outside a pool there is no budget to join, so ask for a bounded width
    # rather than the unlimited bare `-j`.
    if [ -n "${MAKEFLAGS:-}" ] && case "${MAKEFLAGS:-}" in *jobserver-auth*) true ;; *) false ;; esac; then
        cmake --build "$bld"
    else
        cmake --build "$bld" -j "$(nproc 2>/dev/null || echo 4)"
    fi
    echo "   built $bld"
}

# Cross-target build fixtures (id : src : subdir : pkg : target [: profiles]). Stage
# the template, then `cargo build --target <target> -p <pkg>` from <staged>/<subdir>
# — for firmware Entry-pkg fixtures whose codegen artifact (run_plan.rs) the test
# inspects. Gated on the rust target being installed; absent → no stamp → skip.
# The optional 6th field is a comma-separated profile list (default `debug`): a
# fixture that names `debug,release` stages ONCE and builds both profiles into the
# same tree, so a test can boot the -O0 debug ELF (fast link) OR the -O3 release
# ELF (needed when a -O0 zenoh-pico is too slow to finish a session handshake in
# budget — phase-281 W1 / the connected orch_tiers_freertos test).

stage_and_cross_build() {
    local id="$1" src="$2" subdir="$3" pkg="$4" target="$5" profiles="${6:-debug}"
    local staged="$out_root/$id"
    if ! rustup target list --installed 2>/dev/null | grep -qx "$target"; then
        echo "cross-build: target $target not installed — skipping $id" >&2
        return 0
    fi
    echo "== cross-build: $id ($pkg @ $target, profiles: $profiles) =="
    stage_tree "$id" "$src" "$staged"
    rm -f "$staged/.compile-ok"
    # firmware fixtures read the freertos platform sources + cffi headers from
    # the repo via env (the build.rs codegen + cc compile). Build every requested
    # profile into the one staged tree (debug → target/<t>/debug, release →
    # target/<t>/release).
    #
    # phase-336 note: `profiles` here is a manifest-supplied list of TARGET
    # DIRECTORY names (`debug`, `release`), not cargo profile names — `debug` is
    # the directory `dev` writes to. That is why this maps by hand instead of
    # calling `nros profile args`, which speaks profile names.
    local profile
    for profile in ${profiles//,/ }; do
        local profile_flag=()
        # profile-literal-ok: dir vocabulary: `profile` here is a manifest target-directory name
        [ "$profile" = "release" ] && profile_flag=(--release)
        echo "   -- profile: $profile"
        ( cd "$staged/$subdir" \
            && NROS_PLATFORM_FREERTOS_SRC="$repo_root/packages/platform/nros-platform-freertos/src" \
               NROS_PLATFORM_CFFI_INCLUDE="$repo_root/packages/platform/nros-platform-api/include" \
               cargo build "${profile_flag[@]}" --target "$target" -p "$pkg" )
    done
    nros_write_compile_ok "$staged" "$_cc_resolver_arg"
    echo "   built $staged/$subdir (target/$target; profiles: $profiles)"
}

# phase-319 W2 (issue 0351) — the fixture INVENTORY lives in
# `examples/fixtures.toml`, not in arrays here. Six hardcoded colon-delimited
# arrays used to sit at this spot; `check-fixtures-stale.sh` enumerates the
# manifest, so it could not see any of them (issue 0350 hid there for three
# days), and AGENTS.md:79 already said they belong in the manifest.
#
# The per-builder functions below are unchanged — only where the list comes from
# moved. Record fields (\x1f-separated):
#   id, builder, dir, pkg, manifest_dir, target, profiles, output
#
# `NROS_FIXTURE_ID=<id>` narrows to one row, matching workspace-fixtures-build.sh.
id_filter="${NROS_FIXTURE_ID:-}"

# `NROS_FIXTURE_BUILDER=<builder>[,<builder>…]` narrows to whole BUILDERS
# (issue 0871). The id filter selects one row; this selects one kind of row, and
# the difference matters for a caller that can only satisfy some prerequisites.
#
# CI's `check` job is the case that needed it: `check-source-gates` asserts the
# `cxx-syntax` stamps, which need a C++ compiler and nothing else, while the
# `cargo-check` and `cmake-configure` rows in the same manifest need
# `nros-launch-resolve` and `play_launch_parser` from `activate.sh`. Building
# everything there fails on prerequisites the job does not have and never
# reaches the rows it actually needs.
#
# An unknown name is an ERROR, not an empty sweep — the issue-0406 rule the id
# filter already follows one line down: a narrowing that selects nothing must
# say so rather than "succeed".
builder_filter="${NROS_FIXTURE_BUILDER:-}"
# The ONE spelling of this script's builder set — the id guard, the pool fan-out
# and this validation all iterate it (they were three literal copies).
_cc_all_builders="cargo-check cargo-clippy cargo-check-verdict cargo-build cross-build cmake-configure cmake-configure-verdict cxx-syntax cxx-syntax-verdict cxx-compile-verdict fixture-script"
if [ -n "$builder_filter" ]; then
    for _cc_want in ${builder_filter//,/ }; do
        case " $_cc_all_builders " in
            *" $_cc_want "*) ;;
            *) echo "NROS_FIXTURE_BUILDER: unknown builder '$_cc_want' (known: $_cc_all_builders)" >&2
               exit 2 ;;
        esac
    done
fi

# Is this builder in the current narrowing? No filter = every builder.
_cc_builder_enabled() {
    [ -z "$builder_filter" ] && return 0
    case ",$builder_filter," in *",$1,"*) return 0 ;; esac
    return 1
}

compile_check_records() {
    # A disabled builder yields NO rows, so every per-builder loop below is
    # narrowed by this one gate rather than by five copies of the same
    # condition. The counts at the end then report 0 for it, which is honest:
    # nothing was asked for and nothing was built.
    _cc_builder_enabled "$1" || return 0
    python3 "$repo_root/scripts/build/fixtures-manifest.py" list-compile-checks \
        --builder "$1" ${id_filter:+--id "$id_filter"}
}

# Issue 0406 — a narrowing that selects no compile-check row used to run every
# per-builder loop over an empty list and finish "successfully". Decide once,
# up front, whether that emptiness is a benign cross-builder sweep miss or a
# broken invocation; the guard owns the distinction.
if [ -n "$id_filter" ]; then
    _cc_matched=0
    for _cc_builder in $_cc_all_builders; do
        # Deliberately NOT `compile_check_records` — that honours the builder
        # narrowing, and "this id is in a builder you did not ask for" is not
        # the same fact as "this id does not exist" (issue 0406's distinction).
        _cc_matched=$((_cc_matched + $(python3 "$repo_root/scripts/build/fixtures-manifest.py" \
            list-compile-checks --builder "$_cc_builder" --id "$id_filter" | wc -l)))
    done
    # issue 1536 — a `west-*` compile-check row is built by the Zephyr lane
    # (`west-fixtures.sh`), never here. Say so and name the narrowing that
    # builds it, instead of the guard's "not a compile_check_fixture for
    # platform= lang=" — true of no coordinate, and no help.
    if [ "$_cc_matched" -eq 0 ]; then
        for _cc_builder in west-build west-configure; do
            if [ -n "$(python3 "$repo_root/scripts/build/fixtures-manifest.py" \
                list-compile-checks --builder "$_cc_builder" --id "$id_filter")" ]; then
                echo "fixtures: NROS_FIXTURE_ID='${id_filter}' is a ${_cc_builder} compile check, built by the Zephyr west lane, not here. Build it alone with:"
                echo "            NROS_ZEPHYR_FIXTURE_FILTER=${id_filter} bash scripts/build/west-fixtures.sh"
                exit 0
            fi
        done
    fi
    if [ "$_cc_matched" -eq 0 ]; then
        # shellcheck source=scripts/build/fixture-id-guard.sh
        source "$repo_root/scripts/build/fixture-id-guard.sh"
        nros_fixture_id_no_match "$id_filter" env compile_check_fixture "" ""
        exit 0
    fi
    unset _cc_matched _cc_builder
fi

# phase-336 W7 — fan the rows out under the jobserver when this invocation is
# NOT already narrowed to one.
#
# Every row stages its own tree under `$out_root/$id` and builds it, so the rows
# are independent; walking them serially left a 32-core host ~95 % idle
# (measured: 5 of 27 rows in 10 minutes, ONE rustc running). Each unit re-invokes
# this script with `NROS_FIXTURE_ID=<id>`, which is the narrowing the manifest
# reader already supports — so the per-row code path below is unchanged and is
# still exactly what runs.
#
# The pool falls back to a serial walk when an outer jobserver already owns the
# tokens (NROS_JOBSERVER=1) or pinned make 4.4 is absent.
# issue 1656 — `NROS_FIXTURE_IDS=<id>[,<id>…]` narrows the fan-out to an id SET.
# The id filter above selects one row; a lane that must build exactly the stamps
# its tests read (`test-lane-contracts`, the lane census) needs several, DERIVED
# by `scripts/test/lane-compile-stamps.py`, never authored. Every named id must
# be a row this script builds — an unknown one is an error (issue 0406's rule),
# and an EMPTY set builds nothing and says so rather than widening to every row.
ids_filter=""
if [ -n "${NROS_FIXTURE_IDS+set}" ]; then
    [ -z "$id_filter" ] || {
        echo "compile-check: NROS_FIXTURE_ID and NROS_FIXTURE_IDS are exclusive" >&2
        exit 2
    }
    ids_filter="${NROS_FIXTURE_IDS//,/ }"
    if [ -z "${ids_filter// /}" ]; then
        echo "compile-check: NROS_FIXTURE_IDS is empty — nothing to build."
        exit 0
    fi
    _cc_known=" "
    for _cc_builder in $_cc_all_builders; do
        while IFS=$'\x1f' read -r _id _rest; do
            [ -n "$_id" ] && _cc_known="$_cc_known$_id "
        done < <(python3 "$repo_root/scripts/build/fixtures-manifest.py" \
                     list-compile-checks --builder "$_cc_builder")
    done
    for _id in $ids_filter; do
        case "$_cc_known" in
            *" $_id "*) ;;
            *) echo "compile-check: NROS_FIXTURE_IDS names '$_id', which is not a row this script builds" >&2
               exit 2 ;;
        esac
    done
    unset _cc_known _cc_builder _id _rest
fi

if [ -z "$id_filter" ] && { [ "${NROS_COMPILE_CHECK_POOL:-1}" = "1" ] || [ -n "$ids_filter" ]; }; then
    _cc_ids=""
    for _cc_builder in $_cc_all_builders; do
        while IFS=$'\x1f' read -r _id _rest; do
            [ -n "$_id" ] || continue
            case " $_cc_ids " in *" $_id "*) continue ;; esac
            if [ -n "$ids_filter" ]; then
                case " $ids_filter " in *" $_id "*) ;; *) continue ;; esac
            fi
            _cc_ids="$_cc_ids $_id"
        done < <(compile_check_records "$_cc_builder")
    done
    unset _cc_builder _id _rest
    if [ -n "$_cc_ids" ]; then
        # shellcheck source=scripts/build/jobserver-pool.sh
        source "$repo_root/scripts/build/jobserver-pool.sh"
        _cc_rc=0
        # `env -u NROS_FIXTURE_IDS`: a unit is ONE row, and the set filter it
        # would otherwise inherit is exclusive with the id filter it is given.
        _cc_units() {
            for _id in $_cc_ids; do
                printf 'env -u NROS_FIXTURE_IDS NROS_FIXTURE_ID=%s NROS_COMPILE_CHECK_POOL=0 bash %s/scripts/build/compile-check-fixtures.sh\n' \
                    "$_id" "$repo_root"
            done
        }
        if [ "${NROS_COMPILE_CHECK_POOL:-1}" = "1" ]; then
            nros_pool_run compile-check < <(_cc_units) || _cc_rc=$?
        else
            # Pool disabled but an id SET asked for: the same units, serially,
            # KEEPING GOING past a failed unit (the pool's own serial fallback
            # does the same) — the lane census stages this way, so one broken
            # row costs that row's targets their admission and nothing else.
            while IFS= read -r _cc_unit; do
                bash -c "$_cc_unit" || _cc_rc=1
            done < <(_cc_units)
        fi
        # Each unit prints its OWN one-row summary, so the parent must print the
        # aggregate itself — otherwise the last unit's counts (check=1 …) read
        # as the whole stage's.
        if [ "$_cc_rc" = "0" ]; then
            echo "compile-check fixtures built: $(printf '%s\n' $_cc_ids | wc -l) row(s) across $(printf '%s\n' $_cc_all_builders | wc -l) builders."
        fi
        exit $_cc_rc
    fi
    if [ -n "$ids_filter" ]; then
        # Every named id was filtered out by the builder narrowing — say so;
        # falling through would build EVERY row, the opposite of what was asked.
        echo "compile-check: NROS_FIXTURE_IDS selected no row under NROS_FIXTURE_BUILDER='${builder_filter}' — nothing built."
        exit 0
    fi
    unset _cc_ids
fi


# phase-319 W3 (issue 0351) — record the build INPUTS after a successful build.
#
# `.compile-ok` says only THAT a build succeeded, never what from, so a source
# edit left it valid-looking forever. `.inputsig` is the workspace lane's answer
# (`workspace-fixture-signature.sh`): written only on success, recomputed and
# compared by the staleness probe. A failed build leaves the OLD signature
# untouched — but its `.compile-ok`/artifact was already removed by the builder,
# so "failed" and "stale" both surface, never "fresh".
# phase-319 W3 (issue 0351) — mark a fixture whose build FAILED, so the test-side
# resolver can tell "broken" from "toolchain absent". Both used to present as a
# missing artifact, and the light tier skipped on both — which is how issue 0350
# stayed green while this whole lane was red.
#
# Cleared at the start of every attempt (same discipline as `.compile-ok`), so a
# marker only ever describes the most recent run.
clear_build_failed() {
    rm -f "$1/.build-failed" 2>/dev/null || true
}

mark_build_failed() {
    local stamp_dir="$1" id="$2" builder="$3"
    mkdir -p "$stamp_dir"
    printf 'fixture %s (builder %s) failed to build at %s\n' \
        "$id" "$builder" "$(date -u +%Y-%m-%dT%H:%M:%SZ)" > "$stamp_dir/.build-failed"
}

# Run one builder, marking + re-raising on failure. `set -e` still aborts the
# script afterwards (fail-fast is deliberate); the marker is what survives for
# the resolver.
#
# The builder runs in a SUBSHELL with its own `set -e`, NOT as `if ! builder`.
# Bash suppresses errexit for the entire body of a function invoked in a
# condition context, so `if ! build_cmake_fixture …` let a failing `cmake -S`
# fall through to the next line and the function returned its trailing `echo`'s
# status — a broken fixture reported as built. Caught by this phase's own
# acceptance test; the subshell keeps errexit live where the work happens and
# still lets us handle the status.
# Marking uses an ERR TRAP, not a status check, because bash disables errexit for
# anything in a condition context — `if ! builder`, `builder || rc=$?`, AND a
# `( set -e; builder )` subshell inside such a list all let a failing `cmake -S`
# fall through to the next line, so the function returned its trailing `echo`'s
# status and a broken fixture reported as BUILT. Both wrong shapes were caught by
# this phase's own acceptance test before landing.
#
# With the trap there is no condition context: the builder is called bare, so
# errexit still aborts the script (fail-fast is deliberate) and the trap records
# WHICH fixture died on the way out. Needs `set -E` so functions inherit it.
CURRENT_FIXTURE_STAMP_DIR=""
CURRENT_FIXTURE_ID=""
CURRENT_FIXTURE_BUILDER=""

on_fixture_err() {
    [ -n "$CURRENT_FIXTURE_STAMP_DIR" ] || return 0
    mark_build_failed "$CURRENT_FIXTURE_STAMP_DIR" "$CURRENT_FIXTURE_ID" "$CURRENT_FIXTURE_BUILDER"
}
trap on_fixture_err ERR

run_fixture() {
    local stamp_dir="$1" id="$2" builder="$3"; shift 3
    clear_build_failed "$stamp_dir"
    CURRENT_FIXTURE_STAMP_DIR="$stamp_dir"
    CURRENT_FIXTURE_ID="$id"
    CURRENT_FIXTURE_BUILDER="$builder"
    "$@"
    CURRENT_FIXTURE_STAMP_DIR=""
}

write_compile_check_sig() {
    local record="$1" stamp_dir="$2"
    mkdir -p "$stamp_dir"
    bash "$repo_root/scripts/build/compile-check-signature.sh" "$record" \
        > "$stamp_dir/.inputsig" 2>/dev/null || rm -f "$stamp_dir/.inputsig"
}

# cargo-check. A row with a TARGET is an in-place cross-check of an existing
# example dir; without one it is a staged `cargo check` whose stamp is the proof.
while IFS=$'\x1f' read -r id builder dir pkg mdir target profiles output; do
    [ -n "$id" ] || continue
    [ -n "$target" ] && continue
    run_fixture "$out_root/$id" "$id" "$builder" stage_and_check "$id" "$dir"
    write_compile_check_sig "$id$(printf '\x1f')$builder$(printf '\x1f')$dir$(printf '\x1f')$pkg$(printf '\x1f')$mdir$(printf '\x1f')$target$(printf '\x1f')$profiles$(printf '\x1f')$output" "$out_root/$id"
done < <(_lane_on cargo-check && compile_check_records cargo-check || true)

# cargo-clippy. No target-bearing variant: a cross clippy would need the target
# toolchain, and no row asks for one.
while IFS=$'\x1f' read -r id builder dir pkg mdir target profiles output; do
    [ -n "$id" ] || continue
    run_fixture "$out_root/$id" "$id" "$builder" stage_and_clippy "$id" "$dir"
    write_compile_check_sig "$id$(printf '\x1f')$builder$(printf '\x1f')$dir$(printf '\x1f')$pkg$(printf '\x1f')$mdir$(printf '\x1f')$target$(printf '\x1f')$profiles$(printf '\x1f')$output" "$out_root/$id"
done < <(_lane_on cargo-clippy && compile_check_records cargo-clippy || true)

# cargo-check-verdict (issue 1620) — record the compile's verdict, never fail on it.
while IFS=$'\x1f' read -r id builder dir pkg mdir target profiles output; do
    [ -n "$id" ] || continue
    run_fixture "$out_root/$id" "$id" "$builder" stage_and_check_verdict "$id" "$dir"
    write_compile_check_sig "$id$(printf '\x1f')$builder$(printf '\x1f')$dir$(printf '\x1f')$pkg$(printf '\x1f')$mdir$(printf '\x1f')$target$(printf '\x1f')$profiles$(printf '\x1f')$output" "$out_root/$id"
done < <(_lane_on cargo-check-verdict && compile_check_records cargo-check-verdict || true)

# cmake-configure-verdict (issue 1620). Stamps under compile-check/<id>, NOT
# cmake-fixtures/<id>: it shares the verdict contract with the cargo rows above,
# and the signature/stale probe key their dir on `builder = cmake-configure`.
while IFS=$'\x1f' read -r id builder dir pkg mdir target profiles output; do
    [ -n "$id" ] || continue
    run_fixture "$out_root/$id" "$id" "$builder" stage_and_configure_verdict "$id" "$dir"
    write_compile_check_sig "$id$(printf '\x1f')$builder$(printf '\x1f')$dir$(printf '\x1f')$pkg$(printf '\x1f')$mdir$(printf '\x1f')$target$(printf '\x1f')$profiles$(printf '\x1f')$output" "$out_root/$id"
done < <(_lane_on cmake-configure-verdict && compile_check_records cmake-configure-verdict || true)

while IFS=$'\x1f' read -r id builder dir pkg mdir target profiles output; do
    [ -n "$id" ] || continue
    run_fixture "$out_root/$id" "$id" "$builder" \
        stage_and_build "$id" "$dir" "${mdir:-.}" "${pkg:-demo_entry}"
    write_compile_check_sig "$id$(printf '\x1f')$builder$(printf '\x1f')$dir$(printf '\x1f')$pkg$(printf '\x1f')$mdir$(printf '\x1f')$target$(printf '\x1f')$profiles$(printf '\x1f')$output" "$out_root/$id"
done < <(_lane_on cargo-build && compile_check_records cargo-build || true)

while IFS=$'\x1f' read -r id builder dir pkg mdir target profiles output; do
    [ -n "$id" ] || continue
    run_fixture "$out_root/$id" "$id" "$builder" \
        stage_and_cross_build "$id" "$dir" "${mdir:-.}" "$pkg" "$target" "${profiles:-debug}"
    write_compile_check_sig "$id$(printf '\x1f')$builder$(printf '\x1f')$dir$(printf '\x1f')$pkg$(printf '\x1f')$mdir$(printf '\x1f')$target$(printf '\x1f')$profiles$(printf '\x1f')$output" "$out_root/$id"
done < <(_lane_on cross-build && compile_check_records cross-build || true)
# C++ syntax-only compile-checks (id : snippet.cpp under
# packages/testing/nros-tests/fixtures/cpp_compat_snippets/). `c++ -fsyntax-only`
# the snippet against the nros-cpp / nros-c / compat include set — a compile-only
# proof the public C++ API headers type-check. Stamped into build/compile-check
# (same resolver as the cargo compile-checks).
snippet_dir="$repo_root/packages/testing/nros-tests/fixtures/cpp_compat_snippets"

# `cxx_syntax_check <id> [verdict]`. With `verdict` it is the `cxx-syntax-verdict`
# builder (issue 1656): the SAME compile over the same include set, but its exit
# status and diagnostics are RECORDED (`.verdict`, `verdict.stderr`) and the
# build succeeds whatever the compiler said — the shape issue 1620 gave cargo and
# cmake, for a snippet that MUST FAIL (`platform_hdr_baremetal_heap_no_malloc`).
cxx_syntax_check() {
    local id="$1" mode="${2:-stamp}"
    local src="$snippet_dir/$id.cpp"
    local staged="$out_root/$id"
    [ -f "$src" ] || { echo "cxx-syntax: snippet missing: $src" >&2; return 2; }
    echo "== cxx-syntax${mode/#stamp/}: $id =="
    mkdir -p "$staged"
    rm -f "$staged/.compile-ok" "$staged/.verdict" "$staged"/verdict.*
    local cxx="${CXX:-c++}"
    # Issue #34 — the per-build generated config headers MUST precede the
    # source include dir: `packages/api/nros-cpp/include/nros/nros_cpp_config_generated.h`
    # is a stub that `#error`s, so if it is searched first the real header
    # (`target/nros-cpp-generated/nros/...`, emitted by nros-cpp's build.rs) is
    # never reached. Prepend the generated dirs.
    local inc=()
    [ -f "$repo_root/target/nros-cpp-generated/nros/nros_cpp_config_generated.h" ] \
        && inc+=(-I "$repo_root/target/nros-cpp-generated")
    [ -f "$repo_root/target/nros-c-generated/nros/nros_config_generated.h" ] \
        && inc+=(-I "$repo_root/target/nros-c-generated")
    # phase-329 W5 — the platform-header snippets `#include <nros/platform.h>`,
    # which lives ONLY in nros-platform-api (the RFC-0042 D1 canonical header).
    # Prepend it so it resolves; unique location, so no shadowing risk.
    inc+=(-I "$repo_root/packages/platform/nros-platform-api/include"
          -I "$repo_root/packages/api/nros-cpp/include"
          -I "$repo_root/packages/api/nros-c/include")
    # Best-effort: a snippet that doesn't compile (pre-existing API drift or a
    # missing generated header) does NOT fail build-test-fixtures — it just
    # leaves no `.compile-ok`, so the consuming test reports the gap per tier
    # (hard-fail full / [SKIPPED] light). The compile error is in this log.
    #
    # issue 1032 — that sentence was an ASSERTION ABOUT ANOTHER FILE, and it was
    # false for three snippets. `cpp_api_drift.rs` caught the error and skipped
    # unconditionally, and `spin_until_future_complete` had no consumer at all,
    # so "does not fail here because it fails there" failed NOWHERE. Deferring a
    # report is only safe while something is on the other end; if you add a
    # snippet, add its assertion in the same commit.
    # phase-363 W4 — `-MD -MF` so the compiler records which headers it actually
    # read. Without it these rows had NO measured closure: their signature was
    # the snippet plus two hand-named include TREES, so a header reached through
    # a third path (nros-platform-api, or a generated
    # config header under `target/`) was invisible. `-MD` composes with
    # `-fsyntax-only` — no object is produced, the dep list still is.
    # phase-438 W2 — the std surface is REQUESTED now, never discovered from the
    # include path, so a snippet that ports rclcpp code has to ask.
    #
    # A RULE, not a list, because a list drifts: `platform_hdr_*` snippets exist
    # to prove `<nros/platform.h>` type-checks on a FREESTANDING target, and
    # handing them the std surface would weaken exactly what they measure. Every
    # other snippet here reaches `<rclcpp/rclcpp.hpp>` — the ported flavour —
    # and gets the flag, matching what `cmake/NanoRosAmentSurface.cmake` sets
    # for a real consumer.
    #
    # Measured need: `rclcpp_node_options` (rclcpp::NodeOptions) and
    # `spin_until_future_complete` (rclcpp::Node) fail without it;
    # `subscription_with_info` does not. All three are on the same side of the
    # rule, which is the point of having one.
    local std_opt=()
    case "$id" in
        platform_hdr_*) ;;
        *) std_opt=(-DNROS_CPP_STD=1) ;;
    esac
    rm -f "$staged/deps.d"
    if [ "$mode" = verdict ]; then
        _cc_resolver_arg=""
        _cc_capture "$staged" verdict "$cxx" -std=c++14 -fsyntax-only \
            "${std_opt[@]+"${std_opt[@]}"}" -MD -MF "$staged/deps.d" "${inc[@]}" "$src"
        _cc_write_verdict_stamp "$staged" "$_cc_last_rc"
        echo "   recorded $staged/.verdict (exit=$_cc_last_rc)"
        return 0
    fi
    if "$cxx" -std=c++14 -fsyntax-only "${std_opt[@]+"${std_opt[@]}"}" -MD -MF "$staged/deps.d" "${inc[@]}" "$src"; then
        nros_write_compile_ok "$staged"
        echo "   stamped $staged/.compile-ok"
    else
        echo "   cxx-syntax FAILED for $id (no stamp; consuming test will report)" >&2
    fi
}

# cxx-compile-verdict (issue 1656) — ONE compile of a fixture dir's source with
# an argument list the row states as a FILE (`<dir>/cases/<id>.args`, one
# argument per line, `#` comments), by the compiler for the row's `target`
# (`<target>-g++`, newest SDK-store install first, then PATH; no target: the
# host `${CXX:-c++}`). Recorded like every verdict row, plus `verdict.tool`:
# the compiler path, or `absent`. A missing CROSS compiler is a recorded
# verdict, not a build failure — the consuming test decides whether that is a
# skip, as `cross_libc_precedence_gate.rs` did when it ran the compiler itself.
#
# An argument naming a file or directory under the fixture dir is made
# ABSOLUTE, so the `-MD` closure names repo paths `dep-closure.py` can keep
# (it resolves relative entries against the repo root or the depfile's dir —
# never against the compile's cwd).
_cc_resolve_cxx() {
    local target="$1" bin
    if [ -z "$target" ]; then
        command -v "${CXX:-c++}" 2>/dev/null || true
        return 0
    fi
    while IFS= read -r bin; do
        [ -x "$bin" ] && { printf '%s\n' "$bin"; return 0; }
    done < <(compgen -G "$HOME/.nros/sdk/${target}-gcc/*/bin/${target}-g++" | sort -Vr)
    if "${target}-g++" --version >/dev/null 2>&1; then
        command -v "${target}-g++"
    fi
    return 0
}

cxx_compile_verdict() {
    local id="$1" dir="$2" target="$3"
    local src="$repo_root/$dir" staged="$out_root/$id"
    local argfile="$src/cases/$id.args"
    [ -f "$argfile" ] || { echo "cxx-compile-verdict: $id: no argument file $argfile" >&2; return 2; }
    echo "== cxx-compile-verdict: $id =="
    mkdir -p "$staged"
    rm -f "$staged/.verdict" "$staged"/verdict.* "$staged/deps.d"
    _cc_resolver_arg=""
    local args=() a
    while IFS= read -r a || [ -n "$a" ]; do
        case "$a" in ''|'#'*) continue ;; esac
        if [ -e "$src/$a" ]; then a="$src/$a"; fi
        args+=("$a")
    done < "$argfile"
    local cxx; cxx="$(_cc_resolve_cxx "$target")"
    if [ -z "$cxx" ]; then
        printf 'absent\n' > "$staged/verdict.tool"
        : > "$staged/verdict.stdout"
        printf 'no compiler for target %s (%s-g++ not in the SDK store or on PATH)\n' \
            "${target:-host}" "$target" > "$staged/verdict.stderr"
        printf '127\n' > "$staged/verdict.exit"
        _cc_last_rc=127
    else
        printf '%s\n' "$cxx" > "$staged/verdict.tool"
        _cc_capture "$staged" verdict "$cxx" "${args[@]}" -MD -MF "$staged/deps.d" -o /dev/null
    fi
    _cc_write_verdict_stamp "$staged" "$_cc_last_rc"
    echo "   recorded $staged/.verdict (exit=$_cc_last_rc, tool=$(cat "$staged/verdict.tool"))"
}

# fixture-script (issue 1656) — a build-stage proof whose recipe is a SCRIPT,
# kept as `<dir>/build.sh` so the row's `.inputsig` (which hashes `dir`) covers
# it. `build.sh <row-dir>` writes its artifacts and any `*.d` dep-info into the
# row dir and exits non-zero on a broken build; the stamp is written here, only
# on success, like every other builder's. `borrowed_e2e` is the one row: it
# had a script, a stamp and a test, and no row, so no lane built it and no
# probe could call it stale.
stage_and_script() {
    local id="$1" dir="$2"
    local staged="$out_root/$id"
    echo "== fixture-script: $id =="
    [ -f "$repo_root/$dir/build.sh" ] || {
        echo "fixture-script: $id: no $dir/build.sh" >&2
        return 2
    }
    mkdir -p "$staged"
    rm -f "$staged/.compile-ok"
    bash "$repo_root/$dir/build.sh" "$staged"
    nros_write_compile_ok "$staged"
    echo "   stamped $staged/.compile-ok"
}

while IFS=$'\x1f' read -r id builder dir pkg mdir target profiles output; do
    [ -n "$id" ] || continue
    run_fixture "$out_root/$id" "$id" "$builder" stage_and_script "$id" "$dir"
    write_compile_check_sig "$id$(printf '\x1f')$builder$(printf '\x1f')$dir$(printf '\x1f')$pkg$(printf '\x1f')$mdir$(printf '\x1f')$target$(printf '\x1f')$profiles$(printf '\x1f')$output" "$out_root/$id"
done < <(_lane_on fixture-script && compile_check_records fixture-script || true)

while IFS=$'\x1f' read -r id builder dir pkg mdir target profiles output; do
    [ -n "$id" ] || continue
    run_fixture "$out_root/$id" "$id" "$builder" cxx_compile_verdict "$id" "$dir" "$target"
    write_compile_check_sig "$id$(printf '\x1f')$builder$(printf '\x1f')$dir$(printf '\x1f')$pkg$(printf '\x1f')$mdir$(printf '\x1f')$target$(printf '\x1f')$profiles$(printf '\x1f')$output" "$out_root/$id"
done < <(_lane_on cxx-compile-verdict && compile_check_records cxx-compile-verdict || true)

cmake_n=0
if cmake_fixture_prereqs_ok; then
    mkdir -p "$cmake_out"
    while IFS=$'\x1f' read -r id builder dir pkg mdir target profiles output; do
        [ -n "$id" ] || continue
        run_fixture "$cmake_out/$id" "$id" "$builder" build_cmake_fixture "$id" "$dir"
        write_compile_check_sig "$id$(printf '\x1f')$builder$(printf '\x1f')$dir$(printf '\x1f')$pkg$(printf '\x1f')$mdir$(printf '\x1f')$target$(printf '\x1f')$profiles$(printf '\x1f')$output" "$cmake_out/$id"
        cmake_n=$((cmake_n + 1))
    done < <(_lane_on cmake-configure && compile_check_records cmake-configure || true)
    # Phase 246 — the ThreadX `threadx_bringup_rv64` configure-only baker-audit
    # leg is retired with `NanoRosThreadxSystemCodegen.cmake`; the bare-metal
    # riscv64 typed-carrier examples (examples/rv-virt-threadx/{c,cpp}/*)
    # cover the real path.
fi

cxx_n=0
cxx_skipped=""
# Only when a snippet row is actually selected: the header build below is a
# `cargo build -p nros-cpp -p nros-c`, and every pool unit used to run it — for
# a cargo or cmake row too — because this branch asked only "is there a C++
# compiler" (issue 1656).
_cc_cxx_rows="$( { _lane_on cxx-syntax && compile_check_records cxx-syntax; } || true)"
_cc_cxxv_rows="$( { _lane_on cxx-syntax-verdict && compile_check_records cxx-syntax-verdict; } || true)"
if [ -z "$_cc_cxx_rows$_cc_cxxv_rows" ]; then
    :
elif command -v "${CXX:-c++}" >/dev/null 2>&1; then
    # Issue #34 — generate the per-build config headers the snippets need
    # (`nros_cpp_config_generated.h` / `nros_config_generated.h`). nros-cpp's /
    # nros-c's build.rs emit them under `target/nros-{cpp,c}-generated/` on a host
    # build; `cxx_syntax_check` then prepends those dirs. Best-effort: if the host
    # cargo build fails, the headers stay absent and the snippets that include
    # them leave no stamp (consuming test reports the gap per tier). The sizes
    # need not be exact — this is a `-fsyntax-only` check, not a link.
    echo "== generating nros-cpp / nros-c config headers for cxx-syntax =="
    # phase-361 W3 — `std` is EXPLICIT. `nros-c`/`nros-cpp` used to default to
    # it; now `default = []`, and this is a HOST build, so without asking the
    # build is `no_std` and dies on `#[panic_handler]` / "unwinding panics are
    # not supported without std". It failed into the `|| echo` below, which
    # loses the headers silently — the exact shape issue 0464 is about.
    #
    # The RMW backend is load-bearing, not decoration. Both build scripts get
    # their sizes from `probe_nros_sizes`, which builds `nros` with the features
    # this command resolves; with no backend selected the probe returns
    # `EXECUTOR_SIZE = 0` and BOTH scripts then decline to write the header at
    # all ("no RMW backend means no executor sizes to ship"). The build still
    # exits 0, so the `|| echo` below never fires, and the three snippets that
    # reach `nros.hpp` fail against the committed stub:
    #
    #   nros_config_generated.h:37:2: error: #error "must be supplied per-build"
    #   polling_action_server.hpp:231:44: error: NROS_CPP_RAW_ACTION_SERVER_OPAQUE_U64S was not declared
    #
    # That was every scheduled run's `rclcpp_node_options`,
    # `subscription_with_info` and `spin_until_future_complete`. It read as a
    # CI-only fault and was not: on a developer machine the headers are left
    # over from some other build that DID select a backend, so the lane passes
    # on residue. Issue 1031.
    ( cd "$repo_root" && cargo build -q -p nros-cpp -p nros-c \
        --features nros-cpp/std,nros-c/std,nros-cpp/ros-humble,nros-cpp/rmw-zenoh-cffi ) \
        || echo "cxx-syntax: config-header generation build failed (snippets needing them will skip)" >&2
    # A build that exits 0 having written nothing is the case the `||` above
    # cannot see. Say so — the snippets fail either way, but their error is
    # `#error "must be supplied per-build"` twenty frames into a header, which
    # names the stub rather than the step that was supposed to replace it.
    for _h in target/nros-cpp-generated/nros/nros_cpp_config_generated.h \
              target/nros-c-generated/nros/nros_config_generated.h; do
        [ -f "$repo_root/$_h" ] || echo "cxx-syntax: config-header generation produced NO $_h \
(the build succeeded) — snippets including it will fail against the committed stub" >&2
    done
    while IFS=$'\x1f' read -r id builder dir pkg mdir target profiles output; do
        [ -n "$id" ] || continue
        run_fixture "$out_root/$id" "$id" "$builder" cxx_syntax_check "$id"
        write_compile_check_sig "$id$(printf '\x1f')$builder$(printf '\x1f')$dir$(printf '\x1f')$pkg$(printf '\x1f')$mdir$(printf '\x1f')$target$(printf '\x1f')$profiles$(printf '\x1f')$output" "$out_root/$id"
        cxx_n=$((cxx_n + 1))
    done <<< "$_cc_cxx_rows"
    while IFS=$'\x1f' read -r id builder dir pkg mdir target profiles output; do
        [ -n "$id" ] || continue
        run_fixture "$out_root/$id" "$id" "$builder" cxx_syntax_check "$id" verdict
        write_compile_check_sig "$id$(printf '\x1f')$builder$(printf '\x1f')$dir$(printf '\x1f')$pkg$(printf '\x1f')$mdir$(printf '\x1f')$target$(printf '\x1f')$profiles$(printf '\x1f')$output" "$out_root/$id"
        cxx_n=$((cxx_n + 1))
    done <<< "$_cc_cxxv_rows"
else
    cxx_skipped="no C++ compiler (${CXX:-c++})"
    _note_lane_skip "cxx-syntax: $cxx_skipped"
fi

# cargo-check of an existing example dir for a cross target (id : dir : target).
# Proves an example's `nros::main!()` emit type-checks WITHOUT linking — for
# examples that intentionally don't link standalone (e.g. talker-embassy lacks
# the board memory layout). Stamped into build/compile-check (same resolver).
# Gated on the rust target being installed; absent → no stamp → test skips.
cargo_check_n=0
while IFS=$'\x1f' read -r id builder dir pkg mdir target profiles output; do
    [ -n "$id" ] || continue
    # Only the target-bearing cargo-check rows reach here; the staged ones ran above.
    [ -n "$target" ] || continue
    [ -d "$repo_root/$dir" ] || { echo "cargo-check: example missing: $dir" >&2; continue; }
    if ! rustup target list --installed 2>/dev/null | grep -qx "$target"; then
        echo "cargo-check: target $target not installed — skipping $id" >&2
        continue
    fi
    echo "== cargo-check: $id ($target) =="
    mkdir -p "$out_root/$id"
    rm -f "$out_root/$id/.compile-ok"
    if ( cd "$repo_root/$dir" && cargo check --target "$target" ); then
        nros_write_compile_ok "$out_root/$id"
        echo "   stamped $out_root/$id/.compile-ok"
        write_compile_check_sig "$id$(printf '\x1f')$builder$(printf '\x1f')$dir$(printf '\x1f')$pkg$(printf '\x1f')$mdir$(printf '\x1f')$target$(printf '\x1f')$profiles$(printf '\x1f')$output" "$out_root/$id"
        cargo_check_n=$((cargo_check_n + 1))
    else
        echo "   cargo-check FAILED for $id (no stamp)" >&2
    fi
done < <(_lane_on cargo-check-target && compile_check_records cargo-check || true)

# px4 xrce companion examples (#102 / #136 debt). Compile-check only: the
# runtime needs PX4 SITL + a Micro-XRCE-DDS agent, but the generated CDR
# bindings must at least type-check. `px4_msgs` is generated from the vendored
# PX4-Autopilot `.msg` tree by `nros generate-px4-msgs` (no ament/pip dep, just
# the submodule) into each example's gitignored `generated/`. Gated on that
# submodule being checked out; absent → no stamp → the coverage gate keeps
# these as tracked leaves rather than a silent gap.
PX4_XRCE_EXAMPLES=(
    "px4_probe:examples/px4/rust/companion/px4-probe"
    "px4_stub:examples/px4/rust/companion/px4-stub"
    "px4_offboard_companion:examples/px4/rust/companion/offboard-companion"
)
# shellcheck source=scripts/build/fixtures-target-dir.sh
source "$repo_root/scripts/build/fixtures-target-dir.sh"

px4_autopilot_dir="$repo_root/third-party/px4/PX4-Autopilot"
px4_n=0
px4_skipped=""
px4_fail_n=0
if _lane_on px4 && [ -d "$px4_autopilot_dir/msg" ] && command -v nros >/dev/null 2>&1; then
    # issue 0520 — this script is invoked ONCE PER COMPILE-CHECK UNIT (87 of them
    # under `build-test-fixtures lane=all`, in parallel), and every invocation
    # regenerates px4_msgs into the SAME three `<leaf>/generated` dirs. The
    # generator stages the `.msg` tree through `<output>/.px4_msg_stage` and
    # `remove_dir_all`s it on the way out, so concurrent runs delete each other's
    # staging mid-copy. It surfaces as a SOURCE file that plainly exists:
    #
    #     Error: stage .../PX4-Autopilot/msg/GpsDump.msg
    #     Caused by: No such file or directory (os error 2)
    #
    # 15 different `.msg` names across one run, all 201 present on disk. A
    # repo-level advisory lock makes the second invocation queue instead of
    # clobbering the first — the same idiom, and the same reasoning, as the
    # zephyr fixture build lock. flock-absent hosts skip it (best-effort).
    # The lock wraps the CODEGEN CALL ONLY. The `cargo check` below is per-leaf,
    # touches no shared staging, and is the long pole — holding the lock across
    # it would serialize 87 units on nothing.
    # RFC-0070 R1 — the ONE derivation, never a hand-spelled cache path
    # (`check-build-root` gates it). Same shape as the zephyr build lock.
    px4_lockfile="$(nros_build_dir "$NROS_KIND_PX4_MSGS_CODEGEN").lock"
    mkdir -p "$(dirname "$px4_lockfile")"
    px4_gen() {
        # `flock <file> <command>` — the FILE form. NOT `flock 8 <command>`:
        # with a command argument flock treats its first operand as a PATH, not
        # a file descriptor, so that spelling silently created and locked a file
        # named `8` in the cwd. It excluded correctly (same relative path, same
        # cwd) and left an empty `./8` in the repo root as the only evidence.
        if command -v flock >/dev/null 2>&1; then
            flock "$px4_lockfile" nros generate-px4-msgs --px4 "$1" --output "$2"
        else
            nros generate-px4-msgs --px4 "$1" --output "$2"
        fi
    }
    for entry in "${PX4_XRCE_EXAMPLES[@]}"; do
        id="${entry%%:*}"; dir="${entry#*:}"
        [ -d "$repo_root/$dir" ] || { echo "px4: example missing: $dir" >&2; continue; }
        echo "== px4-compile-check: $id =="
        if ! px4_gen "$px4_autopilot_dir" "$repo_root/$dir/generated"; then
            echo "   px4_msgs codegen FAILED for $id (no stamp)" >&2
            continue
        fi
        # issue 0546 — SYNC before checking. These leaves name the runtime by
        # REGISTRY name (`nros = { version = "*" }`), which is normal for an
        # example leaf here: `nros sync` writes the `.cargo/config.toml` whose
        # `[patch.crates-io]` redirects those names at in-repo paths (RFC-0048
        # W9). This block codegen'd `generated/px4_msgs` and then checked
        # WITHOUT syncing, so `version = "*"` resolved the only way left to it —
        # against the public crates.io index:
        #
        #     error: no matching package named `nros` found
        #     location searched: crates.io index
        #
        # No `.cargo/config.toml` exists for these leaves in the repository
        # (`git ls-files examples/px4 | grep -c cargo` is 0), so this was not a
        # host quirk: every checkout that ran the px4 compile-check hit it, and
        # the bindings this block exists to type-check never once did.
        px4_cli="${NROS_CLI_BIN:-${NROS_CLI:-$(command -v nros || true)}}"
        if [ -z "$px4_cli" ]; then
            echo "   px4: nros CLI not found — cannot sync $id (just setup-cli)" >&2
            px4_fail_n=$((px4_fail_n + 1))
            continue
        fi
        if ! ( cd "$repo_root/$dir" && "$px4_cli" sync >/dev/null ); then
            echo "   nros sync FAILED for $id (no stamp)" >&2
            px4_fail_n=$((px4_fail_n + 1))
            continue
        fi
        mkdir -p "$out_root/$id"
        rm -f "$out_root/$id/.compile-ok"
        # phase-340 P2 — pass a `--target-dir`, never the leaf's default.
        #
        # This was a bare `cargo check`, which writes `<leaf>/target/`. That is
        # the exact defect `check-example-leaf-target-dirs` was written for (the
        # freertos `cd <leaf> && cargo build` case), and its STATIC scan could
        # not see it: `dir` here comes from the `"id:path"` entries of
        # `PX4_LEAVES`, so the "variable assigned a value containing examples/"
        # heuristic never fires. The gate's new EXISTENCE half is what caught it
        # — three `examples/px4/rust/companion/*/target` dirs on disk while the
        # command scan reported OK (issue 0196's shape: coverage narrower than
        # the rule).
        #
        # Derived, not a literal, per CLAUDE.md: one group for the host
        # platform, shared with every other cargo fixture at that coordinate.
        # Nothing reads these artifacts — the contract is the `.compile-ok`
        # stamp below — so there is no test-side locator to move with it.
        px4_tdir_flag="$(nros_fixture_target_dir_flag linux)"
        # shellcheck disable=SC2086
        if ( cd "$repo_root/$dir" && cargo check $px4_tdir_flag ); then
            nros_write_compile_ok "$out_root/$id" --resolver
            echo "   stamped $out_root/$id/.compile-ok"
            px4_n=$((px4_n + 1))
        else
            echo "   cargo-check FAILED for $id (no stamp)" >&2
            px4_fail_n=$((px4_fail_n + 1))
        fi
    done

    # issue 0738 — the C++ emitter and the bridge that consumes it were built by
    # NO lane: `just px4 build-bridge-example` had exactly one grep hit, its own
    # definition. So `generate-px4-msgs --lang cpp`, the headers it writes, the
    # `_types.rs`/`_exports.rs` FFI bodies and the crate that includes them could
    # all break with nothing to say so — and issue 0360 already flags that output
    # as a per-variant artifact that must stay paired with its archive.
    #
    # Stages [1/4] and [2/4] of that recipe ONLY. Stage [4/4] is a PX4 SITL
    # `make`, which is far too heavy for a per-change tier and stays on demand;
    # the codegen risk is not there, it is in the emitter and the header shape.
    # What this adds:
    #   1. generate for the bridge's topic set          — the emitter runs
    #   2. compile the generated .hpp standalone (1 TU) — the header parses
    #   3. cargo check the FFI crate                    — the Rust bodies match
    #
    # `debug_key_value` mirrors the recipe's default topic. It does not have to
    # match it — the FFI `build.rs` globs whatever the generator wrote, which is
    # the whole reason the topic list is not restated in the crate.
    px4_bridge_dir="$repo_root/examples/px4/cpp/bridge"
    if [ -d "$px4_bridge_dir/ffi" ]; then
        id="px4_bridge_ffi"
        echo "== px4-compile-check: $id =="
        bridge_gen="$(nros_build_dir "$NROS_KIND_PX4_MSGS_CODEGEN")/bridge-cpp"
        bridge_ok=1
        # issue 0742 — the CRITICAL SECTION is the whole block, not the
        # generator call. This script runs once per compile-check unit, in
        # parallel (32 of them on `lane=native`), and every one of them drives
        # this same `bridge-cpp` path: the `rm -rf` below deletes a sibling's
        # output, its `.px4_msg_stage` and the headers a third one is about to
        # read. The lock that used to wrap only `nros generate-px4-msgs` left
        # all three outside it, so the failures land on files that plainly
        # exist:
        #
        #     Error: write header for DebugKeyValue: No such file or directory
        #     Error: read message file .../.px4_msg_stage/msg/DebugKeyValue.msg
        #     rm: cannot remove '.../.px4_msg_stage/msg': Directory not empty
        #     cc1plus: fatal error: .../debug_key_value.hpp: No such file
        #
        # Extending the lock is nearly free here, unlike the Rust path above
        # where the note about not serializing 87 `cargo check`s belongs: this
        # generates ONE message, syntax-checks ONE header, and its `cargo check`
        # is already serialized by cargo's own build-directory lock (the run log
        # is full of "Blocking waiting for file lock on build directory").
        _px4_bridge_locked=0
        if command -v flock >/dev/null 2>&1; then
            exec 9>"$px4_lockfile"
            flock 9 && _px4_bridge_locked=1
        fi
        rm -rf "$bridge_gen"; mkdir -p "$bridge_gen"
        nros generate-px4-msgs --px4 "$px4_autopilot_dir" --lang cpp \
            --ros-edition jazzy --topics debug_key_value -o "$bridge_gen" || bridge_ok=0
        [ "$bridge_ok" = 1 ] || echo "   px4_msgs C++ codegen FAILED for $id (no stamp)" >&2

        # The header must PARSE on its own. A generated header that only compiles
        # inside the bridge's own TU is the shape that breaks a consumer nobody
        # is building — `-fsyntax-only`, no link, no PX4 headers needed.
        if [ "$bridge_ok" = 1 ]; then
            cxx="${CXX:-c++}"
            if command -v "$cxx" >/dev/null 2>&1; then
                # The include set a PX4 module actually gets, read off the one
                # file that defines it (`_NROS_PX4_INCLUDES` in
                # integrations/px4/NanoRosPx4Module.cmake) rather than restated
                # here — that file's own comment records being born with the
                # wrong paths, which is the argument against a second copy. Only
                # the two the generated headers reach are needed; parsing the
                # cmake list for a syntax check would be more machinery than the
                # check, so the pair is named with a pointer to its source.
                bridge_incs=(
                    -I "$bridge_gen"
                    -I "$repo_root/packages/api/nros-cpp/include"
                    -I "$repo_root/packages/platform/nros-platform-api/include"
                )
                # The per-build config headers, as `cxx_syntax_check` takes
                # them: every generated message header includes
                # `<nros/nros_config_generated.h>` for the RFC-0090 version
                # check (phase-429 W1), and without a real one this failed
                # `fatal error: nros/nros_config_generated.h: No such file`
                # on every host that HAS PX4 — measured 2026-10-06 while
                # proving issue 1700's fix on `lane=tier1`.
                [ -f "$repo_root/target/nros-cpp-generated/nros/nros_cpp_config_generated.h" ] \
                    && bridge_incs+=(-I "$repo_root/target/nros-cpp-generated")
                [ -f "$repo_root/target/nros-c-generated/nros/nros_config_generated.h" ] \
                    && bridge_incs+=(-I "$repo_root/target/nros-c-generated")
                for hpp in "$bridge_gen"/px4_msgs/msg/*.hpp; do
                    [ -f "$hpp" ] || continue
                    case "$(basename "$hpp")" in px4_msgs_msg_*) continue ;; esac
                    if ! "$cxx" -std=c++17 -fsyntax-only "${bridge_incs[@]}" "$hpp"; then
                        echo "   generated header does not compile: $hpp" >&2
                        bridge_ok=0
                    fi
                done
            else
                echo "   px4: no C++ compiler ($cxx) — header syntax check skipped" >&2
            fi
        fi

        if [ "$bridge_ok" = 1 ]; then
            mkdir -p "$out_root/$id"
            rm -f "$out_root/$id/.compile-ok"
            # phase-340 P2 — a derived group dir, never the leaf's `target/`.
            # The RECIPE deliberately uses the leaf default because PX4's make is
            # handed that archive path; a compile-check produces no artifact
            # anyone reads, so it has no reason to write there.
            bridge_tdir_flag="$(nros_fixture_target_dir_flag linux)"
            # shellcheck disable=SC2086
            if ( cd "$px4_bridge_dir/ffi" \
                 && NROS_PX4_BRIDGE_GEN="$bridge_gen" cargo check $bridge_tdir_flag ); then
                nros_write_compile_ok "$out_root/$id"
                echo "   stamped $out_root/$id/.compile-ok"
                px4_n=$((px4_n + 1))
            else
                echo "   cargo-check FAILED for $id (no stamp)" >&2
                px4_fail_n=$((px4_fail_n + 1))
            fi
        else
            px4_fail_n=$((px4_fail_n + 1))
        fi
        # End of issue 0742's critical section — everything above reads or
        # writes the shared `bridge-cpp` tree, including the `cargo check`,
        # which reaches it through `NROS_PX4_BRIDGE_GEN`.
        if [ "$_px4_bridge_locked" = 1 ]; then
            flock -u 9
            exec 9>&-
        fi
    fi
else
    px4_skipped="PX4-Autopilot submodule absent (third-party/px4/PX4-Autopilot)"
    _note_lane_skip "px4: $px4_skipped"
fi

# phase-319 W2 — counts come from the manifest now, not from array lengths.
check_n="$(compile_check_records cargo-check | wc -l)"
verdict_n="$(( $(compile_check_records cargo-check-verdict | wc -l) + $(compile_check_records cmake-configure-verdict | wc -l) + $(compile_check_records cxx-compile-verdict | wc -l) ))"
build_n="$(compile_check_records cargo-build | wc -l)"
# Issue 0695 — a lane that was SKIPPED says so here. `cmake=0` used to be the
# summary for "skipped every one of them" AND for "there were none to build",
# and a reader downstream cannot tell those apart from an artifact that isn't
# there either way.
echo "fixtures built (check=$check_n verdict=$verdict_n build=$build_n cmake=$(_lane_count "$cmake_n" "$cmake_skipped") cxx=$(_lane_count "$cxx_n" "$cxx_skipped") cargo-check=$cargo_check_n px4=$(_lane_count "$px4_n/$((px4_n + px4_fail_n))" "$px4_skipped"))."
if [ "${#lane_skips[@]}" -gt 0 ]; then
    echo "compile-check: ${#lane_skips[@]} lane(s) SKIPPED — their fixtures are NOT built:" >&2
    printf '  - %s\n' "${lane_skips[@]}" >&2
fi

# A NAMED row that was not built is a failure, never a summary line. The skips
# above are right for a sweep — a host without a lane's tool builds the rest —
# but `NROS_FIXTURE_ID=<id>` asks for ONE row, so a skip of its lane means the
# caller got nothing it asked for. Live-peer's host job (run 37570821619) asked
# for `cpp_robot_entry`, got `cmake=SKIPPED(play_launch_parser absent)`, exited
# 0, and the cells then failed FixtureNotBuilt a step later, blaming the code.
# px4 is left out of the sum: it does not honour the id filter.
if [ -n "$id_filter" ] && [ "$(( check_n + verdict_n + build_n + cmake_n + cxx_n + cargo_check_n ))" -eq 0 ]; then
    echo "compile-check: NROS_FIXTURE_ID=$id_filter built NOTHING." >&2
    if [ "${#lane_skips[@]}" -gt 0 ]; then
        echo "  Its lane was skipped; provision what the skip names:" >&2
        printf '    %s\n' "${lane_skips[@]}" >&2
    else
        echo "  No lane selected it — check the id against examples/fixtures.toml." >&2
    fi
    exit 1
fi
