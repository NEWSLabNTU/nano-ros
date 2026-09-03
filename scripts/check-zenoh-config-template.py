#!/usr/bin/env python3
"""zenoh-pico's `config.h` must stay derivable from `config.h.in`.

`zenoh-pico/CMakeLists.txt` runs `configure_file(config.h.in -> config.h)` INTO
THE SOURCE TREE, so `config.h` is a generated artifact -- and it is also
committed, and it is the only config nano-ros actually reads (both lanes,
`zpico-sys` via cc-rs and the Zephyr module, compile zenoh-pico's sources
directly and never run that CMakeLists).

Being both at once is how it drifted:

  * the `#ifndef` guards from `49012370` lived only in `config.h`, so one cmake
    run deleted them and every `-D` on the Zephyr line went silently inert
    (`-DZ_FEATURE_MATCHING=0` yielding MATCHING=1);
  * `config.h` was missing three of OUR OWN features that the template had
    (`Z_FEATURE_LINK_ISOTP`, `..._VENDORED`, `Z_FEATURE_LINK_CUSTOM`);
  * the template could not express the `#ifdef ZENOH_NUTTX` split on
    `Z_CONFIG_SOCKET_TIMEOUT`, so regenerating collapsed 5000/100 into one
    value -- and 5000 ms on Zephyr starves tx and loses the session
    (issues 0129/0139).

Phase 415 made generation lossless. This gate keeps it that way, and it is the
same shape RFC-0054 uses for the committed bindgen output plus
`check-abi-bindings`: make generation reproducible, commit the output, gate the
drift.

It does NOT run cmake -- that would be slow, need a configure, and write into
the source tree. Instead it checks the property cmake's `configure_file` gives
us: the two files are line-for-line identical except that a `@TOKEN@` in the
template stands where a value sits in the header.
"""

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SUB = ROOT / "packages/rmw/zenoh/zpico-sys/zenoh-pico"
HEADER = SUB / "include/zenoh-pico/config.h"
TEMPLATE = SUB / "include/zenoh-pico/config.h.in"

TOKEN = re.compile(r"@([A-Za-z_0-9]+)@")


def line_matches(tpl: str, hdr: str) -> bool:
    """A template line matches a header line if they are equal, or equal once
    every `@TOKEN@` is allowed to stand for one whitespace-free value."""
    if tpl == hdr:
        return True
    if "@" not in tpl:
        return False
    pattern = "".join(
        r"\S+" if part.startswith("@") and part.endswith("@") and len(part) > 2
        else re.escape(part)
        for part in re.split(r"(@[A-Za-z_0-9]+@)", tpl)
    )
    return re.fullmatch(pattern, hdr) is not None


def check(template_text: str, header_text: str):
    """Returns a list of complaints; empty means the header is derivable."""
    tpl = template_text.split("\n")
    hdr = header_text.split("\n")
    if len(tpl) != len(hdr):
        return [
            f"line COUNT differs: config.h.in has {len(tpl)}, config.h has {len(hdr)}. "
            f"A regenerate would rewrite the header wholesale — the two have "
            f"structurally diverged, not merely disagreed on a value."
        ]
    bad = []
    for i, (t, h) in enumerate(zip(tpl, hdr), 1):
        if not line_matches(t, h):
            bad.append(f"line {i}:\n    config.h.in: {t}\n    config.h   : {h}")
    return bad


def self_test():
    """The gate spent no time able to pass without comparing anything — prove it."""
    cases = [
        # (template, header, expect_ok)
        ("#define X @X@", "#define X 1", True),
        ("#define Z_FRAG_MAX_SIZE @FRAG_MAX_SIZE@", "#define Z_FRAG_MAX_SIZE 4096", True),
        ("#ifndef X", "#ifndef X", True),
        # a value drifting is fine (cmake chooses it); a NAME drifting is not
        ("#define X @X@", "#define Y 1", False),
        # the exact regression this gate exists for: guards dropped by a regenerate
        ("#ifndef X", "#define X 1", False),
        # a token may not swallow whitespace, or `@X@` would match a whole line
        ("#define X @X@", "#define X 1 2", False),
    ]
    failures = 0
    for tpl, hdr, ok in cases:
        got = not check(tpl, hdr)
        if got != ok:
            print(f"self-test FAILED: {tpl!r} vs {hdr!r} -> {got}, want {ok}", file=sys.stderr)
            failures += 1
    # and a real one: a dropped line must be caught, not absorbed
    if not check("a\nb\nc", "a\nc"):
        print("self-test FAILED: a dropped line read as derivable", file=sys.stderr)
        failures += 1
    if failures:
        return False
    print("check-zenoh-config-template self-test: OK (6 shape(s) + line-count)")
    return True


def main() -> int:
    if not self_test():
        return 1
    if not HEADER.exists() or not TEMPLATE.exists():
        # A submodule that is not checked out is not this gate's business.
        print("check-zenoh-config-template: SKIP — zenoh-pico not checked out")
        return 0
    bad = check(TEMPLATE.read_text(), HEADER.read_text())
    if bad:
        print("check-zenoh-config-template: config.h is NOT derivable from config.h.in", file=sys.stderr)
        for b in bad[:20]:
            print("  " + b, file=sys.stderr)
        if len(bad) > 20:
            print(f"  ... and {len(bad) - 20} more", file=sys.stderr)
        print("", file=sys.stderr)
        print("  `configure_file(config.h.in -> config.h)` writes into the SOURCE", file=sys.stderr)
        print("  tree, so whichever file you edited, a `cmake` run in the submodule", file=sys.stderr)
        print("  will overwrite the header with what the TEMPLATE says. Mirror the", file=sys.stderr)
        print("  change into both, then re-run. Verify with:", file=sys.stderr)
        print("      cmake -S <submodule> -B /tmp/zpcfg && git -C <submodule> diff --exit-code \\", file=sys.stderr)
        print("          include/zenoh-pico/config.h", file=sys.stderr)
        return 1
    n = sum(1 for line in TEMPLATE.read_text().split("\n") if TOKEN.search(line))
    print(f"check-zenoh-config-template: OK (config.h derivable from config.h.in; {n} substituted line(s))")
    return 0


if __name__ == "__main__":
    sys.exit(main())
