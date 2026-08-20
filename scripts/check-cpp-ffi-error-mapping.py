#!/usr/bin/env python3
"""Issue 0586 — the C++ FFI must not discard a backend error.

`NROS_CPP_RET_TRANSPORT_ERROR` (-100) is documented in its own source as the
catch-all for UNMAPPED variants. Fifteen call sites reached it by writing

    Err(_) => NROS_CPP_RET_TRANSPORT_ERROR,

throwing away a `NodeError` or a `TransportError` that maps perfectly well. A C
or C++ caller was then told "transport error" for a too-long name, a too-small
buffer, an unsupported operation or an incompatible QoS. On an embedded guest the
return code is frequently the only thing that reaches the console — issue 0589
makes printing from Rust fatal on Zephyr native_sim — so an unmapped variant is
not cosmetic, it is the whole diagnosis. Issue 0557 spent a long session on
exactly that: `rc=-100` for a failure that never touched the transport.

This gate enforces the shape, not the outcome: an error path must NAME its error
and hand it to a mapper. The mappers themselves are exhaustive (no `_` arm), so
rustc already refuses a new variant until someone maps it; this covers the other
direction, a new CALL SITE that discards.

Run: python3 scripts/check-cpp-ffi-error-mapping.py
"""

from __future__ import annotations

import pathlib
import re
import sys
import sys as _sys
from pathlib import Path as _Path
_sys.path.insert(0, str(_Path(__file__).resolve().parent / "lib"))
from tracked import tracked  # issue 0721: index lookup, not a walk

REPO = pathlib.Path(__file__).resolve().parent.parent
SRC = REPO / "packages" / "api" / "nros-cpp" / "src"

# `Err(_) => …TRANSPORT_ERROR` — the discard this issue is about, and ONLY that.
#
# Scope matters here. `Err(_) => NROS_CPP_RET_FULL` in a timer or arena path is
# not this bug: "full" is the whole truth, the error carries nothing more, and
# demanding a mapper there would be churn. What 0586 forbids is discarding an
# error in favour of the code documented as the catch-all for UNMAPPED variants —
# that is the one return value which asserts "we did not look".
#
# (The first version of this gate matched every `Err(_) => <any code>` and
# reported 43 sites, most of them correct. A gate that flags correct code teaches
# people to ignore it, which is worse than not having one.)
DISCARD = re.compile(r"Err\(\s*_\s*\)\s*=>\s*(?:crate::)?NROS_CPP_RET_TRANSPORT_ERROR")

# A mapper must exist and must not carry a catch-all arm, or the exhaustiveness
# that makes this whole scheme work is gone.
MAPPERS = ("node_error_to_cpp_ret", "transport_error_to_cpp_ret")


def main() -> int:
    if not SRC.is_dir():
        print(f"check-cpp-ffi-error-mapping: NOT CHECKED — {SRC} is absent")
        return 0

    failures: list[str] = []
    scanned = 0

    for path in tracked(SRC, suffix=".rs"):
        text = path.read_text()
        scanned += 1
        for lineno, line in enumerate(text.splitlines(), 1):
            if line.lstrip().startswith("//") or line.lstrip().startswith("///"):
                continue  # a doc comment quoting the bad shape is not the bad shape
            if DISCARD.search(line):
                rel = path.relative_to(REPO)
                failures.append(
                    f"  {rel}:{lineno}\n"
                    f"    {line.strip()}\n"
                    f"    -> bind the error and map it: "
                    f"`Err(e) => crate::node_error_to_cpp_ret(e)` or "
                    f"`crate::transport_error_to_cpp_ret(e)`"
                )

    # The mappers must stay exhaustive.
    lib = SRC / "lib.rs"
    if lib.is_file():
        lib_text = lib.read_text()
        for mapper in MAPPERS:
            m = re.search(rf"fn {mapper}\([^)]*\)[^{{]*\{{(.*?)\n\}}", lib_text, re.S)
            if m is None:
                failures.append(f"  {mapper} is missing from lib.rs — issue 0586's mappers")
                continue
            body = m.group(1)
            # An UNCONDITIONAL `_ =>` arm is the thing this forbids. A cfg-gated
            # one is not the same claim and is sometimes forced:
            # `TransportError::BackendDynamic` exists per NROS-RMW's `alloc`,
            # which is not this crate's `alloc`, so a config can see the variant
            # while the arm naming it is compiled out (E0004 on the cortex-m
            # Zephyr leaves, 2026-08-16). A `_` under `cfg(not(feature =
            # "alloc"))` covers exactly that gap and leaves exhaustiveness
            # intact wherever the two gates agree — which is where the guarantee
            # was ever meaningful.
            #
            # So: reject a wildcard that is NOT immediately preceded by a `#[cfg`
            # attribute. That keeps the property (no silent catch-all) without
            # forbidding the one shape the feature graph makes necessary.
            for wm in re.finditer(r"^([ \t]*)_\s*=>", body, re.M):
                preceding = body[: wm.start()].rstrip().splitlines()
                guarded = any(
                    line.lstrip().startswith(("#[cfg", "#[allow"))
                    for line in preceding[-2:]
                )
                if guarded:
                    continue
                failures.append(
                    f"  {mapper} has an UNCONDITIONAL `_` arm.\n"
                    f"    -> name every variant. rustc rejecting an unreachable `_` is the\n"
                    f"       point: a NEW variant then fails to compile until someone decides\n"
                    f"       what the C++ caller should see, instead of joining the -100 pile.\n"
                    f"       A `#[cfg(...)]`-gated `_` is allowed — see the note in this script."
                )

    if failures:
        print("check-cpp-ffi-error-mapping: FAILED — a backend error is being discarded\n")
        print("\n".join(failures))
        print(
            "\n  `-100 TRANSPORT_ERROR` is the catch-all for UNMAPPED variants. Returning it\n"
            "  for a mapped one tells a C++ caller the transport failed when it did not.\n"
            "  See docs/issues/archived/0586-*.md and 0557."
        )
        return 1

    print(f"check-cpp-ffi-error-mapping: OK ({scanned} file(s), mappers exhaustive)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
