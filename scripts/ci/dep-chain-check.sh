#!/usr/bin/env bash
# Phase 196.6 — per-platform dependency-chain validation (light, no full builds).
#
# For each (board, rmw) cell it proves the dep chain *resolves* — it does NOT
# compile every platform (that's the sparse `just build-all` / `zephyr-dual-line`
# lanes). Per cell:
#   1. toolchain side : `nros setup <board> --rmw <rmw> --dry-run`
#                       (the [board.*]/[rmw.*] index wiring → prebuilt host tools)
#   2. codegen        : `nros dev codegen rust` in the example (produces the
#                       generated/ interface path-crates the example deps on)
#   3. crate/feature  : `cargo tree` from the example dir (its .cargo/config.toml
#                       [patch.crates-io] + `target=` apply) — resolution only.
#
# Catches a broken feature/crate/toolchain wiring (missing optional dep, a
# feature that won't resolve on a target, a board→toolchain typo) in seconds.
#
# Preconditions (fail loud — never silently pass, per CLAUDE.md):
#   - ROS 2 sourced: `nros dev codegen rust` resolves std_msgs's .msg via
#     AMENT_PREFIX_PATH. `source /opt/ros/<distro>/setup.bash` first.
#   - $NROS points at the `nros` CLI. Phase 218 resolution order:
#     $NROS env → packages/cli/target/release/nros (in-tree build via
#     `just setup-cli`) → `nros` on PATH → ~/.nros/bin/nros
#     (transitional fallback). The packages/codegen submodule that
#     pre-218 carried the in-tree codegen is retired — the CLI now
#     lives at packages/cli/ as a sub-workspace.
#
# Usage: source /opt/ros/humble/setup.bash && scripts/ci/dep-chain-check.sh
set -uo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/../.." # repo root

# The one resolver (issue 1850): this checkout's build, PATH, the store. The
# last spelling only names the path the precondition below reports missing.
# shellcheck source=scripts/build/cargo.sh
. scripts/build/cargo.sh
if [ -n "${NROS:-}" ]; then
    : # already set; respect the override
elif ! NROS="$(nros_cli_bin 2>/dev/null)"; then
    NROS="${NROS_HOME:-$HOME/.nros}/bin/nros"
fi
INDEX="${NROS_SDK_INDEX:-nros-sdk-index.toml}"

# --- preconditions ---
if [ -z "${AMENT_PREFIX_PATH:-}" ]; then
    echo "ERROR: ROS 2 not sourced (AMENT_PREFIX_PATH unset). 'nros dev codegen rust'" >&2
    echo "       needs std_msgs's .msg defs. Run: source /opt/ros/<distro>/setup.bash" >&2
    exit 1
fi
if [ ! -x "$NROS" ]; then
    echo "ERROR: nros CLI not found at '$NROS'. Run 'just setup-cli' (Phase 218)" >&2
    echo "       or set \$NROS to a built binary." >&2
    exit 1
fi
# Absolute — used inside `cd "$ex"` subshells below.
NROS="$(cd "$(dirname "$NROS")" && pwd)/$(basename "$NROS")"

# --- the board × rmw matrix (rust talker; resolvable RMWs only) ---
# Skipped on purpose: linux+cyclonedds (pending 171.C.1), zephyr (the
# west/cmake build is covered by zephyr-dual-line, not this cargo-tree lane),
# and esp32-c3-baremetal, which is DORMANT since #1525: its index row is gone,
# so `nros setup esp32-c3-baremetal` answers `unknown board` (post-submit runs
# 37585621912 and 37586283504). Restore the cell when ESP32 leaves dormancy.
CELLS=(
    "linux:zenoh"
    "linux:xrce"
    "mps2-an385-baremetal:zenoh"
    "mps2-an385-freertos:zenoh"
    "qemu-armv7a-nuttx:zenoh"
    "rv-virt-threadx:zenoh"
    "threadx-linux:zenoh"
)

pass=0
fail=0
failed_cells=()

for cell in "${CELLS[@]}"; do
    board="${cell%%:*}"
    rmw="${cell##*:}"
    ex="examples/${board}/rust/talker"
    echo "::group::${board} / ${rmw}"
    cell_ok=1

    # phase-531 W6: a witness talker is a one-package WORKSPACE (RFC-0122 D3),
    # not a cargo leaf, so it has no `$ex/Cargo.toml`. Its dep chain is the
    # GENERATED ENTRY's: `nros configure` writes `build/<plat>-<rmw>/<image>_entry`
    # plus that image's `nros-cargo.toml`, and `cargo tree` resolves from there
    # with `--config`, which is how `nros build` runs cargo. The image is the
    # one the bringup builds for this board on this RMW with no extra node
    # features (`linux` / `linux_xrce`, not `linux_tls`).
    ws_image=""
    if [ ! -f "$ex/Cargo.toml" ] && [ -f "$ex/.colcon_workspace" ]; then
        if ! ws_image="$(python3 - "$ex" "$board" "$rmw" "$NROS" <<'PY'
import glob, json, subprocess, sys
try:
    import tomllib
except ModuleNotFoundError:
    import tomli as tomllib
ex, board, rmw, nros = sys.argv[1:]

# A bringup names its board by any name the catalog resolves (`freertos`
# for mps2-an385-freertos, `nuttx` for qemu-armv7a-nuttx), so the cell's
# board and an image's are the same board when they resolve to the same
# board package. The CLI is the one resolver; never compare spellings.
_dirs = {}
def board_dir(name):
    if name not in _dirs:
        out = subprocess.run([nros, "board", "info", name],
                             capture_output=True, text=True)
        _dirs[name] = (json.loads(out.stdout)["board_dir"]
                       if out.returncode == 0 else None)
    return _dirs[name]

want = board_dir(board)
if want is None:
    sys.exit(f"`nros board info {board}` does not resolve")
hits = []
defaults = set()
for path in sorted(glob.glob(f"{ex}/src/*/system.toml")):
    doc = tomllib.load(open(path, "rb"))
    sys_rmw = doc.get("system", {}).get("rmw", "zenoh")
    defaults.update(doc.get("system", {}).get("default_images", []))
    dflt = doc.get("image_defaults", {})
    for img, row in (doc.get("image") or {}).items():
        if row.get("node_features"):
            continue
        img_board = row.get("board", dflt.get("board"))
        if img_board is None or board_dir(img_board) != want:
            continue
        if row.get("rmw", dflt.get("rmw", sys_rmw)) == rmw:
            hits.append(img)
# Two images can share a board and an RMW and differ only in deployment (the
# bare-metal talker's `docker` image is its compose test's network). The one
# a plain build takes is the bringup's `default_images` entry, so that breaks
# the tie; a tie it cannot break still fails.
if len(hits) > 1 and len([h for h in hits if h in defaults]) == 1:
    hits = [h for h in hits if h in defaults]
if len(hits) != 1:
    sys.exit(f"want exactly one {board}/{rmw} image without node_features in {ex}, found {hits}")
print(hits[0])
PY
)"; then
            echo "  [FAIL] cannot pick the ${board}/${rmw} image of the workspace witness $ex"
            cell_ok=0
        fi
    fi

    if [ "$cell_ok" -eq 0 ]; then
        :
    elif [ ! -f "$ex/Cargo.toml" ] && [ -z "$ws_image" ]; then
        echo "  [FAIL] no example at $ex"
        cell_ok=0
    else
        # 1. the actual user step: `nros setup <board>` — provisions the board's
        #    prebuilt toolchains AND its source submodules (e.g. the NuttX kernel
        #    and apps). NOT --dry-run: the user does not hand-checkout
        #    submodules, so neither does CI — if a build needs a source, the index
        #    + `nros setup` must provide it (that's part of what this validates).
        #    The store dedups across cells, so toolchains download once.
        if "$NROS" setup "$board" --rmw "$rmw" --index "$INDEX"; then
            echo "  [ok] nros setup (toolchains + sources provisioned)"
        else
            echo "  [FAIL] nros setup $board --rmw $rmw"
            cell_ok=0
        fi

        # 2. codegen the example's interface crates — only when the example
        #    declares ROS msg deps (has a package.xml). Board-driven baremetal
        #    talkers (mps2-an385-baremetal) publish without generated
        #    interfaces, so they ship no package.xml and `nros dev codegen rust`
        #    would fail "Failed to read package.xml" — there is nothing to
        #    generate, so skip codegen for them (#69).
        #    NROS_SKIP_VERSION_CHECK=1: this lane validates dep-chain *resolution*
        #    only (no compile, no runtime), so the abi_guard's stale-standalone-
        #    lockfile mismatch is a false positive here. Example Cargo.locks are
        #    gitignored since phase-277 W7 (never committed), but a stale local
        #    lock from an earlier checkout can still pin an old nros-core
        #    (known-issue #12 pattern: 0.1.0 lock vs 0.5.0 tree after 218.J),
        #    tripping the guard. Bypass so codegen emits generated/ for the
        #    cargo-tree step.
        if [ -n "$ws_image" ]; then
            : # a workspace's messages are generated by `nros configure` below
        elif [ ! -f "$ex/package.xml" ]; then
            echo "  [ok] no package.xml — no generated interfaces, codegen skipped"
        elif ( cd "$ex" && NROS_SKIP_VERSION_CHECK=1 "$NROS" dev codegen rust >/dev/null 2>&1 ); then
            : # generated/ now present
        else
            echo "  [FAIL] nros dev codegen rust (codegen — ROS sourced? msg deps resolvable?)"
            cell_ok=0
        fi

        # 2b. `nros configure` — writes the leaf's `.cargo/config.toml`, which carries
        #     the `[patch.crates-io]` redirect that makes `nros` resolve to this
        #     checkout. Those configs became `nros configure` OUTPUT and were
        #     untracked (3f61284fe), so on a fresh clone — or any tree that has
        #     not synced this leaf — step 3 fails with `no matching package
        #     named 'nros' found`. The check has to perform the setup step it is
        #     validating, exactly as it already does for `nros setup` above.
        tree_dir="$ex"
        tree_cfg=()
        if [ -n "$ws_image" ]; then
            ws_out=""
            if ws_out="$( cd "$ex" && NROS_SKIP_VERSION_CHECK=1 "$NROS" configure 2>&1 )"; then
                # The entry is a cargo package, so the image id's `-` is `_`
                # in its name (`threadx-linux` -> `threadx_linux_entry`).
                entry_dirs=("$ex"/build/*/"${ws_image//-/_}_entry")
                if [ "${#entry_dirs[@]}" -eq 1 ] && [ -f "${entry_dirs[0]}/nros-cargo.toml" ]; then
                    tree_dir="${entry_dirs[0]}"
                    tree_cfg=(--config nros-cargo.toml)
                    echo "  [ok] nros configure (workspace; entry ${tree_dir#"$ex"/})"
                else
                    echo "  [FAIL] nros configure wrote no single ${ws_image//-/_}_entry with nros-cargo.toml under $ex/build"
                    cell_ok=0
                fi
            else
                echo "  [FAIL] nros configure did not complete for the workspace $ex:"
                printf '%s\n' "$ws_out" | tail -20 | sed 's/^/      /'
                cell_ok=0
            fi
        elif [ -f "$ex/package.xml" ] || [ -f "$ex/Cargo.toml" ]; then
            sync_out=""
            if sync_out="$( cd "$ex" && NROS_SKIP_VERSION_CHECK=1 "$NROS" configure 2>&1 )"; then
                : # .cargo/config.toml patch table now present
            else
                # PRINT the refusal. Swallowing it cost a day: `nros configure`
                # refusing for want of `nros-launch-resolve` surfaced only as
                # `cargo tree` saying `no matching package named 'nros' found`
                # three steps later, which reads like a manifest defect and is
                # not one. The reason a step failed belongs in the log of the
                # run that failed.
                echo "  [warn] nros configure did not complete for $ex (dep resolution may fail below)"
                printf '%s\n' "$sync_out" | sed 's/^/      /'
            fi
        fi

        # 3. crate/feature/target dep chain — resolution only (no compile).
        #    Run from the example dir so its .cargo/config.toml patch + target apply.
        #    RMW-selectable examples expose their OWN `rmw-<rmw>` feature; board-
        #    driven ones (post-C6: no `rmw-*` on the example, the board crate
        #    selects the backend) resolve with default features.
        #    Match the package's OWN feature table only — a substring grep over the
        #    whole metadata also hits a DEP's requested features
        #    (e.g. `nros-board-* { features=["rmw-zenoh"] }`), which made
        #    board-driven cells (mps2-an385-baremetal) wrongly pass
        #    `--features rmw-zenoh` and fail "does not contain this feature" (#69).
        feat_args=()
        if [ -n "$ws_image" ]; then
            : # the entry's features are the image's; nothing to select here
        elif ( cd "$ex" && cargo metadata --no-deps --format-version 1 2>/dev/null \
                | python3 -c "import json,sys; sys.exit(0 if 'rmw-${rmw}' in (json.load(sys.stdin)['packages'] or [{}])[0].get('features',{}) else 1)" ); then
            feat_args=(--no-default-features --features "rmw-${rmw}")
        fi
        # ONE run, and its own stderr is the diagnostic. `cargo tree` writes the
        # tree to stdout and its errors to stderr, so keeping stderr is exact; a
        # grep over the merged stream is not, because crate names contain the
        # words it looks for (`grep -iE 'error|failed'` printed three
        # `thiserror v2.0.21` rows instead of the cause — run 36945033296,
        # issue 1625). The repair that followed RE-RAN `cargo tree` and kept
        # its first 10 lines, which is wrong twice: a transient failure does
        # not recur on the re-run, and cargo prints `Updating`/`warning:`/
        # `Locking` lines BEFORE its error, so ten lines from the top are all
        # preamble. Run 37552262262 printed exactly that and still named no
        # cause. Capture the failing run's stderr and print its TAIL.
        #
        # Issue 1992 -- a REGISTRY failure is retried, then reported as one.
        # The root `net.retry = 10` does not reach every cell: the NuttX leaf
        # pins the nightly whose cargo never classes an HTTP/2 framing error
        # as spurious (scripts/lib/cargo_registry.py says why), so one blip
        # failed the cell in five seconds with no retry. The classifier only
        # runs on a failed run, so it can choose WHICH failure is printed,
        # never turn one into a pass.
        if ! tree_attempts="$(python3 scripts/lib/cargo_registry.py attempts)" \
            || ! tree_backoff="$(python3 scripts/lib/cargo_registry.py backoff)"; then
            echo "  [FAIL] scripts/lib/cargo_registry.py failed its own self-test (above)"
            exit 1
        fi
        tree_errfile="$(mktemp)"
        attempt=1
        while [ -z "$ws_image" ] || [ "$cell_ok" -eq 1 ]; do
            tree_rc=0
            tree_err="$( cd "$tree_dir" && cargo tree "${tree_cfg[@]}" "${feat_args[@]}" -e no-dev 2>&1 1>/dev/null )" || tree_rc=$?
            [ "$tree_rc" -eq 0 ] && break
            printf '%s\n' "$tree_err" >"$tree_errfile"
            python3 scripts/lib/cargo_registry.py classify "$tree_errfile" || break
            [ "$attempt" -ge "$tree_attempts" ] && break
            echo "  [retry] cargo tree could not reach the registry (attempt $attempt of $tree_attempts); retrying in $((tree_backoff * attempt)) s"
            sleep "$((tree_backoff * attempt))"
            attempt=$((attempt + 1))
        done
        tree_registry=0
        if [ -n "$ws_image" ] && [ "$cell_ok" -eq 0 ]; then
            tree_rc=skipped
        elif [ "$tree_rc" -ne 0 ] && python3 scripts/lib/cargo_registry.py classify "$tree_errfile"; then
            tree_registry=1
        fi
        rm -f "$tree_errfile"
        if [ "$tree_rc" = skipped ]; then
            echo "  [skip] cargo tree: the workspace entry was not configured (above)"
        elif [ "$tree_rc" -eq 0 ]; then
            echo "  [ok] crate/feature dep chain resolves (${feat_args[*]:-default features})"
        elif [ "$tree_registry" -eq 1 ]; then
            echo "  [FAIL] cargo tree could not reach the crates.io registry after $attempt attempt(s) (issue 1992)."
            echo "         This is a network failure, NOT a dep-chain verdict. Last 20 lines of its stderr:"
            printf '%s\n' "$tree_err" | tail -20 | sed 's/^/      /'
            cell_ok=0
        else
            echo "  [FAIL] cargo tree did not resolve (${feat_args[*]:-default features}), exit ${tree_rc}; last 20 lines of its stderr:"
            printf '%s\n' "$tree_err" | tail -20 | sed 's/^/      /'
            cell_ok=0
        fi
    fi

    if [ "$cell_ok" -eq 1 ]; then
        pass=$((pass + 1)); echo "  PASS"
    else
        fail=$((fail + 1)); failed_cells+=("$cell")
    fi
    echo "::endgroup::"
done

echo ""
echo "dep-chain: $pass passed, $fail failed (of $((pass + fail)) cells)"
if [ "$fail" -ne 0 ]; then
    printf '  FAILED: %s\n' "${failed_cells[@]}"
    exit 1
fi
