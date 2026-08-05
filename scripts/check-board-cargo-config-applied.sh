#!/usr/bin/env bash
#
# issue 0440 — a leaf that deploys to a board must carry that board's STATIC
# link args.
#
# # The failure this prevents
#
# `nros-board.toml` declares a board's `cargo_config`, and RFC-0032's "third
# leg" splits the link: STATIC args (`-Tdramboot.ld`, `--start-group`, the
# `-l<kernel lib>` list) live in the leaf's `.cargo/config.toml`, while the
# dynamic pieces propagate from the board crate's `build.rs`. The leaf file is
# TRACKED and `nros sync` leaves it alone, so the two are kept in step by hand.
#
# phase-338 W2 collapsed 18 `-entry` packages into their node packages and kept
# the NODE package's config — which had only cpu flags, because a node package
# had never needed to link a kernel. All six NuttX Rust entries then failed with
# ~3680 undefined libc references. Nothing caught it: the config is valid TOML,
# `cargo metadata` is happy, `nros sync` is happy, and the loss is visible only
# at link time on the one platform whose archives went missing.
#
# # What it checks
#
# For each board that declares `cargo_config` with `-l` link args, every leaf
# whose `[package.metadata.nros.entry] deploy` names that board must carry a
# representative arg from it. Representative rather than exhaustive on purpose:
# the point is to catch a config that lost the GROUP, not to diff two files that
# legitimately differ in patch tables and paths.

set -euo pipefail
cd "$(dirname "$0")/.."

fail=0
checked=0

# board dir -> (deploy token, a representative static link arg)
while IFS= read -r board_toml; do
    # Only boards whose cargo_config carries a kernel-archive group. A board
    # with no `-l` args has nothing a leaf could silently lose.
    probe="$(grep -oE 'link-arg=-l[a-z]+' "$board_toml" | head -1 || true)"
    [ -n "$probe" ] || continue

    # Deploy tokens this board answers to (`platform = "..."` in each [[board]]).
    while IFS= read -r deploy; do
        [ -n "$deploy" ] || continue
        while IFS= read -r cargo_toml; do
            grep -q "deploy = \"$deploy\"" "$cargo_toml" 2>/dev/null || continue
            leaf_cfg="$(dirname "$cargo_toml")/.cargo/config.toml"
            [ -f "$leaf_cfg" ] || continue
            checked=$((checked + 1))
            if ! grep -q -- "$probe" "$leaf_cfg"; then
                echo "[FAIL] $leaf_cfg" >&2
                echo "       deploys to '$deploy' but carries none of the board's static" >&2
                echo "       link args (expected e.g. '$probe' from $board_toml)." >&2
                echo "       Without them nothing pulls in the kernel archives and the" >&2
                echo "       link fails with thousands of undefined libc references." >&2
                fail=1
            fi
        done < <(git ls-files 'examples/*/Cargo.toml' 'examples/*/*/Cargo.toml' 'examples/*/*/*/Cargo.toml')
    done < <(grep -oE '^platform = "[a-z0-9_-]+"' "$board_toml" | cut -d'"' -f2 | sort -u)
done < <(git ls-files 'packages/boards/*/nros-board.toml')

if [ "$fail" != 0 ]; then
    echo "" >&2
    echo "  The board's \`cargo_config\` in \`nros-board.toml\` is the SSoT" >&2
    echo "  (RFC-0032 third leg). Copy its rustflags into the leaf, or teach" >&2
    echo "  \`nros sync\` to project them so they cannot drift again." >&2
    exit 1
fi

echo "board-cargo-config OK — $checked leaf config(s) carry their board's static link args."
