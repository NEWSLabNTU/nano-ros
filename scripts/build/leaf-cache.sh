#!/usr/bin/env bash
# Sourced, never run. Shared by `workspace-fixtures-build.sh` and
# `census-prepass.sh`: every script that runs `nros build --offline` warms the
# cargo cache for that image first, through this one function. The prepass had
# no warm step and died on a cold cache (issue 1848) while its sibling builder
# had one (issue 0967), which is why there is one copy now.
#
# Self-sufficient: `repo_root` and `nros_cli` default from this file and the CLI ladder.

# nros_warm_leaf_cache <workspace-dir> [<image>] [<the image's dry-run plan, if the caller has it>]
#
# Populate the cargo registry cache for one workspace leaf, ONCE, before the
# `--offline` build that follows. Issue 0967.
#
# `nros build --offline` passes `--frozen` (= `--locked --offline`, issue 0676),
# and for a GENERATED root it first runs `cargo generate-lockfile --offline`.
# Both can only use crates already in the cache. `examples/workspaces/*` leaves
# are SEPARATE cargo workspace roots: CI provisioning warms the cache for the
# ROOT graph, and nothing warmed it for the leaves. So the resolve wanted crates
# nobody had fetched and cargo reported
#
#     error: failed to download `toml v0.9.12+spec-1.1.0`
#     Caused by: attempting to make an HTTP request, but --frozen was specified
#
# — a headline that reads as a network flake and invites a retry that cannot
# help. `host-tests` died on it for 20 consecutive runs, in the step BEFORE any
# test, so the only CI lane that runs E2E ran nothing at all.
#
# This does NOT relax `--locked` or `--frozen`. The BUILD stays hermetic, which
# is the property issue 0676 wants; only this prepare step reaches the network,
# exactly as `nros setup --source` already does. A lockfile that moves because
# CI could not resolve is the drift issues 0359/0378 exist to prevent, and
# fetching is what avoids it — permitting re-resolution would not.
#
# Local runs pay nothing: a warm cache makes this a no-op, which is also why the
# defect never showed up in a developer sweep — only `--offline` exposes it.
# nros_plan_cargo_build_args < <dry-run plan>
#
# Read `nros build --dry-run`'s stage-5 line for a cargo image and print
# `manifest <path>` and, when there is one, `config <file>`. The line is
# `Handoff::display()`: env assignments first (`K=V `, unquoted, so a value
# may hold spaces), then the program (bare `cargo` or a path to it), then its
# args. So the line is found by its `cargo build` words wherever they sit, and
# the args are read from there on. The FIRST `--config` that names a file is
# the image's settings; a later `--config key=value` (`build.build-dir=…`) is
# not a file and is skipped.
nros_plan_cargo_build_args() {
    awk '
        !done {
            for (i = 1; i < NF; i++) {
                if (($i == "cargo" || $i ~ /\/cargo$/) && $(i + 1) == "build") {
                    m = ""; c = ""
                    for (j = i + 2; j < NF; j++) {
                        if ($j == "--manifest-path" && m == "") m = $(j + 1)
                        if ($j == "--config" && c == "" && $(j + 1) !~ /=/) c = $(j + 1)
                    }
                    if (m != "") {
                        print "manifest " m
                        if (c != "") print "config " c
                        done = 1
                    }
                    break
                }
            }
        }'
}

_NROS_WARMED_LEAVES=""
_NROS_WARMED_ROOT=""
_NROS_WARMED_IMAGES=""
nros_warm_leaf_cache() {
    local ws="${1:?nros_warm_leaf_cache: workspace dir}"
    # Self-sufficient, so any driver can call it: the checkout root and the CLI
    # from the caller when it has them, else derived here.
    local repo_root="${repo_root:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)}"
    if ! declare -F nros_cli_bin >/dev/null; then
        # shellcheck source=scripts/build/cargo.sh
        source "$repo_root/scripts/build/cargo.sh"
    fi
    local nros_cli="${nros_cli:-${NROS_CLI_BIN:-$(nros_cli_bin 2>/dev/null || true)}}"
    if [ "${NROS_SKIP_LEAF_FETCH:-0}" != "0" ]; then
        case " $_NROS_WARMED_LEAVES " in *" $ws "*) ;; *)
            echo "  leaf-fetch: SKIPPED for $ws (NROS_SKIP_LEAF_FETCH set)" ;;
        esac
        _NROS_WARMED_LEAVES="$_NROS_WARMED_LEAVES $ws"
        return 0
    fi
    _NROS_WARMED_LEAVES="$_NROS_WARMED_LEAVES $ws"

    # TWO roots, because the generated root draws from both and MEASUREMENT
    # showed the leaf alone is not enough. Against a cold CARGO_HOME, a leaf
    # fetch pulled `allocator-api2` but NOT `toml 0.9.12+spec-1.1.0` — the two
    # crates CI reported. `toml` reaches the generated root through nano-ros
    # PATH deps (cbindgen, nros-bridge, nros-tests), whose registry deps live in
    # the REPO ROOT lock, not the leaf's. Warming only the leaf would have
    # fixed half the failure and looked like a fix.
    if [ -z "$_NROS_WARMED_ROOT" ]; then
        _NROS_WARMED_ROOT=1
        bash "$repo_root/scripts/build/warm-offline-cargo.sh"  # root + std deps for an offline build (issue 2560)
    fi

    # The LEAF half warms the graph the build will actually resolve: the
    # GENERATED cargo root `nros build` writes for this image. It used to test
    # `$ws/Cargo.toml` and fetch there — but phase-445 W6 removed every
    # workspace-root `Cargo.toml` under `examples/workspaces/`
    # (`check-workspace-root-build-files`), so that test failed for all 16
    # workspaces and this half returned early, silently, for every one of them.
    # Only the repo-root fetch above still ran. Crates the root graph does not
    # reach went unfetched, and the esp32 image — whose `esp-hal`,
    # `esp-backtrace` and `allocator-api2` live only in its own generated
    # graph — failed every nightly with `failed to download allocator-api2`
    # "but --frozen was specified", the exact headline this function exists to
    # prevent.
    #
    # `--dry-run --offline` runs stages 1-4 (which WRITE the generated root)
    # and prints the stage-5 commands instead of exec'ing them; it touches no
    # network. The fetch below is then the one sanctioned network step, against
    # that root with its own `--config` (its `[patch.crates-io]` rows decide
    # what resolves). A cmake/west-driven image prints no `cargo build` line and
    # has no cargo root to warm, so it is skipped rather than guessed at.
    # ONE entry for every road (issue 2444's follow-up): the image's plan is
    # read line by line, and each build line warms its own graph. A `cargo build`
    # line fetches its generated root; a `west build` line warms the Rust app
    # zephyr-lang-rust will resolve in (`nros_warm_west_rust_app`); a cmake road
    # prints neither. With no image the dry run plans the workspace's default
    # images (`compile-check-fixtures.sh` builds that way), and every line of it
    # is warmed. Keyed per (workspace, image), never per workspace: a second
    # image of one workspace has its own generated root.
    # A caller that already ran this image's dry run passes its stdout as the
    # third argument, so the image is planned ONCE (the Zephyr runner's
    # signature reads the same plan, issue 1707).
    local img="${2:-}" given_plan="${3:-}"
    local key="$ws|${img:-<default images>}"
    case " $_NROS_WARMED_IMAGES " in *" $key "*) return 0 ;; esac
    _NROS_WARMED_IMAGES="$_NROS_WARMED_IMAGES $key"

    local plan line parsed manifest cfg ok=1 roots=0
    if [ -n "$given_plan" ] && plan="$given_plan" \
        || plan="$(cd "$ws" && "$nros_cli" build ${img:+"$img"} --workspace . --offline --dry-run 2>&1)"; then
        while IFS= read -r line; do
            case "$line" in
                *"west build "*)
                    roots=$((roots + 1))
                    (cd "$ws" && nros_warm_west_rust_app "$line") ;;
                *cargo*" build "*)
                    parsed="$(printf '%s\n' "$line" | nros_plan_cargo_build_args)"
                    manifest="$(sed -n 's/^manifest //p' <<<"$parsed")"
                    cfg="$(sed -n 's/^config //p' <<<"$parsed")"
                    [ -n "$manifest" ] || continue
                    roots=$((roots + 1))
                    ( cd "$ws" && cargo fetch --manifest-path "$manifest" \
                        ${cfg:+--config "$cfg"} >/dev/null 2>&1 ) || ok=0 ;;
            esac
        done <<<"$plan"
        if [ "$roots" -eq 0 ] && [[ $plan == *'driver cargo'* ]]; then
            # A CARGO image whose plan this could not read is the defect, and it
            # used to return here in silence: the plan's line grew an env prefix
            # (the stack-floor flags, a RUSTUP_TOOLCHAIN) and `^cargo build`
            # stopped matching, so nothing was fetched and the --frozen build
            # failed on `failed to download rtic v2.3.0`.
            echo "  leaf-fetch: could NOT find the cargo build line in the plan for ${img:-the default images};" >&2
            echo "              nothing was fetched for its generated root." >&2
            return 0
        fi
        [ "$ok" = 1 ] && return 0
    fi
    # Not fatal: an environment that is offline BY DESIGN with a warm cache is
    # legitimate, and failing here would break it. But say so, because the
    # build that follows may now fail with cargo's misleading "failed to
    # download", and this is the sentence that explains it.
    echo "  leaf-fetch: could NOT warm the cargo cache for $ws." >&2
    echo "              If the build below reports 'failed to download <crate>'" >&2
    echo "              with 'but --frozen was specified', that is why — the" >&2
    echo "              crate is missing from the cache, not from the network." >&2
    echo "              (issue 0967; NROS_SKIP_LEAF_FETCH=1 to silence)" >&2
    return 0
}

# nros_warm_west_rust_app <west-plan-line>
# A west-driven Rust image (Zephyr) has a cargo root too: zephyr-lang-rust runs
# `cargo rustc` in the generated APPLICATION dir, with the zephyr crates patched
# to the west workspace. `nros_warm_leaf_cache` skips a west plan ("no cargo
# root"), which held only while that cargo could reach the network. Issue 2444
# made `nros build --offline` set `CARGO_NET_OFFLINE` for every cargo it drives,
# west's included, so an unwarmed app graph now fails on the first crate only it
# needs: live-peer 43675564346 (2026-11-15) died `failed to download
# annotate-snippets v0.11.5` (zephyr-sys's `bindgen` `experimental` feature)
# with "--offline was specified". So: seed the app's lock from the registry pins,
# as the leaf lanes do (issue 1937), then fetch its graph ONCE with the same
# three patches the build passes. Reads the app and ZEPHYR_BASE off the plan's
# own `west build` line, never a second derivation of them.
nros_warm_west_rust_app() {
    local plan="${1:?nros_warm_west_rust_app: west plan line}"
    [ -n "${NROS_SKIP_LEAF_FETCH:-}" ] && return 0
    local app="" zbase
    # The application is west's one positional argument, before any `--`:
    # skip the options that take a value (`-b`, `-d`, `-p`, `-s`, `-t`).
    local -a w
    read -r -a w <<<"${plan#*west build }"
    local i=0
    while [ "$i" -lt "${#w[@]}" ]; do
        case "${w[$i]}" in
            --) break ;;
            -b | -d | -p | -s | -t | --board | --build-dir | --pristine | --source-dir | --target) i=$((i + 2)) ;;
            -*) i=$((i + 1)) ;;
            *) app="${w[$i]}"; break ;;
        esac
    done
    zbase="$(sed -n 's/.*ZEPHYR_BASE=\([^ ]*\).*/\1/p' <<<"$plan")"
    [ -n "$app" ] && [ -f "$app/Cargo.toml" ] || return 0
    case " ${_NROS_WARMED_APPS:-} " in *" $app "*) return 0 ;; esac
    _NROS_WARMED_APPS="${_NROS_WARMED_APPS:-} $app"
    if [ -z "$zbase" ]; then
        echo "  leaf-fetch: the west plan names no ZEPHYR_BASE; $app is not warmed." >&2
        return 0
    fi
    local rust="$zbase/../modules/lang/rust"
    nros_seed_leaf_lock "$app"
    if ( cd "$app" && CARGO_NET_OFFLINE=false cargo fetch \
            --config "patch.crates-io.zephyr.path=\"$rust/zephyr\"" \
            --config "patch.crates-io.zephyr-build.path=\"$rust/zephyr-build\"" \
            --config "patch.crates-io.zephyr-sys.path=\"$rust/zephyr-sys\"" \
            >/dev/null 2>&1 ); then
        return 0
    fi
    echo "  leaf-fetch: could NOT warm the cargo cache for the west app $app." >&2
    echo "              If its build reports 'failed to download <crate>' with" >&2
    echo "              '--offline was specified', that is why (issue 0967)." >&2
    return 0
}
