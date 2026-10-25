#!/usr/bin/env python3
"""zenoh-pico's committed `config.h` must stay derivable from `config.h.in`.

Since zenoh-pico 1.10, `CMakeLists.txt` runs `configure_file(config.h.in ->
config.h)` into the BINARY dir and upstream's tree carries no header. Every
nano-ros build that reads one compiles zenoh-pico's sources without running
that CMake -- the Zephyr module above all, which overrides knobs with `-D` --
so our fork commits the header that a default configure writes (fork commit
"config: carry a generated config.h for builds that do not run this CMake").
`zpico-sys` builds with ZENOH_GENERIC and reads `zenoh_generic_config.h`
instead.

A committed copy of a generated file drifts, and on the 1.8.0 line this one
did, in both directions:

  * the `#ifndef` guards from `49012370` lived only in `config.h`, so a
    regenerate deleted them and every `-D` on the Zephyr line went silently
    inert (`-DZ_FEATURE_MATCHING=0` yielding MATCHING=1);
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

It does NOT run cmake -- that would be slow and need a configure. Instead it
checks the property cmake's `configure_file` gives us: the two files are
line-for-line identical except that a `@TOKEN@` in the template stands where a
value sits in the header, and a `#cmakedefine X` line became either
`#define X ...` or `/* #undef X */` (1.10's template has two).
"""

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "scripts" / "lib"))
from source_locate import located_or_dest  # noqa: E402 — RFC-0103 D5: the tree is located

SUB = located_or_dest("zenoh-pico")
HEADER = SUB / "include/zenoh-pico/config.h"
TEMPLATE = SUB / "include/zenoh-pico/config.h.in"

GENERATOR = ROOT / "packages/rmw/zenoh/nros-zpico-build/src/lib.rs"

TOKEN = re.compile(r"@([A-Za-z_0-9]+)@")
DEFINE = re.compile(r"#define ([A-Za-z_][A-Za-z_0-9]*)")


CMAKEDEFINE = re.compile(r"#cmakedefine ([A-Za-z_0-9]+)(.*)$")


def line_matches(tpl: str, hdr: str) -> bool:
    """A template line matches a header line if they are equal, or equal once
    every `@TOKEN@` is allowed to stand for one whitespace-free value. A
    `#cmakedefine X rest` line matches `/* #undef X */` (unset) or
    `#define X rest` (set), the two shapes `configure_file` writes."""
    if tpl == hdr:
        return True
    m = CMAKEDEFINE.match(tpl)
    if m:
        name, rest = m.group(1), m.group(2)
        if hdr == f"/* #undef {name} */":
            return True
        return line_matches(f"#define {name}{rest}", hdr)
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


def generic_branch_tokens(template_text: str) -> set[str]:
    """Macros the template defines in its non-generic branch: the ones a
    ZENOH_GENERIC build skips, and so must get from `zenoh_generic_config.h`."""
    lines = template_text.split("\n")
    try:
        start = lines.index("#ifdef ZENOH_GENERIC")
        end = next(i for i, line in enumerate(lines) if line.startswith("#endif /* ZENOH_GENERIC"))
    except (ValueError, StopIteration):
        return set()
    out = set()
    for line in lines[start + 1 : end]:
        m = DEFINE.match(line) or CMAKEDEFINE.match(line)
        if m:
            out.add(m.group(1))
    return out


def generator_macros(rust_text: str) -> set[str]:
    """Every macro `config_header()` writes, conditionally or not."""
    start = rust_text.index("pub fn config_header(")
    body = rust_text[start : rust_text.index("\n}\n", start)]
    return set(DEFINE.findall(body))


def generic_gaps(template_text: str, rust_text: str) -> list[str]:
    """Template knobs `config_header()` never states.

    A ZENOH_GENERIC build takes NOTHING from `config.h.in`'s substituted
    branch, so a knob there that our generator does not write has no value at
    all -- upstream's CMake default never reaches it. At 1.10.1 that was seven
    (the executor's two runtime knobs and the two link timeouts among them),
    and the ones that size an array or a timeout are a compile error, while a
    feature flag silently reads as 0 under `#if`. Issue 0910."""
    return sorted(generic_branch_tokens(template_text) - generator_macros(rust_text))


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
        # `#cmakedefine`, both outcomes, and a renamed one
        ("#cmakedefine X", "/* #undef X */", True),
        ('#cmakedefine H "@H@"', '#define H "p.h"', True),
        ("#cmakedefine X", "/* #undef Y */", False),
    ]
    failures = 0
    for tpl, hdr, ok in cases:
        got = not check(tpl, hdr)
        if got != ok:
            print(f"self-test FAILED: {tpl!r} vs {hdr!r} -> {got}, want {ok}", file=sys.stderr)
            failures += 1
    # and a real one: a dropped line must be caught, not absorbed
    tpl = "#ifdef ZENOH_GENERIC\n#include <g.h>\n#else\n#define Z_A @A@\n#cmakedefine Z_B\n#endif /* ZENOH_GENERIC */\n"
    rs = 'pub fn config_header(\n    writeln!(h, "#define Z_A 1").unwrap();\n}\n'
    if generic_gaps(tpl, rs) != ["Z_B"]:
        print(f"self-test FAILED: generic gap {generic_gaps(tpl, rs)!r}, want ['Z_B']", file=sys.stderr)
        failures += 1
    if not check("a\nb\nc", "a\nc"):
        print("self-test FAILED: a dropped line read as derivable", file=sys.stderr)
        failures += 1
    if failures:
        return False
    print(f"check-zenoh-config-template self-test: OK ({len(cases)} shape(s) + line-count)")
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
        print("  The committed header is what `configure_file` writes from the", file=sys.stderr)
        print("  TEMPLATE, so edit the template and regenerate the header from it:", file=sys.stderr)
        print("      cmake -S <submodule> -B /tmp/zpcfg && cp /tmp/zpcfg/include/zenoh-pico/config.h \\", file=sys.stderr)
        print("          <submodule>/include/zenoh-pico/", file=sys.stderr)
        return 1
    gaps = generic_gaps(TEMPLATE.read_text(), GENERATOR.read_text())
    if gaps:
        print("check-zenoh-config-template: `config_header()` states nothing for", file=sys.stderr)
        for g in gaps:
            print(f"  {g}", file=sys.stderr)
        print("", file=sys.stderr)
        print("  config.h.in defines these only for a build that is NOT ZENOH_GENERIC,", file=sys.stderr)
        print("  so every zpico-sys build has no value for them. Emit each from", file=sys.stderr)
        print(f"  {GENERATOR.relative_to(ROOT)} (and mirror it in", file=sys.stderr)
        print("  scripts/qemu/build-zenoh-pico.sh), stating why its value is ours.", file=sys.stderr)
        return 1
    n = sum(1 for line in TEMPLATE.read_text().split("\n") if TOKEN.search(line))
    print(f"check-zenoh-config-template: OK (config.h derivable from config.h.in; {n} substituted line(s))")
    return 0


if __name__ == "__main__":
    sys.exit(main())
