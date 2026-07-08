#!/usr/bin/env bash
#
# Issue 0160 — drift gate for hand-mirrored FFI structs.
#
# `component.h` re-declares a few `nros_cpp_ffi.h` structs behind
# `#ifndef NROS_CPP_FFI_H` so a plain-C TU can use the component API without
# the (cbindgen-generated) C++ FFI header. Those mirrors are hand-written and
# have drifted on every append so far (phase-273 `callback_group`, phase-282
# `tx_express` — the #131 "stale mirror" ABI class: a mirror-only TU passes a
# SHORTER struct by value than the FFI consumer reads, so the tail field is
# stack garbage).
#
# This gate extracts each mirrored struct body from BOTH headers, normalizes
# comments/whitespace and the C-side enum-name prefix, and fails on any field
# difference. Hooked from `just check-fast` so an append that misses a mirror
# fails the push lane, not a NuttX rebuild three days later.

set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."

MIRROR="packages/core/nros-c/include/nros/component.h"
CANONICAL="packages/core/nros-cpp/include/nros/nros_cpp_ffi.h"

python3 - "$MIRROR" "$CANONICAL" <<'PY'
import re
import sys

mirror_path, canonical_path = sys.argv[1], sys.argv[2]

# (struct tag, {mirror-prefix: canonical-prefix} applied to the mirror's text)
CHECKS = [
    ("nros_cpp_qos_t", {"nros_c_qos_": "nros_cpp_qos_"}),
    ("nros_cpp_integrity_status_t", {}),
]


def struct_fields(path, tag, prefix_map):
    src = open(path).read()
    m = re.search(
        r"typedef struct %s \{(.*?)\n\} %s;" % (re.escape(tag), re.escape(tag)),
        src,
        re.S,
    )
    if not m:
        sys.exit(f"check-ffi-struct-mirrors: struct '{tag}' not found in {path}")
    body = m.group(1)
    body = re.sub(r"/\*.*?\*/", "", body, flags=re.S)  # block comments
    body = re.sub(r"//[^\n]*", "", body)  # line comments
    for old, new in prefix_map.items():
        body = body.replace(old, new)
    fields = []
    for decl in body.split(";"):
        decl = " ".join(decl.split())
        if decl:
            fields.append(decl)
    return fields


failed = False
for tag, prefix_map in CHECKS:
    mirror = struct_fields(mirror_path, tag, prefix_map)
    canonical = struct_fields(canonical_path, tag, {})
    if mirror != canonical:
        failed = True
        print(f"FFI struct mirror DRIFTED: {tag}", file=sys.stderr)
        print(f"  canonical ({canonical_path}):", file=sys.stderr)
        for f in canonical:
            marker = " " if f in mirror else "+"
            print(f"   {marker} {f}", file=sys.stderr)
        print(f"  mirror ({mirror_path}, enum prefixes normalized):", file=sys.stderr)
        for f in mirror:
            marker = " " if f in canonical else "!"
            print(f"   {marker} {f}", file=sys.stderr)
        print(
            "  A field appended to the canonical struct MUST be appended to the\n"
            "  hand mirror too (and initialized in any *_default() helper) — a\n"
            "  mirror-only TU otherwise passes a shorter struct by value (ABI\n"
            "  mismatch, issue 0160 / the #131 stale-mirror class).",
            file=sys.stderr,
        )

sys.exit(1 if failed else 0)
PY

echo "FFI struct mirrors in sync."
