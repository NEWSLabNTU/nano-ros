#!/usr/bin/env python3
"""Issue 0555 — a retired platform symbol whose replacement is a `static inline`.

RFC-0073 / phase-352 replaced `nros_platform_clock_{ms,us}` with
`nros_platform_clock_ns()` plus wrappers in
`packages/platform/nros-platform-api/include/nros/platform.h`:

    static inline uint64_t nros_platform_clock_us(void) { return nros_platform_clock_ns() / 1000u; }
    static inline uint64_t nros_platform_clock_ms(void) { return nros_platform_clock_ns() / 1000000u; }

No port defines those symbols any more — THE WRAPPERS ARE THE DEFINITION, and a
caller gets them by including the header. That makes the header a load-bearing
dependency of anyone who says the name, and nothing checked it.

THE RENAME THEN BROKE THREE CONSUMERS IN A ROW, each visible only once the
previous cleared, because the zephyr lane aborts at the first failure:

  #541                     a stale committed `nros_generated.h`
  upstream 5dc2fa869       ditto, second copy
  #547                     the Cyclone backend hand-declared the ABI in three
                           per-platform `extern "C"` blocks — compiled fine,
                           failed at LINK with `undefined reference to
                           'nros_platform_clock_ms'`
  #548                     the XRCE C shim, same shape, five undefined refs,
                           and it took the whole tier-2 fixture build down

#548 says outright that this "should probably become a gate given this is the
second consumer the rename missed". This is that gate. Both live arms are the
two spellings the failures actually took:

  1. a code reference to a retired name in a TU that does not include
     `nros/platform.h` — the wrapper is not in scope, so it is an undefined
     reference at link (#548);
  2. a hand-written declaration of a retired name outside the defining header —
     an `extern` copy cannot disagree with a `static inline` and win; it only
     lets the file compile and then fail at link (#547).

and a third arm for the shape that started the run:

  3. more than one TRACKED file defining the wrappers. #541 and upstream
     5dc2fa869 were both a stale committed copy of a header, and a second
     definition is how a caller ends up compiling against the old contract
     while the link expects the new one.

Comments are stripped before matching. That is not cosmetic: two files in the
tree mention the retired names ONLY in prose explaining that they are retired
(`nros-board-threadx-qemu-riscv64/startup.c`,
`zephyr/nros_platform_zephyr_shims.c`), and a naive grep reports both. A gate
that cries wolf on its own documentation gets bypassed.

WHAT THIS GATE DOES NOT COVER — verified, not assumed. Replaying the actual
pre-fix sources through it:

    #547  internal.hpp          -> 3 hits (lines 27, 33). CAUGHT.
    #548  platform_aliases.c    -> CLEAN. NOT caught.

#548's file included `nros/platform.h` and declared nothing; what failed was the
include RESOLVING to a stale copy on Zephyr's own include path — a property of
the build's `-I` order, not of any source text. No source-scanning gate can see
that. Arm 3 covers it only in the case where the second copy is TRACKED, which
#548's was not. Saying so here beats implying the class is closed: the honest
remaining answer for that shape is the build-side one (the fixture staleness
probes and #0196's rule that a probe must watch the same inputs the consumer
reads).

Adding another retired symbol is one entry in RETIRED below.
"""

import os
import re
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# The retired names, and the header whose `static inline` now defines them.
RETIRED = ["nros_platform_clock_ms", "nros_platform_clock_us"]
# The header the names used to live in. Phase-352 W6 removed the wrappers from
# it, so it is no longer a definition site — it is only where the vacuity check
# below confirms they stayed gone.
DEFINING_HEADER = "packages/platform/nros-platform-api/include/nros/platform.h"

# `.rs` belongs here for the same reason the C files do — these are `extern "C"`
# symbols, and Rust can declare, define or call them by name in exactly the two
# shapes arms 1-3 describe. A C/C++-only scan left that half of the ABI unscanned.
#
# Not to be confused with the RUST-SIDE rename of the same era: pre-fix
# `c_stub_platform.rs` called `CffiPlatform::clock_ms()`, a trait method, not this
# symbol. That one needs no gate — a removed method is a compile error, whereas a
# removed `extern "C"` symbol compiles and fails at link, which is the whole
# reason this file exists.
SUFFIXES = (".c", ".h", ".cpp", ".hpp", ".cc", ".cxx", ".rs")

# A declaration rather than a call: `uint64_t nros_platform_clock_ms(void);` or
# an `extern` of the same. Anchored at LINE START, and the character class
# admits no parens or braces, so it cannot span a function header — without
# that, `return nros_platform_clock_ms();` reads as a declaration (the gate's
# own self-test caught exactly that). The `return` lookahead covers the
# statement spelled at line start.
DECL_PAT = re.compile(
    r'^[ \t]*(?:extern\s+)?(?!return\b)[A-Za-z_][\w \t*]*\b(%s)\s*\([^)]*\)\s*;'
    % "|".join(RETIRED),
    re.M,
)


CHAR_LIT = re.compile(r"'(?:\\.|[^\\'])'")


def strip_comments(text):
    """Remove /* */ and // comments, preserving string literals and line count."""
    out = []
    i, n = 0, len(text)
    while i < n:
        c = text[i]
        # A Rust lifetime (`'static`, `'a`) is not a char literal, and treating it
        # as one makes the scanner run to the NEXT quote, swallowing whatever code
        # lies between — a stripper that silently eats its input is worse than no
        # stripper. So `'` opens a literal only when it actually closes like one.
        # The rule holds for C too, where every char literal has the same shape.
        if c == "'" and not CHAR_LIT.match(text, i):
            out.append(c)
            i += 1
            continue
        if c == '"' or c == "'":
            quote = c
            out.append(c)
            i += 1
            while i < n:
                if text[i] == "\\" and i + 1 < n:
                    out.append(text[i:i + 2])
                    i += 2
                    continue
                out.append(text[i])
                if text[i] == quote:
                    i += 1
                    break
                i += 1
            continue
        if text.startswith("/*", i):
            j = text.find("*/", i + 2)
            j = n if j == -1 else j + 2
            # keep newlines so reported line numbers stay true
            out.append("".join(ch for ch in text[i:j] if ch == "\n"))
            i = j
            continue
        if text.startswith("//", i):
            j = text.find("\n", i)
            i = n if j == -1 else j
            continue
        out.append(c)
        i += 1
    return "".join(out)


def tracked_files():
    out = subprocess.run(
        ["git", "-C", ROOT, "ls-files", "--",
         "packages", "examples", "tests", "zephyr", "integrations", "cmake"],
        capture_output=True, text=True, check=True,
    ).stdout.split()
    return [f for f in out if f.endswith(SUFFIXES)]


def offenders(files):
    hits = []
    for rel in files:
        path = os.path.join(ROOT, rel)
        try:
            with open(path, encoding="utf-8", errors="replace") as fh:
                raw = fh.read()
        except OSError:
            continue
        code = strip_comments(raw)
        # keep the match, not just the name: the first offset is reported below,
        # and re-searching for it is what the type checker rightly objects to.
        found = {}
        for s in RETIRED:
            m = re.search(r'\b%s\b' % s, code)
            if m:
                found[s] = m
        if not found:
            continue
        used = list(found)
        # arm 2 — a hand copy of the declaration.
        for m in DECL_PAT.finditer(code):
            line = code[:m.start()].count("\n") + 1
            hits.append((rel, line, "declares", m.group(1)))
        # arm 1 — ANY code reference. Phase-352 W6 retired the wrappers
        # outright, so including the header no longer supplies them and there is
        # no spelling of a use that links.
        first = min(code[:m.start()].count("\n") + 1 for m in found.values())
        hits.append((rel, first, "references a RETIRED symbol", ", ".join(used)))
    return hits


def self_test():
    """Both directions. A checker that stopped checking passes silently, which is
    the failure shape this gate exists for."""
    tmp_root = os.path.join(ROOT, "tmp")
    os.makedirs(tmp_root, exist_ok=True)
    with tempfile.TemporaryDirectory(dir=tmp_root) as d:
        probe = os.path.join(d, "probe.c")
        rel = os.path.relpath(probe, ROOT)

        def write(body):
            with open(probe, "w") as fh:
                fh.write(body)

        # 1 — a call with no include IS reported (#548's shape).
        write('uint64_t f(void){ return nros_platform_clock_ms(); }\n')
        if not offenders([rel]):
            sys.stderr.write("self-test: an unincluded use was NOT reported\n")
            sys.exit(2)
        # 2 — phase-352 W6: including the header no longer rescues a use,
        #     because the header no longer defines anything. Any reference is an
        #     error now, and this arm exists to keep the earlier, weaker rule
        #     ("use is fine if you include it") from creeping back.
        write('#include "nros/platform.h"\n'
              'uint64_t f(void){ return nros_platform_clock_ms(); }\n')
        if not offenders([rel]):
            sys.stderr.write("self-test: an INCLUDED use was NOT reported\n")
            sys.exit(2)
        # 3 — a hand declaration is reported (#547's shape: it compiled, then
        #     failed at link). Upstream's header comment names this gate as what
        #     catches it.
        write('#include "nros/platform.h"\n'
              'extern uint64_t nros_platform_clock_ms(void);\n')
        if not offenders([rel]):
            sys.stderr.write("self-test: a hand declaration was NOT reported\n")
            sys.exit(2)
        # 4 — prose about the retired names is not a use. Two real files in this
        #     tree are exactly this, and a naive grep flags both.
        write('/* nros_platform_clock_us() is retired; see RFC-0073. */\n'
              '// nros_platform_clock_ms() too\n'
              'int f(void){ return 0; }\n')
        if offenders([rel]):
            sys.stderr.write("self-test: a comment WAS reported\n")
            sys.exit(2)
        # 5 — arm 3: ANY definition of the wrappers IS reported — after W6 there
        #     is no legitimate one, not even in the header they used to live in.
        write('static inline uint64_t nros_platform_clock_ms(void) { return 0; }\n')
        if not extra_definers([rel]):
            sys.stderr.write("self-test: a duplicate definition was NOT reported\n")
            sys.exit(2)
        # 6 — the Rust arms. `.rs` joined the scan set only after the gate had
        #     already missed a Rust caller, so both directions get pinned here.
        rs = os.path.join(d, "probe.rs")
        rs_rel = os.path.relpath(rs, ROOT)

        def write_rs(body):
            with open(rs, "w") as fh:
                fh.write(body)

        write_rs('unsafe extern "C" {\n    fn nros_platform_clock_ms() -> u64;\n}\n')
        if not offenders([rs_rel]):
            sys.stderr.write("self-test: a Rust extern declaration was NOT reported\n")
            sys.exit(2)
        write_rs('#[unsafe(no_mangle)]\n'
                 'pub extern "C" fn nros_platform_clock_us() -> u64 { 0 }\n')
        if not extra_definers([rs_rel]):
            sys.stderr.write("self-test: a Rust definition was NOT reported\n")
            sys.exit(2)
        # …and a lifetime must not derail the comment stripper. Without the
        # char-literal rule the `'` opens a literal that runs to the next quote,
        # so the retired name after it is never seen and this passes vacuously.
        write_rs("fn f<'a>(x: &'a str) -> &'a str { x }\n"
                 "fn g() -> u64 { unsafe { nros_platform_clock_ms() } }\n")
        if not offenders([rs_rel]):
            sys.stderr.write("self-test: a use after a LIFETIME was NOT reported\n")
            sys.exit(2)
        # prose in Rust is still prose.
        write_rs('// nros_platform_clock_ms is retired; see RFC-0073.\n'
                 'fn f() -> u64 { 0 }\n')
        if offenders([rs_rel]):
            sys.stderr.write("self-test: a Rust comment WAS reported\n")
            sys.exit(2)

        # …and a file that merely calls them is not a DEFINITION (it is still an
        # arm-1 offender, which is a different report).
        write('uint64_t f(void){ return nros_platform_clock_ms(); }\n')
        if extra_definers([rel]):
            sys.stderr.write("self-test: a caller WAS reported as a definer\n")
            sys.exit(2)


# Two spellings of a definition: C's `static inline` wrapper, and Rust's
# `#[no_mangle] … extern "C" fn`. Either one is a second definition site.
DEFINE_PAT = re.compile(
    r'static\s+inline\s+[\w \t*]*\b(?:%s)\s*\('
    r'|extern\s+"C"\s+fn\s+(?:%s)\s*\('
    % ("|".join(RETIRED), "|".join(RETIRED))
)


def extra_definers(files):
    """Arm 3 — a SECOND tracked file defining the wrappers. #541 and upstream
    5dc2fa869 were each a stale committed header copy."""
    out = []
    for rel in files:
        try:
            with open(os.path.join(ROOT, rel), encoding="utf-8", errors="replace") as fh:
                code = strip_comments(fh.read())
        except OSError:
            continue
        if DEFINE_PAT.search(code):
            out.append(rel)
    return out


def main():
    self_test()
    files = tracked_files()
    if not files:
        sys.stderr.write("[FAIL] no tracked sources scanned — this gate would pass vacuously.\n")
        return 1
    if not os.path.exists(os.path.join(ROOT, DEFINING_HEADER)):
        sys.stderr.write(
            f"[FAIL] the defining header is gone: {DEFINING_HEADER}\n"
            "       Either it moved (update DEFINING_HEADER) or the wrappers were\n"
            "       retired too, in which case this gate needs rewriting rather than\n"
            "       passing vacuously.\n"
        )
        return 1
    dupes = extra_definers(files)
    if dupes:
        sys.stderr.write(
            "[FAIL] a SECOND tracked definition of the clock wrappers (issue 0555):\n"
        )
        for rel in dupes:
            sys.stderr.write(f"         {rel}\n")
        sys.stderr.write(
            f"\n       The one definition lives in {DEFINING_HEADER}.\n"
            "       A stale committed copy is how #541 and upstream 5dc2fa869 broke:\n"
            "       a caller compiles against the old contract and links against the new.\n"
        )
        return 1
    hits = offenders(files)
    if hits:
        sys.stderr.write(
            "[FAIL] retired platform clock symbols (issue 0555):\n"
        )
        for rel, line, why, what in hits:
            sys.stderr.write(f"         {rel}:{line}: {why} — {what}\n")
        sys.stderr.write(
            "\n       `nros_platform_clock_{ms,us}` are RETIRED (RFC-0073 / phase-352 W6).\n"
            "       They spent one release as `static inline` wrappers behind\n"
            "       NROS_PLATFORM_LEGACY_CLOCK_UNITS; the wrappers and the escape hatch\n"
            "       are both gone, so NOTHING defines these names and no spelling of a\n"
            "       use links — including <nros/platform.h> no longer rescues one.\n"
            "       Call `nros_platform_clock_ns()` and divide: ms/us were lossy views,\n"
            "       which is why they went (issues 0547, 0548, 0555).\n"
        )
        return 1
    print(f"check-retired-platform-clock-symbols: OK ({len(files)} tracked C/C++/Rust source(s))")
    return 0


if __name__ == "__main__":
    sys.exit(main())
