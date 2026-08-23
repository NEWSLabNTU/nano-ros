#!/usr/bin/env python3
"""Rewrite `EMPTY_VTABLE` in nros-rmw-cffi from the generated bindings.

Phase 376 W4 — the one place a new vtable slot has to be repeated on the Rust
side. Regenerating it beats hand-editing for the same reason the bindings
themselves are generated: the field list has exactly one source of truth, and it
is the C header.

Run after `scripts/gen-abi-bindings.sh` when the vtable gained or lost a slot.
"""
import re
import sys

GEN = "packages/rmw/cffi/src/generated.rs"
LIB = "packages/rmw/cffi/src/lib.rs"

gen = open(GEN, encoding="utf-8").read()
m = re.search(r"pub struct nros_rmw_vtable_t \{(.*?)\n\}", gen, re.S)
if not m:
    sys.exit("gen-empty-vtable: no nros_rmw_vtable_t in the bindings")
fields = re.findall(r"pub (\w+):", m.group(1))

lib = open(LIB, encoding="utf-8").read()
start = lib.index("pub const EMPTY_VTABLE: NrosRmwVtable = NrosRmwVtable {")
end = lib.index("\n};", start) + len("\n};")
body = "\n".join("    %s: None," % f for f in fields)
new = "pub const EMPTY_VTABLE: NrosRmwVtable = NrosRmwVtable {\n%s\n};" % body
if lib[start:end] == new:
    print(f"gen-empty-vtable: unchanged ({len(fields)} slots)")
else:
    open(LIB, "w", encoding="utf-8").write(lib[:start] + new + lib[end:])
    print(f"gen-empty-vtable: rewrote EMPTY_VTABLE ({len(fields)} slots)")
