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

# `pwd -P` resolves symlinks so a checkout reached through a symlinked parent
# records the ONE physical path (issue 0375; mirror of activate.sh).
set -l _nros_root (cd (dirname (status -f)); pwd -P)
set -gx NROS_REPO_DIR $_nros_root
# RFC-0048 (phase-287): find_package(nano_ros) locates nano_rosConfig.cmake via
# CMake's <pkg>_ROOT env var — a sourced shell then needs no -Dnano_ros_ROOT.
set -gx nano_ros_ROOT $_nros_root

# pyo3 0.24 caps the interpreter at 3.13; a rolling distro ships 3.14 and the
# resolver build dies. Build against the stable ABI instead — inert where the
# interpreter is already supported. Mirror of activate.sh.
set -q PYO3_USE_ABI3_FORWARD_COMPATIBILITY; or set -gx PYO3_USE_ABI3_FORWARD_COMPATIBILITY 1

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

# Rustup-managed toolchain (mirror of activate.sh): bootstrap.sh installs
# rustup, but only future shells pick up ~/.cargo/bin — the shell that just
# ran bootstrap needs it wired here (issue #204 probe finding).
if test -d $HOME/.cargo/bin; and not contains $HOME/.cargo/bin $PATH
    set -gx PATH $HOME/.cargo/bin $PATH
end

# `nros` CLI resolution: the in-tree per-checkout binary (mirror of
# `activate.sh`). The pre-218 `~/.nros/bin/nros` curl install is
# retired; `packages/cli/target/release/nros` is the sole source.
if test -x $_nros_root/packages/cli/target/release/nros
    set -gx PATH $_nros_root/packages/cli/target/release $PATH
else if not set -q NROS_QUIET_ACTIVATE; and not command -v nros >/dev/null 2>&1
    # Phase 222.F.2 — first-run hint (fish mirror of activate.sh §222.F.1).
    # See activate.sh for rationale; NROS_QUIET_ACTIVATE=1 suppresses.
    echo "[nano-ros] CLI not built yet. Run:" >&2
    echo "  ./scripts/bootstrap.sh           (builds the CLI from source; installs rustup if needed)" >&2
    echo "  Equivalent, if you have cargo:" >&2
    echo "  git submodule update --init packages/cli/third-party/play_launch \\" >&2
    echo "    && cargo build --release --manifest-path packages/cli/Cargo.toml --bin nros" >&2
    echo "  (set NROS_QUIET_ACTIVATE=1 to suppress this hint.)" >&2
end

# play_launch_parser
set -l _nros_play_root (set -q NROS_HOME; and echo $NROS_HOME/sdk/play_launch_parser; or echo $HOME/.nros/sdk/play_launch_parser)
if test -x $_nros_play_root/bin/play_launch_parser
    set -gx PATH $_nros_play_root/bin $PATH
else
    # phase-327 W3 — `nros setup --tool play_launch_parser` installs to the
    # VERSIONED store layout (sdk/<tool>/<version>/bin), which the unversioned
    # path above misses. This fallback existed only in activate.sh until issue
    # 0372; the two files are hand-mirrored, so it has to be kept in step here.
    for _plp_bin in (find $_nros_play_root -mindepth 2 -maxdepth 2 -type d -name bin 2>/dev/null)
        if test -x $_plp_bin/play_launch_parser
            set -gx PATH $_plp_bin $PATH
            break
        end
    end
    set -e _plp_bin
end

# Cross-compiler toolchains installed by `nros setup` (SDK store
# ~/.nros/sdk/<tool>/<version>/bin). A cross-gcc MUST be on PATH for cargo's
# `linker=` and NuttX/Zephyr `make` to find it (e.g. riscv-none-elf-gcc, Phase
# 194.3c). Scoped to a store-bin whitelist so qemu stays off
# PATH (resolved via build/<tool>); zenohd is whitelisted for the book's
# first-node flow (issue #204). Mirror of the activate.sh block.
set -l _nros_sdk (set -q NROS_HOME; and echo $NROS_HOME/sdk; or echo $HOME/.nros/sdk)
if test -d $_nros_sdk
    for _nros_tcbin in $_nros_sdk/*/*/bin $_nros_sdk/*/bin
        # issue 0663 — the tool list is DATA, in scripts/sdk-path-tools.txt,
        # shared with activate.sh. This copy was a hand-written chain and had
        # already drifted from the bash one (it lacked `espflash`), so the same
        # provisioned host behaved differently depending on the shell.
        set -l _nros_want 0
        if test -d $_nros_tcbin
            if count $_nros_tcbin/*-gcc >/dev/null 2>&1
                set _nros_want 1
            else
                for _nros_tool in (string trim (string replace -r '#.*' '' < $_nros_root/scripts/sdk-path-tools.txt))
                    test -n "$_nros_tool"; or continue
                    if test -x $_nros_tcbin/$_nros_tool
                        set _nros_want 1
                        break
                    end
                end
            end
        end
        if test $_nros_want -eq 1
            set -gx PATH $_nros_tcbin $PATH
        end
    end
end

# Pinned ninja + make (Phase 176 jobserver tooling) — no block here any more:
# both are ordinary SDK-store tools, so the generic store loop above puts them
# on PATH via `scripts/sdk-path-tools.txt`. Keeping a hand-written block in each
# shell is exactly the drift that file was created to end (issue 0663 —
# `espflash` was in the bash one and not this one).

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

# sdk-env.sh is POSIX — fish can't `source` it directly, so ask it to EMIT
# fish. It has a `--fish` printer whose variable list is derived from the
# `just/sdk-env.just` SSoT, so this stays in step with bash by construction.
#
# Issue 0451 — this used to dump the subshell's whole `env` and import only
# names matching `NROS_*`. That filter dropped every third-party SDK root:
# measured in a clean environment, a fish user got 2 of 15 probed variables —
# no FREERTOS_DIR, NUTTX_DIR, THREADX_DIR, IDF_PATH, LWIP_DIR,
# PX4_AUTOPILOT_DIR or TBAND_DIR. The filter existed only because an `env` dump
# also carries PATH and everything else; emitting exactly the SSoT variables
# removes the reason for a filter, and with it the third place this list was
# spelled.
if test -f $_nros_root/scripts/sdk-env.sh
    bash $_nros_root/scripts/sdk-env.sh --fish 2>/dev/null | source
end

set -e _nros_root

# Zephyr's `west` is NOT put on PATH here — see the activate.sh twin. It is
# resolved on demand by `nros_zephyr_activate` (scripts/build/zephyr-python.sh).
#
# Worth recording that this block could never have worked anyway: `_nros_root`
# is erased by the `set -e _nros_root` a few lines above, so the test ran
# against an empty path and was always false. Two shells claiming the same
# behaviour, one of them dead — which is the argument for resolving in ONE
# place that both shells reach rather than mirroring logic per shell.
