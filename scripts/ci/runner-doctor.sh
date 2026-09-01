#!/usr/bin/env bash
#
# Assert that every label a self-hosted runner CLAIMS is actually true.
# phase-395 W6; design: docs/development/multi-agent-ci-workflow.md.
#
# WHY THIS EXISTS, and why it is the first of the four runner scripts.
#
# A GitHub Actions job is routed by LABEL (`runs-on: [self-hosted, linux,
# nros-sdk-zephyr]`), and a label is a string typed by whoever registered the
# machine. Nothing on GitHub's side checks that the string is true. So a runner
# labelled `nros-sdk-zephyr` with no SDK does not fail to be scheduled — it wins
# the job and then fails INSIDE the build, deep in `FindZephyr-sdk.cmake` or in
# a linker that cannot find a cross compiler. The author of the PR sees a red
# check on their change. That red is indistinguishable from a code failure, and
# it is the most expensive kind of wrong signal this project has: phase-395 W0
# measured that agents run a 40-90 minute local treadmill precisely BECAUSE CI
# reds carry no information.
#
# This is the same class as the vacuous gates catalogued in
# docs/development/multi-agent-ci-workflow.md ("Gates must be able to fail"):
# a claim that is never checked is a claim that is eventually false.
#
# So: `runner-register.sh` refuses to register a runner this script rejects, and
# `runner-provision.sh` finishes by running it, so "provisioned" and "actually
# has it" cannot drift.
#
# TWO WAYS TO USE THIS FILE
#
#   executed   scripts/ci/runner-doctor.sh <labels> [--check] [--quiet]
#              Standalone. Exit 0 = every claim holds; 1 = at least one does
#              not; 2 = usage error.
#
#   sourced    . scripts/ci/runner-doctor.sh   then call `nros_runner_doctor`.
#              The label vocabulary lives HERE and nowhere else, so
#              provision/register cannot grow a second idea of what a label
#              means. (Issue 0833 is what a second copy of a list does: the
#              installer and the checker disagreed about one cross target and
#              `just doctor` reported OK on a host that could not build.)
#
# READ-ONLY BY CONSTRUCTION. `--check` is accepted and changes nothing, because
# there is nothing to change: this script only probes. The flag exists so the
# four runner scripts share one calling convention and an operator can type
# `--check` at any of them without having to remember which ones are safe.
#
# NO MACHINE-SPECIFIC PATHS. Every location is derived from this file's own
# checkout, from an environment variable the operator sets, or from a repo SSoT
# (`config/rust-targets.txt`, `scripts/dev/zenohd.sh`,
# `scripts/build/riscv64-toolchain.sh`). A runner may be behind NAT, on another
# distro, and owned by a different user than the machine this was written on.

# --- the label vocabulary ----------------------------------------------------
#
# Kept identical to the table in docs/development/multi-agent-ci-workflow.md
# ("Labels, not hostnames"). Adding a label is: a row here, a claim function
# below, and a provisioning arm in runner-provision.sh. Three edits, all in this
# directory, none in a workflow file — which is the point of labelling by
# CAPABILITY rather than by hostname.
NROS_RUNNER_LABELS="nros-sdk-zephyr nros-qemu nros-ros2 nros-big"

# Labels GitHub applies itself, or that an operator may add for their own
# routing. They assert nothing about this repo, so there is nothing to verify —
# but see `_nros_runner_unknown_label` below for why a typo is still a failure.
NROS_RUNNER_IMPLICIT_LABELS="self-hosted linux Linux x64 X64 arm64 ARM64"

# Repo root: this file is at <root>/scripts/ci/, so two levels up. Resolved from
# BASH_SOURCE rather than $PWD because provision/register source this file and
# then legitimately `cd` elsewhere. `NROS_RUNNER_REPO_ROOT` overrides it for the
# case where the runner's checkout is not the checkout this script was read from.
_nros_runner_repo_root() {
    if [ -n "${NROS_RUNNER_REPO_ROOT:-}" ]; then
        printf '%s' "${NROS_RUNNER_REPO_ROOT%/}"
        return 0
    fi
    (cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
}

# Wire the checkout's own PATH before probing anything.
#
# NOT a convenience. `activate.sh` is this repo's env/PATH SSoT, and the sweep
# contract in CLAUDE.md is that every lane sources it first — it is what puts
# `~/.nros/sdk/<tool>/<ver>/bin` on PATH. Probing a bare login shell instead
# asks a question no job ever asks, and answers it wrong: measured on the
# machine this was written on, `arm-none-eabi-gcc` reported MISSING while
# `~/.nros/sdk/arm-none-eabi-gcc/13.2-nros1/bin/arm-none-eabi-gcc` was installed
# and every lane could see it. A doctor telling a working host it is broken is
# the failure mode a doctor exists to prevent (issue 0654).
#
# `activate.sh` installs nothing and is documented never to error, but the flags
# are relaxed around it anyway and restored afterwards: this file is also
# SOURCED by runner-provision.sh under `set -euo pipefail`, and an unset
# variable in somebody else's file must not abort provisioning.
_nros_runner_activate() {
    if [ -n "${_NROS_RUNNER_ACTIVATED:-}" ]; then return 0; fi
    _NROS_RUNNER_ACTIVATED=1
    if [ "${NROS_RUNNER_NO_ACTIVATE:-0}" = "1" ]; then return 0; fi
    local root saved
    root="$(_nros_runner_repo_root)"
    if [ ! -r "$root/activate.sh" ]; then return 0; fi
    saved="$-"
    set +eu
    # shellcheck source=/dev/null
    . "$root/activate.sh" >/dev/null 2>&1 || true
    case "$saved" in *e*) set -e ;; esac
    case "$saved" in *u*) set -u ;; esac
    return 0
}

# --- reporting ---------------------------------------------------------------
#
# The `  [OK] ` / `  [MISSING] ` prefixes are `just doctor`'s, deliberately: an
# operator reads both, and two report formats for one question is one format too
# many. Every MISSING line is followed by the REMEDY, because a diagnosis
# without a command is a puzzle.
_nros_runner_ok()      { [ -n "${NROS_RUNNER_QUIET:-}" ] || printf '  [OK] %s\n' "$*"; }
_nros_runner_info()    { [ -n "${NROS_RUNNER_QUIET:-}" ] || printf '  [INFO] %s\n' "$*"; }
_nros_runner_missing() { printf '  [MISSING] %s\n' "$*" >&2; }
_nros_runner_fix()     { printf '            %s\n' "$*" >&2; }

# --- nros-sdk-zephyr ---------------------------------------------------------
#
# "the 9.2 GB SDK is provisioned and warm".
#
# Four separate things have to hold, and each has been the sole cause of a
# failure at some point:
#
#   1. `west` resolves — the Zephyr build is driven by it.
#   2. the Zephyr SOURCE workspace exists. native_sim needs the sources and no
#      SDK; every cross Zephyr target needs both.
#   3. the SDK version the workspace's own `zephyr/SDK_VERSION` demands is
#      installed AND REGISTERED. Registration (`~/.cmake/packages/Zephyr-sdk/*`)
#      is what `find_package(Zephyr-sdk)` consults; an unpacked-but-unregistered
#      SDK fails at cmake configure with an error naming neither the version nor
#      the setup step. This mirrors `just zephyr doctor`.
#   4. a toolchain binary is actually THERE. A half-unpacked or interrupted
#      download leaves the directory and the registration behind, so checking
#      for the directory alone is the check that passes on the broken case.
# Where cmake says the Zephyr SDK for <want> is — issue 0980.
#
# `find_package(Zephyr-sdk)` consults the CMake USER PACKAGE REGISTRY,
# `~/.cmake/packages/Zephyr-sdk/<hash>`, each file holding the directory that
# contains `Zephyr-sdkConfig.cmake` — i.e. `<sdk>/cmake`. `scripts/zephyr/
# setup.sh` writes it, and per `scripts/build/zephyr-toolchain.sh` it is the
# branch every SDK-toolchain board in this tree actually takes.
#
# Prints the SDK ROOT (the `cmake` component stripped) for an entry naming
# `zephyr-sdk-<want>`, or nothing. An entry whose directory EXISTS wins over one
# whose does not, so a stale registration left behind by a deleted SDK cannot
# shadow a live one — while a stale entry alone is still printed, because the
# caller reports that as its own diagnosis rather than as "unregistered".
_nros_runner_zephyr_sdk_registry_path() {
    local want="$1" reg dir best="" stale=""
    for reg in "$HOME"/.cmake/packages/Zephyr-sdk/*; do
        [ -f "$reg" ] || continue
        dir="$(tr -d '\r\n' < "$reg" 2>/dev/null || true)"
        [ -n "$dir" ] || continue
        case "$dir" in *"zephyr-sdk-$want"*) ;; *) continue ;; esac
        # The registry records `<sdk>/cmake`; tolerate an entry that records the
        # SDK root itself, which older setup scripts wrote.
        [ "$(basename "$dir")" = "cmake" ] && dir="$(dirname "$dir")"
        if [ -d "$dir" ]; then
            best="$dir"
            break
        fi
        [ -n "$stale" ] || stale="$dir"
    done
    printf '%s' "${best:-$stale}"
}

# Where THIS host's Zephyr SDK <want> is, and what said so.
#
# Prints `<path><TAB><origin>`. Precedence mirrors `FindZephyr-sdk.cmake`:
# the explicit env var, then the cmake user package registry, then the
# checkout-relative location `scripts/zephyr/setup.sh` uses on a dev box.
#
# The last of those is a FALLBACK and nothing more. It cannot be the primary:
# see the long note in `_nros_runner_check_sdk_zephyr` — `actions/checkout`
# git-cleans ignored files, so a self-hosted runner's SDK is necessarily
# outside the checkout.
_nros_runner_zephyr_sdk_path() {
    local want="$1" root="$2" from_registry=""

    if [ -n "${ZEPHYR_SDK_INSTALL_DIR:-}" ]; then
        # Both spellings are in the wild: the SDK itself, or its parent.
        case "$(basename "$ZEPHYR_SDK_INSTALL_DIR")" in
            zephyr-sdk-*) printf '%s\tZEPHYR_SDK_INSTALL_DIR' "$ZEPHYR_SDK_INSTALL_DIR" ;;
            *)            printf '%s/zephyr-sdk-%s\tZEPHYR_SDK_INSTALL_DIR' \
                              "${ZEPHYR_SDK_INSTALL_DIR%/}" "$want" ;;
        esac
        return 0
    fi

    from_registry="$(_nros_runner_zephyr_sdk_registry_path "$want")"
    if [ -n "$from_registry" ]; then
        printf '%s\tcmake package registry' "$from_registry"
        return 0
    fi

    printf '%s/scripts/zephyr/sdk/zephyr-sdk-%s\tcheckout default' "$root" "$want"
}

_nros_runner_check_sdk_zephyr() {
    local root fail=0
    root="$(_nros_runner_repo_root)"

    # The Zephyr venv is this lane's, not the session's (issue 0698) — the same
    # activation `just zephyr doctor` does, so `west` is looked for where the
    # lane would look for it rather than only on the caller's PATH.
    if [ -r "$root/scripts/build/zephyr-python.sh" ]; then
        # shellcheck source=/dev/null
        . "$root/scripts/build/zephyr-python.sh" 2>/dev/null || true
        if command -v nros_zephyr_activate >/dev/null 2>&1; then
            nros_zephyr_activate >/dev/null 2>&1 || true
        fi
    fi

    if command -v west >/dev/null 2>&1; then
        _nros_runner_ok "west: $(west --version 2>/dev/null | head -1)"
    else
        _nros_runner_missing "west is not on PATH"
        _nros_runner_fix "scripts/ci/runner-provision.sh nros-sdk-zephyr"
        fail=1
    fi

    # Mirrors `ZEPHYR_WORKSPACE` in just/zephyr.just. Restated rather than
    # imported because that derivation lives in a `just` variable, which no
    # shell can read without invoking `just` — and this script must work on a
    # machine where `just` is not yet installed.
    local ws="${NROS_ZEPHYR_WORKSPACE:-}"
    if [ -z "$ws" ]; then
        if [ "${NROS_ZEPHYR_VERSION:-3.7}" = "4.4" ]; then
            ws="$root/../nano-ros-workspace-4.4"
        elif [ -d "$root/zephyr-workspace" ]; then
            ws="$root/zephyr-workspace"
        elif [ -d "$root/../nano-ros-workspace" ]; then
            ws="$root/../nano-ros-workspace"
        else
            ws="$root/zephyr-workspace"
        fi
    fi

    if [ -d "$ws/zephyr" ]; then
        _nros_runner_ok "Zephyr workspace: $ws"
    else
        _nros_runner_missing "no Zephyr workspace at $ws"
        _nros_runner_fix "scripts/ci/runner-provision.sh nros-sdk-zephyr"
        _nros_runner_fix "(or set NROS_ZEPHYR_WORKSPACE if it lives elsewhere)"
        return 1   # nothing below can be judged without the sources
    fi

    # `zephyr/SDK_VERSION` is Zephyr's own statement of what THIS source tree
    # needs. Reading it rather than pinning a version here is what keeps this
    # check correct across the 3.7 / 4.4 lines, which want different SDKs.
    local want=""
    [ -f "$ws/zephyr/SDK_VERSION" ] && want="$(cat "$ws/zephyr/SDK_VERSION" 2>/dev/null)"
    if [ -z "$want" ]; then
        _nros_runner_missing "$ws/zephyr/SDK_VERSION is absent or empty"
        _nros_runner_fix "the workspace is incomplete — re-run:"
        _nros_runner_fix "scripts/ci/runner-provision.sh nros-sdk-zephyr"
        return 1
    fi

    # Where the SDK is, resolved the way the BUILD resolves it — issue 0980.
    #
    # This used to be a path derived from the checkout
    # (`<root>/scripts/zephyr/sdk/zephyr-sdk-<ver>`, where `scripts/zephyr/
    # setup.sh` puts it on a dev box) with `ZEPHYR_SDK_INSTALL_DIR` as the only
    # override. That is not where a self-hosted runner's SDK can be. Both
    # `scripts/zephyr/sdk/` and `/zephyr-workspace` are gitignored, and
    # `actions/checkout` defaults to `git clean -ffdx` — `-x` removes IGNORED
    # files — so anything provisioned inside the job checkout is deleted at the
    # top of the next job. A runner's 9.2 GB SDK must live outside the checkout,
    # which made the derived path wrong by construction and the doctor failed a
    # host that builds fine. That is the exact failure mode this file's header
    # says it exists to prevent (issue 0654).
    #
    # `scripts/build/zephyr-toolchain.sh` already states where the build looks:
    #
    #   With `ZEPHYR_SDK_INSTALL_DIR` still unset the lookup takes the same
    #   `else()` search branch as before, and the in-tree SDK is found the way
    #   it always was — through the CMake user package registry
    #   (`~/.cmake/packages/Zephyr-sdk/*`, written by `scripts/zephyr/setup.sh`)
    #
    # So the registry is not a fallback here, it is the normal answer, and this
    # function's own registration check three blocks down was already reading
    # it — printing the true path in an `[OK]` line while the checks above
    # called the SDK missing from a path nothing uses. One fact, two
    # derivations, and the disagreement reported as a diagnosis.
    #
    # Precedence mirrors `FindZephyr-sdk.cmake`: the env var first, then the
    # registry, then the checkout default for a plain dev box with neither.
    local resolved sdk_path sdk_origin
    resolved="$(_nros_runner_zephyr_sdk_path "$want" "$root")"
    sdk_path="${resolved%%	*}"
    sdk_origin="${resolved#*	}"

    if [ -d "$sdk_path" ]; then
        _nros_runner_ok "Zephyr SDK $want unpacked: $sdk_path (via $sdk_origin)"
    else
        _nros_runner_missing "Zephyr SDK $want is not at $sdk_path (via $sdk_origin)"
        _nros_runner_fix "this workspace's zephyr/SDK_VERSION demands exactly $want."
        if [ "$sdk_origin" = "checkout default" ]; then
            _nros_runner_fix "nothing points anywhere else: ZEPHYR_SDK_INSTALL_DIR is unset and"
            _nros_runner_fix "no ~/.cmake/packages/Zephyr-sdk/ entry names zephyr-sdk-$want."
            _nros_runner_fix "On a self-hosted runner the SDK must live OUTSIDE the job checkout —"
            _nros_runner_fix "actions/checkout git-cleans ignored files, and scripts/zephyr/sdk/ is one."
        fi
        _nros_runner_fix "scripts/ci/runner-provision.sh nros-sdk-zephyr"
        fail=1
    fi

    # A toolchain binary, not merely the directory. See point 4 in the header.
    # arm-zephyr-eabi is the one every Zephyr cross cell in this tree needs; an
    # SDK missing it is an SDK whose `setup.sh -t` list was wrong.
    local armgcc="$sdk_path/arm-zephyr-eabi/bin/arm-zephyr-eabi-gcc"
    if [ -x "$armgcc" ]; then
        _nros_runner_ok "arm-zephyr-eabi toolchain present (SDK is unpacked, not just downloaded)"
    else
        _nros_runner_missing "no arm-zephyr-eabi-gcc under $sdk_path"
        # Only claim the interrupted-install story when the directory is
        # actually there. It was printed unconditionally, so the commonest case
        # — no SDK at that path at all — was reported as a half-unpacked one,
        # which sends the reader to the wrong remedy.
        if [ -d "$sdk_path" ]; then
            _nros_runner_fix "the SDK directory exists but its toolchains are not unpacked —"
            _nros_runner_fix "an interrupted install leaves exactly this state."
        else
            _nros_runner_fix "there is no SDK at that path — see the line above."
        fi
        _nros_runner_fix "scripts/ci/runner-provision.sh nros-sdk-zephyr"
        fail=1
    fi

    # Registration is what cmake actually consults. Unregistered = a configure
    # failure that names neither the version nor the remedy.
    #
    # The registered path must EXIST, not merely be named. A registry entry
    # outlives the SDK it points at (the directory is deleted, the entry is
    # not), and grepping the file's text alone reports `[OK]` for an SDK that
    # is gone — the same "named is not present" mistake one layer up.
    local have=""
    have="$(_nros_runner_zephyr_sdk_registry_path "$want")"
    if [ -n "$have" ] && [ -d "$have" ]; then
        _nros_runner_ok "Zephyr SDK $want registered with cmake ($have/cmake)"
    elif [ -n "$have" ]; then
        _nros_runner_missing "cmake's registry names $have for SDK $want, but nothing is there"
        _nros_runner_fix "a stale ~/.cmake/packages/Zephyr-sdk/ entry outlived its SDK."
        _nros_runner_fix "Re-register from the SDK that IS installed:  (cd <sdk> && ./setup.sh -h -c)"
        fail=1
    else
        _nros_runner_missing "Zephyr SDK $want is not registered with cmake"
        _nros_runner_fix "find_package(Zephyr-sdk) reads ~/.cmake/packages/Zephyr-sdk/;"
        _nros_runner_fix "an unregistered SDK fails at configure, not at download."
        _nros_runner_fix "Run the SDK's own registration:  (cd $sdk_path && ./setup.sh -h -c)"
        fail=1
    fi

    return "$fail"
}

# --- nros-qemu ---------------------------------------------------------------
#
# "QEMU + the RTOS toolchains".
#
# What is asserted is the set a cross-RUN lane (L4) cannot start without, and
# each item is checked the way the platform's own `just <plat> doctor` checks it
# — including the >= 7.2 rule, which exists because `-netdev
# dgram,local.type=unix,...` (the workaround for QEMU 6.2's broken cross-process
# multicast) arrived in 7.2 and the NuttX / ThreadX-RV64 multi-instance cells
# need it.
_nros_runner_check_qemu() {
    local root fail=0
    root="$(_nros_runner_repo_root)"

    # The project-local patched build is the PRIMARY path (phase-143): the test
    # harness resolves it itself, so a host with it needs nothing from the
    # system. Check it first for that reason, not merely as a fallback.
    local patched="$root/build/qemu/bin/qemu-system-arm"
    if [ -x "$patched" ]; then
        _nros_runner_ok "patched qemu-system-arm: $patched ($("$patched" --version 2>/dev/null | head -1))"
    elif command -v qemu-system-arm >/dev/null 2>&1; then
        local ver major minor
        # `|| true` because this file is also SOURCED by runner-provision.sh,
        # which runs under `set -e` + `pipefail`: a `head -1` that closes the
        # pipe early would otherwise abort provisioning inside a probe.
        ver="$(qemu-system-arm --version 2>/dev/null | head -1 \
               | sed -E 's/^[^0-9]*([0-9]+\.[0-9]+).*/\1/' || true)"
        major="${ver%%.*}"; minor="${ver##*.}"
        if [ -n "$ver" ] && { [ "$major" -gt 7 ] || { [ "$major" -eq 7 ] && [ "$minor" -ge 2 ]; }; }; then
            _nros_runner_ok "system qemu-system-arm $ver (>= 7.2, supports -netdev dgram unix)"
        else
            _nros_runner_missing "qemu-system-arm is $ver — the RTOS multi-instance cells need >= 7.2"
            _nros_runner_fix "-netdev dgram,local.type=unix is a 7.2 feature; below it the"
            _nros_runner_fix "NuttX and ThreadX-RV64 DDS cells fail as delivery timeouts."
            _nros_runner_fix "scripts/ci/runner-provision.sh nros-qemu   (builds build/qemu, no sudo)"
            fail=1
        fi
    else
        _nros_runner_missing "no qemu-system-arm (neither $patched nor PATH)"
        _nros_runner_fix "scripts/ci/runner-provision.sh nros-qemu"
        fail=1
    fi

    if command -v qemu-system-riscv64 >/dev/null 2>&1; then
        _nros_runner_ok "qemu-system-riscv64"
    else
        _nros_runner_missing "no qemu-system-riscv64 — the ThreadX-RV64 and NuttX-RISCV cells cannot run"
        _nros_runner_fix "scripts/ci/runner-provision.sh nros-qemu"
        fail=1
    fi

    if command -v arm-none-eabi-gcc >/dev/null 2>&1; then
        _nros_runner_ok "arm-none-eabi-gcc: $(arm-none-eabi-gcc -dumpversion 2>/dev/null)"
    else
        _nros_runner_missing "no arm-none-eabi-gcc (FreeRTOS / NuttX / bare-metal ARM)"
        _nros_runner_fix "nros setup --tool arm-none-eabi-gcc   (prebuilt, no sudo — issue 0368 F2)"
        fail=1
    fi

    # The riscv64 bare-metal prefix comes from the repo's ONE resolver (issue
    # 0657): the index provisions xPack `riscv-none-elf-*`, while Ubuntu's
    # package is `riscv64-unknown-elf-*`, and twenty files once hardcoded the
    # second. Asking the resolver is the only way to be right on both hosts.
    if [ -r "$root/scripts/build/riscv64-toolchain.sh" ]; then
        # shellcheck source=/dev/null
        . "$root/scripts/build/riscv64-toolchain.sh" 2>/dev/null || true
        local rv=""
        command -v nros_riscv64_prefix >/dev/null 2>&1 && rv="$(nros_riscv64_prefix 2>/dev/null || true)"
        if [ -n "$rv" ]; then
            _nros_runner_ok "riscv64 bare-metal toolchain: ${rv}-gcc"
        else
            _nros_runner_missing "no riscv64 bare-metal cross-gcc"
            _nros_runner_fix "nros setup --tool riscv-none-elf-gcc   (pinned dist, bundles newlib)"
            fail=1
        fi
    fi

    # Cross Rust targets. Read from config/rust-targets.txt through the repo's
    # own reader — never a second copy of the list. Issue 0833 is precisely the
    # bug a second copy causes: `armv8r-none-eabihf` was in the installer and
    # not in the checker, so a host that could not configure the FreeRTOS C++
    # lane reported OK.
    if [ -r "$root/scripts/lib/rust-targets.sh" ] && command -v rustup >/dev/null 2>&1; then
        # shellcheck source=/dev/null
        . "$root/scripts/lib/rust-targets.sh" 2>/dev/null || true
        local installed missing="" t
        # One `rustup target list`, matched with a shell `case`: a no-match and
        # a `grep` FAILURE are both exit>=1, and telling those apart is the
        # whole job of this loop (scripts/lib/grep-q.sh).
        installed=" $(rustup target list --installed 2>/dev/null | tr '\n' ' ') "
        while read -r t; do
            [ -n "$t" ] || continue
            case "$installed" in
                *" $t "*) ;;
                *) missing="$missing $t" ;;
            esac
        done < <(nros_rust_targets rustup 2>/dev/null)
        if [ -z "$missing" ]; then
            _nros_runner_ok "rust cross targets (config/rust-targets.txt)"
        else
            _nros_runner_missing "rust cross targets absent:$missing"
            _nros_runner_fix "just workspace rust-targets"
            fail=1
        fi
    else
        _nros_runner_info "rustup not on PATH — cross targets not verified"
    fi

    return "$fail"
}

# --- nros-ros2 ---------------------------------------------------------------
#
# "a real ROS 2 install for interop lanes".
#
# Three claims, and the third is the one that took 13 of 20
# `check-required-features-tests` red on a host that HAS ROS (issue 0774): the
# router RESOLVING is not the router RUNNING. `rmw_zenohd` links `libzenohc.so`
# by SONAME, so unless the ROS prefix's `opt/zenoh_cpp_vendor/lib` is on the
# loader path, some other correctly-installed `libzenohc.so` wins and the
# router SEGVs mid-startup, reporting only `signal: 11`.
#
# The router path itself comes from `scripts/dev/zenohd.sh` — the SSoT, shared
# with `just doctor` and `nros_tests::process::ros_zenohd_path`. Constructing
# `/opt/ros/$ROS_DISTRO/...` here would tell a working host it is broken, which
# is the failure mode a doctor exists to prevent (issue 0654).
_nros_runner_check_ros2() {
    local root fail=0
    root="$(_nros_runner_repo_root)"

    # A runner's JOB sources setup.bash; this script generally runs outside
    # that. So accept either evidence: a sourced environment
    # (AMENT_PREFIX_PATH), or a named distro whose prefix exists on disk.
    if [ -n "${AMENT_PREFIX_PATH:-}" ]; then
        _nros_runner_ok "ROS environment sourced (AMENT_PREFIX_PATH is set)"
    elif [ -n "${ROS_DISTRO:-}" ] && [ -f "/opt/ros/$ROS_DISTRO/setup.bash" ]; then
        _nros_runner_ok "ROS $ROS_DISTRO installed (/opt/ros/$ROS_DISTRO/setup.bash)"
    else
        _nros_runner_missing "no ROS 2 install this script can name"
        _nros_runner_fix "Set ROS_DISTRO in the runner's environment (its .env file), or"
        _nros_runner_fix "source the setup before invoking this script. nano-ros does not"
        _nros_runner_fix "install ROS: it is a system package and needs root."
        _nros_runner_fix "Ubuntu:      https://docs.ros.org/ — then apt install ros-<distro>-desktop"
        _nros_runner_fix "Other hosts: docs/development/ros2-on-non-ubuntu.md (Ubuntu distrobox)"
        return 1   # nothing below can resolve without a prefix
    fi

    local zenohd=""
    if [ -r "$root/scripts/dev/zenohd.sh" ]; then
        # shellcheck source=/dev/null
        . "$root/scripts/dev/zenohd.sh" 2>/dev/null || true
        command -v nros_zenohd_bin >/dev/null 2>&1 && zenohd="$(nros_zenohd_bin 2>/dev/null || true)"
    fi
    if [ -n "$zenohd" ] && [ -x "$zenohd" ]; then
        _nros_runner_ok "rmw_zenoh_cpp router: $zenohd"
    else
        _nros_runner_missing "cannot locate rmw_zenoh_cpp/rmw_zenohd (RFC-0075)"
        _nros_runner_fix "Every zenoh interop lane SKIPs without it, and a SKIP reads as green."
        _nros_runner_fix "Install ros-<distro>-rmw-zenoh-cpp, or set NROS_RMW_ZENOHD."
        fail=1
    fi

    # Issue 0774 — the PAIRED zenoh library must exist beside the router.
    # <prefix>/lib/rmw_zenoh_cpp/rmw_zenohd  ->  <prefix>/opt/zenoh_cpp_vendor/lib
    if [ -n "$zenohd" ] && [ -x "$zenohd" ]; then
        local prefix libdir
        prefix="$(cd "$(dirname "$zenohd")/../.." 2>/dev/null && pwd || true)"
        libdir="$prefix/opt/zenoh_cpp_vendor/lib"
        if [ -d "$libdir" ] && [ -e "$libdir/libzenohc.so" ]; then
            _nros_runner_ok "paired libzenohc.so present ($libdir)"
        else
            _nros_runner_missing "no paired libzenohc.so under $libdir"
            _nros_runner_fix "The router links libzenohc by SONAME. Without its own vendor dir on"
            _nros_runner_fix "the loader path, another libzenohc.so wins and the router SEGVs"
            _nros_runner_fix "mid-startup — reported only as \`signal: 11\` (issue 0774)."
            _nros_runner_fix "Install the matching ros-<distro>-zenoh-cpp-vendor package."
            fail=1
        fi
    fi

    return "$fail"
}

# --- nros-big ----------------------------------------------------------------
#
# ">= 16 cores, for fixture fan-out".
#
# The threshold is the design doc's, overridable because "big" is relative to
# the fleet, not absolute. RAM and free disk are REPORTED and not asserted: the
# label makes a claim about cores and only about cores, and a doctor that fails
# on a claim nobody made is a doctor people learn to bypass. Disk pressure is
# `runner-sweep.sh`'s job, which has budgets and can actually fix it.
_nros_runner_check_big() {
    local want="${NROS_RUNNER_BIG_CORES:-16}" have
    have="$(nproc 2>/dev/null || getconf _NPROCESSORS_ONLN 2>/dev/null || echo 0)"
    if [ "$have" -ge "$want" ] 2>/dev/null; then
        _nros_runner_ok "cores: $have (>= $want)"
    else
        _nros_runner_missing "cores: $have — the nros-big label claims >= $want"
        _nros_runner_fix "This is a hardware fact; nothing can provision it."
        _nros_runner_fix "Either drop nros-big from this runner's labels, or lower the bar"
        _nros_runner_fix "deliberately with NROS_RUNNER_BIG_CORES=<n>."
        return 1
    fi

    # Informational context an operator wants at the same moment.
    local memkb="" avail=""
    [ -r /proc/meminfo ] && memkb="$(awk '/^MemTotal:/ {print $2; exit}' /proc/meminfo 2>/dev/null)"
    [ -n "$memkb" ] && _nros_runner_info "RAM: $((memkb / 1024 / 1024)) GiB"
    avail="$(df -P -BG "$(_nros_runner_repo_root)" 2>/dev/null | awk 'NR==2 {print $4}' || true)"
    if [ -n "$avail" ]; then
        _nros_runner_info "free disk on the checkout's filesystem: $avail"
    fi
    return 0
}

# --- dispatch ----------------------------------------------------------------

# `nros_runner_label_known <label>` — is this one of ours?
nros_runner_label_known() {
    local l
    for l in $NROS_RUNNER_LABELS; do [ "$l" = "$1" ] && return 0; done
    return 1
}

# An unknown label is only a failure when it LOOKS like one of ours.
#
# `self-hosted`, `linux`, `X64` are GitHub's own; an operator's fleet may carry
# site labels we know nothing about. Those assert nothing here and are reported,
# not failed. But `nros-sdk-zepyhr` is a typo, and a typo is the exact shape
# this script exists to catch: the runner would advertise a capability nobody
# routes to, or — worse, once someone fixes the workflow — one it does not have.
_nros_runner_unknown_label() {
    local label="$1" l
    for l in $NROS_RUNNER_IMPLICIT_LABELS; do
        [ "$l" = "$label" ] && { _nros_runner_info "$label — GitHub's own label; nothing to assert"; return 0; }
    done
    case "$label" in
        nros-*)
            _nros_runner_missing "$label is not a nano-ros capability label"
            _nros_runner_fix "Known labels: $NROS_RUNNER_LABELS"
            _nros_runner_fix "An 'nros-' label we do not define is a typo, and a typo here"
            _nros_runner_fix "registers a runner that advertises a capability nothing checks."
            return 1
            ;;
        *)
            _nros_runner_info "$label — not a nano-ros label; nothing to assert"
            return 0
            ;;
    esac
}

# `nros_runner_check_label <label>` — verify one label. 0 = holds.
nros_runner_check_label() {
    case "$1" in
        nros-sdk-zephyr) _nros_runner_check_sdk_zephyr ;;
        nros-qemu)       _nros_runner_check_qemu ;;
        nros-ros2)       _nros_runner_check_ros2 ;;
        nros-big)        _nros_runner_check_big ;;
        *)               _nros_runner_unknown_label "$1" ;;
    esac
}

# `nros_runner_labels_split <csv-or-spaces>` — one label per line.
# Accepts the comma form the GitHub runner's `--labels` takes, so an operator
# can paste the same string at any of the four scripts.
nros_runner_labels_split() {
    # `printf '%s\n'`, NOT `printf '%s'`. Without the trailing newline the final
    # label reaches `read` with a non-zero status, so a `while read` loop drops
    # it — silently, and only ever the LAST one, which is the shape that looks
    # like "the check for that label passed". Caught by running the four labels
    # against this host and noticing `nros-big` never printed a section.
    printf '%s\n' "$1" | tr ',' '\n' | tr ' ' '\n' | sed '/^$/d'
}

# `nros_runner_doctor <labels>` — check them all. 0 = every claim holds.
#
# Every label is checked even after one fails. An operator provisioning a
# machine wants the whole list of what is missing, not the first item; the
# one-at-a-time shape is what makes provisioning a machine an afternoon of
# round trips. Same reasoning as `just doctor`'s `set +e`.
nros_runner_doctor() {
    local labels="$1" label fail=0 n=0
    if [ -z "$labels" ]; then
        echo "runner-doctor: no labels given" >&2
        return 2
    fi
    _nros_runner_activate
    while read -r label; do
        [ -n "$label" ] || continue
        n=$((n + 1))
        [ -n "${NROS_RUNNER_QUIET:-}" ] || printf '\n=== %s ===\n' "$label"
        nros_runner_check_label "$label" || fail=1
    done < <(nros_runner_labels_split "$labels")

    echo ""
    if [ "$fail" -ne 0 ]; then
        echo "runner-doctor: FAIL — at least one label claims something this host does not have." >&2
        echo "  A runner that lies about its labels wins jobs it cannot run, and the red" >&2
        echo "  lands on the PR author's change looking like a code failure." >&2
        echo "  Fix the host (scripts/ci/runner-provision.sh <labels>) or drop the label." >&2
        return 1
    fi
    echo "runner-doctor: OK — all $n label(s) verified."
    return 0
}

# --- self-test ---------------------------------------------------------------
#
# Issue 0980. The SDK resolver is the one part of this file that can be checked
# without a provisioned host, and it is the part that was wrong: it answered
# "where is the SDK?" from the checkout while the build answers it from the
# cmake user package registry, so a runner whose SDK lives outside the checkout
# — which is every self-hosted runner, since `actions/checkout` git-cleans
# ignored files — was told it was broken while building fine.
#
# Temp dirs only: no cmake, no cargo, no SDK, no network. `just check
# runner-doctor-sdk-resolution` runs it on the fast line.
_nros_runner_self_test() {
    local tmp fails=0 saved_home="${HOME}" saved_env="${ZEPHYR_SDK_INSTALL_DIR:-}"
    tmp="$(mktemp -d)"
    # shellcheck disable=SC2064
    trap "rm -rf '$tmp'; HOME='$saved_home'" RETURN

    _st_ok()  { printf '  ok   %s\n' "$1"; }
    _st_bad() { printf '  FAIL %s: %s\n' "$1" "$2" >&2; fails=$((fails + 1)); }

    # <case> -> a HOME with a registry, and a checkout root, both empty.
    _st_stage() {
        local d="$tmp/$1"
        mkdir -p "$d/home/.cmake/packages/Zephyr-sdk" "$d/root/scripts/zephyr/sdk"
        printf '%s' "$d"
    }
    # _st_register <case-dir> <entry-name> <sdk-dir> <create|absent>
    _st_register() {
        [ "$4" = "create" ] && mkdir -p "$3/cmake"
        printf '%s\n' "$3/cmake" > "$1/home/.cmake/packages/Zephyr-sdk/$2"
    }
    # _st_resolve <case-dir> -> "<path>\t<origin>"
    _st_resolve() { HOME="$1/home" _nros_runner_zephyr_sdk_path 0.16.8 "$1/root"; }
    _st_expect() { # <label> <got> <want>
        [ "$2" = "$3" ] && _st_ok "$1" || _st_bad "$1" "got '$2'"
    }

    unset ZEPHYR_SDK_INSTALL_DIR

    # Issue 0980 — the regression. The SDK is outside the checkout and only the
    # registry knows where; the checkout path must NOT win merely by being the
    # default. This is the case that failed every merge-group L3 job.
    local d got
    d="$(_st_stage outside)"
    _st_register "$d" aaa "$tmp/elsewhere/zephyr-sdk-0.16.8" create
    got="$(_st_resolve "$d")"
    _st_expect "an SDK outside the checkout is found via the cmake registry" \
        "$got" "$tmp/elsewhere/zephyr-sdk-0.16.8	cmake package registry"

    # A live entry beats a stale one whatever order the glob yields them in —
    # `aaa` sorts first and points at nothing.
    d="$(_st_stage stale_and_live)"
    _st_register "$d" aaa "$tmp/gone/zephyr-sdk-0.16.8" absent
    _st_register "$d" bbb "$tmp/live/zephyr-sdk-0.16.8" create
    got="$(_st_resolve "$d")"
    _st_expect "a live registry entry beats a stale one" \
        "$got" "$tmp/live/zephyr-sdk-0.16.8	cmake package registry"

    # A stale entry ALONE is still returned, so the caller can report "cmake
    # names this path and nothing is there" instead of the misleading
    # "not registered with cmake".
    d="$(_st_stage stale_only)"
    _st_register "$d" aaa "$tmp/gone2/zephyr-sdk-0.16.8" absent
    got="$(_st_resolve "$d")"
    _st_expect "a stale-only entry is reported, not silently discarded" \
        "$got" "$tmp/gone2/zephyr-sdk-0.16.8	cmake package registry"

    # An entry for a DIFFERENT version must not be mistaken for this one —
    # `zephyr/SDK_VERSION` demands an exact version.
    d="$(_st_stage wrong_version)"
    _st_register "$d" aaa "$tmp/other/zephyr-sdk-0.17.4" create
    got="$(_st_resolve "$d")"
    _st_expect "an entry for another SDK version is ignored" \
        "$got" "$d/root/scripts/zephyr/sdk/zephyr-sdk-0.16.8	checkout default"

    # Neither env nor registry: the dev-box default, preserved.
    d="$(_st_stage bare)"
    got="$(_st_resolve "$d")"
    _st_expect "with no env and no registry the checkout default is used" \
        "$got" "$d/root/scripts/zephyr/sdk/zephyr-sdk-0.16.8	checkout default"

    # The explicit override wins over the registry, in both spellings.
    d="$(_st_stage env_sdk)"
    _st_register "$d" aaa "$tmp/live/zephyr-sdk-0.16.8" create
    got="$(ZEPHYR_SDK_INSTALL_DIR="$tmp/chosen/zephyr-sdk-0.16.8" _st_resolve "$d")"
    _st_expect "ZEPHYR_SDK_INSTALL_DIR naming the SDK beats the registry" \
        "$got" "$tmp/chosen/zephyr-sdk-0.16.8	ZEPHYR_SDK_INSTALL_DIR"

    d="$(_st_stage env_parent)"
    _st_register "$d" aaa "$tmp/live/zephyr-sdk-0.16.8" create
    got="$(ZEPHYR_SDK_INSTALL_DIR="$tmp/parent" _st_resolve "$d")"
    _st_expect "ZEPHYR_SDK_INSTALL_DIR naming the parent appends the version" \
        "$got" "$tmp/parent/zephyr-sdk-0.16.8	ZEPHYR_SDK_INSTALL_DIR"


    # The whole zephyr check, on a host shaped like the runner that failed:
    # workspace and SDK outside the checkout, registry pointing at the SDK,
    # ZEPHYR_SDK_INSTALL_DIR unset. Every sub-check must agree. Before the fix
    # this printed two `[MISSING]` lines and an `[OK]` naming the very path it
    # had just called missing — one fact, two derivations, the disagreement
    # reported as a diagnosis.
    local self e2e out
    self="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/$(basename "${BASH_SOURCE[0]}")"
    e2e="$tmp/e2e"
    mkdir -p "$e2e"/{home/.cmake/packages/Zephyr-sdk,root,ws/zephyr,bin} \
             "$e2e"/sdk/zephyr-sdk-0.16.8/{cmake,arm-zephyr-eabi/bin}
    printf '0.16.8\n' > "$e2e/ws/zephyr/SDK_VERSION"
    printf '#!/bin/sh\necho "West version: v1.5.0"\n' > "$e2e/bin/west"
    printf '#!/bin/sh\nexit 0\n' > "$e2e/sdk/zephyr-sdk-0.16.8/arm-zephyr-eabi/bin/arm-zephyr-eabi-gcc"
    chmod +x "$e2e/bin/west" "$e2e/sdk/zephyr-sdk-0.16.8/arm-zephyr-eabi/bin/arm-zephyr-eabi-gcc"
    printf '%s\n' "$e2e/sdk/zephyr-sdk-0.16.8/cmake" > "$e2e/home/.cmake/packages/Zephyr-sdk/aaa"

    _st_doctor() { # <home> -> runs the label check in a clean environment
        PATH="$e2e/bin:$PATH" HOME="$1" NROS_RUNNER_REPO_ROOT="$e2e/root" \
        NROS_ZEPHYR_WORKSPACE="$e2e/ws" NROS_RUNNER_NO_ACTIVATE=1 \
        ZEPHYR_SDK_INSTALL_DIR= bash "$self" nros-sdk-zephyr 2>&1
    }

    if out="$(_st_doctor "$e2e/home")" && ! printf '%s' "$out" | grep -q MISSING; then
        _st_ok "a runner with its SDK outside the checkout verifies clean"
    else
        _st_bad "a runner with its SDK outside the checkout verifies clean" \
            "$(printf '%s' "$out" | grep -E 'MISSING|FAIL' | head -2 | tr '\n' ';')"
    fi

    # ...and it can still FAIL. With no registry entry and no SDK anywhere, the
    # check must go red — a doctor that cannot fail is the vacuous gate this
    # file's header is about.
    mkdir -p "$e2e/empty-home/.cmake/packages/Zephyr-sdk"
    if _st_doctor "$e2e/empty-home" >/dev/null 2>&1; then
        _st_bad "a host with no SDK at all still fails" "exited 0"
    else
        _st_ok "a host with no SDK at all still fails"
    fi

    [ -n "$saved_env" ] && export ZEPHYR_SDK_INSTALL_DIR="$saved_env"
    HOME="$saved_home"

    if [ "$fails" -ne 0 ]; then
        echo "runner-doctor self-test: FAIL ($fails)" >&2
        return 1
    fi
    echo "runner-doctor self-test OK"
    return 0
}

# --- standalone entry point --------------------------------------------------
#
# Guarded so `. scripts/ci/runner-doctor.sh` defines the functions and runs
# nothing. Note `set -e` is deliberately NOT used even on this path: the point
# of the script is to report EVERY unmet claim in one pass, and `-e` would stop
# at the first. `set -u` and `pipefail` still apply, matching
# scripts/check-issue-index.sh.
_nros_runner_doctor_main() {
    set -uo pipefail
    local labels="" arg
    for arg in "$@"; do
        case "$arg" in
            --check|--dry-run)
                # Accepted and inert: this script only probes, so its normal
                # mode already makes no changes. The flag exists so all four
                # runner scripts take the same one.
                ;;
            --quiet) NROS_RUNNER_QUIET=1 ;;
            --self-test)
                # Probes nothing on this host; see `_nros_runner_self_test`.
                _nros_runner_self_test
                return $?
                ;;
            -h|--help)
                cat <<EOF
usage: scripts/ci/runner-doctor.sh <labels> [--check] [--quiet]
       scripts/ci/runner-doctor.sh --self-test

  <labels>   comma- or space-separated, e.g. nros-qemu,nros-sdk-zephyr,nros-big
             GitHub's own labels (self-hosted, linux, X64) are accepted and
             assert nothing.

  --check    accepted and inert — this script never changes anything.
  --quiet    print only failures.

  --self-test  check this file's own SDK-location resolver against temp dirs.
               Probes nothing on this host; needs no SDK. Issue 0980.

Known labels: $NROS_RUNNER_LABELS

Env:
  NROS_RUNNER_REPO_ROOT   the checkout to inspect (default: this file's own)
  NROS_RUNNER_BIG_CORES   the nros-big threshold (default 16)
  NROS_ZEPHYR_WORKSPACE   where the Zephyr sources live
  ZEPHYR_SDK_INSTALL_DIR  where the Zephyr SDK was unpacked

Exit: 0 every claim holds, 1 at least one does not, 2 usage error.
EOF
                return 0
                ;;
            -*)
                echo "runner-doctor: unknown option '$arg'" >&2
                return 2
                ;;
            *)
                if [ -n "$labels" ]; then
                    labels="$labels,$arg"
                else
                    labels="$arg"
                fi
                ;;
        esac
    done
    if [ -z "$labels" ]; then
        echo "runner-doctor: usage: scripts/ci/runner-doctor.sh <labels> [--check]" >&2
        echo "  Known labels: $NROS_RUNNER_LABELS" >&2
        return 2
    fi
    nros_runner_doctor "$labels"
}

if [ "${BASH_SOURCE[0]}" = "$0" ]; then
    _nros_runner_doctor_main "$@"
    exit $?
fi
