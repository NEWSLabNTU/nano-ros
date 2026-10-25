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

A step that never exits (a QEMU boot, a router) is `probe=NN probe-run=SECS`:
it runs in its own session, and must print — within SECS — every line of the
chapter's output block tagged `probe-expect=NN`, so the documented output is
the assertion (phase-517 W10; scripts/probe/probe-run.sh). `probe-keep` leaves
it running for the steps after it. `variant=<v>` selects one of several runs a
chapter documents (`--variant`).

Usage:
  extract-book-steps.py --out probe-steps.sh \
      [--subst 'OLD:::NEW' ...] [--variant V] [--prelude FILE ...] \
      chapter1.md chapter2.md ...
"""

import argparse
import re
import shlex
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
# phase-517 W10 — one chapter can document SEVERAL runs of one platform (NuttX
# boots on Arm `virt` and on RISC-V `rv-virt`; each is its own probe track). A
# block tagged `variant=rv-virt` is extracted only for `--variant rv-virt`; an
# untagged block applies to every variant. Like `track=`, a variant with no
# block is not an error — but a `--variant` that selects NOTHING in the
# chapters is, because that is a typo silently probing the default run.
VARIANT_TOKEN_RE = re.compile(r"(?:^|\s)variant=([A-Za-z0-9_,+-]+)(?:\s|$)")
# phase-517 W10 — a Run step NEVER EXITS (QEMU publishes until killed), so it
# cannot run inline. `probe=NN probe-run=SECS` marks such a block: it runs in
# its own session and must print, within SECS, every line of the chapter's
# output block tagged `probe-expect=NN` (the "Expected output" a reader
# compares against). The documented output IS the assertion.
RUN_TOKEN_RE = re.compile(r"(?:^|\s)probe-run=(\d+)(?:\s|$)")
KEEP_TOKEN_RE = re.compile(r"(?:^|\s)probe-keep(?:\s|$)")
EXPECT_TOKEN_RE = re.compile(r"(?:^|\s)probe-expect=(\d+)(?:\s|$)")


def _tokens(info: str, path: Path, lineno: int):
    """Parse the probe tokens of one fence info string."""
    dtok = DISTRO_TOKEN_RE.search(info)
    ttok = TRACK_TOKEN_RE.search(info)
    vtok = VARIANT_TOKEN_RE.search(info)
    rtok = RUN_TOKEN_RE.search(info)
    tracks = set(ttok.group(1).split(",")) if ttok else None
    if tracks is not None and not tracks <= TRACKS:
        sys.exit(
            f"{path}:{lineno}: unknown probe track(s) "
            f"{sorted(tracks - TRACKS)} (known: {sorted(TRACKS)})"
        )
    return {
        "distros": set(dtok.group(1).split(",")) if dtok else None,
        "tracks": tracks,
        "variants": set(vtok.group(1).split(",")) if vtok else None,
        "run": int(rtok.group(1)) if rtok else None,
        "keep": bool(KEEP_TOKEN_RE.search(info)),
    }


def extract_blocks(path: Path):
    """Return (steps, expects) for one chapter.

    steps:   [{order, lineno, body, distros, tracks, variants, run}]
    expects: [{order, lineno, lines, variants}] — `probe-expect=NN` blocks
    """
    steps, expects = [], []
    in_fence = False
    cur = None
    body = []
    for lineno, line in enumerate(path.read_text().splitlines(), 1):
        m = FENCE_RE.match(line)
        if not in_fence:
            info = m.group(1) if (m and m.group(1)) else None
            tok = PROBE_TOKEN_RE.search(info) if info else None
            etok = EXPECT_TOKEN_RE.search(info) if info else None
            if tok and etok:
                sys.exit(f"{path}:{lineno}: a block is a probe step OR an expected output, not both")
            if tok or etok:
                in_fence = True
                cur = _tokens(info, path, lineno)
                cur["lineno"] = lineno
                if tok:
                    cur["kind"] = "step"
                    cur["order"] = int(tok.group(1))
                else:
                    cur["kind"] = "expect"
                    cur["order"] = int(etok.group(1))
                    if cur["run"] is not None or cur["tracks"] or cur["distros"]:
                        sys.exit(
                            f"{path}:{lineno}: an expected-output block takes only "
                            "`probe-expect=NN` (and `variant=`) — its step carries the rest"
                        )
                body = []
            elif line.startswith("```"):
                # untagged fence — skip to its close so an inner
                # ``` line can't be mistaken for an opener
                in_fence = "skip"
        elif in_fence == "skip":
            if line.startswith("```") and not (m and m.group(1)):
                in_fence = False
        else:
            if line.startswith("```"):
                if cur["kind"] == "step":
                    cur["body"] = "\n".join(body)
                    steps.append(cur)
                else:
                    # Every non-blank line is a substring the run must print;
                    # a line that is only `...` / `…` stands for "and so on".
                    cur["lines"] = [
                        ln.strip() for ln in body
                        if ln.strip() and ln.strip() not in ("...", "…")
                    ]
                    if not cur["lines"]:
                        sys.exit(f"{path}:{lineno}: probe-expect block states no output")
                    expects.append(cur)
                in_fence = False
            else:
                body.append(line)
    if in_fence:
        sys.exit(f"{path}: unterminated fence at line {cur['lineno']}")
    return steps, expects


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
    ap.add_argument("--variant", default=None,
                    help="which documented run of a multi-run chapter; blocks "
                         "tagged variant=<other> are skipped, untagged ones kept")
    # phase-517 W10 — shell text placed after `set -euo pipefail`, ahead of
    # every step: the helpers the emitted run steps call. The container has no
    # checkout to source them from, so they travel inside the script.
    ap.add_argument("--prelude", action="append", default=[],
                    help="FILE whose text is placed before the first step")
    ap.add_argument("files", nargs="+")
    args = ap.parse_args()

    after = {}  # order -> (file, text)
    for spec in args.after_step:
        n, sep, f = spec.partition("=")
        if not sep or not n.isdigit():
            sys.exit(f"probe extract: bad --after-step (want N=FILE): {spec}")
        after[int(n)] = (f, Path(f).read_text())

    steps = []  # (order, file, lineno, body, run)
    expects = {}  # order -> (file, lineno, lines)
    skipped = {}  # order -> distros it IS tagged for, when filtered out
    variant_seen = False
    for f in args.files:
        p = Path(f)
        if not p.is_file():
            sys.exit(f"probe extract: no such chapter: {f}")
        file_steps, file_expects = extract_blocks(p)
        for e in file_expects:
            if e["variants"] is not None:
                if args.variant not in e["variants"]:
                    continue
                variant_seen = True
            if e["order"] in expects:
                sys.exit(f"probe extract: two probe-expect={e['order']} blocks ({f}:{e['lineno']})")
            expects[e["order"]] = (f, e["lineno"], e["lines"])
        for b in file_steps:
            order, lineno, body = b["order"], b["lineno"], b["body"]
            distros, tracks, variants = b["distros"], b["tracks"], b["variants"]
            if tracks is not None and args.track not in tracks:
                continue
            if variants is not None:
                if args.variant not in variants:
                    continue
                variant_seen = True
            if distros is not None and args.distro not in distros:
                # Remember that this ORDER exists for some distro, so a
                # requested distro nobody tagged fails loudly below instead of
                # silently dropping the step (a probe missing its host-prereq
                # step still "passes" on an image that happens to ship the
                # packages — the exact false green this filter could create).
                skipped.setdefault(order, set()).update(distros)
                continue
            if b["keep"] and b["run"] is None:
                sys.exit(f"{f}:{lineno}: `probe-keep` needs `probe-run=SECS`")
            steps.append((order, f, lineno, body, b["run"], b["keep"]))

    if args.variant is not None and not variant_seen:
        sys.exit(
            f"probe extract: --variant {args.variant} selects no block in "
            f"{args.files} — a typo here would silently probe the default run"
        )
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
    run_orders = {s[0] for s in steps if s[4] is not None}
    unexpected = sorted(run_orders - set(expects))
    if unexpected:
        sys.exit(
            f"probe extract: run step(s) {unexpected} have no `probe-expect=NN` "
            "output block — a run with nothing to assert would pass by starting"
        )
    orphans = sorted(set(expects) - run_orders)
    if orphans:
        sys.exit(
            f"probe extract: probe-expect block(s) {orphans} name no `probe-run` "
            "step in this extraction — the documented output would go unchecked"
        )

    out = []
    out.append("#!/usr/bin/env bash")
    out.append("# GENERATED by scripts/probe/extract-book-steps.py — DO NOT EDIT.")
    out.append("# Source of truth: the probe=NN fenced blocks in the book chapters below.")
    out.append("set -euo pipefail")
    for order, f, lineno, body, run, keep in steps:
        out.append("")
        if run is None:
            out.append(f"echo '=== probe step {order} ({f}:{lineno}) ==='")
            out.append(body)
        else:
            ef, eline, lines = expects[order]
            out.append(
                f"echo '=== probe step {order} ({f}:{lineno}), run for up to {run}s, "
                f"expecting the output at {ef}:{eline} ==='"
            )
            tag = f"NROS_PROBE_RUN_{order}"
            if tag in body:
                sys.exit(f"{f}:{lineno}: step body contains the heredoc tag {tag}")
            out.append(f"cat >/tmp/probe-run-{order}.sh <<'{tag}'")
            # No `-u`: a run block commonly sources a ROS `setup.bash`, which is
            # not nounset-clean — and a reader's terminal has no `-u` either.
            out.append("set -eo pipefail")
            out.append(body)
            out.append(tag)
            quoted = " ".join(shlex.quote(ln) for ln in lines)
            out.append(f"nros_probe_run_expect {order} {run} {int(keep)} /tmp/probe-run-{order}.sh {quoted}")
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

    if args.prelude:
        head = "set -euo pipefail\n"
        prelude = "".join(Path(f).read_text().rstrip("\n") + "\n" for f in args.prelude)
        script = script.replace(head, head + prelude, 1)

    Path(args.out).write_text(script)
    print(f"probe extract: {len(steps)} steps -> {args.out}")


if __name__ == "__main__":
    main()
