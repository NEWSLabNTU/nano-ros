#!/usr/bin/env python3
"""phase-483: the Rust `nros` facade has ONE node type, `nros::Node`.

RFC-0089 "Settled: ... Rust takes rclrs's shape with ONE node type". Until
phase-483 a Rust user met four node-shaped names in `nros::`: the session
handle `NodeHandle`, the executor handle `NodeCtx`, the component TRAIT
`Node`, and the standalone `StandaloneNode` struct, plus the component's own
`DeclaredNode`. Issue 0784 was the cost: nothing told a user which one was
rclrs's. Phase-483 W2-W4 made `nros::Node` the one type (a component's
`register` gets it too), renamed the trait `nros::Component`, and moved the
rest out of the facade.

A second node type comes back the way the first ones arrived: someone adds a
`pub use` because it was handy. So this gate refuses, in the `nros` crate's
own sources (`packages/api/nros/src/`):

* a `pub` item named `Node` that is a TRAIT (the name is the node TYPE now);
* a `pub` definition, alias or re-export of a RETIRED node-shaped name.

And in `nros-node`, the type the facade re-exports, it refuses the name
`NodeCtx` coming back as a type or alias.

What it does not check: that `Executor::create_node` and
`NodeContext::create_node` return the same type. The compiler does, through
`tests/rclrs_talker_port.rs` (a ported program under `use nros as rclrs;`)
and `packages/rmw/metadata/tests/component_registration.rs` (a component's
`register`).

Comments are exempt, as history. `--self-test` runs the scanner over
in-memory samples, both directions.
"""
from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(REPO / "scripts" / "lib"))
import comments  # noqa: E402  (scripts/lib/comments.py, the shared stripper)

FACADE = "packages/api/nros/src/"
NODE_CRATE = "packages/core/nros-node/src/"

# The node-shaped names phase-483 took out of `nros::`. `NodeHandle` and
# `StandaloneNode` still exist in `nros-node` (the bench programs import them
# from there); what is refused is the facade exporting them again.
RETIRED = (
    "NodeHandle",
    "NodeCtx",
    "StandaloneNode",
    "StandaloneNodeError",
    "DeclaredNode",
    "NodeConfig",
    "PublisherHandle",
    "SubscriptionHandle",
)

PUB_DEF = re.compile(r"\bpub(?:\([^)]*\))?\s+(?:struct|enum|trait|type|union)\s+(\w+)")
PUB_TRAIT_NODE = re.compile(r"\bpub(?:\([^)]*\))?\s+(?:unsafe\s+)?trait\s+Node\b")
PUB_USE = re.compile(r"\bpub(?:\([^)]*\))?\s+use\s+([^;]+);", re.S)
ALIAS_OR_LEAF = re.compile(r"(\w+)\s*(?:as\s+(\w+))?\s*$")


def exported_names(use_body: str) -> list[str]:
    """The names a `pub use` makes visible, after `as` renames."""
    body = re.sub(r"\s+", " ", use_body)
    names = []
    # `a::b::{C, D as E, f::G}` and plain `a::b::C as D`.
    for part in re.split(r"[{},]", body):
        part = part.strip()
        if not part or part.endswith("::"):
            continue
        leaf = part.split("::")[-1].strip()
        m = ALIAS_OR_LEAF.search(leaf)
        if m:
            names.append(m.group(2) or m.group(1))
    return names


def scan_facade(src: str) -> list[tuple[int, str]]:
    code = comments.strip_comments(src, "rust")
    hits = []
    for m in PUB_TRAIT_NODE.finditer(code):
        hits.append((code.count("\n", 0, m.start()) + 1, "a `pub trait Node`"))
    for m in PUB_DEF.finditer(code):
        if m.group(1) in RETIRED:
            hits.append((code.count("\n", 0, m.start()) + 1, f"defines `{m.group(1)}`"))
    for m in PUB_USE.finditer(code):
        for name in exported_names(m.group(1)):
            if name in RETIRED:
                hits.append((code.count("\n", 0, m.start()) + 1, f"re-exports `{name}`"))
    return hits


def scan_node_crate(src: str) -> list[tuple[int, str]]:
    code = comments.strip_comments(src, "rust")
    hits = []
    for m in re.finditer(r"\b(?:struct|enum|type)\s+NodeCtx\b", code):
        hits.append((code.count("\n", 0, m.start()) + 1, "defines `NodeCtx`"))
    return hits


def tracked(root: str) -> list[str]:
    out = subprocess.run(
        ["git", "ls-files", "-z", "--", f"{root}*.rs", f"{root}**/*.rs"],
        cwd=REPO, check=True, capture_output=True,
    ).stdout.decode()
    return sorted({p for p in out.split("\0") if p})


def self_test() -> int:
    facade_bad = [
        ("trait Node", "pub trait Node {\n}\n", 1),
        ("unsafe trait Node", "pub unsafe trait Node {}\n", 1),
        ("re-export", "pub use nros_node::{Executor, NodeHandle};\n", 1),
        ("renamed re-export", "pub use nros_node::StandaloneNode as Standalone;\npub use x::Foo as NodeCtx;\n", 1),
        ("alias", "pub type DeclaredNode<'a> = Node<'a, 'static>;\n", 1),
        ("crate-visible def", "pub(crate) struct PublisherHandle;\n", 1),
    ]
    facade_good = [
        ("the node type", "pub use nros_node::Node;\npub struct Executor;\n"),
        ("the component trait", "pub trait Component {}\npub trait DeclarativeNode {}\n"),
        ("comment", "// pub use nros_node::NodeHandle;\n/* pub trait Node {} */\n"),
        ("private import", "use nros_node::NodeHandle;\n"),
        ("longer name", "pub struct NodeHandleError;\npub trait NodeLike {}\n"),
    ]
    failed = 0
    for name, src, want in facade_bad:
        got = len(scan_facade(src))
        if got != want:
            print(f"self-test FAIL (facade {name}): {got} finding(s), want {want}", file=sys.stderr)
            failed += 1
    for name, src in facade_good:
        got = scan_facade(src)
        if got:
            print(f"self-test FAIL (facade {name}): flagged {got}", file=sys.stderr)
            failed += 1
    if len(scan_node_crate("pub type NodeCtx<'e, 's> = Node<'e, 's>;\n")) != 1:
        print("self-test FAIL (nros-node alias)", file=sys.stderr)
        failed += 1
    if scan_node_crate("/// It was `NodeCtx` until phase-483.\npub struct Node;\n"):
        print("self-test FAIL (nros-node doc comment)", file=sys.stderr)
        failed += 1
    return failed


def main() -> int:
    if "--self-test" in sys.argv[1:]:
        failed = self_test()
        print("self-test ok" if not failed else f"self-test: {failed} case(s) failed")
        return 1 if failed else 0
    if self_test():
        print("check-rust-one-node-type: the scanner's self-test failed", file=sys.stderr)
        return 1
    findings = []
    facade = tracked(FACADE)
    for path in facade:
        text = (REPO / path).read_text(encoding="utf-8", errors="replace")
        findings += [f"{path}:{n}: {what}" for n, what in scan_facade(text)]
    for path in tracked(NODE_CRATE):
        text = (REPO / path).read_text(encoding="utf-8", errors="replace")
        findings += [f"{path}:{n}: {what}" for n, what in scan_node_crate(text)]
    if not facade:
        print(f"check-rust-one-node-type: no tracked .rs under {FACADE} -- refusing to pass", file=sys.stderr)
        return 1
    if findings:
        print("check-rust-one-node-type: a second node type is back in `nros::` (phase-483).", file=sys.stderr)
        print("`nros::Node` is the one node type and `nros::Component` the component trait;", file=sys.stderr)
        print("a component's `register` gets the same `Node` a program does (RFC-0089):", file=sys.stderr)
        for f in findings:
            print(f"  {f}", file=sys.stderr)
        return 1
    print(f"check-rust-one-node-type: OK ({len(facade)} facade file(s); one node type)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
