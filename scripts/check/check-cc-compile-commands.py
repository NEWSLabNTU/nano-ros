#!/usr/bin/env python3
"""issue 2658 -- every cc-rs compile of image C goes through
`nros_cc_flags::compile_commands::compile`, which writes the compile's
`compile_commands.json`.

# The defect this gate exists for

The stack-floor walk types each indirect call in an image's C from clang's IR
of the same translation unit, compiled from the build's compile commands
(issue 2516). cmake and west write those commands. Cargo does not, so C a build
script compiled with a bare `cc::Build::compile` had none, and every indirect
call in it was bound to every function C can hold. Measured on realtime-c's
FreeRTOS image: zenoh-pico alone, compiled by `zpico-sys`, was 18-21 % of the
walk, and C TUs typed went from 56 to 146 once it had commands.

The class is every build script that compiles C into an image: board
kernels, network stacks and platform ports, zenoh-pico, the XRCE and Cyclone
glue. One site left on a bare `.compile(` silently keeps its C untyped, and
nothing else would notice: the walk only gets looser, never fails.

# The rules

  1. REACH. A tracked Rust source may not call cc-rs's `.compile(` (comments
     stripped), except the wrapper's own definition. `.try_compile(` is a
     probe whose object never reaches an image and is allowed. A file that
     legitimately cannot use the wrapper is in `EXEMPT` with the reason.
  2. MATCH. At least one wrapper call must be found: a gate that matches
     nothing reports OK forever (issue 1220's shape).
  3. NO STALE ENTRIES. An `EXEMPT` path must exist and still call `.compile(`.

The classifier self-tests on every run, and a planted bare compile must be
refused.

Run: python3 scripts/check/check-cc-compile-commands.py
"""

import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

BARE = re.compile(r"\.compile\s*\(")
WRAPPED = re.compile(r"\bcompile_commands::compile\s*\(")
DEFINITION = "packages/tooling/nros-cc-flags/src/compile_commands.rs"

# path -> reason. A bare cc-rs compile, on purpose. EMPTY: every site in the
# tree went through the wrapper when this gate landed.
EXEMPT: dict = {}


def strip_comments(text: str) -> str:
    """Drop `//` line comments (and doc comments) so prose earns nothing."""
    return "\n".join(line.split("//", 1)[0] for line in text.splitlines())


def bare_sites(text: str) -> list[int]:
    """1-based line numbers of each bare `.compile(` call in `text`."""
    out = []
    for i, line in enumerate(strip_comments(text).splitlines(), 1):
        if BARE.search(line):
            out.append(i)
    return out


def classify(text: str, rel: str, exempt=None) -> list[str]:
    exempt = EXEMPT if exempt is None else exempt
    if rel == DEFINITION or rel in exempt:
        return []
    return [f"{rel}:{n}" for n in bare_sites(text)]


def self_test() -> list[str]:
    bad = []
    planted = 'fn main() {\n    let mut b = cc::Build::new();\n    b.file("x.c");\n    b.compile("x");\n}\n'
    if classify(planted, "packages/x/build.rs") != ["packages/x/build.rs:4"]:
        bad.append("a planted bare `.compile(` is refused, by file:line")
    chain = 'fn f(b: &mut cc::Build) {\n    b.file("x.c")\n        .compile("x");\n}\n'
    if classify(chain, "y.rs") != ["y.rs:3"]:
        bad.append("a bare `.compile(` ending a builder chain is refused")
    ok = ('fn main() {\n    let mut b = cc::Build::new();\n'
          '    nros_cc_flags::compile_commands::compile(&mut b, "x");\n'
          '    let _ = b.try_compile("probe");\n'
          '    // b.compile("in a comment");\n    /// b.compile("in a doc");\n}\n')
    if classify(ok, "z.rs"):
        bad.append("the wrapper, a try_compile probe and comments pass")
    if classify(planted, DEFINITION):
        bad.append("the wrapper's own definition may call `.compile(`")
    if classify(planted, "e.rs", exempt={"e.rs": "reason"}):
        bad.append("an exempt file passes")
    return bad


def tracked_rust() -> list[str]:
    out = subprocess.run(["git", "-C", ROOT, "ls-files", "-z", "--", "*.rs"],
                         check=True, capture_output=True).stdout
    return [p for p in out.decode().split("\0") if p]


def main() -> int:
    say = lambda m: print(f"check-cc-compile-commands: {m}")
    fails = self_test()
    for f in fails:
        say(f"SELFTEST FAILED: {f}")
    hits, wrapped, files = [], 0, 0
    for rel in tracked_rust():
        try:
            text = open(os.path.join(ROOT, rel), encoding="utf-8", errors="replace").read()
        except OSError:
            continue
        hits += classify(text, rel)
        n = len(WRAPPED.findall(strip_comments(text)))
        wrapped += n
        files += bool(n)
    stale = []
    for rel, reason in EXEMPT.items():
        p = os.path.join(ROOT, rel)
        if not reason or not os.path.isfile(p) or not bare_sites(open(p).read()):
            stale.append(rel)
    if hits:
        say("a cc-rs compile of C that bypasses nros_cc_flags::compile_commands::compile, "
            "so the stack-floor walk gets no compile commands for it and binds every "
            "indirect call in that C to everything C can hold (issue 2658). Write "
            "`nros_cc_flags::compile_commands::compile(&mut build, \"<lib>\")`:")
        for h in hits:
            print(f"    {h}")
    if stale:
        say("EXEMPT entries that name no file, no reason, or no bare compile any more:")
        for s in stale:
            print(f"    {s}")
    if not wrapped:
        say("found no wrapper call at all: the pattern no longer matches the tree")
    if fails or hits or stale or not wrapped:
        return 1
    say(f"OK ({wrapped} compile(s) in {files} file(s) go through the wrapper, "
        f"{len(EXEMPT)} exempt; selftest: a planted bare compile is refused)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
