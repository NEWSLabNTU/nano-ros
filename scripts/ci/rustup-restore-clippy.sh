#!/usr/bin/env bash
# Make `cargo clippy` usable on every installed toolchain, and FAIL if the
# toolchain this checkout's builds resolve is left without it. Issue 1474.
#
# Called from the `Unblock … clippy-preview conflict` steps in `live-peer.yml`
# and `nightly.yml`, which used to carry two hand copies of a sequence that
# could not work. Zephyr's Rust build runs `cargo clippy` as a ninja step
# (zephyr-lang-rust's `run_rust_clippy`), so a toolchain without clippy fails
# the FIXTURE build — thirty minutes after this step reported success.
#
# WHY THE ORDER IS THE WHOLE FIX. Every row below was measured against an
# isolated RUSTUP_HOME holding a real minimal toolchain (2026-09-29):
#
#   delete files -> add            "component clippy is up to date", binaries
#                                  GONE — CI's exact error. The old sequence.
#   delete files -> remove -> add  WEDGED: remove fails ("directory does not
#                                  exist: 'bin/clippy-driver'") and rolls back,
#                                  add still says "up to date".
#   untracked orphan -> add        "detected conflict: 'bin/cargo-clippy'" —
#                                  the clippy-preview conflict the old step was
#                                  written for.
#   remove -> clear orphans -> add WORKS, in both the clean and orphan cases.
#
# rustup decides "installed" from its own manifest, not from the files, so a
# file deleted behind its back is still installed as far as `add` is concerned
# and can no longer be removed either. `remove` has to run while rustup's files
# are still there; only then is a leftover file an orphan that is safe to
# delete, because rustup no longer claims it.
set -uo pipefail

rustup_home="$(rustup show home 2>/dev/null || echo "$HOME/.rustup")"

# The toolchain the build uses. Resolved from where the builds run, so a
# `rust-toolchain.toml` pin is honoured. Empty if rustup cannot say, in which
# case no toolchain is treated as required and every failure is reported only.
required="$(rustup show active-toolchain 2>/dev/null | awk '{print $1}')"

rc=0
while read -r tc; do
    [ -n "$tc" ] || continue

    rustup component remove clippy --toolchain "$tc" >/dev/null 2>&1 || true
    find "$rustup_home/toolchains/$tc" -maxdepth 2 -type f \
        \( -name cargo-clippy -o -name clippy-driver \) -delete 2>/dev/null || true

    if rustup component add clippy --toolchain "$tc" >/dev/null 2>&1 \
        && rustup run "$tc" cargo clippy --version >/dev/null 2>&1; then
        echo "  clippy: OK on $tc"
    elif [ "$tc" = "$required" ]; then
        echo "  clippy: NOT usable on $tc — the toolchain this checkout's builds" >&2
        echo "          resolve. Zephyr's Rust build runs cargo clippy as a ninja" >&2
        echo "          step, so the fixture build would fail on it later with" >&2
        echo "          \"the 'cargo-clippy' binary … is not applicable\". Issue 1474." >&2
        rc=1
    else
        echo "  clippy: not usable on $tc (not the build's toolchain; reported only)" >&2
    fi
done < <(rustup toolchain list 2>/dev/null | awk '{print $1}')

exit "$rc"
