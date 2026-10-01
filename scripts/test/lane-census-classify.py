#!/usr/bin/env python3
"""phase-475 W1 — classify a lane census's junit per TARGET.

A target's outcome is the worst of its cases: FAIL > FIXTURE > TIMEOUT > SKIP >
PASS. What each one tells a lane:

* PASS    — reached a verdict here with nothing staged. ADMISSIBLE.
* FAIL    — reached a verdict that is red, OR reported an unmet precondition as
            a failure instead of a skip. The census cannot tell those apart and
            does not pretend to: read the message. Both are findings — a real
            red nobody sees, or a skip-discipline defect.
* FIXTURE — needs a staged fixture. Admissible only where the job builds it.
* SKIP    — a precondition this environment lacks; the class and message say
            which. Counted per missing thing, so the image owner can see what
            one more package would buy.
* TIMEOUT — hung instead of skipping. Always a defect.

Skip recognition is `nros_tests::skip_marker`'s rule — a `[SKIPPED` PREFIX,
classed or not — never the bare `[SKIPPED]`, which is the five-site bug issue
0658 fixed."""
import re, sys, json, collections
import xml.etree.ElementTree as ET

junit = sys.argv[1]
root = ET.parse(junit).getroot()
SKIP = re.compile(r"\[SKIPPED(?::(\w+))?\]\s*(.*)")
FIXTURE = re.compile(r"not prebuilt|BuildFailed|fixture binary|NROS_FIXTURE|fixtures-built|build-test-fixtures", re.I)
RANK = {"PASS": 0, "SKIP": 1, "TIMEOUT": 2, "FIXTURE": 3, "FAIL": 4}

per = collections.defaultdict(lambda: {"cases": 0, "outcome": "PASS", "why": collections.Counter()})
for suite in root.iter("testsuite"):
    name = suite.get("name", "")
    # nextest suite names are `<crate>::<binary>`
    target = name.split("::", 1)[1] if "::" in name else name
    for case in suite.iter("testcase"):
        t = per[target]; t["cases"] += 1
        fail = case.find("failure") if case.find("failure") is not None else case.find("error")
        if fail is None:
            continue
        text = (fail.get("message") or "") + "\n" + (fail.text or "")
        so = case.find("system-out"); se = case.find("system-err")
        text += "\n" + ((so.text if so is not None else "") or "") + ((se.text if se is not None else "") or "")
        m = SKIP.search(text)
        if "timed out" in text.lower() or "TIMEOUT" in (fail.get("type") or ""):
            kind, why = "TIMEOUT", "timed out"
        elif m:
            kind, why = "SKIP", f"{m.group(1) or 'capability'}: {m.group(2).strip()[:90]}"
        elif FIXTURE.search(text):
            kind, why = "FIXTURE", (FIXTURE.search(text).group(0))
        else:
            first = next((l.strip() for l in text.splitlines() if l.strip() and "panicked" not in l), "")
            kind, why = "FAIL", first[:110]
        if RANK[kind] > RANK[t["outcome"]]:
            t["outcome"] = kind
        t["why"][f"{kind} | {why}"] += 1

by = collections.defaultdict(list)
for tgt, t in sorted(per.items()):
    by[t["outcome"]].append(tgt)
print(f"targets that RAN: {len(per)}")
for k in ["PASS", "SKIP", "TIMEOUT", "FIXTURE", "FAIL"]:
    print(f"  {k:8s} {len(by[k])}")
json.dump({k: {"outcome": v["outcome"], "cases": v["cases"], "why": dict(v["why"])} for k, v in per.items()},
          open(sys.argv[2], "w"), indent=1, sort_keys=True)
for k in ["PASS", "FAIL", "TIMEOUT"]:
    print(f"\n== {k} ==")
    for tgt in by[k]:
        top = per[tgt]["why"].most_common(1)
        print(f"  {tgt:44s} {('— ' + top[0][0]) if top else ''}"[:170])
