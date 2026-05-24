#!/usr/bin/env python3
"""Print the slowest tests from a cargo-nextest JUnit report."""

from __future__ import annotations

import argparse
import sys
import xml.etree.ElementTree as ET
from dataclasses import dataclass
from pathlib import Path


@dataclass(frozen=True)
class TestCase:
    seconds: float
    binary: str
    name: str
    status: str


def testcase_status(case: ET.Element) -> str:
    failure = case.find("failure")
    error = case.find("error")
    skipped = case.find("skipped")
    if failure is not None:
        message = failure.get("message") or ""
        text = "".join(failure.itertext())
        if "[SKIPPED]" in message or "[SKIPPED]" in text:
            return "env-skip"
        return "fail"
    if error is not None:
        return "error"
    if skipped is not None:
        return "skip"
    return "pass"


def load_cases(junit: Path) -> list[TestCase]:
    root = ET.parse(junit).getroot()
    cases: list[TestCase] = []
    for case in root.iter("testcase"):
        raw_time = case.get("time")
        if raw_time is None:
            continue
        try:
            seconds = float(raw_time)
        except ValueError:
            continue
        cases.append(
            TestCase(
                seconds=seconds,
                binary=case.get("classname") or "<unknown>",
                name=case.get("name") or "<unknown>",
                status=testcase_status(case),
            )
        )
    return cases


def format_seconds(seconds: float) -> str:
    if seconds >= 60:
        minutes = int(seconds // 60)
        remainder = seconds - minutes * 60
        return f"{minutes}m{remainder:05.2f}s"
    return f"{seconds:6.3f}s"


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Print the slowest tests from a cargo-nextest JUnit XML report."
    )
    parser.add_argument(
        "junit",
        nargs="?",
        default="target/nextest/default/junit.xml",
        type=Path,
        help="Path to nextest junit.xml.",
    )
    parser.add_argument(
        "-n",
        "--limit",
        type=int,
        default=20,
        help="Number of slow tests to print.",
    )
    args = parser.parse_args()

    if args.limit <= 0:
        return 0
    if not args.junit.is_file():
        print(f"No nextest JUnit report found at {args.junit}", file=sys.stderr)
        return 0

    try:
        cases = load_cases(args.junit)
    except ET.ParseError as err:
        print(f"Failed to parse {args.junit}: {err}", file=sys.stderr)
        return 1

    if not cases:
        print("No nextest testcase timings found")
        return 0

    slow = sorted(cases, key=lambda case: case.seconds, reverse=True)[: args.limit]
    print(f"Slowest nextest tests (top {len(slow)}):")
    for case in slow:
        print(
            f"  {format_seconds(case.seconds)}  "
            f"{case.status:<8}  {case.binary}  {case.name}"
        )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
