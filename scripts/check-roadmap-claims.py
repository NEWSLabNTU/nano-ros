#!/usr/bin/env python3
"""An active roadmap phase must not contradict itself.

phase-419 W2.

`check-roadmap-status.sh` asserts that a phase HAS a status line and says why it
stops there: "it does not judge whether the status is CURRENT, only that one
exists ... Staleness of the CONTENT is a human call."

That is right for most of what a stale phase hides and wrong for one part of it,
and the boundary is sharp. When a header says "nothing landed" and the body
carries ticked work items, no judgment is involved -- the document contradicts
ITSELF, and a reader has to open it to find that out. When a phase is superseded
because a later phase reversed its direction, judgment is all there is. This gate
takes only the first kind. phase-419 W3 owns the second, as a report and never as
a gate.

Measured 2026-09-04, sweeping the 13 oldest open phases: 8 had a status line that
had outlived its content. Of the 11 findings, 7 were mechanical and 4 needed
judgment. Three rules cover the mechanical ones this gate can see on its own;
the fourth (a cited path that no longer exists) is `check-ci-doc-workflow-refs`'s
job and was extended there rather than reimplemented here -- two gates answering
one question is how the api-parity pair came to disagree by 25 symbols.

RULES

R1  The header claims the phase has not started, the header does not itself
    admit progress, and the body carries TICKED work items.

    Keyed on ticked `- [x]` boxes, NOT on counting the word "LANDED". That is
    not a detail: phase-419's own doc says "PROPOSED -- nothing landed" in its
    header and contains 13 occurrences of LANDED, every one prose about OTHER
    phases. A first cut that counted words would have flagged the phase
    proposing this gate, on its first run. A ticked checkbox is a structural
    claim about THIS document's own work items; a word in a sentence is not.

    The "does not itself admit progress" clause is what keeps the rule usable.
    Without it the rule flags phase-375, whose header reads "PROPOSED -- W0
    landed, W1-W5 not started" -- honest self-disclosure. A gate that flags
    honesty is one people switch off.

R3  A phase's `**Owns:**` line names an issue that is RESOLVED or archived,
    without saying so.

    phase-356 is the case: its Owns line reads "[issue 0527] ..., [issue 0403]
    (resolved), [issue 0260]" -- 0403 annotated, 0260 not, and 0260 had been
    resolved and archived for weeks while W3's remainder was described as
    outstanding. The annotation is already the house convention; this asserts it.

R4  A document in `docs/roadmap/` that says it is not a work-item phase.

    phase-275-276 read "NOT A WORK-ITEM PHASE ... it carries no open acceptance
    criteria of its own" while sitting in the active series. Branch notes filed
    as a phase; one line to catch, and it recurs.

RATCHET, not an absolute. The baseline may only SHRINK. A gate that demanded
zero on its first run would be switched off, and the four R1 hits standing today
are findings for a person to work, not debt to hide.
"""

import os
import re
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
ROADMAP = os.path.join("docs", "roadmap")
BASELINE = os.path.join(".config", "roadmap-claims-baseline.txt")

# A status line in any of the shapes `check-roadmap-status.sh` accepts. Kept in
# step with that gate deliberately: two different ideas of where a status lives
# would make one of them wrong about every phase.
STATUS = re.compile(r"^\s*(?:\*\*|##\s*)?Status\b", re.I)

# The header claims nothing has started.
NOT_STARTED = re.compile(r"not started|nothing landed|\bplanned\b|\bproposed\b|\bdraft\b", re.I)

# ...and does not itself say otherwise. Checked on the SAME text, so a header
# that discloses partial progress is never flagged.
ADMITS = re.compile(
    r"landed|\bdone\b|complete|in progress|partial|superseded|closed|"
    r"w\d+[^.]{0,20}(landed|done)",
    re.I,
)

TICKED = re.compile(r"^\s*[-*]\s*\[x\]", re.I | re.M)
# A bold field label opening a line: `**Implements:**`, `**Blocked on:**`.
NEW_FIELD = re.compile(r"^\s*\*\*[A-Z][^*]{0,40}[:.]\*\*")
OWNS = re.compile(r"^\*\*Owns[:.]?\*\*(.*?)(?=^\*\*|\Z)", re.M | re.S)
ISSUE_LINK = re.compile(r"\[([^\]]*?)\]\(([^)]*?/(\d{4})-[a-z0-9-]+\.md)\)")
NOT_A_PHASE = re.compile(r"NOT A WORK-ITEM PHASE", re.I)


def status_line(text):
    for line in text.split("\n"):
        if STATUS.match(line):
            return line
    return ""


def header_block(text):
    """The status line plus its continuation.

    A status is routinely two or three wrapped lines -- phase-375's "PROPOSED --
    W0 landed, W1-W5 not started" fits on one, but phase-419's does not, and
    reading only the first line would lose the admission and flag it.
    """
    lines = text.split("\n")
    for i, line in enumerate(lines):
        if STATUS.match(line):
            out = [line]
            for nxt in lines[i + 1 :]:
                if not nxt.strip():
                    break
                # A new bold FIELD ends the status, even with no blank line
                # between them. phase-325 runs Status / Implements / Successor
                # to / Informed by / Blocked on as five contiguous lines, and
                # its "W0-W2 are done" sits in **Blocked on:** -- a different
                # field. Reading it as part of the status made the header look
                # like it admitted progress, and the phase (whose status says
                # "Draft. Not started." over 14 ticked boxes) went unflagged.
                if NEW_FIELD.match(nxt):
                    break
                out.append(nxt)
            return " ".join(out)
    return ""


def issue_status(root, path):
    """`status:` from an issue's frontmatter, or None when unreadable."""
    full = os.path.join(root, path)
    if not os.path.exists(full):
        # An archived issue is a resolved one whose link was not updated; the
        # missing path is `check-doc-refs`'s finding, not this gate's.
        return None
    try:
        with open(full, encoding="utf-8") as fh:
            head = fh.read(2000)
    except OSError:
        return None
    m = re.search(r"^status:\s*(\S+)", head, re.M)
    return m.group(1).strip() if m else None


def check(root):
    problems = []
    rdir = os.path.join(root, ROADMAP)
    if not os.path.isdir(rdir):
        return problems
    for name in sorted(os.listdir(rdir)):
        if not name.endswith(".md"):
            continue
        rel = os.path.join(ROADMAP, name)
        try:
            with open(os.path.join(rdir, name), encoding="utf-8") as fh:
                text = fh.read()
        except OSError:
            continue

        # ---- R4: a doc that says it is not a phase, filed as one -----------
        #
        # Searched in the STATUS field only, not the whole document. Searching
        # the body flagged phase-419 itself -- the phase that SPECIFIES this
        # rule quotes the phrase while describing it. That is the same trap as
        # counting the word LANDED: a gate that cannot tell a claim from a
        # sentence about a claim flags its own specification, which is exactly
        # what `check-workflow-repo-env` warns about ("a gate that cannot tell a
        # command from a sentence about a command is worse than no gate").
        #
        # phase-275 put it where a claim belongs: "**Status.** NOT A WORK-ITEM
        # PHASE - these are branch working notes".
        if NOT_A_PHASE.search(header_block(text)):
            problems.append(
                (rel, "R4", "says NOT A WORK-ITEM PHASE while in the active roadmap series")
            )

        # ---- R1: header says unstarted, body has ticked work items ---------
        head = header_block(text)
        ticked = len(TICKED.findall(text))
        if head and ticked and NOT_STARTED.search(head) and not ADMITS.search(head):
            problems.append(
                (
                    rel,
                    "R1",
                    f"header claims not-started ({status_line(text).strip()[:60]!r}) "
                    f"but {ticked} work item(s) are ticked",
                )
            )

        # ---- R3: an owned issue is resolved, unannotated --------------------
        for owns in OWNS.findall(text):
            for label, path, num in ISSUE_LINK.findall(owns):
                if "resolved" in label.lower() or "archived" in label.lower():
                    continue
                # The annotation is often placed after the link rather than in it.
                tail = owns[owns.find(path) + len(path) : owns.find(path) + len(path) + 40]
                if re.search(r"resolved|archived|closed", tail, re.I):
                    continue
                st = issue_status(root, path.replace("../", "docs/"))
                if st in ("resolved", "wontfix"):
                    problems.append(
                        (rel, "R3", f"Owns issue {num}, which is `status: {st}`, without saying so")
                    )
    return problems


def load_baseline(root):
    path = os.path.join(root, BASELINE)
    if not os.path.exists(path):
        return set()
    out = set()
    with open(path, encoding="utf-8") as fh:
        for line in fh:
            line = line.split("#", 1)[0].strip()
            if line:
                out.add(line)
    return out


def key(p):
    return f"{p[0]}\t{p[1]}"


def self_test(quiet=False):
    """Each case asserts a failure this gate must catch, plus the clean cases.

    Fixture documents rather than the live tree: every finding this gate was
    built from has since been CORRECTED, so asserting against `docs/roadmap/`
    would assert nothing. The two clean cases are the ones a first cut got
    wrong, and they are the reason the rule has its shape.
    """
    cases = [
        (
            "r1-contradiction",
            "phase-900-x.md",
            "# P\n\n**Status.** Planned\n\n- [x] a done thing\n",
            True,
        ),
        (
            "r1-honest-self-disclosure-is-NOT-a-finding",
            "phase-901-x.md",
            "# P\n\n**Status (2026-01-01). PROPOSED — W0 landed, W1–W5 not started.**\n\n- [x] W0\n",
            False,
        ),
        (
            "r1-word-LANDED-in-prose-is-NOT-a-finding",
            "phase-902-x.md",
            "# P\n\n**Status.** PROPOSED — nothing landed.\n\nphase-1 LANDED. phase-2 LANDED.\n",
            False,
        ),
        (
            "r1-genuinely-not-started-is-clean",
            "phase-903-x.md",
            "# P\n\n**Status.** Not Started.\n\n- [ ] a thing\n",
            False,
        ),
        (
            "r4-not-a-work-item-phase",
            "phase-904-x.md",
            "# P\n\n**Status.** NOT A WORK-ITEM PHASE — branch notes.\n",
            True,
        ),
        (
            # The gate flagged its OWN specification on its first live run:
            # phase-419 quotes this phrase while describing R4. Pinned so the
            # rule can never widen back to the whole document.
            "r4-the-phrase-in-PROSE-is-NOT-a-finding",
            "phase-905-x.md",
            "# P\n\n**Status.** In progress.\n\nR4 flags a doc that says "
            "NOT A WORK-ITEM PHASE in its status.\n",
            False,
        ),
    ]
    failures = 0
    with tempfile.TemporaryDirectory() as tmp:
        rd = os.path.join(tmp, ROADMAP)
        os.makedirs(rd)
        for name, fname, body, want in cases:
            for stale in os.listdir(rd):
                os.remove(os.path.join(rd, stale))
            with open(os.path.join(rd, fname), "w", encoding="utf-8") as fh:
                fh.write(body)
            got = bool(check(tmp))
            if got != want:
                print(f"  self-test FAIL: {name} — got {got}, want {want}")
                failures += 1
            elif not quiet:
                print(f"  ok    {name}")

        # R3 needs an issue tree beside the phase.
        for stale in os.listdir(rd):
            os.remove(os.path.join(rd, stale))
        idir = os.path.join(tmp, "docs", "issues")
        os.makedirs(idir, exist_ok=True)
        with open(os.path.join(idir, "0001-x.md"), "w", encoding="utf-8") as fh:
            fh.write("---\nid: 1\nstatus: resolved\n---\n")
        with open(os.path.join(rd, "phase-905-x.md"), "w", encoding="utf-8") as fh:
            fh.write("# P\n\n**Status.** In progress.\n\n**Owns:** [issue 0001](../issues/0001-x.md)\n")
        if not check(tmp):
            print("  self-test FAIL: r3-owns-a-resolved-issue — not caught")
            failures += 1
        elif not quiet:
            print("  ok    r3-owns-a-resolved-issue")

        with open(os.path.join(rd, "phase-905-x.md"), "w", encoding="utf-8") as fh:
            fh.write(
                "# P\n\n**Status.** In progress.\n\n"
                "**Owns:** [issue 0001](../issues/0001-x.md) (resolved)\n"
            )
        if check(tmp):
            print("  self-test FAIL: r3-annotated-resolved-is-clean — flagged anyway")
            failures += 1
        elif not quiet:
            print("  ok    r3-annotated-resolved-is-clean")

    if failures:
        print(f"check-roadmap-claims self-test: FAILED ({failures})")
        return 1
    return 0


def main(argv):
    if len(argv) > 1 and argv[1] == "--self-test":
        return self_test()

    # Always, not only behind the flag: a negative control nobody runs decays
    # into a comment, and this rule's whole job is to fire. Same shape as
    # `scripts/check-knob-delivery.py`; gated by `check-gate-selftests`.
    rc = self_test(quiet=True)
    if rc:
        return rc

    problems = check(ROOT)
    baseline = load_baseline(ROOT)
    keys = {key(p) for p in problems}

    if len(argv) > 1 and argv[1] == "--write-baseline":
        path = os.path.join(ROOT, BASELINE)
        with open(path, "w", encoding="utf-8") as fh:
            fh.write(
                "# phase-419 W2 — roadmap phases whose claims contradict their own\n"
                "# body. RATCHET: this list may only SHRINK. Each line is a finding\n"
                "# for a person to work, not debt to hide; fix the phase and delete\n"
                "# the line.\n"
            )
            for k in sorted(keys):
                fh.write(k + "\n")
        print(f"wrote {BASELINE} — {len(keys)} entr(ies)")
        return 0

    new = [p for p in problems if key(p) not in baseline]
    gone = sorted(baseline - keys)

    if new:
        print("check-roadmap-claims: a phase contradicts its own body:")
        for path, rule, why in new:
            print(f"  [{rule}] {path}")
            print(f"        {why}")
        print(
            "\n  A status line that has outlived its content is worse than none:\n"
            "  a reader cannot use it without re-deriving it. Correct the phase,\n"
            "  or -- if the claim is right and the shape is wrong -- say why in\n"
            f"  the doc. Accept a known one with `--write-baseline` ({BASELINE}).\n"
            "  phase-419."
        )
        return 1

    if gone:
        print("check-roadmap-claims: baseline entr(ies) no longer offend — delete them:")
        for g in gone:
            print(f"  {g}")
        print("\n  The baseline may only SHRINK, and it may not go stale.")
        return 1

    print(
        f"check-roadmap-claims: OK — {len(problems)} known finding(s), no new ones "
        f"across {len(os.listdir(os.path.join(ROOT, ROADMAP)))} active phase doc(s)."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
