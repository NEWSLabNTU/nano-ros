#!/usr/bin/env python3
"""Every C subscription entry point that HAS the type forwards its bound -- issue 1376.

WHAT IS BEING MEASURED

`rclc_subscription_init_*` keeps the type descriptor on the
`rcl_subscription_t` (`ROSIDL_GET_MSG_TYPE_SUPPORT(...)`), and the generated
descriptor carries the type's `__RX_MAX_SERIALIZED_SIZE` as
`serialized_size_max`. An `rclc_executor_add_subscription*` entry point that
takes such a subscription therefore has the receive bound in hand. Passing a
literal `0` hint from there puts the registration on the `unbounded` row
(issue 1319's `c_raw_no_hint`): the arena slot is priced at the executor's
closure buffer rather than at the message. The sizing descriptor cannot see
which entry point a call site picks, so it credits every C entry with the typed
hint, and one such site makes that credit untrue in the UNDER direction.

Until issue 1376 was fixed, four of the five entry points that take an
`rcl_subscription_t` passed `0` (`_raw`, `_raw_validated`, `_with_info`,
`_in_group`), with comments dating from before phase-492 gave the descriptor a
bound. Measured: this gate over that source reports exactly those four.

THE RULE

In `packages/api/nros-c/src/executor.rs`, inside every `pub ... extern "C" fn
rclc_executor_add_subscription*` whose parameters include an
`rcl_subscription_t`, the last argument of each `add_arena_subscription_c_*`
call must not be the literal `0`. The function shape decides it, not a list:
an entry point that takes type NAMES (`_raw_with_info`) has no descriptor and
so no bound, and its `0` is the honest answer.

The sibling for the C++ headers is `check-cpp-subscription-bound-supplied`.

WHY A FAST-LANE GATE

No build: a read of one tracked source file.

Usage::

    check-c-subscription-bound-supplied.py
    check-c-subscription-bound-supplied.py --selftest
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
SOURCE = REPO / "packages/api/nros-c/src/executor.rs"

_ENTRY = re.compile(
    r'pub\s+unsafe\s+extern\s+"C"\s+fn\s+(rclc_executor_add_subscription\w*)\s*\('
)
_CALL = re.compile(r"\.?add_arena_subscription_c_\w+(?:::<[^>]*>)?\s*\(")


def _strip_comments(text: str) -> str:
    text = re.sub(r"/\*.*?\*/", "", text, flags=re.S)
    return re.sub(r"//[^\n]*", "", text)


def _balanced(text: str, open_at: int) -> int:
    """Index just past the bracket matching text[open_at] ('(' or '{')."""
    pair = {"(": ")", "{": "}"}
    opener = text[open_at]
    closer = pair[opener]
    depth = 0
    for i in range(open_at, len(text)):
        c = text[i]
        if c == opener:
            depth += 1
        elif c == closer:
            depth -= 1
            if depth == 0:
                return i + 1
    raise ValueError("unbalanced bracket")


def _split_args(inner: str) -> list[str]:
    args, depth, cur = [], 0, []
    for c in inner:
        if c in "([{<":
            depth += 1
        elif c in ")]}>":
            depth -= 1
        if c == "," and depth == 0:
            args.append("".join(cur).strip())
            cur = []
        else:
            cur.append(c)
    tail = "".join(cur).strip()
    if tail:
        args.append(tail)
    return args


def scan(text: str) -> tuple[list[str], int]:
    """Return (violations, number of subscription-taking entry points seen)."""
    text = _strip_comments(text)
    violations: list[str] = []
    seen = 0
    for m in _ENTRY.finditer(text):
        name = m.group(1)
        params_end = _balanced(text, m.end() - 1)
        params = text[m.end() : params_end - 1]
        body_open = text.index("{", params_end)
        body = text[body_open : _balanced(text, body_open)]
        if "rcl_subscription_t" not in params:
            continue
        seen += 1
        for call in _CALL.finditer(body):
            inner_end = _balanced(body, call.end() - 1)
            args = _split_args(body[call.end() : inner_end - 1])
            if args and args[-1] == "0":
                violations.append(
                    f"{name}: passes a literal 0 receive hint to "
                    f"{call.group(0).strip('.( ')} although it takes an "
                    "rcl_subscription_t, whose descriptor states the bound -- "
                    "forward subscription_rx_bound(subscription) (issue 1376)"
                )
    return violations, seen


_GOOD = """
pub unsafe extern "C" fn rclc_executor_add_subscription_raw(
    executor: *mut rclc_executor_t,
    subscription: *mut rcl_subscription_t,
) -> rcl_ret_t {
    let rx_bytes = subscription_rx_bound(subscription);
    register(|e| { e.add_arena_subscription_c_callback::<N>(a, b, None, rx_bytes) })
}
pub unsafe extern "C" fn rclc_executor_add_subscription_raw_with_info(
    executor: *mut rclc_executor_t,
    type_name: *const c_char,
) -> rcl_ret_t {
    rust_exec.add_arena_subscription_c_info_callback::<N>(a, b, 0)
}
"""

_CASES = [
    # (label, source, expected violation count)
    ("subscription form forwards the bound", _GOOD, 0),
    (
        "subscription form passes 0",
        _GOOD.replace("None, rx_bytes)", "None, 0)"),
        1,
    ),
    (
        "0 hidden behind a trailing comment",
        _GOOD.replace("None, rx_bytes)", "None, // no hint\n 0,\n)"),
        1,
    ),
    (
        "names-only form gains a subscription parameter",
        _GOOD.replace(
            "type_name: *const c_char,",
            "type_name: *const c_char, subscription: *mut rcl_subscription_t,",
        ),
        1,
    ),
]


def selftest() -> bool:
    ok = True
    for label, src, want in _CASES:
        got = len(scan(src)[0])
        if got != want:
            print(
                f"  case {label!r}: {got} violation(s), expected {want}",
                file=sys.stderr,
            )
            ok = False
    return ok


def main() -> int:
    if not selftest():
        print("check-c-subscription-bound-supplied: SELFTEST FAILED", file=sys.stderr)
        return 1
    if sys.argv[1:] == ["--selftest"]:
        print(
            f"check-c-subscription-bound-supplied --selftest: {len(_CASES)} case(s) OK"
        )
        return 0
    violations, seen = scan(SOURCE.read_text())
    if seen == 0:
        print(
            "check-c-subscription-bound-supplied: found no subscription-taking "
            f"rclc entry point in {SOURCE.relative_to(REPO)} -- the scan is "
            "broken, not the tree",
            file=sys.stderr,
        )
        return 1
    if violations:
        print(
            f"check-c-subscription-bound-supplied: {len(violations)} site(s) drop "
            "the type's receive bound:",
            file=sys.stderr,
        )
        for v in violations:
            print(f"  {v}", file=sys.stderr)
        return 1
    print(
        f"check-c-subscription-bound-supplied: OK -- all {seen} rclc entry "
        "points that take an rcl_subscription_t forward its bound"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
