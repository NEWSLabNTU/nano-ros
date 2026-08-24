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

For each positional initialiser: the sequence of `/*slot*/` comment names EQUALS
the header's first N fields. A PREFIX, not a subsequence.

The first version of this check allowed a subsequence, reasoning that an
initialiser may stop early and let C++ zero-fill the rest — which is true, and
irrelevant. Positional initialisation has no way to SKIP: entry i fills field i,
full stop. An initialiser that names fields 1..20 and then 25 is not "ordered
with a gap", it is four entries writing the wrong fields with comments that look
right, and a subsequence test passes it. Stopping early is fine; skipping is the
bug this file exists to catch, and the weaker rule could not see it.

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
    # Swapped pair: rejected. Early stop: accepted. Unknown name: rejected.
    # SKIP: rejected — the case the first version of this check let through.
    if is_prefix([order[1], order[0]] + order[2:5], order):
        bad.append("a swapped pair was accepted")
    if not is_prefix(order[:5], order):
        bad.append("a legitimate early stop was rejected")
    if is_prefix(["not_a_slot"], order):
        bad.append("an unknown name was accepted")
    if is_prefix(order[:3] + order[4:6], order):
        bad.append("a SKIPPED field was accepted — entry i must be field i")
    if bad:
        for b in bad:
            sys.stderr.write("check-vtable-positional-order --self-test: " + b + "\n")
        return 2
    print(
        f"check-vtable-positional-order --self-test: OK "
        f"({len(order)} header slots, 4 case(s))"
    )
    return 0


def is_prefix(names, order):
    return len(names) <= len(order) and names == order[: len(names)]


def first_disagreement(names, order):
    """(index, name, explanation) of the first entry that is not field[i]."""
    for i, n in enumerate(names):
        if i >= len(order):
            return i, n, "is past the end of the struct"
        if n != order[i]:
            if n not in order:
                why = "names no slot in the header at all"
            else:
                why = (
                    f"but field {i} is `{order[i]}` — the initialiser has "
                    "shifted, or it SKIPPED a field (positional initialisation "
                    "cannot skip: entry i fills field i)"
                )
            return i, n, why
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
            print(
                f"  {rel}: OK ({len(names)} annotated slot(s) == the header's "
                f"first {len(names)} fields)"
            )
            continue
        i, n, why = bad
        failed = True
        sys.stderr.write(
            f"[FAIL] {rel}: entry {i} is annotated `{n}`, {why}.\n"
            "       A positional initialiser's comments are the only thing "
            "saying which slot a line fills, and nothing else checks them. "
            "Adjacent slots that share a signature swap SILENTLY when the "
            "header gains a field in the middle.\n"
        )
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
