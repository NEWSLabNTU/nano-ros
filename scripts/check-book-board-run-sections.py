#!/usr/bin/env python3
"""Every supported board says how to run it, in the book — phase-517 W9.

WHAT THIS ASSERTS

`nros build` ends at an artifact (there is no `nros run` / `nros flash`), so
the book is the one place a reader learns which command boots the image on
QEMU / a simulator and which flashes a real board. For every board row of
`packages/boards/board-support.toml` at tier 1, 2 or 3:

  * the row names its run block, `book_run = "<chapter>.md#<anchor>"` — a page
    under `book/src/` and the id mdbook gives one of its headings;
  * a row with `real_hardware = true` names its flash block, `book_flash`, the
    same way (and needs no run block when nothing emulates it — S32Z270);
  * a row with `execution_class = "hardware"` states `real_hardware`, true or
    false, because that class means "CI cannot boot it", which is true of a
    physical board AND of a licence-free model CI does not run (the ARM FVP),
    and only the first is flashed;

and each named block must exist:

  * the chapter is tracked and listed in `book/src/SUMMARY.md`;
  * one of its headings has that mdbook id;
  * that heading reads `Run …` (book_run) or `Flash …` (book_flash);
  * the section under it, up to the next heading of the same or a higher
    level, holds at least one fenced code block — the command.

The rows and the chapters are both READ, never listed here. A board row whose
chapter the table of contents marks `(…, dormant)` or `(…, retired)` (the ESP32
pages, issue 1525) is skipped, with the same SUMMARY derivation
`check-book-links.py` and `check-book-board-names.py` use; today every ESP32
row is `scaffold` anyway, which no tier promise reaches.

The commands themselves are not executed here; that is phase-517 W10's
per-chapter probe.

Run:  python3 scripts/check-book-board-run-sections.py [--selftest]

The self-test runs on every invocation and must FAIL a registry whose board's
run block was removed from its chapter.
"""

import argparse
import pathlib
import re
import subprocess
import sys

try:
    import tomllib
except ModuleNotFoundError:  # 3.10 backport, same spelling as the sibling gates
    import tomli as tomllib

ROOT = pathlib.Path(__file__).resolve().parent.parent
REGISTRY = "packages/boards/board-support.toml"
SUMMARY = "book/src/SUMMARY.md"

TIERS = {"1", "2", "3"}
HEADING = re.compile(r"^(#{1,6})\s+(.*?)\s*#*\s*$")
FENCE = re.compile(r"^\s*(```|~~~)")
PARKED = re.compile(r"\[[^\]]*\((?:[^)]*,\s*)?(?:dormant|retired)\)\]\(\./([^)]+)\)", re.I)
LISTED = re.compile(r"\]\(\./([^)#]+)\)")


def mdbook_id(text: str) -> str:
    """mdbook's `normalize_id` over a heading's text (tags and backticks gone)."""
    text = re.sub(r"<[^>]*>", "", text).replace("`", "")
    out = []
    for ch in text:
        if ch.isalnum() or ch in "_-":
            out.append(ch.lower())
        elif ch.isspace():
            out.append("-")
    return "".join(out)


def sections(page: str) -> dict[str, tuple[str, str]]:
    """{id: (heading text, section body)} for every heading outside a fence.

    Repeated ids get mdbook's `-1`, `-2` suffixes."""
    lines = page.splitlines()
    heads = []
    in_fence = False
    for n, line in enumerate(lines):
        if FENCE.match(line):
            in_fence = not in_fence
            continue
        if in_fence:
            continue
        m = HEADING.match(line)
        if m:
            heads.append((n, len(m.group(1)), m.group(2)))
    out: dict[str, tuple[str, str]] = {}
    seen: dict[str, int] = {}
    for i, (n, level, text) in enumerate(heads):
        end = len(lines)
        for n2, level2, _ in heads[i + 1:]:
            if level2 <= level:
                end = n2
                break
        base = mdbook_id(text)
        k = seen.get(base, 0)
        seen[base] = k + 1
        hid = base if k == 0 else f"{base}-{k}"
        out[hid] = (text, "\n".join(lines[n + 1:end]))
    return out


def has_fence(body: str) -> bool:
    return any(FENCE.match(line) for line in body.splitlines())


def check_ref(where: str, key: str, ref, verb: str, pages: dict[str, str],
              listed: set[str]) -> list[str]:
    if not isinstance(ref, str) or "#" not in ref:
        return [f"{where}: `{key}` must be \"<chapter>.md#<anchor>\" (got {ref!r})"]
    chapter, anchor = ref.split("#", 1)
    rel = f"book/src/{chapter}"
    if rel not in pages:
        return [f"{where}: `{key}` names {rel}, which is not a tracked book page"]
    if chapter not in listed:
        return [f"{where}: `{key}` names {rel}, which SUMMARY.md does not list — "
                f"mdbook would not render it"]
    secs = sections(pages[rel])
    if anchor not in secs:
        near = sorted(h for h in secs if h.startswith(verb.lower()))
        return [f"{where}: `{key}` = {ref!r}: {rel} has no heading with id `{anchor}`"
                + (f" (its `{verb} …` headings: {', '.join(near)})" if near else
                   f" — and it has no `{verb} …` heading at all")]
    text, body = secs[anchor]
    errs = []
    if not text.replace("`", "").lower().startswith(verb.lower()):
        errs.append(f"{where}: `{key}` = {ref!r}: heading \"{text}\" must start with "
                    f"\"{verb}\"")
    if not has_fence(body):
        errs.append(f"{where}: `{key}` = {ref!r}: the section under \"{text}\" has no "
                    f"fenced command block")
    return errs


def run(registry: dict, pages: dict[str, str]) -> tuple[list[str], int]:
    summary = pages.get(SUMMARY, "")
    parked = set(PARKED.findall(summary))
    listed = set(LISTED.findall(summary))
    errors: list[str] = []
    checked = 0
    for row in registry.get("board", []):
        if str(row.get("tier", "")) not in TIERS:
            continue
        where = f"{REGISTRY}: {row.get('crate', '<no crate>')}@{row.get('matrix_platform') or '-'}"
        refs = [row.get(k) for k in ("book_run", "book_flash")]
        chapters = {r.split("#", 1)[0] for r in refs if isinstance(r, str)}
        if chapters and chapters <= parked:
            continue  # a dormant chapter — parked text, kept unchanged on purpose
        checked += 1
        real = row.get("real_hardware")
        if row.get("execution_class") == "hardware" and not isinstance(real, bool):
            errors.append(
                f"{where}: `execution_class = \"hardware\"` row must state "
                f"`real_hardware = true|false` — a physical board is flashed, a model "
                f"CI does not run (FVP) is run; the class alone cannot say which")
        if real is not None and not isinstance(real, bool):
            errors.append(f"{where}: `real_hardware` must be a boolean")
        if "book_run" in row:
            errors += check_ref(where, "book_run", row["book_run"], "Run", pages, listed)
        elif real is not True:
            errors.append(
                f"{where}: tier {row.get('tier')} board names no run block — add "
                f"`book_run = \"<chapter>.md#<anchor>\"` pointing at the chapter's "
                f"\"Run on QEMU / simulator\" section")
        if "book_flash" in row:
            errors += check_ref(where, "book_flash", row["book_flash"], "Flash", pages,
                                listed)
        elif real is True:
            errors.append(
                f"{where}: `real_hardware = true` but no `book_flash` — a physical "
                f"board's chapter needs a \"Flash a real board\" section (a build-only "
                f"board says so there)")
    return errors, checked


def selftest(verbose: bool = False) -> int:
    summary = ("- [A](./getting-started/a.md)\n- [B](./getting-started/b.md)\n"
               "- [E (esp, dormant)](./getting-started/e.md)\n")
    good_a = ("# A\n\n## Run on QEMU\n\n```sh\nqemu-system-arm -kernel x\n```\n\n"
              "## Flash a real board\n\nBuild-only.\n\n```sh\nnros build hw\n```\n")
    pages = {SUMMARY: summary, "book/src/getting-started/a.md": good_a,
             "book/src/getting-started/b.md": "# B\n",
             "book/src/getting-started/e.md": "# E\n"}
    qemu = {"crate": "q", "tier": 2, "execution_class": "cross-run",
            "book_run": "getting-started/a.md#run-on-qemu"}
    hw = {"crate": "h", "tier": 3, "execution_class": "hardware", "real_hardware": True,
          "book_flash": "getting-started/a.md#flash-a-real-board"}

    def errs(rows, pg=None):
        return run({"board": rows}, pg or pages)[0]

    no_run = dict(pages)
    no_run["book/src/getting-started/a.md"] = good_a.split("## Run on QEMU")[0] + \
        "## Flash a real board\n\n```sh\nx\n```\n"
    no_fence = dict(pages)
    no_fence["book/src/getting-started/a.md"] = "# A\n\n## Run on QEMU\n\nJust run it.\n"
    fence_in_next = dict(pages)
    fence_in_next["book/src/getting-started/a.md"] = (
        "# A\n\n## Run on QEMU\n\nprose\n\n## Other\n\n```sh\nx\n```\n")
    sub_heading = dict(pages)
    sub_heading["book/src/getting-started/a.md"] = (
        "# A\n\n## Run on QEMU\n\n### Step 1\n\n```sh\nx\n```\n")
    cases = [
        ("good rows pass", errs([qemu, hw]), False),
        ("a board's run block removed from its chapter fails", errs([qemu], no_run), True),
        ("a row naming no run block fails",
         errs([{k: v for k, v in qemu.items() if k != "book_run"}]), True),
        ("a run section with no command fails", errs([qemu], no_fence), True),
        ("a fence under the NEXT heading does not count", errs([qemu], fence_in_next), True),
        ("a fence under a sub-heading counts", errs([qemu], sub_heading), False),
        ("a real board with no flash block fails",
         errs([{k: v for k, v in hw.items() if k != "book_flash"}]), True),
        ("hardware class without real_hardware fails",
         errs([{k: v for k, v in hw.items() if k != "real_hardware"}]), True),
        ("a model (hardware class, real_hardware=false) needs a run block",
         errs([{"crate": "f", "tier": 3, "execution_class": "hardware",
                "real_hardware": False}]), True),
        ("an unlisted chapter fails",
         errs([dict(qemu, book_run="getting-started/z.md#run-on-qemu")]), True),
        ("a heading not starting with Run fails",
         errs([dict(qemu, book_run="getting-started/a.md#flash-a-real-board")]), True),
        ("a scaffold row is not read", errs([{"crate": "s", "tier": "scaffold"}]), False),
        ("a dormant chapter's row is skipped",
         errs([dict(qemu, book_run="getting-started/e.md#run")]), False),
    ]
    failed = 0
    for name, got, want_fail in cases:
        if bool(got) != want_fail:
            failed += 1
            print(f"check-book-board-run-sections: SELFTEST FAILED: {name} -> {got}",
                  file=sys.stderr)
        elif verbose:
            print(f"  ok: {name}")
    if mdbook_id("Run on QEMU (RISC-V `rv-virt`)") != "run-on-qemu-risc-v-rv-virt":
        failed += 1
        print("check-book-board-run-sections: SELFTEST FAILED: mdbook id", file=sys.stderr)
    if failed:
        return 1
    if verbose:
        print("check-book-board-run-sections: selftest OK")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--selftest", action="store_true")
    args = ap.parse_args()
    if args.selftest:
        return selftest(verbose=True)
    if selftest():
        return 1

    registry = tomllib.loads((ROOT / REGISTRY).read_text())
    listed = subprocess.run(
        ["git", "ls-files", "book/src/**/*.md", "book/src/*.md"],
        cwd=ROOT, capture_output=True, text=True, check=True,
    ).stdout.split()
    pages = {rel: (ROOT / rel).read_text() for rel in listed}
    errors, checked = run(registry, pages)
    if not checked:
        print(f"check-book-board-run-sections: read NO tier 1-3 rows from {REGISTRY}",
              file=sys.stderr)
        return 1
    if errors:
        print(f"check-book-board-run-sections: {len(errors)} board(s) the book does not "
              f"say how to run or flash:", file=sys.stderr)
        for e in errors:
            print(f"  {e}", file=sys.stderr)
        print(
            "\n`nros build` prints the next command and stops; the book is where a reader"
            "\nlearns it. Each tier 1-3 row names its chapter's \"Run …\" section"
            "\n(`book_run`), and a physical board its \"Flash …\" section (`book_flash`).",
            file=sys.stderr,
        )
        return 1
    print(f"check-book-board-run-sections: OK ({checked} tier 1-3 board row(s))")
    return 0


if __name__ == "__main__":
    sys.exit(main())
