#!/usr/bin/env python3
"""Each book holds only its reader's pages (RFC-0123 D8, phase-532 W8).

Folds `check-book-page-map` (every page has one map row, the map is
well-formed, both SUMMARY.md files are what the map renders) and adds:

A. **One URL, one book.** The Internals book is served at `internals/` under
   the user book (D7), so no user page may live under `book/src/internals/`:
   both books would then claim the same URL prefix.
B. **No contributor material in the user book** (D3). A "**Contributors:**"
   lead or a heading that names contributors is the shape contributor material
   took when it lived inside user chapters; it belongs in Internals now.
C. **The generated, gitignored user pages obey the user-surface rules.**
   `reference/static-pool-inventory.md` and `reference/configuration-surface.md`
   are not tracked, so the tracked-file gates never saw them. They are rendered
   in memory (`--stdout`, so parallel gates never race on the file) and checked
   for internal ids, `just` recipes and checkout steps with the same rules as
   `check-user-surface-ids`, `check-book-no-just` and `check-book-no-checkout`.

D. **The user book teaches the user tier** (RFC-0117 D2, RFC-0123 D9): no
   `nros dev …` and no old spelling a CLI alias table still accepts. The
   spellings are READ from the CLI source (`user::ALIASES`, `dev::ALIASES`),
   so a new alias is covered without editing this gate.
E. **No board fact typed in prose** (RFC-0114 D12, RFC-0123 D9): no tier
   number, and none of the registry's generated status words (or the legacy
   table statuses "Verified" / "Ready" / "Untested") outside the generated
   pages and regions. A board's status is the registry's to say.

`migrating/` is exempt from D and E: naming what was retired is its job.

The self-test runs on every invocation. Exit 0 when clean; 1 with one line
per problem otherwise.
"""

from __future__ import annotations

import importlib.util
import pathlib
import re
import subprocess
import sys

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE / "lib"))
import book_pages  # noqa: E402

ROOT = book_pages.ROOT
CONTRIB_LEAD = re.compile(r"^\s*(?:>\s*)*\*\*Contributors?\b", re.I)
CONTRIB_HEADING = re.compile(r"^#{1,6}\s.*\bcontributors?\b", re.I)
FENCE = re.compile(r"^\s*(?:```|~~~)")


def load(name: str, path: pathlib.Path):
    spec = importlib.util.spec_from_file_location(name, path)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


CLI_SRC = ROOT / "packages/cli/nros-cli-core/src/cmd"
ALIAS_ROW = re.compile(r'\(\s*"([^"]+)"\s*,\s*"[^"]+"\s*\)')
GEN_BEGIN = re.compile(r"<!--\s*BEGIN GENERATED")
GEN_END = re.compile(r"<!--\s*END GENERATED")
TIER = re.compile(r"\b[Tt]ier[- ]?[0-9]\b")
STATUS = re.compile(r"\b(?:Verified (?:in emulator|on host|on hardware)|Boots, manual)\b")
LEGACY_CELL = re.compile(r"\|\s*\**(?:Verified|Ready|Untested)\**\s*[¹²³⁴⁵⁶⁷⁸⁹]*\s*\|")


def old_spellings() -> list[str]:
    """Old command paths the CLI still accepts as hidden aliases, read from
    the `ALIASES` tables of `user.rs` and `dev.rs`."""
    out = []
    for f in ("user.rs", "dev.rs"):
        path = CLI_SRC / f
        if not path.exists():
            continue
        text = path.read_text()
        i = text.find("pub const ALIASES")
        if i < 0:
            continue
        block = text[i:text.find("];", i)]
        out += ALIAS_ROW.findall(block)
    return sorted(set(out), key=len, reverse=True)


# A `probe=NN` fence is EXECUTED by `just probe` in a clean container, so its
# command cannot change without re-running its probe track. The `nros dev`
# steps still inside probe fences are front-door gaps (no user verb yet: the
# SDK manifest), listed in phase-532 W9. They are counted, not exempted: the
# count may only fall.
PROBE_FENCE_BASELINE = 0
PROBE_FENCE = re.compile(r"^\s*(?:>\s*)*(?:```|~~~)\S*.*\bprobe=")


def rule_d(rel: str, text: str, olds: list[str], probe_hits: list[str] | None = None) -> list[str]:
    if "/migrating/" in rel:
        return []
    pat = re.compile(r"\bnros (?:dev\b|(?:" + "|".join(re.escape(o) for o in olds) + r")(?![\w-]))") if olds \
        else re.compile(r"\bnros dev\b")
    out, fence, probe = [], False, False
    for n, line in enumerate(text.splitlines(), 1):
        if FENCE.match(re.sub(r"^\s*(?:>\s*)*", "", line)):
            fence, probe = (not fence), (not fence and bool(PROBE_FENCE.match(line)))
            continue
        m = pat.search(line)
        if not m:
            continue
        msg = f"{rel}:{n}: `{m.group(0)}` is not the user tier — use its user spelling, or move it to Internals"
        if fence and probe and probe_hits is not None:
            probe_hits.append(msg)
        else:
            out.append(msg)
    return out


def rule_e(rel: str, text: str, generated: bool) -> list[str]:
    if generated or "/migrating/" in rel:
        return []
    out, in_gen = [], False
    for n, line in enumerate(text.splitlines(), 1):
        if GEN_BEGIN.search(line):
            in_gen = True
        elif GEN_END.search(line):
            in_gen = False
            continue
        if in_gen:
            continue
        for rx, what in ((TIER, "a tier number"), (STATUS, "a registry status word"), (LEGACY_CELL, "a typed support status")):
            m = rx.search(line)
            if m:
                out.append(f"{rel}:{n}: {what} typed in prose ({m.group(0).strip()!r}) — link the generated board page instead")
    return out


def rule_a(user_paths: list[str]) -> list[str]:
    return [f"{p}: a user page under book/src/internals/ shares the Internals book's URL prefix (RFC-0123 D7)"
            for p in user_paths if p.startswith("book/src/internals/")]


def rule_b(rel: str, text: str) -> list[str]:
    out, in_fence = [], False
    for n, line in enumerate(text.splitlines(), 1):
        if FENCE.match(line):
            in_fence = not in_fence
            continue
        if in_fence:
            continue
        if CONTRIB_LEAD.match(line) or CONTRIB_HEADING.match(line):
            out.append(f"{rel}:{n}: contributor material in the user book — move it to Internals: {line.strip()[:80]}")
    return out


def page_body(raw: str) -> str:
    """Drop a generator's self-test chatter before the page itself."""
    lines = raw.splitlines(keepends=True)
    for i, line in enumerate(lines):
        if line.startswith(("<!--", "#")):
            return "".join(lines[i:])
    return raw


def rule_c(ids_mod, nojust_re, checkout_mod) -> list[str]:
    out = []
    for p in book_pages.load():
        if p.book != "user" or not p.generated or not p.old:
            continue
        tracked = subprocess.run(["git", "ls-files", "--error-unmatch", f"book/src/{p.old}"], cwd=ROOT,
                                 capture_output=True).returncode == 0
        if tracked:
            continue  # the tracked-file gates already read it
        r = subprocess.run([sys.executable, str(ROOT / p.generated), "--stdout"], cwd=ROOT,
                           capture_output=True, text=True)
        if r.returncode != 0:
            out.append(f"{p.generated} --stdout failed: {r.stderr.strip()[:200]}")
            continue
        rel, text = f"book/src/{p.path}", page_body(r.stdout)
        out += ids_mod.book_page_hits(rel, text)
        out += [f"{rel}:{n}: `just` on the user track: {line.strip()[:80]}"
                for n, line in enumerate(text.splitlines(), 1) if nojust_re.search(line)]
        out += checkout_mod.scan_page(rel, text)
        out += rule_b(rel, text)
    return out


def self_test() -> None:
    assert rule_a(["book/src/internals/x.md", "book/src/realtime/x.md"]) == [
        "book/src/internals/x.md: a user page under book/src/internals/ shares the Internals book's URL prefix (RFC-0123 D7)"]
    page = ("# T\n\n> **Contributors:** run the lane\n\n## Contributor setup\n\n"
            "```sh\n# **Contributors** in a fence is code\n```\n\nThanks to our contributors.\n")
    hits = rule_b("p.md", page)
    assert len(hits) == 2 and "p.md:3:" in hits[0] and "p.md:5:" in hits[1], hits
    assert page_body("gen self-test: OK\n<!-- G -->\n# P\n") == "<!-- G -->\n# P\n"
    olds = ["ws list", "init", "generate"]
    # The CLI is spelled "nros" + " " + verb here so check-cli-old-spellings
    # does not read this fixture as old spellings in the tree.
    cli = "nros" + " "
    d = rule_d("book/src/a.md", f"run `{cli}dev sdk root`\n`{cli}ws list`\n`{cli}generate-px4-msgs`\n`{cli}init`\n", olds)
    assert [h.split(":")[1] for h in d] == ["1", "2", "4"], d
    assert rule_d("book/src/migrating/x.md", f"{cli}init", olds) == []
    ph: list[str] = []
    assert rule_d("book/src/a.md", "```sh probe=20\nnros dev sdk root\n```\nnros dev x\n", olds, ph) and len(ph) == 1
    e = rule_e("book/src/a.md", "a Tier 1 board\n<!-- BEGIN GENERATED x -->\n| b | Verified |\n<!-- END GENERATED x -->\n"
               "| c | Verified ¹ |\nVerified in emulator, daily\nverified by hand\n", False)
    assert [h.split(":")[1] for h in e] == ["1", "5", "6"], e
    assert rule_e("book/src/reference/boards/x.md", "Tier 1", True) == []


def main() -> int:
    self_test()
    errs: list[str] = []
    pm = load("check_book_page_map", HERE / "check-book-page-map.py")
    if pm.main() != 0:
        errs.append("check-book-page-map failed (above)")
    user_paths = book_pages.tracked_user_pages()
    errs += rule_a(user_paths)
    olds = old_spellings()
    generated = {f"book/src/{p.path}" for p in book_pages.load() if p.generated} | \
                {f"book/src/{p.old}" for p in book_pages.load() if p.generated and p.old}
    probe_hits: list[str] = []
    for rel in user_paths:
        text = (ROOT / rel).read_text()
        errs += rule_b(rel, text)
        errs += rule_d(rel, text, olds, probe_hits)
        errs += rule_e(rel, text, rel in generated)
    if len(probe_hits) > PROBE_FENCE_BASELINE:
        errs += probe_hits
        errs.append(f"{len(probe_hits)} dev-tier step(s) in probe fences, baseline {PROBE_FENCE_BASELINE}: "
                    "a probe fence gained one")
    elif len(probe_hits) < PROBE_FENCE_BASELINE:
        print(f"check-book-audience: {len(probe_hits)} dev-tier step(s) in probe fences — lower "
              f"PROBE_FENCE_BASELINE from {PROBE_FENCE_BASELINE}")
    ids_mod = load("check_user_surface_ids", HERE / "check" / "check-user-surface-ids.py")
    nojust = load("check_book_no_just", HERE / "check-book-no-just.py")
    checkout = load("check_book_no_checkout", HERE / "check-book-no-checkout.py")
    nojust_re = re.compile(nojust.INLINE_CMD.pattern + "|" + nojust.FENCE_CMD.pattern)
    errs += rule_c(ids_mod, nojust_re, checkout)
    if errs:
        print(f"check-book-audience: {len(errs)} problem(s):")
        for e in errs:
            print(f"  {e}")
        return 1
    print(f"check-book-audience: OK ({len(user_paths)} user page(s) plus the generated ones; one URL, one book)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
