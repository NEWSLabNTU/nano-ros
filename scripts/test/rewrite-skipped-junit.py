#!/usr/bin/env python3
"""Rewrite ``[SKIPPED]`` ``<failure>`` entries in a JUnit XML to ``<skipped>``.

The ``nros_tests::skip!`` macro (see ``packages/testing/nros-tests/src/lib.rs``)
panics with ``[SKIPPED] <reason>`` to signal "environment-conditional skip"
(missing fixture, missing emulator, missing cross toolchain, …) — the
CLAUDE.md contract bans bare ``eprintln+return`` for skips because those
report PASS, masking real regressions.

Nextest records each panic as ``<failure>`` in junit.xml (the ``<skipped>``
channel is reserved for the ``#[ignore]`` attribute, not runtime skips).
Downstream JUnit consumers — CI dashboards, code-coverage walls, the
``_count-real-failures`` / ``_test-summary`` recipe in ``justfile``,
``scripts/test/failed-filterset.py`` — all read the ``<failure>`` count and
report CI red even when every "failure" is a ``[SKIPPED]``.

This post-processor reads ``target/nextest/default/junit.xml`` (or the path
passed on the CLI), for each ``<testcase>`` whose ``<failure message="...">``
or body starts with ``[SKIPPED]``:

* replaces the ``<failure>`` element with ``<skipped message="...">``,
  preserving the message attribute and panic body;
* decrements the enclosing ``<testsuite failures="N">`` count by one and
  increments its ``skipped="N"`` count by one;
* propagates the same delta to the top-level ``<testsuites>`` element.

The file is written back atomically (``write to .tmp + os.replace``) so a
crash mid-write leaves the original untouched.

Usage::

    rewrite-skipped-junit.py [JUNIT_PATH]

Exit code 0 even when the file is absent or has no ``[SKIPPED]`` entries —
the rewriter is idempotent and safe to chain at the tail of any test recipe.

See ``docs/development/test-harness.md`` for the surrounding skip/tally
semantics (Phase 214.R).
"""

from __future__ import annotations

import os
import sys
import xml.etree.ElementTree as ET
from typing import Optional

DEFAULT_JUNIT = "target/nextest/default/junit.xml"
SKIP_MARKER = "[SKIPPED]"


def _is_skipped_failure(failure: ET.Element) -> bool:
    """Return True if the ``<failure>`` element is a ``[SKIPPED]`` marker."""
    msg = failure.get("message") or ""
    body = failure.text or ""
    return SKIP_MARKER in msg or SKIP_MARKER in body


def _decr_attr(elem: ET.Element, attr: str, delta: int) -> None:
    """Add ``delta`` (may be negative) to integer attribute ``attr`` on ``elem``."""
    try:
        cur = int(elem.get(attr, "0"))
    except ValueError:
        cur = 0
    new = cur + delta
    if new < 0:
        new = 0
    elem.set(attr, str(new))


def rewrite(junit_path: str) -> int:
    """Rewrite ``junit_path`` in place. Returns the number of testcases rewritten.

    Returns 0 when the file is absent, unparseable, or has no ``[SKIPPED]``
    failures — every case is a no-op exit (no side effect on disk).
    """
    if not os.path.exists(junit_path):
        return 0
    try:
        tree = ET.parse(junit_path)
    except ET.ParseError as exc:
        print(
            f"rewrite-skipped-junit: warning: failed to parse {junit_path}: {exc}",
            file=sys.stderr,
        )
        return 0

    root = tree.getroot()
    total_rewritten = 0

    # JUnit shape: <testsuites> > <testsuite>* > <testcase>* > <failure>?
    # nextest sometimes omits the outer <testsuites> and emits a single
    # <testsuite> root — handle both. ``iter("testsuite")`` works in both
    # cases (returns the root itself when it is a <testsuite>).
    for suite in root.iter("testsuite"):
        suite_delta = 0
        for tc in suite.findall("testcase"):
            # A testcase may have multiple <failure> entries in principle;
            # rewrite only if ALL failures are [SKIPPED] markers — a real
            # failure mixed with a skip remains a failure.
            failures = tc.findall("failure")
            if not failures:
                continue
            if not all(_is_skipped_failure(f) for f in failures):
                continue
            for f in failures:
                msg = f.get("message") or SKIP_MARKER
                body = f.text or ""
                tc.remove(f)
                skipped = ET.SubElement(tc, "skipped")
                skipped.set("message", msg)
                if body:
                    skipped.text = body
            suite_delta += 1
            total_rewritten += 1
        if suite_delta:
            _decr_attr(suite, "failures", -suite_delta)
            _decr_attr(suite, "skipped", suite_delta)

    if total_rewritten == 0:
        return 0

    # Propagate to top-level <testsuites> if present.
    if root.tag == "testsuites":
        _decr_attr(root, "failures", -total_rewritten)
        _decr_attr(root, "skipped", total_rewritten)

    tmp = junit_path + ".tmp"
    tree.write(tmp, encoding="utf-8", xml_declaration=True)
    os.replace(tmp, junit_path)
    return total_rewritten


def main(argv: Optional[list[str]] = None) -> int:
    args = list(sys.argv[1:] if argv is None else argv)
    junit = args[0] if args else DEFAULT_JUNIT
    n = rewrite(junit)
    if n:
        print(
            f"rewrite-skipped-junit: rewrote {n} [SKIPPED] failure(s) "
            f"to <skipped> in {junit}"
        )
    return 0


if __name__ == "__main__":
    sys.exit(main())
