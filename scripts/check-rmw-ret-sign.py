#!/usr/bin/env python3
"""Phase 376 W3.d — nobody may test an RMW status by its SIGN.

The campaign adopts upstream's return-code values: `RMW_RET_OK = 0`,
`RMW_RET_ERROR = 1`, `RMW_RET_TIMEOUT = 2`, … Ours were negative (`-1`, `-2`, …),
so every caller written as

    if (ret < 0) { /* failed */ }

stops detecting errors the moment the values flip — silently, and in the
direction that reads as success. That is the migration's sharp edge: not a
compile error, not a test failure, just error handling that no longer runs.

This gate finds those call sites BEFORE the flip, so the sweep is a list rather
than an archaeology exercise afterwards.

# What counts

A comparison of an RMW-status-valued expression against zero using `<`, `>=`,
`<= -1` or similar. The status-valued expressions are the vtable slots that
return one, plus the names the wrappers give them.

Deliberately NOT flagged: `== 0`, `!= 0`, and comparisons against a named
constant (`ret == RMW_RET_OK`). Those keep working under any numbering, which is
exactly why they are the spelling the migration moves callers to.

# The dual-return slots are the reason this is subtle

Eleven slots multiplex a COUNT and a STATUS through one `int32_t`: non-negative
is bytes/messages/0-or-1, negative is the error. For those, `< 0` is not a bug
today — it is the documented contract. It becomes a bug when W3.d gives the
count its own out-parameter and the return becomes a plain status. So this gate
reports them as MIGRATION SITES rather than defects, and the two lists are kept
apart: one is "fix now", the other is "fix with the slot".

Run: python3 scripts/check-rmw-ret-sign.py
"""

import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# Slots whose return is PURELY a status today. A sign test on these is already
# fragile and must be gone before the values flip.
STATUS_ONLY = {
    "create_session", "destroy_session", "drive_io", "create_publisher",
    "publish_raw", "create_subscription", "create_service", "send_reply",
    "create_client", "send_request_raw", "register_subscription_event",
    "register_publisher_event", "assert_publisher_liveliness",
    "set_wake_callback", "pub_loan", "pub_commit", "publish_streamed",
    "ping_session",
    # Converted by W3.d step A — they are status-only now, so a sign test on
    # any of them is in the "fix before the flip" class like the rest.
    "take", "take_request", "take_response", "take_sequence",
    "take_loaned_message", "has_data", "has_request",
    "service_server_is_available", "subscription_supports_in_place",
    "process_raw_in_place", "next_deadline_ms",
}

# Slots that multiplex count-or-flag with status. `< 0` is their CONTRACT today;
# it becomes wrong when W3.d moves the count to an out-parameter.
# NOTE `set()`, not `{}` — the latter is an empty DICT, which made the
# self-test's `STATUS_ONLY & DUAL_RETURN` raise a TypeError. Caught immediately,
# which is what a self-test that checks its own invariants is for.
DUAL_RETURN: set[str] = set()
_DUAL_RETURN_NOTE = {
    # EMPTY as of phase 376 W3.d step A (2026-08-23): every one of the eleven
    # slots that multiplexed a count-or-flag with a status now reports through
    # an out-parameter, so `rmw_vtable.h` contains no `int32_t (*slot)` at all.
    #
    # Kept as a named, empty set rather than deleted: the distinction between
    # "a sign test that is the contract" and "a sign test that is a bug" is what
    # this gate is about, and a future RTOS-only slot could reintroduce the
    # first. An empty set says the migration finished; a deleted one says
    # somebody forgot why there were two lists.
}

# `something = slot(...)` then `if (something < 0)` is the shape; a single-file
# regex cannot follow the variable, so the heuristic is deliberately narrow: a
# sign test on the CALL itself, or on a variable assigned from one within a few
# lines. Narrow on purpose — a gate that guesses produces findings people learn
# to dismiss.
# `(?<![<>])` keeps a bit-shift out: `Self(1 << 0)` contains the characters
# `< 0` and is not a comparison at all. That one false positive attributed a
# QoS flag constant to `ping_session`, which is the kind of finding that teaches
# a reader the whole list is noise.
SIGN_TEST = re.compile(r"(?:(?<![<>])<\s*0|>=\s*0|<=\s*-\s*1|(?<!-)>\s*-\s*1)")


def tracked():
    out = subprocess.run(
        ["git", "ls-files", "-z", "*.c", "*.h", "*.cpp", "*.hpp", "*.rs"],
        cwd=ROOT, capture_output=True, text=True, check=False,
    )
    return [p for p in out.stdout.split("\0") if p]


def strip_comments(text, rust):
    text = re.sub(r"/\*.*?\*/", " ", text, flags=re.S)
    text = re.sub(r"(?m)//.*$", " ", text)
    if rust:
        # bindgen carries the C doc block into a `#[doc = "…"]` STRING, which is
        # not a comment and so survives the pass above — the doc for
        # `try_recv_raw` says "`< 0` — error", and reading that as a call site
        # put two entries in the first run of this audit that are prose about
        # the contract, not code that tests it.
        text = re.sub(r"(?m)^\s*#\[doc\s*=.*$", " ", text)
        text = re.sub(r"(?m)^\s*///.*$", " ", text)
    return text


def scan_file(rel, names, window=30):
    """[(line_no, slot, text)] — a sign test within `window` lines after a slot call.

    A window, not a data-flow analysis, and the first version of this file did
    something cleverer and found NOTHING: it required `name(` on one line and
    the test on the same line or within three of an assignment it could parse.
    Real call sites look like

        let rc = unsafe {
            (self.vtable.try_recv_raw.expect("rmw vtable: try_recv_raw"))(
                ...
            )
        };
        if rc < 0 {

    — the call is behind `.expect(...)(`, the assignment spans five lines, and
    the test is nine lines below the name. Reporting zero would have been the
    worst outcome available: a clean bill of health for the exact sweep this
    tool exists to produce. So the rule is deliberately loose and the output is
    a REVIEW LIST, not a verdict.

    The window is 30 lines because 12 was still too tight: `sub_borrow`,
    `try_recv_sequence` and `process_raw_in_place` each sit behind a
    `let Some(f) = self.vtable.<slot> else { ... }` guard, a trampoline
    definition, or a fallback branch, which puts 15-25 lines between the name
    and the test. Each widening here was driven by a site verified BY HAND
    first — the alternative is tuning a number until the output looks tidy,
    which optimises for a quiet report rather than a complete one.
    """
    path = os.path.join(ROOT, rel)
    try:
        raw = open(path, encoding="utf-8", errors="replace").read()
    except OSError:
        return []
    body = strip_comments(raw, rel.endswith(".rs"))
    lines = body.splitlines()

    hits = []
    seen = {}
    for i, line in enumerate(lines):
        for name in names:
            if re.search(r"\b" + re.escape(name) + r"\b", line):
                seen[name] = i
        if SIGN_TEST.search(line):
            for name, at in sorted(seen.items(), key=lambda kv: -kv[1]):
                if 0 <= i - at <= window:
                    hits.append((i + 1, name, line.strip()))
                    break
    return hits


def self_test():
    bad = []
    cases = [
        ("if (rc < 0) { }", True, "sign test"),
        ("if (rc == NROS_RMW_RET_OK) { }", False, "named constant"),
        ("if (rc != 0) { }", False, "!= 0 survives any numbering"),
        ("if (n >= 0) { }", True, ">= 0 is a sign test too"),
        ("pub const RELIABILITY: Self = Self(1 << 0);", False, "a bit-shift is not a comparison"),
        ("x = y >> 0;", False, "nor a right shift"),
    ]
    for text, should, label in cases:
        if bool(SIGN_TEST.search(text)) != should:
            bad.append(f"{label!r}: {text!r}")
    if STATUS_ONLY & DUAL_RETURN:
        bad.append(f"a slot cannot be both status-only and dual: {STATUS_ONLY & DUAL_RETURN}")
    if bad:
        for b in bad:
            sys.stderr.write("check-rmw-ret-sign --self-test: " + b + "\n")
        sys.exit(2)
    print(f"check-rmw-ret-sign --self-test: OK ({len(cases)} case(s))")


# ---------------------------------------------------------------------------
# Issue 0773 — the gate above scans a WINDOW around vtable SLOT names, and the
# bug that actually shipped was one layer below them.
#
# A backend's INTERNAL helper returned a byte count or an `NROS_RMW_RET_*`
# constant through one `int32_t`, and its caller separated the two with
# `if (x < 0)`. No slot name appears within thirty lines of some of those
# tests, so the window found nothing and reported 0/0 while the cyclonedds
# cancel path was turning `NO_DATA` (1003) into a length and then
# `BUFFER_TOO_SMALL` (1005) into a slice bound.
#
# The window cannot be widened into this: the defect is not "a sign test near a
# slot", it is "a function that returns a length OR a status at all". So this
# check is structural instead — a C/C++ function whose body returns both an
# `NROS_RMW_RET_*` constant and a cast-to-integer length is the shape, wherever
# it sits and whatever its caller does with it.
RET_CONST = re.compile(r"\breturn\s+NROS_RMW_RET_[A-Z_]+\s*;")
LEN_RETURN = re.compile(
    r"\breturn\s+(?:static_cast<u?int(?:8|16|32|64)_t>\(|\(u?int(?:8|16|32|64)_t\)\s*)"
)
# `rmw_ret_t` is a typedef for `int32_t`, so a function declared to return a
# STATUS can still `return static_cast<int32_t>(len)` and compile. That spelling
# is the same defect and was missed by a first version of this check that only
# matched `intN_t` heads — caught by probing the gate instead of trusting it.
FN_HEAD = re.compile(
    r"(?m)^(?:static\s+)?(?:u?int(?:8|16|32|64)_t|rmw_ret_t)\s+([A-Za-z_]\w*)"
    r"\s*\([^;{]*\)\s*\{"
)


def scan_multiplexers(rel):
    """[(line, fn)] — integer-returning functions that return BOTH a status
    constant and a cast length."""
    path = os.path.join(ROOT, rel)
    try:
        src = open(path, encoding="utf-8", errors="replace").read()
    except OSError:
        return []
    body_src = strip_comments(src, rust=False)
    out = []
    for m in FN_HEAD.finditer(body_src):
        depth = 1
        i = m.end()
        while i < len(body_src) and depth:
            if body_src[i] == "{":
                depth += 1
            elif body_src[i] == "}":
                depth -= 1
            i += 1
        body = body_src[m.end():i]
        if RET_CONST.search(body) and LEN_RETURN.search(body):
            out.append((body_src[: m.start()].count("\n") + 1, m.group(1)))
    return out


def main():
    self_test()
    files = tracked()
    now, later = [], []
    for rel in files:
        for line_no, slot, text in scan_file(rel, STATUS_ONLY):
            now.append((rel, line_no, slot, text))
        for line_no, slot, text in scan_file(rel, DUAL_RETURN):
            later.append((rel, line_no, slot, text))

    multiplexers = []
    for rel in files:
        if not rel.endswith((".c", ".cpp")):
            continue
        for line_no, fn in scan_multiplexers(rel):
            multiplexers.append((rel, line_no, fn))

    print("rmw return-sign audit (phase 376 W3.d)")
    print(f"  sign tests on STATUS-ONLY results : {len(now)}   <- fix before the flip")
    print(f"  sign tests on DUAL-RETURN results : {len(later)} <- fix with the slot")
    print()
    if now:
        print("## status-only (a sign test here is already fragile)")
        for rel, line_no, slot, text in now:
            print(f"  {rel}:{line_no}  [{slot}]  {text[:88]}")
        print()
    if multiplexers:
        sys.stderr.write(
            f"[FAIL] {len(multiplexers)} function(s) return a LENGTH or an "
            "`NROS_RMW_RET_*` status through one integer (issue 0773):\n"
        )
        for rel, line_no, fn in multiplexers:
            sys.stderr.write(f"         {rel}:{line_no}  {fn}\n")
        sys.stderr.write(
            "\n       Our status codes are POSITIVE since W3.d step B, so no sign\n"
            "       test can separate the two any more and the compiler cannot see\n"
            "       the difference. Return the status; put the length in an\n"
            "       out-parameter, as the vtable slots themselves do.\n"
        )
        return 1

    if later:
        print("## dual-return (the contract today; changes with W3.d)")
        for rel, line_no, slot, text in later[:40]:
            print(f"  {rel}:{line_no}  [{slot}]  {text[:88]}")
        if len(later) > 40:
            print(f"  … and {len(later) - 40} more")
    return 0


if __name__ == "__main__":
    sys.exit(main())
