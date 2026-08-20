#!/usr/bin/env python3
"""A test that cannot fail is worse than no test — reject the shapes that report PASS.

Issue 0702. Eight instances of ONE class were found and fixed by hand in a single
day, each having survived months:

  * `Err(e) => { eprintln!("[INFO] could not build …"); }` with the only
    assertion inside the `Ok` arm — `test_action_binaries_exist` passed when the
    binaries did not exist.
  * `Err(_) => { eprintln!("SKIP: cc not found"); return; }` x3 — a file whose
    purpose is "the generated C compiles" concluding that without a compiler.
  * `eprintln!("Skipping test: {e}"); return Ok(())` x13 — `rosidl-codegen`
    reading `/opt/ros/jazzy` on a humble host, so the suite answered "19 tests,
    0.027 s, all green" over work it never did (#0693).
  * `match open() { Ok(..) => assert.., Err(e) => println!("expected in some
    environments") }` x4 — accepting either outcome asserts nothing, and those
    were green over a capability that had NEVER been present (#0682).

Every one printed something a human would read as "fine" and returned success.
Grep found them only because somebody went looking; this makes the shape
unspellable at authoring time instead.

WHAT IS REJECTED — two shapes, both "print a diagnosis, report PASS":

  1. a diverging arm (`Err(..) =>`) whose body prints and then falls through,
     with no `panic!`, `assert*`, `skip!`, `?`, `return Err`, or `unreachable!`.

  2. a FINAL `else` block that prints and decides nothing — the last statement
     of the test, so reaching it ends the test green. Added after issue 0711
     found a live one the `Err`-arm rule could not see:

         if result.received_count > 0 {
             eprintln!("[PASS] Peer mode communication works");
         } else {
             eprintln!("[INFO] No messages received - peer discovery may ...");
             eprintln!("[INFO] This is expected on some network configurations");
         }

     That test ran with a session that never opened, received zero messages, and
     was reported GREEN — while explaining itself in a way that reads like a
     note. The gate scanned the file and passed it, because the gate's coverage
     was narrower than the rule it enforces (issue 0196's shape).

     `else` specifically, and only when it ENDS the function: a trailing
     `if verbose { eprintln!(..) }` after real assertions is ordinary, and an
     `if/else` in the middle of a test is not the verdict.

     KNOWN LIMIT, stated rather than papered over: a print-and-pass `else`
     NESTED inside another block is not caught, because "is this the function's
     verdict?" stops being answerable without parsing. One such site was found
     by hand and fixed while widening this rule
     (`cargo-nano-ros/tests/integration_tests.rs`, a `std_msgs` lookup that
     warned and passed). If more turn up, the answer is a real Rust parser, not
     a looser brace heuristic — the first draft of this rule used "the next
     character is `}`" and flagged a reporting LOOP, which is how a gate starts
     costing more than it catches.

WHAT IS NOT — the honest spellings, all of which stay available:
  * `nros_tests::skip!(…)`  — the harness counts it, junit records it
  * `panic!` / `assert!` / `.expect(…)` / `?` / `return Err(..)`
  * a helper returning `bool`/`Option` that a CALLER turns into a skip (the
    `require_*` pattern — the print is a note, the decision is elsewhere)
  * printing INSIDE an arm that also asserts or propagates

Run:  python3 scripts/check-tests-can-fail.py [--self-test]
"""

import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# Test code only. A `src/` fallback that prints and continues is a runtime
# decision, not a test reporting a verdict it did not reach.
TRACKED_GLOBS = (
    "packages/*/*/tests/*.rs",
    "packages/*/*/*/tests/*.rs",
    "packages/*/*/*/*/tests/*.rs",
)

PRINTS = re.compile(r"\b(?:e?println!|print!|eprint!)\s*\(")
# Anything that makes the arm a real verdict or hands the decision upward.
DECIDES = re.compile(
    r"\b(?:panic!|unreachable!|todo!|unimplemented!"
    r"|assert!|assert_eq!|assert_ne!|debug_assert!"
    r"|skip!|expect\(|unwrap\(|bail!|return\s+Err|\?\s*[;,)])"
)

# `Err(..) => { … }` / `Err(..) => expr,` — the match-arm form.
ERR_ARM = re.compile(r"\bErr\s*\(\s*[^)]*\)\s*=>\s*", re.S)

# `} else {` — the fall-through form (shape 2 above).
ELSE_BLOCK = re.compile(r"\belse\s*\{")


def _strip_comments(src: str) -> str:
    """Blank out `//` comments and string literals.

    Issue 0683 taught this the hard way from the other side: a regex that reads
    a doc comment as code reports a call site that is prose. Blanking rather
    than deleting keeps every byte offset, so reported line numbers stay true.
    """
    out = list(src)
    i, n = 0, len(src)
    while i < n:
        c = src[i]
        if c == "/" and i + 1 < n and src[i + 1] == "/":
            j = src.find("\n", i)
            j = n if j < 0 else j
            for k in range(i, j):
                out[k] = " "
            i = j
        elif c == "/" and i + 1 < n and src[i + 1] == "*":
            j = src.find("*/", i + 2)
            j = n if j < 0 else j + 2
            for k in range(i, j):
                if out[k] != "\n":
                    out[k] = " "
            i = j
        elif c == '"':
            j = i + 1
            while j < n:
                if src[j] == "\\":
                    j += 2
                    continue
                if src[j] == '"':
                    break
                j += 1
            for k in range(i, min(j + 1, n)):
                if out[k] != "\n":
                    out[k] = " "
            i = j + 1
        else:
            i += 1
    return "".join(out)


# The `require_*` shape the docstring promises to leave alone: the arm PRINTS a
# note and yields a value its CALLER turns into a skip. `router_locator()` in
# `cffi_smoke.rs` is the live example — `Err(e) => { eprintln!(…); None }`, and
# the caller does `let Some(x) = router_locator() else { skip!() }`. The
# decision is real, it just is not here, and flagging it would push authors
# toward deleting the note rather than toward asserting.
_YIELDS_UP = re.compile(r"(?:None|false)\s*,?\s*\}\s*$")


def _hands_decision_up(body: str) -> bool:
    return bool(_YIELDS_UP.search(body.strip()))


def _block_at(src: str, start: int):
    """Return (body, end) for the `{…}` starting at/after `start`, else None."""
    i = start
    while i < len(src) and src[i].isspace():
        i += 1
    if i >= len(src) or src[i] != "{":
        return None
    depth, j = 0, i
    while j < len(src):
        if src[j] == "{":
            depth += 1
        elif src[j] == "}":
            depth -= 1
            if depth == 0:
                return src[i : j + 1], j + 1
        j += 1
    return None


def offenders(paths):
    """Report (path, line, snippet) for arms that print and decide nothing."""
    found = []
    for rel in paths:
        full = os.path.join(ROOT, rel)
        try:
            with open(full, encoding="utf8", errors="replace") as fh:
                raw = fh.read()
        except OSError:
            continue
        src = _strip_comments(raw)
        for m in ERR_ARM.finditer(src):
            blk = _block_at(src, m.end())
            if blk is None:
                continue  # `Err(e) => expr,` — an expression, not a body
            body, _ = blk
            if _hands_decision_up(body):
                continue
            if PRINTS.search(body) and not DECIDES.search(body):
                line = src[: m.start()].count("\n") + 1
                snippet = " ".join(raw.splitlines()[line - 1].split())[:70]
                found.append((rel, line, snippet))

        for m in ELSE_BLOCK.finditer(src):
            blk = _block_at(src, m.end() - 1)
            if blk is None:
                continue
            body, end = blk
            if not PRINTS.search(body) or DECIDES.search(body):
                continue
            # Only when the block ENDS the FUNCTION, so reaching it IS the
            # verdict. "The next character is `}`" is NOT that test — it also
            # matches an else that ends a `for` body, which is how the first
            # draft of this rule flagged `report_portability_baseline`, a
            # reporting loop with no verdict to give. So require that the brace
            # closing this block is followed by item level: EOF, or the start of
            # the next item / attribute / doc comment.
            rest = src[end:].lstrip()
            if not rest.startswith("}"):
                continue
            after = rest[1:].lstrip()
            if after and not re.match(r"(#\[|//|/\*|pub\b|fn\b|mod\b|impl\b|use\b|const\b|static\b|type\b|struct\b|enum\b)", after):
                continue
            line = src[: m.start()].count("\n") + 1
            snippet = " ".join(raw.splitlines()[line - 1].split())[:70]
            found.append((rel, line, snippet))
    return found


def tracked_test_files():
    out = subprocess.run(
        ["git", "ls-files", *TRACKED_GLOBS],
        cwd=ROOT,
        capture_output=True,
        text=True,
        check=True,
    ).stdout.split()
    return [p for p in out if p.endswith(".rs")]


def self_test():
    import tempfile

    tmp = os.path.join(ROOT, "tmp")
    os.makedirs(tmp, exist_ok=True)
    with tempfile.TemporaryDirectory(dir=tmp) as d:
        probe = os.path.join(d, "probe_test.rs")
        rel = os.path.relpath(probe, ROOT)

        def write(body):
            with open(probe, "w") as fh:
                fh.write(body)

        def check(expect_hit, why):
            hit = bool(offenders([rel]))
            if hit != expect_hit:
                sys.stderr.write(f"self-test: {why}\n")
                sys.exit(2)

        # The shape this gate exists for.
        write('fn t(){ match go() { Ok(v)=>assert!(v), Err(e)=>{ eprintln!("nope {e}"); } } }\n')
        check(True, "a print-only Err arm was NOT reported")

        # …and it is the ABSENCE of a decision that matters, not the print.
        write('fn t(){ match go() { Ok(v)=>assert!(v), Err(e)=>{ eprintln!("{e}"); panic!("x"); } } }\n')
        check(False, "an Err arm that panics was reported")
        write('fn t(){ match go() { Ok(v)=>assert!(v), Err(e)=>{ eprintln!("{e}"); nros_tests::skip!("no"); } } }\n')
        check(False, "an Err arm that skips was reported")
        write('fn t(){ match go() { Ok(v)=>assert!(v), Err(e)=>{ eprintln!("{e}"); assert!(false); } } }\n')
        check(False, "an Err arm that asserts was reported")

        # An arm that prints nothing is not this gate's business.
        write("fn t(){ match go() { Ok(v)=>assert!(v), Err(_)=>{ } } }\n")
        check(False, "a silent Err arm was reported")

        # Shape 2 — the issue 0711 form: a FINAL else that prints and decides
        # nothing, so reaching it ends the test green.
        write('fn t(){ if ok() { assert!(true); } else { eprintln!("expected sometimes"); } }\n')
        check(True, "a print-only FINAL else was NOT reported")

        # …but only when it ends the function. A trailing debug print after the
        # verdict, or an if/else mid-test, is ordinary code.
        write('fn t(){ if v() { eprintln!("dbg"); } else { eprintln!("dbg2"); } assert!(go()); }\n')
        check(False, "a mid-test if/else was reported")
        write('fn t(){ if ok() { assert!(true); } else { eprintln!("x"); panic!("no"); } }\n')
        check(False, "a final else that panics was reported")
        write('fn t(){ if ok() { assert!(true); } else { nros_tests::skip!("absent"); } }\n')
        check(False, "a final else that skips was reported")

        # Prose must not be read as code — issue 0683's lesson, inverted.
        write('fn t(){ /* Err(e) => { eprintln!("x"); } */ assert!(true); }\n')
        check(False, "a commented-out arm was reported")
        write('fn t(){ let s = "Err(e) => { eprintln!(\\"x\\"); }"; assert!(!s.is_empty()); }\n')
        check(False, "a string literal was read as an arm")

    sys.stdout.write("check-tests-can-fail self-test: OK\n")


def main():
    if "--self-test" in sys.argv:
        self_test()
        return
    self_test()
    files = tracked_test_files()
    bad = offenders(files)
    if bad:
        sys.stderr.write(
            "error: %d test arm(s) print a diagnosis and report PASS.\n\n" % len(bad)
        )
        for rel, line, snippet in bad:
            sys.stderr.write(f"  {rel}:{line}\n      {snippet}\n")
        sys.stderr.write(
            "\nAn `Err` arm that only prints makes the test unable to fail: the run\n"
            "goes green whether the thing under test worked or not, and the message\n"
            "reads like a note rather than a defect. Say what you mean instead:\n"
            "  * the precondition is genuinely absent -> `nros_tests::skip!(…)`\n"
            "  * the failure is real                  -> `panic!` / `assert!` / `?`\n"
            "  * the caller decides                   -> return `bool`/`Option`\n"
            "See issue 0702 for the eight instances that motivated this.\n"
        )
        sys.exit(1)
    sys.stdout.write(
        "tests-can-fail OK — %d test file(s), no print-and-pass arms.\n" % len(files)
    )


if __name__ == "__main__":
    main()
