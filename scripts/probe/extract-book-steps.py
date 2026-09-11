#!/usr/bin/env python3
"""Extract `probe=NN`-tagged fenced code blocks from book chapters into one
bash script (issue #204 — clean-system bootstrap probe).

The book is the single source of truth for setup steps: a fenced block whose
info string carries a `probe=NN` token (e.g. ```` ```sh probe=20 ````) is a
probe step. mdBook ignores the extra token, so the rendered page is
unchanged. Blocks are concatenated in ascending NN order (NN unique across
all scanned files) into a script that runs in ONE shell, so `cd` /
`source` state carries between steps — exactly like the reader's terminal.

Substitutions (`--subst 'LITERAL:::REPLACEMENT'`) exist for the few spots
where the book text can't run verbatim (the pinned release tag in the clone
line, the `<board>` placeholder). Each substitution must match EXACTLY ONCE
across the extracted steps — if a book edit breaks the pattern, extraction
fails loudly instead of silently probing something else.

Usage:
  extract-book-steps.py --out probe-steps.sh \
      [--subst 'OLD:::NEW' ...] chapter1.md chapter2.md ...
"""

import argparse
import re
import sys
from pathlib import Path

FENCE_RE = re.compile(r"^```+\s*(\S.*)?$")
PROBE_TOKEN_RE = re.compile(r"(?:^|\s)probe=(\d+)(?:\s|$)")
# issue 0373 — a step can be distro-specific (the host-prereq block is
# apt/dnf/pacman). `distro=arch` (or a comma list) restricts a block to those
# hosts; an untagged block applies everywhere. Two blocks may then share a
# probe order, as long as no two of them survive the same --distro filter.
DISTRO_TOKEN_RE = re.compile(r"(?:^|\s)distro=([A-Za-z0-9_,+-]+)(?:\s|$)")
# phase-447 A3 — a step can also be specific to HOW the reader got `nros`.
# `installation.md` documents two front doors that must never run in one shell:
# the contributor's clone + bootstrap (`track=checkout`) and the user's release
# install (`track=installed`). The installed track exists to prove a build with
# NO checkout on the machine, so a checkout step leaking into it would make it
# pass for exactly the reason the dead-end it gates survived (RFC-0099 D1).
#
# Unlike `distro=`, a step missing for a track is NOT an error: the two flows
# legitimately have different steps. What IS an error is a track name nobody
# knows, because a typo there silently drops the step from every track.
TRACK_TOKEN_RE = re.compile(r"(?:^|\s)track=([A-Za-z0-9_,+-]+)(?:\s|$)")
TRACKS = {"checkout", "installed"}


def extract_blocks(path: Path):
    """Yield (order, lineno, body, distros, tracks) for each probe-tagged fence."""
    blocks = []
    in_fence = False
    order = None
    start = None
    distros = None
    tracks = None
    body = []
    for lineno, line in enumerate(path.read_text().splitlines(), 1):
        m = FENCE_RE.match(line)
        if not in_fence:
            if m and m.group(1):
                tok = PROBE_TOKEN_RE.search(m.group(1))
                if tok:
                    in_fence = True
                    order = int(tok.group(1))
                    start = lineno
                    dtok = DISTRO_TOKEN_RE.search(m.group(1))
                    distros = (
                        set(dtok.group(1).split(",")) if dtok else None
                    )
                    ttok = TRACK_TOKEN_RE.search(m.group(1))
                    tracks = set(ttok.group(1).split(",")) if ttok else None
                    if tracks is not None and not tracks <= TRACKS:
                        sys.exit(
                            f"{path}:{lineno}: unknown probe track(s) "
                            f"{sorted(tracks - TRACKS)} (known: {sorted(TRACKS)})"
                        )
                    body = []
                elif line.startswith("```"):
                    # untagged fence — skip to its close so an inner
                    # ``` line can't be mistaken for an opener
                    in_fence = "skip"
            elif line.startswith("```"):
                in_fence = "skip"
        elif in_fence == "skip":
            if line.startswith("```") and not (m and m.group(1)):
                in_fence = False
        else:
            if line.startswith("```"):
                blocks.append((order, start, "\n".join(body), distros, tracks))
                in_fence = False
            else:
                body.append(line)
    if in_fence:
        sys.exit(f"{path}: unterminated fence at line {start}")
    return blocks


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", required=True)
    ap.add_argument("--subst", action="append", default=[],
                    help="LITERAL:::REPLACEMENT, must match exactly once")
    ap.add_argument("--distro", default="debian",
                    help="host distro the steps are extracted FOR; blocks tagged "
                         "distro=<other> are skipped (default: debian)")
    ap.add_argument("--track", default="checkout", choices=sorted(TRACKS),
                    help="how the reader got `nros`; blocks tagged track=<other> "
                         "are skipped (default: checkout)")
    # phase-447 A3 — a probe-owned CHECK between two book steps. A verifier
    # appended at the end only runs if every step before it succeeds, so two
    # different defects that both stop the run early (a regressed SDK root, an
    # unprovisionable source) would report as ONE failure at the same step.
    # Checking a property the moment it becomes checkable keeps each defect
    # failing at its own step. The named step must exist in this extraction.
    ap.add_argument("--after-step", action="append", default=[],
                    help="N=FILE: append FILE's text right after step N's block")
    ap.add_argument("files", nargs="+")
    args = ap.parse_args()

    after = {}  # order -> (file, text)
    for spec in args.after_step:
        n, sep, f = spec.partition("=")
        if not sep or not n.isdigit():
            sys.exit(f"probe extract: bad --after-step (want N=FILE): {spec}")
        after[int(n)] = (f, Path(f).read_text())

    steps = []  # (order, file, lineno, body)
    skipped = {}  # order -> distros it IS tagged for, when filtered out
    for f in args.files:
        p = Path(f)
        if not p.is_file():
            sys.exit(f"probe extract: no such chapter: {f}")
        for order, lineno, body, distros, tracks in extract_blocks(p):
            if tracks is not None and args.track not in tracks:
                continue
            if distros is not None and args.distro not in distros:
                # Remember that this ORDER exists for some distro, so a
                # requested distro nobody tagged fails loudly below instead of
                # silently dropping the step (a probe missing its host-prereq
                # step still "passes" on an image that happens to ship the
                # packages — the exact false green this filter could create).
                skipped.setdefault(order, set()).update(distros)
                continue
            steps.append((order, f, lineno, body))

    if not steps:
        sys.exit(
            f"probe extract: no probe=NN blocks for distro '{args.distro}' — "
            "book tags removed, or every block is tagged for another distro?"
        )
    orders = [s[0] for s in steps]
    dropped = sorted(o for o in skipped if o not in orders)
    if dropped:
        detail = "; ".join(
            f"step {o} exists only for {'/'.join(sorted(skipped[o]))}" for o in dropped
        )
        sys.exit(
            f"probe extract: no block for distro '{args.distro}' at "
            f"step(s) {dropped} — {detail}. Tag a block for this distro in the "
            "book, or probe one of the distros above."
        )
    missing_after = sorted(set(after) - set(orders))
    if missing_after:
        sys.exit(
            f"probe extract: --after-step names step(s) {missing_after}, which this "
            "extraction does not have — the book's tags moved; the check would "
            "silently never run"
        )
    dupes = {o for o in orders if orders.count(o) > 1}
    if dupes:
        sys.exit(f"probe extract: duplicate probe order(s): {sorted(dupes)}")
    steps.sort(key=lambda s: s[0])

    out = []
    out.append("#!/usr/bin/env bash")
    out.append("# GENERATED by scripts/probe/extract-book-steps.py — DO NOT EDIT.")
    out.append("# Source of truth: the probe=NN fenced blocks in the book chapters below.")
    out.append("set -euo pipefail")
    for order, f, lineno, body in steps:
        out.append("")
        out.append(f"echo '=== probe step {order} ({f}:{lineno}) ==='")
        out.append(body)
        if order in after:
            check_file, check_text = after[order]
            out.append(f"echo '=== probe check after step {order} ({Path(check_file).name}) ==='")
            out.append(check_text.rstrip("\n"))
    script = "\n".join(out) + "\n"

    for s in args.subst:
        if ":::" not in s:
            sys.exit(f"probe extract: bad --subst (no ':::'): {s}")
        old, new = s.split(":::", 1)
        n = script.count(old)
        if n != 1:
            sys.exit(
                f"probe extract: --subst pattern matched {n} times "
                f"(want exactly 1): {old!r}\n"
                "The book text drifted from what the probe expects — "
                "update the substitution in run-bootstrap-probe.sh or the book."
            )
        script = script.replace(old, new)

    Path(args.out).write_text(script)
    print(f"probe extract: {len(steps)} steps -> {args.out}")


if __name__ == "__main__":
    main()
