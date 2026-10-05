#!/usr/bin/env python3
"""A test reader appends a child's output through `nros_tests::capture::append`.

Issue 1697. Every process wrapper in `nros-tests` accumulated a fixture's
console with `output.push_str(&String::from_utf8_lossy(&buf[..n]))`, with no
limit. A Zephyr guest looping on a log line (issue 1696) grew the test process
to 91 GB, and a test failure became a host-wide out-of-memory event. The fix is
one bounded spelling, `capture::append`. This gate refuses the raw spelling
anywhere else under `packages/testing/`, so the next reader cannot bring the
unbounded shape back.

The rule matches a `push_str` of a `from_utf8_lossy` over a BYTE SLICE, which is
what a read loop does with what `read()` returned. A `push_str` of a formatted
string, or of a whole `Output` that has already exited, is not a stream and is
not this hazard.
"""

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SCOPE = "packages/testing"
EXEMPT = {"packages/testing/nros-tests/src/capture.rs"}
RAW = re.compile(r"\.push_str\(\s*&String::from_utf8_lossy\(\s*&[A-Za-z_][A-Za-z0-9_]*\[\s*\.\.")


def problems(files: dict) -> list:
    out = []
    for path, text in sorted(files.items()):
        if path in EXEMPT:
            continue
        for n, line in enumerate(text.splitlines(), 1):
            if RAW.search(line):
                out.append(f"  {path}:{n}: {line.strip()}")
    return out


def self_test() -> None:
    bad = {"a.rs": "    output.push_str(&String::from_utf8_lossy(&buf[..n]));\n"}
    assert problems(bad), "the raw read-loop spelling must fail"
    bad2 = {"b.rs": "                dst.push_str(&String::from_utf8_lossy(&buffer[..n]));\n"}
    assert problems(bad2), "any buffer name must fail"
    ok = {
        "c.rs": "    crate::capture::append(&mut output, &buf[..n]);\n"
        "    s.push_str(&format!(\"x {}\", 1));\n"
        "    s.push_str(&String::from_utf8_lossy(&out.stdout));\n",
        "packages/testing/nros-tests/src/capture.rs": "    capture.push_str(&String::from_utf8_lossy(&b[..n]));\n",
    }
    assert not problems(ok), f"the bounded and non-stream shapes must pass: {problems(ok)}"


def tracked() -> dict:
    names = subprocess.run(
        ["git", "-C", str(ROOT), "ls-files", "-z", f"{SCOPE}/*.rs"],
        capture_output=True,
        text=True,
        check=True,
    ).stdout.split("\0")
    return {n: (ROOT / n).read_text(encoding="utf-8") for n in names if n}


def main() -> int:
    self_test()
    out = problems(tracked())
    if out:
        print("check-test-capture-bounded: FAIL\n", file=sys.stderr)
        print("\n".join(out), file=sys.stderr)
        print(
            "\n  A read loop accumulates through `nros_tests::capture::append(&mut out, &buf[..n])`,"
            "\n  which bounds the capture (issue 1697). An unbounded `push_str` grew a test to 91 GB.",
            file=sys.stderr,
        )
        return 1
    print("check-test-capture-bounded: OK — every read loop under packages/testing is bounded; self-test 3 cases OK")
    return 0


if __name__ == "__main__":
    sys.exit(main())
