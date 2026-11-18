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

Several blocks of one chapter may share a `probe=NN`: they are one step, run in
page order (a command, the file it writes, the command after). A block tagged
`file=PATH` or `append=PATH` is a file the reader writes, not a command: the
script writes its body to PATH (scripts/mdbook-file-blocks.py shows PATH above
it in the book).

A step that never exits (a QEMU boot, a router) is `probe=NN probe-run=SECS`:
it runs in its own session, and must print — within SECS — every line of the
chapter's output block tagged `probe-expect=NN`, so the documented output is
the assertion (phase-517 W10; scripts/probe/probe-run.sh). `probe-keep` leaves
it running for the steps after it. `variant=<v>` selects one of several runs a
chapter documents (`--variant`).

A block inside a language tab group (`<!-- tabs -->` … `<!-- /tabs -->`,
scripts/lib/book_tabs.py) belongs to its tab's language: a tabbed page holds
one step per language under one `probe=NN`, and `--lang rust|c|cpp` picks the
track's. Untabbed blocks apply to every language. A tabbed step with no
`--lang`, or a `--lang` that selects no tabbed block, is an error.

Usage:
  extract-book-steps.py --out probe-steps.sh \
      [--subst 'OLD:::NEW' ...] [--variant V] [--lang L] [--prelude FILE ...] \
      chapter1.md chapter2.md ...
  extract-book-steps.py --self-test
"""

import argparse
import re
import shlex
import subprocess
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "lib"))
import book_tabs  # noqa: E402  the language tab groups (RFC-0124 D9)

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
# A step can be a FILE the reader writes, shown as the file itself rather than
# a `cat > path <<'EOF'` command: ```` ```yaml probe=NN file=manifest/west.yml ````
# (or `append=PATH` for text added to the end of a file). The block's body is
# the file's content; the probe writes it there, making the directory first.
# The book renders the path above the block (scripts/mdbook-file-blocks.py).
# `edit=PATH` on a ```diff block changes part of an existing file: lines
# starting with a space are context, `-` lines are replaced by `+` lines. The
# context and `-` lines must occur in the file exactly once.
FILE_TOKEN_RE = re.compile(r"(?:^|\s)(file|append|edit)=(\S+)(?:\s|$)")

# The edit applier, run as `python3 -c` with the hunk on stdin. No single
# quote in it: the script quotes it in single quotes.
EDIT_APPLIER = """import sys
p = sys.argv[1]
h = sys.stdin.read().split(chr(10))
h = h[:-1] if h and h[-1] == "" else h
old, new = [], []
for ln in h:
    t, b = (ln[:1], ln[1:]) if ln else (" ", "")
    if t not in " +-":
        sys.exit("probe edit: %s: a hunk line starts with a space, - or +: %r" % (p, ln))
    if t in " -":
        old.append(b)
    if t in " +":
        new.append(b)
s = open(p).read()
a = chr(10).join(old)
n = s.count(a)
if n != 1:
    sys.exit("probe edit: %s: the lines to change occur %d times, not once" % (p, n))
open(p, "w").write(s.replace(a, chr(10).join(new)))
"""


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
        "file": FILE_TOKEN_RE.search(info),
    }


def file_step(body: list[str], mode: str, path: str, where: str) -> str:
    """The shell that writes a `file=` / `append=` block's body to its path,
    or applies an `edit=` block's hunk to it."""
    tag = "NROS_PROBE_FILE"
    if any(ln == tag for ln in body):
        sys.exit(f"{where}: file block contains the heredoc tag {tag}")
    q = shlex.quote(path)
    if mode == "edit":
        tag = "NROS_PROBE_EDIT"
        if any(ln == tag for ln in body):
            sys.exit(f"{where}: edit block contains the heredoc tag {tag}")
        return "\n".join([f"python3 -c '{EDIT_APPLIER}' {q} <<'{tag}'", *body, tag])
    op = ">>" if mode == "append" else ">"
    return "\n".join([f"mkdir -p \"$(dirname {q})\"", f"cat {op} {q} <<'{tag}'", *body, tag])


def expected_text(line: str) -> str:
    """What a run must print for one line of an expected-output block. The
    whole line, or, when it holds `…` (a value that changes per run, such as a
    message counter: `data: 'Hello World: …'`), the text before the `…`."""
    return line.split("…", 1)[0].strip()


def extract_blocks(path: Path):
    """Return (steps, expects) for one chapter.

    steps:   [{order, lineno, body, distros, tracks, variants, run}]
    expects: [{order, lineno, lines, variants}] — `probe-expect=NN` blocks
    """
    steps, expects = [], []
    in_fence = False
    cur = None
    body = []
    text = path.read_text()
    try:
        langs = book_tabs.line_langs(text)
    except book_tabs.TabError as e:
        sys.exit(f"{path}:{e.line}: {e}")
    for lineno, line in enumerate(text.splitlines(), 1):
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
                cur["lang"] = langs[lineno - 1]
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
                    if cur["file"]:
                        if cur["run"] is not None:
                            sys.exit(f"{path}:{cur['lineno']}: a file block cannot be a `probe-run` step")
                        cur["body"] = file_step(body, cur["file"].group(1), cur["file"].group(2),
                                                f"{path}:{cur['lineno']}")
                    else:
                        cur["body"] = "\n".join(body)
                    steps.append(cur)
                else:
                    # Every non-blank line is a substring the run must print;
                    # a line that is only `...` / `…` stands for "and so on".
                    # A `…` inside a line ends what must match (expected_text).
                    cur["lines"] = [
                        expected_text(ln) for ln in body
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


def order_body(steps, order, where):
    """One `probe=NN` step's shell, for a caller that reads a page's setup
    steps itself rather than through `main` (the starter image's context
    writer, `scripts/ci/lib/image-context.sh`).

    The same rule `main` applies: every block of the page under one order is
    ONE step, joined in page order, so a command, the `file=` block it writes
    and the command that uses it stay one step however the page splits them
    (#3989 split the native_sim starter's west workspace step into three and
    the image's own one-block reader refused the release's image). A block
    inside a language tab is refused, since a reader that picks no language
    cannot choose between tabs, and so is a step with no block at all."""
    found = [s for s in steps if s.get("kind", "step") == "step" and s["order"] == order]
    if not found:
        sys.exit(f"{where}: no probe={order} block")
    tabbed = [s for s in found if s.get("lang")]
    if tabbed:
        sys.exit(f"{where}:{tabbed[0]['lineno']}: probe={order} is inside a language tab; "
                 "a setup step every language shares must be untabbed")
    runs = [s for s in found[:-1] if s.get("run") is not None]
    if runs:
        sys.exit(f"{where}:{runs[0]['lineno']}: step {order} continues after its `probe-run` "
                 "block; only a step's last block may run")
    return "\n".join(s["body"] for s in found)


def main():
    if sys.argv[1:] == ["--self-test"]:
        self_test()
        return
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
    # phase-537 W2 (RFC-0124 D9) — the language a track replays a tabbed page
    # in. Blocks in another language's tab are skipped; untabbed ones kept.
    ap.add_argument("--lang", default=None, choices=sorted(book_tabs.LABELS.values()),
                    help="which language tab of a tabbed page; blocks in another "
                         "language's tab are skipped, untabbed ones kept")
    # phase-517 W10 — shell text placed after `set -euo pipefail`, ahead of
    # every step: the helpers the emitted run steps call. The container has no
    # checkout to source them from, so they travel inside the script.
    ap.add_argument("--prelude", action="append", default=[],
                    help="FILE whose text is placed before the first step")
    # phase-525 W4 (RFC-0116 D1) — the field budget is a WALL CLOCK from the
    # reader's first `nros setup` to the chapter's last asserted line. The
    # clock starts before the first step with a line beginning with TEXT, and
    # every step after it reports the elapsed time (nros_probe_clock_report in
    # probe-run.sh), failing once it passes SECS. Requires the helper prelude.
    ap.add_argument("--clock-from", default=None, metavar="TEXT",
                    help="start the clock before the first step with a line "
                         "beginning with TEXT")
    ap.add_argument("--clock-budget", type=int, default=None, metavar="SECS",
                    help="with --clock-from: fail the step that ends past SECS")
    # phase-537 W5 (RFC-0124 D3) — a Tutorial track replays the chapters before
    # its own, on one growing project. A reader starts each chapter with the
    # last one's terminals closed, so the processes a chapter left running
    # (`probe-keep`: the talker under `nros run`, its router) are stopped
    # before the first step of the next chapter, never left to hold its port.
    ap.add_argument("--close-between-chapters", action="store_true",
                    help="stop the steps a chapter left running (probe-keep) "
                         "before the first step of the next chapter")
    ap.add_argument("files", nargs="+")
    args = ap.parse_args()
    if (args.clock_from is None) != (args.clock_budget is None):
        sys.exit("probe extract: --clock-from and --clock-budget go together")

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
    lang_seen = False
    lang_unpicked = []  # tabbed blocks met with no --lang
    for f in args.files:
        p = Path(f)
        if not p.is_file():
            sys.exit(f"probe extract: no such chapter: {f}")
        file_steps, file_expects = extract_blocks(p)
        for e in file_expects:
            if e["lang"] is not None:
                if args.lang is None:
                    lang_unpicked.append(f"{f}:{e['lineno']}")
                    continue
                if args.lang != e["lang"]:
                    continue
                lang_seen = True
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
            if b["lang"] is not None:
                if args.lang is None:
                    lang_unpicked.append(f"{f}:{lineno}")
                    continue
                if args.lang != b["lang"]:
                    continue
                lang_seen = True
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
            if steps and steps[-1][0] == order and steps[-1][1] == f:
                # Several blocks of one chapter under one `probe=NN` are one
                # step, run in page order: a command, the file it needs (a
                # `file=` block), the command that uses it. Only the last may
                # be a `probe-run` step.
                prev = steps[-1]
                if prev[4] is not None:
                    sys.exit(f"{f}:{lineno}: step {order} continues after its `probe-run` "
                             "block; only a step's last block may run")
                steps[-1] = (order, f, prev[2], prev[3] + "\n" + body, b["run"], b["keep"])
                continue
            steps.append((order, f, lineno, body, b["run"], b["keep"]))

    if args.variant is not None and not variant_seen:
        sys.exit(
            f"probe extract: --variant {args.variant} selects no block in "
            f"{args.files} — a typo here would silently probe the default run"
        )
    if lang_unpicked:
        sys.exit(
            f"probe extract: block(s) at {', '.join(lang_unpicked)} sit in a language "
            "tab, and no --lang picks one — set the track's `lang` in the registry"
        )
    if args.lang is not None and not lang_seen:
        sys.exit(
            f"probe extract: --lang {args.lang} selects no tabbed block in "
            f"{args.files} — the track names a language its chapter does not tab"
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
    clock_on = False
    if args.clock_budget is not None:
        out.append(f"PROBE_CLOCK_BUDGET={args.clock_budget}")
    prev_file = None
    for order, f, lineno, body, run, keep in steps:
        out.append("")
        if args.close_between_chapters and prev_file is not None and f != prev_file:
            out.append(f"echo '=== probe: {Path(prev_file).name} ends; its terminals close ==='")
            out.append("nros_probe_stop_kept")
        prev_file = f
        if (args.clock_from is not None and not clock_on
                and any(ln.lstrip().startswith(args.clock_from) for ln in body.splitlines())):
            out.append(f"nros_probe_clock_start {order}")
            clock_on = True
        if run is None:
            out.append(f"echo '=== probe step {order} ({f}:{lineno}) ==='")
            # No `-u` inside a step either, for the run blocks' reason below: a
            # step that sources a ROS `setup.bash` (which reads unset variables
            # such as AMENT_TRACE_SETUP_FILES) must work as it does in a
            # reader's terminal. The harness's own lines keep `-u`.
            out.append("set +u")
            out.append(body)
            out.append("set -u")
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
        if clock_on:
            out.append(f"nros_probe_clock_report {order}")
        if order in after:
            check_file, check_text = after[order]
            out.append(f"echo '=== probe check after step {order} ({Path(check_file).name}) ==='")
            out.append(check_text.rstrip("\n"))
    if args.clock_from is not None and not clock_on:
        sys.exit(
            f"probe extract: --clock-from {args.clock_from!r} matches no step line — "
            "the clock would never start and the budget would go unchecked"
        )
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


def self_test() -> None:
    """A tabbed chapter extracts each language's step and output under one
    order; an untabbed chapter is untouched by --lang's absence; the misuses
    fail."""
    book_tabs.self_test()
    tabbed = (
        "# T\n\n```bash probe=10\nnros setup linux\n```\n\n<!-- tabs -->\n\n#### Rust\n\n"
        "```bash probe=20\ncargo build\n```\n\n```bash probe=30 probe-run=5\n./rust-talker\n```\n\n"
        "```text probe-expect=30\nrust says hi\n```\n\n#### C\n\n```bash probe=20\ncmake --build build\n```\n\n"
        "```bash probe=30 probe-run=5\n./build/c-talker\n```\n\n```text probe-expect=30\nc says hi\n```\n\n"
        "<!-- /tabs -->\n\n```bash probe=40\necho done\n```\n"
    )
    plain = "# P\n\n```bash probe=10\necho one\n```\n"
    me = Path(__file__).resolve()

    def extract(text: str, *args: str) -> tuple[int, str]:
        with tempfile.TemporaryDirectory() as d:
            (Path(d) / "c.md").write_text(text)
            r = subprocess.run([sys.executable, str(me), "--out", f"{d}/o.sh", *args, f"{d}/c.md"],
                               capture_output=True, text=True)
            out = (Path(d) / "o.sh").read_text() if r.returncode == 0 else r.stderr
            return r.returncode, out.replace(d, "<d>")

    rc, rust = extract(tabbed, "--lang", "rust")
    assert rc == 0, rust
    assert "cargo build" in rust and "./rust-talker" in rust and "'rust says hi'" in rust, rust
    assert "cmake" not in rust and "c-talker" not in rust and "c says hi" not in rust, rust
    assert "nros setup linux" in rust and "echo done" in rust
    rc, c = extract(tabbed, "--lang", "c")
    assert rc == 0 and "cmake --build build" in c and "'c says hi'" in c and "cargo" not in c, c
    rc, err = extract(tabbed)
    assert rc != 0 and "sit in a language tab" in err, err
    rc, err = extract(tabbed, "--lang", "cpp")
    assert rc != 0 and "selects no tabbed block" in err, err
    rc, err = extract(plain, "--lang", "rust")
    assert rc != 0 and "selects no tabbed block" in err, err
    rc, one = extract(plain)
    assert rc == 0 and "echo one" in one, one
    rc, err = extract("<!-- tabs -->\n#### Go\n```bash probe=1\nx\n```\n<!-- /tabs -->\n")
    assert rc != 0 and "c.md:2:" in err and "is not one of" in err, err
    # phase-537 W5 — a chained Tutorial track closes a chapter's kept steps
    # before the next chapter's first step, and only between chapters.
    with tempfile.TemporaryDirectory() as d:
        (Path(d) / "a.md").write_text("```bash probe=10\necho a1\n```\n```bash probe=11\necho a2\n```\n")
        (Path(d) / "b.md").write_text("```bash probe=20\necho b1\n```\n")
        r = subprocess.run([sys.executable, str(me), "--out", f"{d}/o.sh", "--close-between-chapters",
                            f"{d}/a.md", f"{d}/b.md"], capture_output=True, text=True)
        assert r.returncode == 0, r.stderr
        o = (Path(d) / "o.sh").read_text()
        assert o.count("nros_probe_stop_kept") == 1, o
        assert o.index("echo a2") < o.index("nros_probe_stop_kept") < o.index("echo b1"), o
    # A file block writes its body to its path; `append=` adds to the end.
    rc, fb = extract("```yaml probe=10 file=manifest/west.yml\nmanifest:\n  self: {}\n```\n"
                     "```text probe=20 append=a b/prj.conf\nCONFIG_X=y\n```\n")
    assert rc == 0, fb
    assert "mkdir -p \"$(dirname manifest/west.yml)\"\ncat > manifest/west.yml <<'NROS_PROBE_FILE'\n" \
        "manifest:\n  self: {}\nNROS_PROBE_FILE\n" in fb, fb
    rc, fb = extract("```text probe=20 append=b/prj.conf\nCONFIG_X=y\n```\n")
    assert rc == 0 and "cat >> b/prj.conf <<'NROS_PROBE_FILE'\nCONFIG_X=y\n" in fb, fb
    rc, err = extract("```yaml probe=10 probe-run=5 file=x.yml\na: 1\n```\n")
    assert rc != 0 and "cannot be a `probe-run` step" in err, err
    # Blocks of one chapter sharing a `probe=NN` are one step, in page order.
    rc, ms = extract("```bash probe=10\nmkdir -p m\n```\ntext\n```yaml probe=10 file=m/w.yml\na: 1\n```\n"
                     "```bash probe=10\nwest init -l m\n```\n```bash probe=11\necho next\n```\n")
    assert rc == 0 and ms.count("=== probe step 10 ") == 1, ms
    assert ms.index("mkdir -p m") < ms.index("cat > m/w.yml") < ms.index("west init") < ms.index("echo next"), ms
    rc, err = extract("```bash probe=10 probe-run=5\nqemu\n```\n```text probe-expect=10\nhi\n```\n"
                      "```bash probe=10\necho after\n```\n")
    assert rc != 0 and "only a step's last block may run" in err, err
    # An edit block's hunk is applied: `-` lines replaced by `+` lines.
    rc, ed = extract("```diff probe=10 edit=b/system.toml\n [image.x]\n-locator = \"serial/UART_0\"\n"
                     "+locator = \"tcp/10.0.2.2:7447\"\n```\n")
    assert rc == 0 and "python3 -c" in ed, ed
    with tempfile.TemporaryDirectory() as d:
        (Path(d) / "b").mkdir()
        (Path(d) / "b/system.toml").write_text('[image.x]\nlocator = "serial/UART_0"\nrmw = "zenoh"\n')
        body = ed.split("set +u\n", 1)[1].rsplit("\nset -u", 1)[0]
        r = subprocess.run(["bash", "-c", body], cwd=d, capture_output=True, text=True)
        assert r.returncode == 0, r.stderr
        # system.toml own text: the selftest reads back the file it wrote.
        got = (Path(d) / "b/system.toml").read_text()
        assert got == '[image.x]\nlocator = "tcp/10.0.2.2:7447"\nrmw = "zenoh"\n', got
        r = subprocess.run(["bash", "-c", body], cwd=d, capture_output=True, text=True)
        assert r.returncode != 0 and "occur 0 times" in r.stderr, r.stderr
    # `…` in an expected line: the text before it must print.
    rc, ex = extract("```bash probe=10 probe-run=5\nros2 topic echo /chatter\n```\n"
                     "```text probe-expect=10\ndata: 'Hello World: …'\n```\n")
    assert rc == 0 and shlex.quote("data: 'Hello World:") in ex, ex
    # order_body: a reader of setup steps gets the same one-step join.
    with tempfile.TemporaryDirectory() as d:
        pg = Path(d) / "p.md"
        pg.write_text("```bash probe=34\nmkdir -p m && cd m\n```\ntext\n"
                      "```yaml probe=34 file=manifest/west.yml\nmanifest: {}\n```\n"
                      "```bash probe=34\nwest init -l manifest\n```\n```bash probe=36\nnext\n```\n")
        st, _ = extract_blocks(pg)
        b34 = order_body(st, 34, str(pg))
        assert b34.index("mkdir -p m") < b34.index("cat > manifest/west.yml") < b34.index("west init"), b34
        assert "next" not in b34 and order_body(st, 36, str(pg)) == "next", b34
        for text, msg in (
            ("```bash probe=10\na\n```\n", "no probe=34 block"),
            ("<!-- tabs -->\n\n#### Rust\n\n```bash probe=34\na\n```\n\n#### C\n\n"
             "```bash probe=34\nb\n```\n\n<!-- /tabs -->\n", "inside a language tab"),
            ("```bash probe=34 probe-run=5\na\n```\n```bash probe=34\nb\n```\n",
             "only a step's last block may run"),
        ):
            pg.write_text(text)
            st, _ = extract_blocks(pg)
            try:
                order_body(st, 34, str(pg))
            except SystemExit as e:
                assert msg in str(e), (msg, e)
            else:
                raise AssertionError(f"order_body accepted a page it must refuse ({msg})")
    print("extract-book-steps: self-test OK")


if __name__ == "__main__":
    main()
