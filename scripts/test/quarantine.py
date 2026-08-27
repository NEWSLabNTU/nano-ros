#!/usr/bin/env python3
"""Flake quarantine — phase-395 W5.

A quarantined test still RUNS and still RECORDS; it stops BLOCKING. This is the
prerequisite for batching, because a batch red is ambiguous between a defect and
a flake and bisection multiplies the cost — one flake ejects and re-tests every
innocent PR in the batch.

Two halves, and the second is the one that keeps this honest:

`--demote JUNIT`
    Rewrite a quarantined `<failure>` into `<skipped type="nros:quarantine">`,
    keeping the original failure text inside the node. Rewriting rather than
    filtering is deliberate, and it is the same reason `rewrite-skipped-junit.py`
    rewrites: every downstream consumer (`_count-real-failures`, `_test-summary`,
    `failed-filterset.py`, any CI dashboard) then agrees about what happened,
    instead of each re-deriving it and drifting.

`--check`
    The gate. An entry is refused when it has EXPIRED, when its issue is not
    open, or when a required field is missing.

WHY EXPIRY IS A HARD FAILURE

Quarantine without expiry is deletion with extra steps: the test stops blocking,
everyone stops looking, and nobody can later say whether it still describes a
real behaviour. Extending a date is a perfectly good decision; making it
silently is not. So the gate forces the decision to be taken again, by a person,
on a date they chose.

WHY A DEMOTION THAT MATCHED NOTHING IS REPORTED

An entry naming a test that no longer exists — renamed, deleted, or simply
mistyped — is INERT while reading as "we are tracking this flake". That is the
issue-0743 class exactly: a stale `.config/nextest.toml` `test()` override is
silently dead, and five of them were. It cannot be checked statically here
either, because rstest case names (`fn::case_1`) appear in no source file. So
`--demote` reports every entry that saw NO result in the run it just examined,
which is the one moment the information exists.

WHY NOT WILDCARDS

An entry matches a test name exactly, or as an rstest case parent (`name::case`).
Nothing broader. A pattern quarantine silences tests nobody chose to silence,
and the whole cost of this mechanism is paid to avoid exactly that.

Usage::

    quarantine.py --check
    quarantine.py --demote [JUNIT]
"""

import argparse
import datetime
import os
import re
import sys
import xml.etree.ElementTree as ET

try:
    import tomllib
except ModuleNotFoundError:  # python < 3.11
    import tomli as tomllib

# `scripts/test/quarantine.py` -> repo root is THREE levels up. Getting this
# wrong loads no registry and the gate reports OK over an empty list — a gate
# that cannot fail, which is worse than no gate. Asserted below.
ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
REGISTRY = os.path.join(ROOT, ".config", "flake-quarantine.toml")
DEFAULT_JUNIT = os.path.join("target", "nextest", "default", "junit.xml")

REQUIRED = ("test", "binary", "issue", "added", "expires", "evidence", "reason")
DATE = re.compile(r"^\d{4}-\d{2}-\d{2}$")


def load():
    if not os.path.exists(REGISTRY):
        # Not "no entries" — the registry is tracked, so its absence means the
        # path is wrong and every caller would silently see an empty quarantine.
        raise SystemExit(
            f"quarantine: registry not found at {REGISTRY}.\n"
            f"  This is a PATH bug, not an empty quarantine: the file is tracked."
        )
    with open(REGISTRY, "rb") as fh:
        return tomllib.load(fh).get("quarantined", [])


def matches(entry, classname, name):
    """Exact test name, or an rstest case under it. Never a pattern."""
    if entry.get("binary") and entry["binary"] != classname:
        return False
    t = entry.get("test", "")
    return name == t or name.startswith(t + "::")


# ------------------------------------------------------------------- --check

def issue_status(num):
    """(exists, status) for `docs/issues/NNNN-*.md`."""
    d = os.path.join(ROOT, "docs", "issues")
    for base in (d, os.path.join(d, "archived")):
        if not os.path.isdir(base):
            continue
        for fn in os.listdir(base):
            if fn.startswith(f"{num}-") and fn.endswith(".md"):
                with open(os.path.join(base, fn), encoding="utf8") as fh:
                    head = fh.read(4000)
                m = re.search(r"^status:\s*(\S+)", head, re.M)
                return True, (m.group(1) if m else "?")
    return False, None


def check():
    entries = load()
    today = datetime.date.today()
    errs = []

    for i, e in enumerate(entries):
        who = e.get("test") or f"<entry {i}>"
        for field in REQUIRED:
            if not str(e.get(field, "")).strip():
                errs.append(f"{who}: missing required field `{field}`")
        for field in ("added", "expires"):
            v = str(e.get(field, ""))
            if v and not DATE.match(v):
                errs.append(f"{who}: `{field}` must be YYYY-MM-DD, got {v!r}")

        exp = str(e.get("expires", ""))
        if DATE.match(exp):
            when = datetime.date.fromisoformat(exp)
            if when < today:
                errs.append(
                    f"{who}: quarantine EXPIRED on {exp} ({(today - when).days} days ago).\n"
                    f"      Decide again — this is not a formality:\n"
                    f"        * fixed, or no longer flaky?  delete the entry.\n"
                    f"        * still flaky and still not worth fixing?  extend `expires`\n"
                    f"          and say in issue {e.get('issue', '?')} what changed.\n"
                    f"      An entry that never expires is a deleted test that still\n"
                    f"      looks like a tracked one."
                )
            elif (when - today).days <= 14:
                print(
                    f"[warn] {who}: quarantine expires in {(when - today).days} day(s) "
                    f"({exp}) — issue {e.get('issue', '?')}"
                )

        num = str(e.get("issue", ""))
        if num:
            if not re.fullmatch(r"\d{4}", num):
                errs.append(f"{who}: `issue` must be a 4-digit id, got {num!r}")
            else:
                exists, status = issue_status(num)
                if not exists:
                    errs.append(
                        f"{who}: issue {num} does not exist under docs/issues/.\n"
                        f"      A quarantine records that we chose not to fix something YET;\n"
                        f"      without an issue that choice has no owner."
                    )
                elif status != "open":
                    errs.append(
                        f"{who}: issue {num} is `{status}`, not open. If the defect is\n"
                        f"      resolved the test should be unquarantined, not left non-blocking."
                    )

    if errs:
        print(f"check-flake-quarantine: {len(errs)} problem(s):\n", file=sys.stderr)
        for e in errs:
            print(f"  - {e}", file=sys.stderr)
        return 1

    if not entries:
        print("check-flake-quarantine OK — nothing quarantined.")
        return 0
    print(
        f"check-flake-quarantine OK — {len(entries)} quarantined test(s), "
        f"each with an open issue and an unexpired review date."
    )
    for e in entries:
        print(f"    {e['binary']} {e['test']}  (issue {e['issue']}, expires {e['expires']})")
    return 0


# ------------------------------------------------------------------ --demote

def demote(junit):
    entries = load()
    if not entries:
        return 0
    try:
        tree = ET.parse(junit)
    except (FileNotFoundError, ET.ParseError):
        return 0
    root = tree.getroot()

    seen = {id(e): 0 for e in entries}
    passed = {id(e): 0 for e in entries}
    demoted = []

    for case in root.iter("testcase"):
        cls = case.get("classname") or ""
        name = case.get("name") or ""
        entry = next((e for e in entries if matches(e, cls, name)), None)
        if entry is None:
            continue
        seen[id(entry)] += 1
        fail = case.find("failure")
        if fail is None:
            if case.find("skipped") is None:
                passed[id(entry)] += 1
            continue
        # Keep the failure TEXT — "still records" is the whole distinction
        # between this and deleting the test.
        original = (fail.get("message") or "") + "\n" + (fail.text or "")
        case.remove(fail)
        node = ET.SubElement(case, "skipped")
        node.set("type", "nros:quarantine")
        node.set("message", f"QUARANTINED (issue {entry['issue']}, expires {entry['expires']})")
        node.text = f"quarantined, NOT blocking. Original failure:\n{original}"
        demoted.append(f"{cls} {name}")

    if demoted:
        print(
            f"\n[quarantine] {len(demoted)} failure(s) DEMOTED — recorded, not blocking:",
            file=sys.stderr,
        )
        for d in demoted:
            print(f"    {d}", file=sys.stderr)
        print(
            "    These do not fail the lane. They are still in the junit as\n"
            "    <skipped type=\"nros:quarantine\"> with the original failure text.",
            file=sys.stderr,
        )

    for e in entries:
        if seen[id(e)] == 0:
            continue
        if passed[id(e)] == seen[id(e)]:
            print(
                f"[quarantine] {e['test']} PASSED {passed[id(e)]}/{seen[id(e)]} in this run.\n"
                f"    If that holds, delete the entry — issue {e['issue']}. A quarantine\n"
                f"    kept past its usefulness is a test nobody is watching.",
                file=sys.stderr,
            )

    # An entry that matched NOTHING is inert while reading as tracked — the
    # issue-0743 class. This run is the only moment that is knowable.
    absent = [e for e in entries if seen[id(e)] == 0]
    if absent:
        print(
            f"[quarantine] {len(absent)} entr(y/ies) matched NO test in this run:",
            file=sys.stderr,
        )
        for e in absent:
            print(f"    {e['binary']} {e['test']}", file=sys.stderr)
        print(
            "    Either this lane does not run them (normal), or the name is stale\n"
            "    — renamed, deleted, or mistyped. A stale entry protects nothing\n"
            "    while reading as though it does.",
            file=sys.stderr,
        )

    if demoted:
        tree.write(junit, encoding="utf-8", xml_declaration=True)
    return 0


# ----------------------------------------------------------------- --selftest

SYNTHETIC = """<?xml version="1.0"?>
<testsuites><testsuite name="s">
  <testcase classname="nros-tests::action_raw_goal_e2e"
            name="action_raw_goal_ships_one_cdr_header">
    <failure message="timeout">test timed out after 60s</failure>
  </testcase>
  <testcase classname="nros-tests::action_raw_goal_e2e" name="some_other_test">
    <failure message="assert">left != right</failure>
  </testcase>
  <testcase classname="nros-tests::other_suite"
            name="action_raw_goal_ships_one_cdr_header">
    <failure message="assert">different BINARY, must not be demoted</failure>
  </testcase>
</testsuite></testsuites>
"""


def selftest():
    """Prove the failure paths. A gate that cannot fail reads as coverage."""
    import copy
    import tempfile

    passed = failed = 0

    def ok(desc, cond, detail=""):
        nonlocal passed, failed
        if cond:
            print(f"  ok    {desc}")
            passed += 1
        else:
            print(f"  FAIL  {desc}{(': ' + detail) if detail else ''}")
            failed += 1

    base = {
        "test": "t", "binary": "b", "issue": "0854",
        "added": "2026-01-01", "expires": "2099-01-01",
        "evidence": "e", "reason": "r",
    }
    today = datetime.date.today()

    print("--check refuses what it must")
    global load
    real_load = load
    for desc, mutate, want in [
        ("an EXPIRED entry fails", {"expires": "2000-01-01"}, 1),
        ("a missing `evidence` fails", {"evidence": ""}, 1),
        ("a missing `reason` fails", {"reason": ""}, 1),
        ("a malformed date fails", {"expires": "next tuesday"}, 1),
        ("a non-4-digit issue fails", {"issue": "12"}, 1),
        ("an issue that does not exist fails", {"issue": "9999"}, 1),
        ("a well-formed entry passes", {}, 0),
    ]:
        e = dict(base); e.update(mutate)
        load = lambda _e=e: [_e]
        import io, contextlib
        buf = io.StringIO()
        with contextlib.redirect_stderr(buf), contextlib.redirect_stdout(io.StringIO()):
            rc = check()
        ok(desc, rc == want, f"rc={rc}")
    # An entry whose issue is RESOLVED must fail: if it is fixed, the test
    # should be unquarantined, not left permanently non-blocking.
    resolved = next(
        (f[:4] for f in sorted(os.listdir(os.path.join(ROOT, "docs", "issues", "archived")))
         if f.endswith(".md") and f[:4].isdigit()),
        None,
    )
    if resolved:
        e = dict(base); e["issue"] = resolved
        load = lambda _e=e: [_e]
        import io, contextlib
        with contextlib.redirect_stderr(io.StringIO()), contextlib.redirect_stdout(io.StringIO()):
            rc = check()
        ok(f"an entry on a RESOLVED issue ({resolved}) fails", rc == 1, f"rc={rc}")
    load = real_load

    print("\n--demote rewrites exactly the quarantined failure")
    e = {**base, "test": "action_raw_goal_ships_one_cdr_header",
         "binary": "nros-tests::action_raw_goal_e2e"}
    load = lambda: [e]
    import io, contextlib
    with tempfile.TemporaryDirectory() as d:
        j = os.path.join(d, "junit.xml")
        with open(j, "w", encoding="utf8") as fh:
            fh.write(SYNTHETIC)
        with contextlib.redirect_stderr(io.StringIO()):
            demote(j)
        root = ET.parse(j).getroot()
        cases = {(c.get("classname"), c.get("name")): c for c in root.iter("testcase")}
        q = cases[("nros-tests::action_raw_goal_e2e", "action_raw_goal_ships_one_cdr_header")]
        ok("the quarantined failure became <skipped>", q.find("skipped") is not None)
        ok("...and is no longer a <failure>", q.find("failure") is None)
        sk = q.find("skipped")
        ok("...typed so the skip budget can classify it",
           (sk.get("type") if sk is not None else "") == "nros:quarantine")
        ok("...KEEPING the original failure text (it still RECORDS)",
           "timed out after 60s" in ((sk.text or "") if sk is not None else ""))
        other = cases[("nros-tests::action_raw_goal_e2e", "some_other_test")]
        ok("a DIFFERENT test in the same binary still fails",
           other.find("failure") is not None)
        same = cases[("nros-tests::other_suite", "action_raw_goal_ships_one_cdr_header")]
        ok("the SAME name in a different BINARY still fails (binary is part of the key)",
           same.find("failure") is not None)

        with contextlib.redirect_stderr(io.StringIO()):
            demote(j)
        root2 = ET.parse(j).getroot()
        ok("demote is idempotent",
           sum(1 for c in root2.iter("testcase") if c.find("failure") is not None) == 2)

    print("\nan rstest CASE under a quarantined parent is covered; a sibling is not")
    e2 = {**base, "test": "parent", "binary": "b"}
    ok("`parent::case_1` matches", matches(e2, "b", "parent::case_1"))
    ok("`parent` matches", matches(e2, "b", "parent"))
    ok("`parent_other` does NOT match (no prefix wildcards)",
       not matches(e2, "b", "parent_other"))
    ok("a different binary does NOT match", not matches(e2, "other", "parent"))
    load = real_load

    print(f"\n{passed} passed, {failed} failed")
    if failed:
        print("selftest: FAILED")
        return 1
    print("selftest: ok — expiry, a resolved issue and a missing field each FAIL;\n"
          "  demotion keeps the failure text, is idempotent, and is keyed on\n"
          "  binary+test so a same-named test elsewhere still blocks.")
    return 0


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--check", action="store_true")
    ap.add_argument("--demote", nargs="?", const=DEFAULT_JUNIT, default=None)
    ap.add_argument("--selftest", action="store_true")
    a = ap.parse_args()
    if a.selftest:
        return selftest()
    if a.check:
        return check()
    if a.demote is not None:
        return demote(a.demote)
    ap.print_help()
    return 1


if __name__ == "__main__":
    sys.exit(main())
