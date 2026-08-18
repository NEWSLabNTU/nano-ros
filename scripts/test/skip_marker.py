"""Where a `[SKIPPED]` marker can live in a JUnit testcase, and how to find it.

`nros_tests::skip!` signals "environment-conditional skip" by PANICKING with
`[SKIPPED] <reason>`. Two scripts classify those out of the failure count —
`rewrite-skipped-junit.py` (rewrites them to `<skipped>`) and
`name-real-failures.py` (names what is left). Both read the `<failure>`
element's `message` attribute and body, and that is not always where the marker
ends up.

## Why this module exists

`nros-rmw-zenoh::zenoh_integration two_sessions_deliver_cross_session_through_router`
skipped with

    [SKIPPED] second session refused — shim built with ZPICO_MAX_SESSIONS=1

and reported as a REAL failure, reddening tier 1 on its own. Its JUnit entry:

    <failure/>                     <- message attr None, body None
    <system-out>… test result: FAILED …</system-out>
    <system-err>
      thread '…' panicked at …zenoh_integration.rs:242:13:
      [SKIPPED] second session refused — shim built with ZPICO_MAX_SESSIONS=1
    </system-err>

Whether the panic text lands in `<failure>` or only in `<system-err>` depends on
the harness and how the test binary was invoked; the classifier assumed the
first. This is the issue-0196 shape again — a check whose coverage is narrower
than the rule it enforces — and the consequence is worse than a wrong count: a
skip counted as a failure is a red that no fix can clear, which is what teaches
people to stop believing the suite.

## Why not just search the whole output

Because a test can legitimately PRINT the marker without being skipped itself.
`entry_matrix` reports one line per cell and several read
`  <cell>: [SKIPPED] … fixture not built`, while the test itself passes or fails
on its own terms. Scanning `<system-err>` for the substring would let any such
test launder a real failure into a skip — the exact inversion of the defect
being fixed, and a far more expensive one.

So the stream forms require the marker to be the panic MESSAGE: the line
immediately after a `panicked at …` line, which is precisely what `skip!`
produces and what an incidental report line never is (those are indented, and
they are not preceded by a panic header).
"""

import re

# Issue 0584 — the marker may carry a CLASS: `[SKIPPED:lane] …`. Plain
# `[SKIPPED]` reads as the default class.
SKIP_CLASS_RE = re.compile(r"\[SKIPPED(?::([a-z_]+))?\]")

# Anchored form, for the `<failure message=…>` / body case: the payload IS the
# panic message there, so the marker starts it.
#: The marker's invariant prefix. `[SKIPPED]` and `[SKIPPED:<class>]` both start
#: with it — matching the BARE spelling is issue 0658. Mirrors
#: `nros_tests::skip_marker::PREFIX` on the Rust side.
PREFIX = "[SKIPPED"

SKIP_AT_START_RE = re.compile(r"^\[SKIPPED(?::([a-z_]+))?\]")

# Stream form: the marker is the panic message, i.e. the line right after the
# `panicked at <file>:<line>:<col>:` header. `[^\S\n]*` allows trailing spaces
# on the header line but NOT indentation before the marker — an incidental
# report line is indented and is not preceded by a panic header.
PANIC_SKIP_RE = re.compile(
    r"panicked at [^\n]*\n\[SKIPPED(?::([a-z_]+))?\]",
    re.MULTILINE,
)


def skip_class_in(payloads, streams=()):
    """The skip class if this testcase is a `skip!`, else None.

    ``payloads`` are `<failure>`-owned strings (message attribute, body), where
    the marker must START the text. ``streams`` are `<system-out>` /
    `<system-err>` bodies, where it must be a panic message.

    Returns the class name (`"capability"` when the marker is unclassed), or
    None when this is a real failure.
    """
    for text in payloads:
        if not text:
            continue
        m = SKIP_AT_START_RE.match(text.lstrip())
        if m:
            return m.group(1) or "capability"
    for text in streams:
        if not text:
            continue
        m = PANIC_SKIP_RE.search(text)
        if m:
            return m.group(1) or "capability"
    return None


def testcase_streams(testcase):
    """The `<system-out>` / `<system-err>` bodies of a `<testcase>`, in order."""
    out = []
    for tag in ("system-out", "system-err"):
        for node in testcase.iter(tag):
            out.append(node.text or "")
    return out
