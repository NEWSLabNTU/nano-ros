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

out_root="$(nros_build_dir "$NROS_KIND_COMPILE_CHECK")"
mkdir -p "$out_root"

# id : source template dir (carries @NANO_ROS_ROOT@ placeholders)

# Per-id staging hook: overwrite files in the staged tree before `cargo check`.
# Used by the n9 forms — each is the same workspace with a different
# `nros::main!(...)` invocation in the Entry pkg's main.rs.
post_stage() {
    local id="$1" staged="$2"
    local main_rs="$staged/src/demo_entry/src/main.rs"
    case "$id" in
        main_macro_form1)
            printf '//! n9 form 1 (no args).\n\nnros::main!();\n' > "$main_rs" ;;
        main_macro_form2)
            printf '//! n9 form 2 (board only).\n\nnros::main!(board = ::nros_board_linux::LinuxBoard);\n' > "$main_rs" ;;
        main_macro_form3)
            # phase-330 W7 — the canonical multi-node form is INPUT-addressed.
            printf '//! n9 form 3 (launch, default — the canonical multi-node form).\n\nnros::main!(launch = "demo_bringup");\n' > "$main_rs" ;;
        main_macro_form4)
            # phase-330 W7.g — the ONE compile proof that the DEPRECATED
            # `model =` arm still works during its window; resolves the BUILD
            # artifact (the sync below materialises it) via the ladder.
            printf '//! n9 form 4 (all explicit: board + explicit model file — DEPRECATED arm).\n\nnros::main!(\n    board = ::nros_board_linux::LinuxBoard,\n    model = "demo_bringup:config/system_model.yaml",\n);\n' > "$main_rs" ;;
        orch_tiers_single)
            # Strip the tier table so the macro takes the legacy single-tier
            # BoardEntry::run path (RFC-0032 §5 gate G.4).
            #
            # phase-319 W2 — this used to strip `[tiers.*]` from system.toml, but
            # the entry is `nros::main!(model = ...)` and phase-296 made the
            # MODEL authoritative: the strip stopped doing anything, the fixture
            # kept emitting multi-tier, and
            # `single_tier_system_takes_the_legacy_boardentry_run_path` went red.
            # Strip both — the model because it is what the macro reads, the
            # system.toml because a stale copy there would be a second source of
            # truth (issue 0351's theme one layer over).
            local model="$staged/src/demo_bringup/config/system_model.yaml"
            if [ -f "$model" ]; then
                python3 - "$model" <<'PYSTRIP'
import sys
path = sys.argv[1]
out, skip = [], False
for line in open(path):
    if line.startswith("  tiers:"):
        skip = True
        continue
    if skip:
        # The tier table ends at the next key at the same indent (e.g. bindings:).
        if line.startswith("  ") and not line.startswith("   ") and line.strip():
            skip = False
        else:
            continue
    out.append(line)
open(path, "w").writelines(out)
PYSTRIP
            fi
            local sys="$staged/src/demo_bringup/system.toml"
            if [ -f "$sys" ]; then
                sed -n '0,/^\[tiers\./{/^\[tiers\./!p}' "$sys" > "$sys.tmp" && mv "$sys.tmp" "$sys"
            fi ;;
        *) : ;;  # no overlay (orch_tiers_multi uses the fixture verbatim)
    esac
}

# Build fixtures (id : src): same staging, but `cargo build -p demo_entry`
# producing a runnable binary at build/compile-check/<id>/target/debug/demo_entry
# that the test executes (e.g. boot/run-tier assertions). The compile is still
# the build stage; the test runs the prebuilt binary.

stage_tree() {
    local id="$1" src="$2" staged="$3"
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
    post_stage "$id" "$staged"
    # phase-330 W7.g — templates no longer carry committed SystemModels
    # (W4.a); resolve them into the staged workspace's build dir the same way
    # a user build does. AFTER post_stage, so form/tier rewrites of the
    # INPUTS (main.rs, system.toml) are what gets resolved.
    # Any staged pkg (package.xml) can be a bringup — system.toml is OPTIONAL
    # to the resolver (o4's bringup is launch/ + package.xml only).
    if find "$staged" -maxdepth 3 -name package.xml -print -quit 2>/dev/null | grep -q .; then
        local _sync_cli="${NROS_CLI_BIN:-${NROS_CLI:-$(command -v nros || true)}}"
        if [ -z "$_sync_cli" ]; then
            echo "compile-check: nros CLI not found — cannot resolve staged models (just setup-cli)" >&2
            return 2
        fi
        ( cd "$staged" && "$_sync_cli" sync >/dev/null )
    fi
}

stage_and_check() {
    local id="$1" src="$2"
    local staged="$out_root/$id"
    echo "== compile-check: $id =="
    stage_tree "$id" "$src" "$staged"
    rm -f "$staged/.compile-ok"
    ( cd "$staged" && cargo check --manifest-path Cargo.toml )
    date -u +%Y-%m-%dT%H:%M:%SZ > "$staged/.compile-ok"
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
    date -u +%Y-%m-%dT%H:%M:%SZ > "$staged/.compile-ok"
    # profile-literal-ok: dir vocabulary: echoes the manifest's target-directory name
    echo "   built $staged/$manifest_dir/target/debug/$pkg"
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
    # The C/mixed Entry templates parse launch XML via play_launch_parser.
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
    date -u +%Y-%m-%dT%H:%M:%SZ > "$staged/.compile-ok"
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
_cc_all_builders="cargo-check cargo-build cross-build cmake-configure cxx-syntax"
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
    for _cc_builder in cargo-check cargo-build cross-build cmake-configure cxx-syntax; do
        # Deliberately NOT `compile_check_records` — that honours the builder
        # narrowing, and "this id is in a builder you did not ask for" is not
        # the same fact as "this id does not exist" (issue 0406's distinction).
        _cc_matched=$((_cc_matched + $(python3 "$repo_root/scripts/build/fixtures-manifest.py" \
            list-compile-checks --builder "$_cc_builder" --id "$id_filter" | wc -l)))
    done
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
if [ -z "$id_filter" ] && [ "${NROS_COMPILE_CHECK_POOL:-1}" = "1" ]; then
    _cc_ids=""
    for _cc_builder in cargo-check cargo-build cross-build cmake-configure cxx-syntax; do
        while IFS=$'\x1f' read -r _id _rest; do
            [ -n "$_id" ] || continue
            case " $_cc_ids " in *" $_id "*) continue ;; esac
            _cc_ids="$_cc_ids $_id"
        done < <(compile_check_records "$_cc_builder")
    done
    unset _cc_builder _id _rest
    if [ -n "$_cc_ids" ]; then
        # shellcheck source=scripts/build/jobserver-pool.sh
        source "$repo_root/scripts/build/jobserver-pool.sh"
        _cc_rc=0
        nros_pool_run compile-check < <(
            for _id in $_cc_ids; do
                printf 'NROS_FIXTURE_ID=%s NROS_COMPILE_CHECK_POOL=0 bash %s/scripts/build/compile-check-fixtures.sh\n' \
                    "$_id" "$repo_root"
            done
        ) || _cc_rc=$?
        # Each unit prints its OWN one-row summary, so the parent must print the
        # aggregate itself — otherwise the last unit's counts (check=1 …) read
        # as the whole stage's.
        if [ "$_cc_rc" = "0" ]; then
            echo "compile-check fixtures built: $(printf '%s\n' $_cc_ids | wc -l) row(s) across 5 builders."
        fi
        exit $_cc_rc
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

cxx_syntax_check() {
    local id="$1"
    local src="$snippet_dir/$id.cpp"
    local staged="$out_root/$id"
    [ -f "$src" ] || { echo "cxx-syntax: snippet missing: $src" >&2; return 2; }
    echo "== cxx-syntax: $id =="
    mkdir -p "$staged"
    rm -f "$staged/.compile-ok"
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
          -I "$repo_root/packages/api/nros-c/include"
          -I "$repo_root/cmake/compat/include")
    # Best-effort: a snippet that doesn't compile (pre-existing API drift or a
    # missing generated header) does NOT fail build-test-fixtures — it just
    # leaves no `.compile-ok`, so the consuming test reports the gap per tier
    # (hard-fail full / [SKIPPED] light). The compile error is in this log.
    # phase-363 W4 — `-MD -MF` so the compiler records which headers it actually
    # read. Without it these rows had NO measured closure: their signature was
    # the snippet plus two hand-named include TREES, so a header reached through
    # a third path (nros-platform-api, the cmake compat shim, or a generated
    # config header under `target/`) was invisible. `-MD` composes with
    # `-fsyntax-only` — no object is produced, the dep list still is.
    rm -f "$staged/deps.d"
    if "$cxx" -std=c++14 -fsyntax-only -MD -MF "$staged/deps.d" "${inc[@]}" "$src"; then
        date -u +%Y-%m-%dT%H:%M:%SZ > "$staged/.compile-ok"
        echo "   stamped $staged/.compile-ok"
    else
        echo "   cxx-syntax FAILED for $id (no stamp; consuming test will report)" >&2
    fi
}

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
    # riscv64 typed-carrier examples (examples/qemu-riscv64-threadx/{c,cpp}/*)
    # cover the real path.
fi

cxx_n=0
cxx_skipped=""
if command -v "${CXX:-c++}" >/dev/null 2>&1; then
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
    ( cd "$repo_root" && cargo build -q -p nros-cpp -p nros-c \
        --features nros-cpp/std,nros-c/std,nros-cpp/ros-humble ) \
        || echo "cxx-syntax: config-header generation build failed (snippets needing them will skip)" >&2
    while IFS=$'\x1f' read -r id builder dir pkg mdir target profiles output; do
        [ -n "$id" ] || continue
        run_fixture "$out_root/$id" "$id" "$builder" cxx_syntax_check "$id"
        write_compile_check_sig "$id$(printf '\x1f')$builder$(printf '\x1f')$dir$(printf '\x1f')$pkg$(printf '\x1f')$mdir$(printf '\x1f')$target$(printf '\x1f')$profiles$(printf '\x1f')$output" "$out_root/$id"
        cxx_n=$((cxx_n + 1))
    done < <(_lane_on cxx-syntax && compile_check_records cxx-syntax || true)
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
        date -u +%Y-%m-%dT%H:%M:%SZ > "$out_root/$id/.compile-ok"
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
            date -u +%Y-%m-%dT%H:%M:%SZ > "$out_root/$id/.compile-ok"
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
                date -u +%Y-%m-%dT%H:%M:%SZ > "$out_root/$id/.compile-ok"
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
build_n="$(compile_check_records cargo-build | wc -l)"
# Issue 0695 — a lane that was SKIPPED says so here. `cmake=0` used to be the
# summary for "skipped every one of them" AND for "there were none to build",
# and a reader downstream cannot tell those apart from an artifact that isn't
# there either way.
echo "fixtures built (check=$check_n build=$build_n cmake=$(_lane_count "$cmake_n" "$cmake_skipped") cxx=$(_lane_count "$cxx_n" "$cxx_skipped") cargo-check=$cargo_check_n px4=$(_lane_count "$px4_n/$((px4_n + px4_fail_n))" "$px4_skipped"))."
if [ "${#lane_skips[@]}" -gt 0 ]; then
    echo "compile-check: ${#lane_skips[@]} lane(s) SKIPPED — their fixtures are NOT built:" >&2
    printf '  - %s\n' "${lane_skips[@]}" >&2
fi
