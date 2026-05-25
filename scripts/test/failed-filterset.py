#!/usr/bin/env python3
"""Emit a cargo-nextest filterset selecting the real failed tests from a JUnit report.

A "real" failure is a `<testcase>` with a `<failure>` whose message/body does
NOT contain `[SKIPPED]` (the marker emitted by `nros_tests::skip!` for
environment-conditional skips — see `just _count-real-failures`).

Usage:
  failed-filterset.py [JUNIT]            # print one nextest -E filterset (empty if none)
  failed-filterset.py [JUNIT] --names    # print "<binary-id>\\t<test-name>" lines

The nextest binary id equals the JUnit `classname`, so each failure maps to
`(binary_id(=<classname>) & test(=<name>))`, unioned with `|`.
"""
import sys
import xml.etree.ElementTree as ET

DEFAULT_JUNIT = "target/nextest/default/junit.xml"


def real_failures(junit_path):
    try:
        root = ET.parse(junit_path).getroot()
    except (FileNotFoundError, ET.ParseError):
        return []
    seen = set()
    out = []
    for tc in root.iter("testcase"):
        fails = tc.findall("failure")
        if not fails:
            continue
        blob = " ".join((f.get("message") or "") + " " + (f.text or "") for f in fails)
        if "[SKIPPED]" in blob:
            continue
        name = tc.get("name")
        cls = tc.get("classname")
        if not name or not cls:
            continue
        key = (cls, name)
        if key not in seen:
            seen.add(key)
            out.append(key)
    return sorted(out)


def main():
    args = [a for a in sys.argv[1:]]
    names_mode = "--names" in args
    args = [a for a in args if a != "--names"]
    junit = args[0] if args else DEFAULT_JUNIT

    failures = real_failures(junit)
    if names_mode:
        for cls, name in failures:
            print(f"{cls}\t{name}")
        return
    print(" | ".join(f"(binary_id(={cls}) & test(={name}))" for cls, name in failures))


if __name__ == "__main__":
    main()
