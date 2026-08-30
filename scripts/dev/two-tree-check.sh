#!/usr/bin/env bash
# Build a nano-ros workspace that lives in NEITHER the nano-ros checkout NOR
# the west workspace — RFC-0085 D1's actual shape, end to end.
#
# WHY A SCRIPT AND NOT A TEST
#
# Compiling at test time is banned (AGENTS.md Testing): a test consumes a
# prebuilt fixture. This compiles, needs a Zephyr, and takes minutes. The
# plan-level half — which is where every location decision is made — IS tested,
# in `packages/cli/nros-cli-core/tests/build_verb_pipeline.rs`:
#
#   the_zephyr_workspace_can_live_in_a_third_tree
#   a_plan_is_answerable_with_no_zephyr_on_the_machine
#   a_missing_framework_names_how_to_supply_it
#
# What only a real build can show is the part those cannot reach: `nros sync`
# rewriting absolute paths into a MOVED tree, and the compile actually
# succeeding from there.
#
# WHY IT SCAFFOLDS RATHER THAN COPYING AN EXAMPLE
#
# The first version copied `examples/workspaces/rust` to $TMPDIR. That can never
# work, and issue 0905 is the record of finding out why: an IN-TREE example is
# not portable by construction. Its leaf `Cargo.toml` path-deps the framework
# relatively —
#
#     nros = { path = "../../../../../packages/api/nros" }
#
# which from `/tmp/<x>/ws/src/<leaf>` resolves to `/packages/api/nros`, and its
# generated `.cargo/config.toml` climbs six levels into this checkout. Both are
# correct where they were written and meaningless one directory elsewhere.
#
# A real user's workspace has neither: it names `nros` by version and lets
# `nros sync` write the patch. So the check builds that shape — with the same
# verbs a user runs — instead of moving a tree that was never meant to move.
#
# WHAT THIS PROVES, AND WHAT IT DOES NOT
#
# Proves: the three trees can be three trees. The workspace is created outside
# the checkout and never lived in it, the Zephyr is named explicitly, the
# framework is named explicitly, and the image builds.
#
# Does NOT prove: resolution from a PUBLISHED nano-ros. `nros sync` patches the
# registry names to this checkout, which is what a user does today (RFC-0040 —
# the crates are not published). When they are, this is the check that should
# gain a no-`NROS_REPO_DIR` variant.
#
# Usage:
#   scripts/dev/two-tree-check.sh [--keep]
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
KEEP=0

while [ $# -gt 0 ]; do
    case "$1" in
        --keep)      KEEP=1; shift ;;
        -h|--help)   sed -n '2,32p' "${BASH_SOURCE[0]}"; exit 0 ;;
        *) echo "two-tree-check: unknown argument $1" >&2; exit 2 ;;
    esac
done

# The Zephyr, resolved the same way `nros build` resolves it, so this script
# cannot disagree with the thing it is checking.
ZEPHYR_WS="${NROS_ZEPHYR_WORKSPACE:-$REPO/zephyr-workspace}"
if [ ! -d "$ZEPHYR_WS/zephyr" ]; then
    echo "two-tree-check: no Zephyr at $ZEPHYR_WS/zephyr" >&2
    echo "  Set NROS_ZEPHYR_WORKSPACE, or run: just zephyr setup" >&2
    exit 1
fi

command -v nros >/dev/null 2>&1 || {
    echo "two-tree-check: no \`nros\` on PATH — source ./activate.sh" >&2
    exit 1
}

# OUTSIDE the checkout. `mktemp -d` lands in $TMPDIR, which is what makes this a
# different tree rather than a subdirectory pretending to be one.
OUT="$(mktemp -d "${TMPDIR:-/tmp}/nros-two-tree-XXXXXX")"
cleanup() { [ "$KEEP" -eq 1 ] || rm -rf "$OUT"; }
trap cleanup EXIT

WS="$OUT/ws"
mkdir -p "$WS/src"

echo "two-tree-check: three trees"
echo "  framework : $REPO"
echo "  zephyr    : $ZEPHYR_WS"
echo "  workspace : $WS   (scaffolded here; never inside the checkout)"
echo

cd "$WS"

echo "==> nros new system demo_bringup"
nros new system demo_bringup --component-name talker_pkg --into src >/dev/null

# The node package, by hand and deliberately.
#
# `nros new <name> --platform native` makes a standalone RUNNABLE project — it
# pins a board and a platform port, which is right for what it is and wrong for
# a package an entry links onto a different platform. A node package is
# board-agnostic: one `nros` dep, `alloc` + `rmw-cffi`, nothing else. That is
# what `examples/workspaces/rust/src/talker_pkg` carries, and there is no verb
# that emits it — so this writes the minimum rather than pretending otherwise.
mkdir -p src/talker_pkg/src
cat > src/talker_pkg/package.xml <<'XML'
<?xml version="1.0"?>
<package format="3">
  <name>talker_pkg</name>
  <version>0.1.0</version>
  <description>Node package for the two-tree check.</description>
  <maintainer email="dev@example.com">dev</maintainer>
  <license>Apache-2.0</license>
</package>
XML
cat > src/talker_pkg/Cargo.toml <<'TOML'
[package]
name = "talker_pkg"
version = "0.1.0"
edition = "2024"
publish = false

[lib]
crate-type = ["rlib"]

# Board- and RMW-agnostic, like every node package: `alloc` is the universal
# baseline and `rmw-cffi` is the vtable seam. The entry chooses the platform.
[dependencies]
nros = { version = "*", default-features = false, features = ["alloc", "rmw-cffi"] }
TOML
cat > src/talker_pkg/src/lib.rs <<'RS'
#![no_std]

//! Minimal node package for the two-tree check.
//!
//! It declares a node that creates nothing. What is under test here is WHERE
//! the three trees are, not what the nodes do — so this is the smallest thing
//! that still exercises the real seam: `nros::node!` emits the free
//! `register()` the entry's `nros::main!` calls, and a signature mismatch here
//! is a compile error in generated code, which is exactly the failure a node
//! package must not be able to cause.

use nros::{Callback, CallbackCtx, ExecutableNode, Node, NodeContext, NodeResult};

pub struct Talker;

impl Node for Talker {
    const NAME: &'static str = "talker";
    const ENTITY_BOUNDS: nros::EntityBounds = nros::EntityBounds::exact(0, 0, 0, 0, 0);

    fn register(_ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        Ok(())
    }
}

impl ExecutableNode for Talker {
    type State = ();

    fn init() -> Self::State {}

    fn on_callback(_state: &mut Self::State, _cb: Callback<'_>, _ctx: &mut CallbackCtx<'_>) {}
}

nros::node!(Talker);
RS

echo "==> nros new entry zephyr_entry --platform zephyr"
nros new entry zephyr_entry --platform zephyr >/dev/null

echo
echo "==> nros sync   (writes this tree's patch table)"
NROS_REPO_DIR="$REPO" nros sync

echo
echo "==> nros build demo_bringup:zephyr_entry"
NROS_REPO_DIR="$REPO" nros build demo_bringup:zephyr_entry \
    --zephyr-workspace "$ZEPHYR_WS"

ELF="$WS/build/zephyr/zephyr.elf"
if [ ! -f "$ELF" ]; then
    echo "two-tree-check: FAIL — no artifact at $ELF" >&2
    exit 1
fi

echo
echo "two-tree-check: OK"
echo "  artifact: $ELF"
[ "$KEEP" -eq 1 ] && echo "  tree kept at $OUT (--keep)"
exit 0
