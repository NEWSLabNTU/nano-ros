#!/usr/bin/env bash
# Phase 214.M.2 — re-append the patched libc to a NuttX example's
# `.cargo/config.toml` after `nros ws sync` has rendered it.
#
# Why this exists
# ---------------
# The pinned nightly-2026-04-11 `std`'s `sys/net/hostname/unix.rs`
# references `libc::_SC_HOST_NAME_MAX`, but crates.io `libc 0.2.183`
# does not define that constant for the NuttX target. The patched fork
# at `third-party/nuttx/libc/` adds it (commit `bc6c8dfc6 Add
# _SC_HOST_NAME_MAX for NuttX target`) plus the rest of the
# NuttX-specific symbols needed by `-Z build-std`.
#
# `packages/boards/nros-board-nuttx-qemu-arm/nros-board.toml` declares
# the libc `[patch.crates-io]` line as part of its `cargo_config`
# template, but `nros` 0.3.7's `ws sync` drops the `[patch.crates-io]`
# section when it renders the file. Until the upstream CLI bug is
# fixed, this shell helper re-adds the patch line in-place.
#
# The patch is idempotent (skips when already present) and a no-op for
# non-NuttX fixtures (detected by the `target = "armv7a-nuttx-*"`
# line in `.cargo/config.toml`).
#
# Usage: nros_nuttx_libc_patch <example_dir>
#
# Hard constraint (CLAUDE.md): we do not touch nros-cli's codegen
# logic. The fix-up runs strictly after `ws sync` in the host-side
# shell.

set -euo pipefail

# Apply the libc patch to `<dir>/.cargo/config.toml` when the dir
# targets NuttX (`armv7a-nuttx-eabi*`). Idempotent. Cargo resolves
# `[patch.crates-io]` paths in `.cargo/config.toml` against the
# **invocation cwd** (the dir holding `Cargo.toml`), not against the
# config file's directory — so we compute the relative path from the
# example dir. The existing smoke fixture at
# `packages/testing/nros-tests/bins/logging-smoke-nuttx-qemu-arm/`
# follows the same convention.
nros_nuttx_libc_patch() {
    local dir="${1:?usage: nros_nuttx_libc_patch <example_dir>}"
    local cfg="$dir/.cargo/config.toml"

    [ -f "$cfg" ] || return 0

    # Only NuttX fixtures need the patch.
    if ! grep -qE '^target = "armv7a-nuttx-eabi' "$cfg"; then
        return 0
    fi

    # Idempotent — skip when already patched.
    if grep -q 'third-party/nuttx/libc' "$cfg"; then
        return 0
    fi

    local root rel
    root="${NROS_REPO_DIR:-${NROS_REPO_ROOT:-${PWD:-}}}"
    if [ -z "$root" ] || [ ! -d "$root/third-party/nuttx/libc" ]; then
        echo "nuttx-libc-patch: cannot resolve repo root (NROS_REPO_DIR / NROS_REPO_ROOT / PWD)" >&2
        return 1
    fi
    rel="$(realpath --relative-to="$dir" "$root/third-party/nuttx/libc")"

    # #127 blocker-1 — a config that already carries a `[patch.crates-io]`
    # table (e.g. an Entry pkg whose `nros ws sync` rendered the generated
    # msg-crate patches) must get the libc line INSERTED under that table.
    # Appending a second `[patch.crates-io]` header is invalid TOML
    # ("could not parse TOML configuration"), which broke every nuttx
    # Entry-pkg fixture build. Only when no table exists do we append the
    # header + line.
    if grep -qE '^\[patch\.crates-io\]' "$cfg"; then
        local tmp
        tmp="$(mktemp "$cfg.XXXXXX")"
        awk -v rel="$rel" '
            { print }
            !done && /^\[patch\.crates-io\]$/ {
                print "# Phase 214.M / #127 — patched libc for build-std (post-`ws sync`"
                print "# fix-up from scripts/build/nuttx-libc-patch.sh). The pinned"
                print "# nightly'\''s `std` references `libc::_SC_HOST_NAME_MAX`, which"
                print "# crates.io libc does not expose for the NuttX target; the fork at"
                print "# `third-party/nuttx/libc/` adds it. Inserted under the EXISTING"
                print "# [patch.crates-io] table — a second header is invalid TOML."
                print "libc = { path = \"" rel "\" }"
                done = 1
            }
        ' "$cfg" > "$tmp"
        mv "$tmp" "$cfg"
        return 0
    fi

    cat >> "$cfg" <<EOF

# Phase 214.M — patched libc for build-std (post-\`ws sync\` fix-up
# from scripts/build/nuttx-libc-patch.sh). The pinned nightly's
# \`std\` references \`libc::_SC_HOST_NAME_MAX\`, which crates.io
# \`libc 0.2.183\` does not expose for the NuttX target. The patched
# fork at \`third-party/nuttx/libc/\` adds it. \`nros-board.toml\`
# declares this patch but \`nros ws sync\` 0.3.7 strips it from the
# rendered config — re-append until the upstream CLI is fixed.
[patch.crates-io]
libc = { path = "$rel" }
EOF
}
