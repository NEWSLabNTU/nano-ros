#!/usr/bin/env python3
"""A phase's OPEN work item, read against the API-parity ledger — issue 1489.

phase-419 W2 (`check-roadmap-claims.py`) catches a phase contradicting ITSELF.
This is the next layer: a claim that is mechanically checkable, but only
against an artifact the document does not contain. A phase states "18 rows
(C 6, Rust 12)" as open work; another phase does that work and has no reason to
edit the first; the first keeps presenting finished work in the voice of a
plan. phase-417's build list did that for 36 rows across G1-G5 while
`docs/reference/api-parity-ledger/<topic>/*.json` -- every row carries a `verdict` --
already said they had landed.

THE BINDING IS DERIVED, NEVER AUTHORED

There is no map from work items to ledger rows. An authored map is the drift
this repo refuses elsewhere (the RMW parity map's 28 stale slots), and it would
be one more document that goes stale when a sibling phase merges. The rule keys
on what the item already SAYS: a backticked ledger key -- `c:wait_for_publishers`,
`cpp:Timer::is_ready`, `rust:QoSProfile::parameters_default` -- in the item's
own text. Those spellings are the ledger's own (`SCHEMA.md`: "Key is
'<lang>:<normalized item>' exactly as the report prints it"), so the name in
the doc and the key in the ledger are one string.

A WORK ITEM is a heading or a bold opener carrying an id (`## W1 — ...`,
`**G1 — ...**`, `### W2.a`), up to the next one. It is CLOSED, and makes no
claim, when its own label says so (`— DONE`, `(landed)`) or when a later bold
line names the same id (`**G6 LANDED 2026-09-21.**`, the convention phase-417
already used). Only a label counts, never a word in the body: phase-419's
first cut counted LANDED in prose and would have flagged its own spec. A key
on a ticked `- [x]` line is claimed done and skipped.

WHAT A NAMED KEY ANSWERS, per the ledger

  open      the key's own row is `gap`, or it has none and inherits a `gap`
            from its owning type or a covering glob (`api-parity.py lookup`'s
            two fallbacks; the bucket condition needs the extraction, so this
            errs toward OPEN, which is the side that can only under-report).
  closed    its own row carries any other verdict, or a `retired` reason, or
            there is no row at all. `api-parity --check` fails on every
            difference without a row, so a name with none has no recorded
            difference -- it matches upstream. A MISSPELLED key reads the same
            way; that is why this is a report a person reads, and why the
            output names the key rather than a count.

THREE OUTCOMES PER PHASE (issue 1043), never a quiet pass

  STALE           an open item names keys the ledger has closed -- all of
                  them (`stale`) or some (`partly closed`).
  VERIFIED        every key an open item names is still `gap`.
  NOT APPLICABLE  no open item names a ledger key, so the ledger can say
                  nothing. Most phases have no ledger at all. An open item
                  that claims "N rows" while naming no key is listed as
                  UNBOUND inside this outcome -- that is exactly what
                  phase-417's G1-G5 looked like, and saying "not verified"
                  about it is the honest answer, not "OK".

GATE OR REPORT: A REPORT, deliberately.

The claim's truth is a function of a file OTHER phases edit. phase-444 closing
rows made phase-417 false without touching it, so as a gate this would go red
on whichever pull request merged the sibling work -- a PR that changed no part
of the phase it would be blamed for, ejecting the merge-queue batch behind it.
That is the coupling issue 1489 says it exists to REMOVE ("Nothing here says
phase-444 should have edited phase-417"), and phase-419 put its cross-file
work on the report for the same reason. So the verdict lives in `just
roadmap-audit` / the monthly `roadmap-audit.yml`, which exit 0 on a finding.

What DOES run on every push is the negative control. `self_test()` is called
from `check-roadmap-claims.py`'s own self-test on the fast line: synthetic
documents against a synthetic ledger, so it cannot fail because a sibling
merged, and it costs milliseconds. A report whose rule silently stopped firing
would otherwise be found on the 1st of a month, by nobody.
"""

from __future__ import annotations

import fnmatch
import importlib.util
import os
import re
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
ROADMAP = os.path.join(ROOT, "docs", "roadmap")

# A ledger key as the doc writes it: backticked, one of the ledger's three
# language prefixes, then a normalized item. Operators (`cpp:operator==`) and
# destructors are legal item spellings, so the tail is wider than an ident.
KEY = re.compile(r"`((?:c|cpp|rust):[A-Za-z_~][A-Za-z0-9_:~=!<>]*)`")

# An item opener: a heading or a bold span whose FIRST token is an id.
# `G1`, `W2`, `W2.a`, `W-B6`, `W-R1`, `R3`. A heading or bold line with no id
# (`## Why`, `**Stage 6 was never gated ...**`) is not an item.
ITEM_ID = r"(?P<id>(?:[A-Z]-)?[A-Z]{1,3}\d+(?:\.[a-z0-9]+)*)"
HEADING = re.compile(r"^(?P<hashes>#{2,6})\s+(?:" + ITEM_ID + r"\b)?(?P<rest>.*)$")
BOLD_OPENER = re.compile(r"^\*\*" + ITEM_ID + r"\b(?P<rest>.*)$")

# A label that closes the item. Case-insensitive because it is read from the
# item's OWN label ("(2026-10-21, landed)" and "— DONE" both occur), never from
# the body. A negated one ("not landed", "nothing done") is not a closure.
CLOSED_LABEL = re.compile(
    r"(?<!not )(?<!nothing )\b(landed|done|closed|retired|completed?)\b|✅|\bMET\b",
    re.I,
)
TICKED = re.compile(r"^\s*[-*]\s*\[[xX]\]")
# A count of rows the item claims, for the UNBOUND case.
ROW_CLAIM = re.compile(r"\b(\d+)\s+rows?\b", re.I)


def _api_parity():
    path = os.path.join(ROOT, "scripts", "api-parity.py")
    spec = importlib.util.spec_from_file_location("_nros_api_parity", path)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def load_ledger(ledger_dir=None):
    """The ledger, merged by `api-parity.py`'s own loader.

    ONE merge, not a second one: that loader refuses a duplicate key (two row
    files claiming one) and skips `_doc`. A second spelling here would agree
    until the day it did not.
    """
    return _api_parity().load_ledger(ledger_dir)


def classify_key(ledger, key):
    """("open"|"closed", reason) for one named key."""
    own = ledger.get(key)
    if own is not None:
        if own.get("retired"):
            return "closed", "retired"
        verdict = own.get("verdict") or "?"
        if verdict == "gap":
            return "open", "gap"
        return "closed", verdict
    lang, _, item = key.partition(":")
    if "::" in item:
        owner = ledger.get(f"{lang}:{item.rsplit('::', 1)[0]}")
        if owner is not None and owner.get("verdict") == "gap":
            return "open", "inherits gap from its type"
    for lkey, entry in ledger.items():
        klang, _, pattern = lkey.partition(":")
        if klang == lang and "*" in pattern and fnmatch.fnmatchcase(item, pattern):
            if entry.get("verdict") == "gap":
                return "open", f"inherits gap from {lkey}"
    return "closed", "no row"


def _bold_span(lines, i):
    """The bold opener's text up to its closing `**` (it may wrap lines)."""
    buf = lines[i][2:]
    j = i
    while "**" not in buf and j + 1 < len(lines) and lines[j + 1].strip():
        j += 1
        buf += " " + lines[j]
    return buf.split("**", 1)[0]


def work_items(text):
    """[(id, label, closed, open_keys, row_claim)] for each item in a doc."""
    lines = text.split("\n")
    items = []
    cur = None
    for i, line in enumerate(lines):
        h = HEADING.match(line)
        b = BOLD_OPENER.match(line) if not h else None
        if h:
            if h.group("id"):
                cur = {"id": h.group("id"), "label": line.strip(), "closed": False,
                       "keys": [], "body": []}
                cur["closed"] = bool(CLOSED_LABEL.search(line))
                items.append(cur)
            else:
                # A heading with no id ends the item: what follows is not its.
                cur = None
            continue
        if b:
            label = _bold_span(lines, i)
            if cur is not None and b.group("id") == cur["id"]:
                # `**G6 LANDED 2026-09-21.**` -- the item closing itself.
                if CLOSED_LABEL.search(label):
                    cur["closed"] = True
            else:
                cur = {"id": b.group("id"), "label": label.strip(), "closed": False,
                       "keys": [], "body": []}
                cur["closed"] = bool(CLOSED_LABEL.search(label))
                items.append(cur)
        if cur is None:
            continue
        cur["body"].append(line)
        if TICKED.match(line):
            continue
        for k in KEY.findall(line):
            if k not in cur["keys"]:
                cur["keys"].append(k)
    out = []
    for it in items:
        claim = ROW_CLAIM.search(it["label"])
        out.append((it["id"], it["label"], it["closed"], it["keys"],
                    int(claim.group(1)) if claim else None))
    return out


def check_doc(text, ledger):
    """(outcome, findings, unbound) for one phase document.

    outcome is "stale" | "verified" | "not-applicable".
    findings: [(item_id, label, [(key, reason)] closed, n_named)]
    unbound:  [(item_id, label, claimed_rows)]
    """
    findings, unbound = [], []
    bound = False
    for item_id, label, closed, keys, claim in work_items(text):
        if closed:
            continue
        if not keys:
            if claim:
                unbound.append((item_id, label, claim))
            continue
        bound = True
        shut = [(k, why) for k in keys for state, why in [classify_key(ledger, k)]
                if state == "closed"]
        if shut:
            findings.append((item_id, label, shut, len(keys)))
    if findings:
        return "stale", findings, unbound
    return ("verified" if bound else "not-applicable"), findings, unbound


def audit(paths=None, ledger=None):
    """{name: (outcome, findings, unbound)} over the active phases, or `paths`."""
    ledger = load_ledger() if ledger is None else ledger
    if paths is None:
        paths = [os.path.join(ROADMAP, n) for n in sorted(os.listdir(ROADMAP))
                 if n.endswith(".md")]
    result = {}
    for p in paths:
        with open(p, encoding="utf-8") as fh:
            result[os.path.basename(p)[:-3]] = check_doc(fh.read(), ledger)
    return result


def render(result, markdown=False):
    stale = {n: r for n, r in result.items() if r[0] == "stale"}
    verified = sorted(n for n, r in result.items() if r[0] == "verified")
    na = sorted(n for n, r in result.items() if r[0] == "not-applicable")
    unbound = {n: r[2] for n, r in result.items() if r[2]}
    out = []
    if markdown:
        out.append("\n## Open work items the API-parity ledger has closed (issue 1489)\n")
        out.append(f"{len(stale)} stale, {len(verified)} verified, {len(na)} not applicable "
                   "(no open item names a ledger key, so the ledger can say nothing).\n")
        if stale:
            out.append("| phase | item | keys the ledger has closed |")
            out.append("| --- | --- | --- |")
            for n, (_, findings, _) in sorted(stale.items()):
                for item_id, _label, shut, named in findings:
                    keys = ", ".join(f"`{k}` ({why})" for k, why in shut)
                    out.append(f"| `{n}` | {item_id} | {len(shut)} of {named}: {keys} |")
        if unbound:
            out.append("\nNOT VERIFIED — an open item claims rows and names no ledger key:\n")
            for n, items in sorted(unbound.items()):
                for item_id, _label, claim in items:
                    out.append(f"* `{n}` {item_id}: claims {claim} row(s)")
        return "\n".join(out)
    out.append(f"\nledger claims (issue 1489): {len(stale)} stale, {len(verified)} verified, "
               f"{len(na)} not applicable.")
    for n, (_, findings, _) in sorted(stale.items()):
        out.append(f"── {n}")
        for item_id, _label, shut, named in findings:
            word = "stale" if len(shut) == named else "partly closed"
            out.append(f"     {item_id}: {word}, {len(shut)} of {named} named key(s) closed")
            for k, why in shut:
                out.append(f"         {k}  ({why})")
    for n in verified:
        out.append(f"   verified: {n}")
    for n, items in sorted(unbound.items()):
        for item_id, _label, claim in items:
            out.append(f"   NOT VERIFIED: {n} {item_id} claims {claim} row(s), names no ledger key")
    if na:
        out.append(f"   not applicable: {len(na)} phase(s) name no ledger key in an open item")
    return "\n".join(out)


def self_test(quiet=True):
    """Negative control plus the clean shapes. Returns the failure count.

    Synthetic documents against a synthetic ledger written through the REAL
    loader, so nothing here reads the live tree and nothing a sibling phase
    merges can turn it red.
    """
    shard = {
        "c:wait_for_publishers": {"verdict": "divergence", "why": "landed"},
        "rust:Node::get_node_names": {"verdict": "divergence", "why": "landed"},
        "c:still_open": {"verdict": "gap", "why": "nobody did it"},
        "cpp:Gone": {"verdict": "gap", "why": "the whole type"},
        "rust:Old::name": {"verdict": "rename", "why": "x", "retired": "renamed in #1"},
    }
    g1 = ("# P\n\n**Status.** In flight.\n\n"
          "**G1 — graph forwarders. 2 rows (C 1, Rust 1). CHEAP.** Covers\n"
          "`c:wait_for_publishers` and `rust:Node::get_node_names`.\n")
    cases = [
        # The negative control: phase-417's G1 shape, keys stated, both closed.
        ("open-item-whose-rows-landed-is-STALE", g1, "stale"),
        ("same-item-closed-by-a-LANDED-line-is-not-a-claim",
         g1 + "\n**G1 LANDED 2026-09-21** in abc1234.\n", "not-applicable"),
        ("closed-in-its-own-heading-label",
         "# P\n\n## W0 — measured (2026-10-21, landed)\n\n`c:wait_for_publishers`\n",
         "not-applicable"),
        ("open-item-whose-rows-are-still-gap-is-VERIFIED",
         "# P\n\n**G2 — one row. CHEAP.** `c:still_open`.\n", "verified"),
        ("one-closed-one-open-is-still-a-finding",
         "# P\n\n**G2 — two rows.** `c:still_open`, `c:wait_for_publishers`.\n", "stale"),
        ("no-row-at-all-is-closed",
         "# P\n\n**G3 — x.** `cpp:Never::recorded`.\n", "stale"),
        ("retired-row-is-closed",
         "# P\n\n**G3 — x.** `rust:Old::name`.\n", "stale"),
        ("member-inheriting-a-gap-type-is-open",
         "# P\n\n**G3 — x.** `cpp:Gone::member`.\n", "verified"),
        ("ticked-box-key-is-claimed-done",
         "# P\n\n## W1 — x\n\n- [x] `c:wait_for_publishers`\n- [ ] `c:still_open`\n", "verified"),
        ("key-in-prose-outside-any-item-is-not-a-claim",
         "# P\n\n## Why\n\n`c:wait_for_publishers` shipped.\n", "not-applicable"),
        ("no-ledger-key-is-NOT-APPLICABLE",
         "# P\n\n## W1 — build the thing\n\nNo keys.\n", "not-applicable"),
    ]
    failures = 0
    with tempfile.TemporaryDirectory() as tmp:
        # Through the ledger's own writer (issue 1091: one file per row), so
        # the planted ledger has the layout the real one does.
        _api_parity().ledger_files.write_rows(tmp, "other", shard, doc=["self-test"])
        ledger = load_ledger(tmp)
        for name, doc, want in cases:
            got = check_doc(doc, ledger)[0]
            if got != want:
                print(f"  self-test FAIL: {name} — got {got}, want {want}")
                failures += 1
            elif not quiet:
                print(f"  ok    {name}")
        # phase-417's G1-G5 BEFORE the fix: a count, no key. Reported, not passed.
        _, _, unbound = check_doc(
            "# P\n\n**G4 — matched counts. 5 rows (C 2, C++ 2, Rust 1). CHEAP.** Thin.\n",
            ledger)
        if [(u[0], u[2]) for u in unbound] != [("G4", 5)]:
            print(f"  self-test FAIL: a row count with no key is UNBOUND — got {unbound}")
            failures += 1
        elif not quiet:
            print("  ok    a-row-count-with-no-key-is-UNBOUND")
    return failures


if __name__ == "__main__":
    # `python3 scripts/lib/roadmap_ledger_claims.py [phase.md ...]` -- the
    # self-test, then the report over the active phases or the named docs (an
    # archived phase is named explicitly; the default scope is the active
    # series, the same one every phase-419 rule reads). Exit 1 only if the
    # self-test fails: a finding is not a failure, this is a report.
    import sys

    n = self_test(quiet=False)
    print("roadmap_ledger_claims self-test: " + ("OK" if not n else f"FAILED ({n})"))
    if n:
        sys.exit(1)
    print(render(audit(sys.argv[1:] or None)))
