# nano-ros workspace activation — fish shell.
#
# Phase 218.C — fish-shell mirror of `activate.sh`. Source after clone:
#
#     source ./activate.fish
#
# Hand-maintained sibling of `activate.sh`. When you change one, sync
# the other. The two files share no autogen pipeline by design — a
# generator would be a sharper edge than parallel hand-edits across
# two ~50 LoC files.

set -l _nros_root (cd (dirname (status -f)); pwd)
set -gx NROS_REPO_DIR $_nros_root

# ROS 2 Humble — fish needs `bass` or a hand-port of setup.bash. The
# user `source setup.fish` if their ROS install ships one; otherwise
# we leave AMENT/CMAKE prefix paths unset. The recipes that need ROS
# either source it themselves (just/zephyr.just) or document the
# requirement in their README.
if test -f /opt/ros/humble/setup.fish
    source /opt/ros/humble/setup.fish
else if test -f /opt/ros/humble/setup.bash
    echo "activate.fish: fish shell — /opt/ros/humble/setup.bash exists but no setup.fish." >&2
    echo "Install the 'bass' fish plugin (https://github.com/edc/bass) and run:" >&2
    echo "    bass source /opt/ros/humble/setup.bash" >&2
    echo "or use a bash subshell for ROS-dependent commands." >&2
end

# `nros` CLI resolution: the in-tree per-checkout binary (mirror of
# `activate.sh`). The pre-218 `~/.nros/bin/nros` curl install is
# retired; `packages/cli/target/release/nros` is the sole source.
if test -x $_nros_root/packages/cli/target/release/nros
    set -gx PATH $_nros_root/packages/cli/target/release $PATH
else if not set -q NROS_QUIET_ACTIVATE; and not command -v nros >/dev/null 2>&1
    # Phase 222.F.2 — first-run hint (fish mirror of activate.sh §222.F.1).
    # See activate.sh for rationale; NROS_QUIET_ACTIVATE=1 suppresses.
    echo "[nano-ros] CLI not built yet. Run one of:" >&2
    echo "  ./scripts/bootstrap.sh base      (recommended; bare machine OK)" >&2
    echo "  git submodule update --init packages/cli/third-party/ros-launch-manifest \\" >&2
    echo "    && cargo build --release --manifest-path packages/cli/Cargo.toml --bin nros   (if you have cargo)" >&2
    echo "  ./scripts/install-nros-prebuilt.sh   (tagged checkout; downloads prebuilt)" >&2
    echo "  (set NROS_QUIET_ACTIVATE=1 to suppress this hint.)" >&2
end

# play_launch_parser
set -l _nros_home_play (set -q NROS_HOME; and echo $NROS_HOME/sdk/play_launch_parser/bin; or echo $HOME/.nros/sdk/play_launch_parser/bin)
if test -x $_nros_home_play/play_launch_parser
    set -gx PATH $_nros_home_play $PATH
end

# Cross-compiler toolchains installed by `nros setup` (SDK store
# ~/.nros/sdk/<tool>/<version>/bin). A cross-gcc MUST be on PATH for cargo's
# `linker=` and NuttX/Zephyr `make` to find it (e.g. riscv-none-elf-gcc, Phase
# 194.3c). Scoped to store bin dirs holding a `*-gcc` so qemu/zenohd stay off
# PATH (resolved via build/<tool>). Mirror of the activate.sh block.
set -l _nros_sdk (set -q NROS_HOME; and echo $NROS_HOME/sdk; or echo $HOME/.nros/sdk)
if test -d $_nros_sdk
    for _nros_tcbin in $_nros_sdk/*/*/bin $_nros_sdk/*/bin
        # Cross-gcc toolchains, plus build host tools the RTOS `make` invokes by
        # bare name (genromfs — the NuttX rv-virt etc/ ROMFS bake, Phase 194.3c),
        # and sccache (issue #74) — RUSTC_WRAPPER + the zephyr CMake launcher
        # auto-use it once on PATH.
        if test -d $_nros_tcbin; and begin; count $_nros_tcbin/*-gcc >/dev/null 2>&1; or test -x $_nros_tcbin/genromfs; or test -x $_nros_tcbin/sccache; end
            set -gx PATH $_nros_tcbin $PATH
        end
    end
end

# Pinned ninja + make (Phase 176 jobserver tooling)
if test -x $_nros_root/third-party/ninja/ninja
    set -gx PATH $_nros_root/third-party/ninja $PATH
end
if test -x $_nros_root/third-party/make/make
    set -gx PATH $_nros_root/third-party/make $PATH
end

# Project `.env` — fish doesn't natively `source` POSIX dotenv files;
# parse KEY=value pairs manually. Lines with comments or empty are
# skipped. Quotes are stripped if the value is fully wrapped in them.
if test -f $_nros_root/.env
    while read -l line
        # strip leading whitespace
        set line (string trim $line)
        # skip comments + empties
        if test -z "$line"; or string match -q '#*' -- $line
            continue
        end
        set -l kv (string split -m 1 = $line)
        if test (count $kv) -ne 2
            continue
        end
        set -l key (string trim $kv[1])
        set -l val (string trim $kv[2])
        # unwrap matched quotes
        set val (string replace -r '^"(.*)"$' '$1' -- $val)
        set val (string replace -r "^'(.*)'\$" '$1' -- $val)
        set -gx $key $val
    end <$_nros_root/.env
end

# sdk-env.sh is POSIX — fish can't `source` it directly. Spawn a bash
# subshell to evaluate the exports + capture them. Same approach as
# the `bass` plugin uses for setup.bash.
if test -f $_nros_root/scripts/sdk-env.sh
    set -l _nros_env_dump (bash -c "set -a; source $_nros_root/scripts/sdk-env.sh; env" 2>/dev/null)
    for line in $_nros_env_dump
        set -l kv (string split -m 1 = $line)
        if test (count $kv) -eq 2; and string match -q 'NROS_*' -- $kv[1]
            set -gx $kv[1] $kv[2]
        end
    end
end

set -e _nros_root
