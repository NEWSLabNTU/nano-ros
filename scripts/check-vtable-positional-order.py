#!/usr/bin/env python3
"""Issue 0780 — a positional vtable initialiser must agree with the header.

`nros_rmw_vtable_t` has 70-odd slots, and two backends fill it with a POSITIONAL
initialiser annotated by comments:

    /*create_client*/             client_create,
    /*destroy_client*/            client_destroy,
    /*send_request*/              service_send_request_raw,

The comment is the only thing telling a reader which slot a line initialises,
and the comment is not checked by anything. Insert a slot in the middle of the
header and every line below it now initialises its NEIGHBOUR while still
carrying the old name.

Sometimes the compiler catches that: it did on 2026-08-25, when two slots were
added before `publisher_event_init` and the shifted entries had incompatible
function-pointer types. That is luck, not safety. Adjacent slots with the SAME
signature — `destroy_service` / `destroy_client`, `subscription_take_event` /
`publisher_take_event` differ only in a handle type, and several `get_*` graph
slots are identical — would swap SILENTLY and produce a backend that calls the
wrong function with a perfectly valid signature.

Issue 0773's write-up proposed exactly this check and deferred it; issue 0780's
slot insertion is what made it worth writing.

WHAT IT CHECKS

For each positional initialiser: the sequence of `/*slot*/` comment names is a
PREFIX-ORDERED SUBSEQUENCE of the header's field order. Subsequence rather than
equality because an initialiser may legitimately stop early (C++ zero-fills the
rest) — but it may never name them out of order, and it may never name a field
that does not exist.

Run: python3 scripts/check-vtable-positional-order.py
"""

import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
VTABLE = os.path.join(
    ROOT, "packages", "core", "nros-rmw-abi", "include", "nros", "rmw_vtable.h"
)

# `/*name*/ value,` — the annotation style both positional backends use.
ANNOTATED = re.compile(r"/\*\s*([a-z_][a-z_0-9]*)\s*\*/\s*[A-Za-z_&]")


def header_field_order():
    """Slot names, in declaration order, from the struct body."""
    src = open(VTABLE, encoding="utf-8").read()
    body = re.sub(r"/\*.*?\*/", " ", src, flags=re.S)
    body = re.sub(r"(?m)//.*$", " ", body)
    body = body[body.index("typedef struct nros_rmw_vtable_t {"):]
    body = body[: body.index("} nros_rmw_vtable_t;")]
    names = []
    for m in re.finditer(r"\(\s*\*\s*([a-z_0-9]+)\s*\)\s*\(", body):
        if m.group(1) not in names:
            names.append(m.group(1))
    # Nested callback PARAMETERS match the same shape; they are not slots.
    return [n for n in names if n not in ("cb", "chunk_cb", "size_cb")]


def initialisers():
    """[(path, [slot names in the order the file writes them])]"""
    listing = subprocess.run(
        ["git", "-C", ROOT, "ls-files", "packages/rmw/*/*/src/vtable.c",
         "packages/rmw/*/*/src/vtable.cpp"],
        capture_output=True, text=True, check=False,
    ).stdout.split()
    out = []
    for rel in listing:
        try:
            src = open(os.path.join(ROOT, rel), encoding="utf-8").read()
        except OSError:
            continue
        names = [m.group(1) for m in ANNOTATED.finditer(src)]
        if names:
            out.append((rel, names))
    return out


def self_test():
    bad = []
    order = header_field_order()
    if len(order) < 40:
        bad.append(f"header parse produced only {len(order)} slots — regex broke")
    if "create_session" not in order or "take" not in order:
        bad.append("header parse is missing known slots")
    # A shifted sequence must be REJECTED, an early stop ACCEPTED.
    shifted = [order[1], order[0]] + order[2:5]
    if is_ordered_subsequence(shifted, order):
        bad.append("a swapped pair was accepted")
    if not is_ordered_subsequence(order[:5], order):
        bad.append("a legitimate early stop was rejected")
    if is_ordered_subsequence(["not_a_slot"], order):
        bad.append("an unknown name was accepted")
    if bad:
        for b in bad:
            sys.stderr.write("check-vtable-positional-order --self-test: " + b + "\n")
        return 2
    print(
        f"check-vtable-positional-order --self-test: OK "
        f"({len(order)} header slots, 3 case(s))"
    )
    return 0


def is_ordered_subsequence(names, order):
    pos = 0
    for n in names:
        try:
            nxt = order.index(n, pos)
        except ValueError:
            return False
        pos = nxt + 1
    return True


def first_disagreement(names, order):
    pos = 0
    for i, n in enumerate(names):
        try:
            nxt = order.index(n, pos)
        except ValueError:
            where = "names no slot in the header" if n not in order else (
                "appears BEFORE a slot it should follow — the initialiser has "
                "shifted relative to the header"
            )
            return i, n, where
        pos = nxt + 1
    return None


def main():
    rc = self_test()
    if rc:
        return rc
    order = header_field_order()
    inits = initialisers()
    if not inits:
        sys.stderr.write(
            "[FAIL] no annotated positional initialiser found. Either both "
            "backends moved to designated initialisers (delete this check) or "
            "the scan broke.\n"
        )
        return 1

    failed = False
    for rel, names in inits:
        bad = first_disagreement(names, order)
        if bad is None:
            print(f"  {rel}: OK ({len(names)} annotated slot(s), in header order)")
            continue
        i, n, why = bad
        failed = True
        sys.stderr.write(
            f"[FAIL] {rel}: entry {i} is annotated `{n}`, which {why}.\n"
            "       A positional initialiser's comments are the only thing "
            "saying which slot a line fills, and nothing else checks them. "
            "Adjacent slots that share a signature swap SILENTLY when the "
            "header gains a field in the middle.\n"
        )
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
