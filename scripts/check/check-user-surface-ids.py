#!/usr/bin/env python3
"""check-user-surface-ids — the user surface speaks in actions, never internal ids.

phase-525 W1 (RFC-0116 D2). An adopter reads `nros --help`, a refusal from a
verb, and the getting-started chapters. None of those may name a phase, an
issue, a pull request or an RFC: the reader has no tree to look the id up in,
so an id where a sentence or a command belongs is a dead end. The phase or
issue that built a verb belongs in a `//` comment above its help string; a
refusal names the next command; a chapter states the fact the issue holds, or
moves the citation into a contributor block.

Subjects, all checked on every run:

* the refusal and message strings of `packages/cli/nros-cli-core/src/cmd/`:
  every Rust string literal outside comments and `#[cfg(test)]` modules (the
  `bail!`/`eyre!`/`anyhow!` texts, `println!`/`eprintln!` lines, and anything
  a refusal is built from);
* `book/src/getting-started/*.md`, with the contributor-block exemption
  `check-book-no-just` uses (`scripts/lib/book_contributor.py`, one rule for
  both gates). HTML comments do not render and are not read;
* the RENDERED help: `nros --help`, `nros help --all`, and `--help` of every
  verb and nested verb, hidden ones included (clap turns `///` doc comments
  into help text, so this is the only reading that sees what renders);
* the output of `nros board list`, `nros board show <board>` for every board
  the list names, and `nros doctor --report`, which in addition may hold no
  tier number (`tier 1`, `tier 2`, `tier 3` — phase-522 W5).

The rendered subjects need the in-tree CLI; with none built they are reported
NOT VERIFIED through the skip ledger (issue 1043's middle outcome), never a
quiet pass. The source and book subjects need nothing.

Forbidden tokens: `phase-NNN`, `Phase NNN`, `issue NNNN`, `#NNNN`, `RFC-NNNN`.
Baseline zero, no allowlist. `--self-test` runs both directions of every rule
and runs first on every check.
"""

from __future__ import annotations

import argparse
import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts" / "lib"))
import book_contributor  # noqa: E402
import check_skip  # noqa: E402

NAME = "check-user-surface-ids"
CMD_DIR = "packages/cli/nros-cli-core/src/cmd"
BOOK_GLOB = "book/src/getting-started/*.md"

ID = re.compile(
    r"\b[Pp]hase[- ]\d{2,4}\b"
    r"|\b[Ii]ssues?[- ]#?\d{3,4}\b"
    r"|(?<![\w&/#])#\d{3,4}\b"
    r"|\bRFC-?\d{4}\b"
)
TIER = re.compile(r"\b[Tt]ier[- ]?[123]\b")
HTML_COMMENT = re.compile(r"<!--.*?-->", re.S)
# A markdown link's TARGET does not render; its text does.
LINK_TARGET = re.compile(r"\]\([^)\s]*\)")


def ids(text: str, tiers: bool = False) -> list[str]:
    found = [m.group(0) for m in ID.finditer(text)]
    if tiers:
        found += [m.group(0) for m in TIER.finditer(text)]
    return found


# --- Rust string literals -------------------------------------------------


def rust_strings(src: str, spans: bool = False) -> list:
    """Every string literal in `src` with its 1-based line, skipping comments,
    char literals and `#[cfg(test)]` modules. Not a parser: enough of the
    lexer to tell a string from a comment, which is all this gate asks."""
    out: list[tuple[int, str]] = []
    i, n, line = 0, len(src), 1
    test_depth = None  # brace depth at which a cfg(test) module closes
    depth = 0
    pending_test = False
    while i < n:
        c = src[i]
        if c == "\n":
            line += 1
            i += 1
            continue
        if src.startswith("//", i):
            j = src.find("\n", i)
            i = n if j < 0 else j
            continue
        if src.startswith("/*", i):
            j = src.find("*/", i + 2)
            j = n if j < 0 else j + 2
            line += src.count("\n", i, j)
            i = j
            continue
        if src.startswith("#[cfg(test)]", i):
            pending_test = True
            i += len("#[cfg(test)]")
            continue
        m = re.compile(r"b?r(#*)\"").match(src, i)
        if m and (i == 0 or not (src[i - 1].isalnum() or src[i - 1] == "_")):
            close = '"' + m.group(1)
            j = src.find(close, m.end())
            j = n if j < 0 else j
            if test_depth is None:
                out.append((line, m.end(), j) if spans else (line, src[m.end() : j]))
            line += src.count("\n", i, j)
            i = j + len(close)
            continue
        if c == '"' or (c == "b" and src.startswith('b"', i)):
            start = i + (2 if c == "b" else 1)
            j = start
            while j < n and src[j] != '"':
                j += 2 if src[j] == "\\" else 1
            if test_depth is None:
                # A `\` line continuation joins the two lines in the value.
                value = re.sub(r"\\\n\s*", "", src[start:j])
                out.append((line, start, j) if spans else (line, value))
            line += src.count("\n", i, j)
            i = j + 1
            continue
        if c == "'":
            if src.startswith("\\", i + 1):
                j = src.find("'", i + 2)
                i = n if j < 0 else j + 1
                continue
            if i + 2 < n and src[i + 2] == "'":
                i += 3
                continue
            i += 1  # a lifetime
            continue
        if c == "{":
            depth += 1
            if pending_test and test_depth is None:
                test_depth = depth
            pending_test = False
        elif c == "}":
            if test_depth is not None and depth == test_depth:
                test_depth = None
            depth -= 1
        elif c == ";":
            pending_test = False  # `#[cfg(test)] use …;` / `mod tests;`
        i += 1
    return out


def source_hits() -> list[str]:
    files = subprocess.run(
        ["git", "ls-files", f"{CMD_DIR}/*.rs", f"{CMD_DIR}/**/*.rs"],
        cwd=ROOT, capture_output=True, text=True, check=True,
    ).stdout.split()
    bad = []
    for rel in sorted(set(files)):
        if Path(rel).name == "tests.rs" or "/tests/" in rel:
            continue
        for ln, s in rust_strings((ROOT / rel).read_text()):
            for tok in ids(s):
                bad.append(f"{rel}:{ln}: {tok!r} in {s.strip()[:90]!r}")
    return bad


# --- the book ---------------------------------------------------------------


def book_page_hits(rel: str, text: str) -> list[str]:
    # Blank out HTML comments (they do not render), keeping line numbers.
    text = HTML_COMMENT.sub(lambda m: "\n" * m.group(0).count("\n"), text)
    bad = []
    for ln in book_contributor.walk(text.splitlines()):
        if ln.exempt:
            continue
        for tok in ids(LINK_TARGET.sub("]", ln.text)):
            bad.append(f"{rel}:{ln.number}: {tok!r} in {ln.text.strip()[:90]!r}")
    return bad


def book_hits() -> list[str]:
    files = subprocess.run(
        ["git", "ls-files", BOOK_GLOB], cwd=ROOT, capture_output=True, text=True, check=True
    ).stdout.split()
    bad = []
    for rel in files:
        bad += book_page_hits(rel, (ROOT / rel).read_text())
    return bad


# --- the rendered surface ---------------------------------------------------

# clap prints commands under "Commands:" or under a `help_heading` group
# ("Build:", "Run on a target:"); arguments and options are not verbs.
GROUP_HEADER = re.compile(r"^([A-Z][\w ,/-]*):\s*$")
NOT_VERBS = {"Options", "Arguments", "Quick start"}
COMMAND_ROW = re.compile(r"^  ([a-z][a-z0-9-]*)(?:,\s*[a-z0-9-]+)*(?:\s|$)")


def subcommands(help_text: str) -> list[str]:
    out, inside, usage_seen = [], False, False
    for line in help_text.splitlines():
        if line.startswith("Usage:"):
            usage_seen = True
            continue
        if not usage_seen:
            continue
        m = GROUP_HEADER.match(line)
        if m:
            inside = m.group(1) not in NOT_VERBS
            continue
        if inside:
            if line and not line.startswith(" "):
                inside = False
                continue
            m = COMMAND_ROW.match(line)
            if m and m.group(1) != "help" and m.group(1) not in out:
                out.append(m.group(1))
    return out


def run(cli: str, args: list[str], cwd: Path | None = None) -> str:
    env = dict(os.environ, NO_COLOR="1", COLUMNS="200", CLICOLOR="0")
    r = subprocess.run(
        [cli, *args], cwd=cwd or ROOT, env=env, capture_output=True, text=True, timeout=120
    )
    return r.stdout + r.stderr


def rendered_hits(cli: str) -> list[str]:
    bad = []

    def check(label: str, text: str, tiers: bool = False) -> None:
        for ln, line in enumerate(text.splitlines(), 1):
            for tok in ids(line, tiers):
                bad.append(f"`{label}` line {ln}: {tok!r} in {line.strip()[:90]!r}")

    check("nros --help", run(cli, ["--help"]))
    top = run(cli, ["help", "--all"])
    check("nros help --all", top)
    seen: set[tuple[str, ...]] = set()
    queue = [(v,) for v in dict.fromkeys(subcommands(run(cli, ["--help"])) + subcommands(top))]
    while queue:
        path = queue.pop(0)
        if path in seen or len(path) > 4:
            continue
        seen.add(path)
        text = run(cli, [*path, "--help"])
        check("nros " + " ".join(path) + " --help", text)
        queue += [(*path, s) for s in subcommands(text)]

    listing = run(cli, ["board", "list"])
    check("nros board list", listing, tiers=True)
    boards, body = [], False
    for line in listing.splitlines():
        if line.startswith("---"):
            body = True  # rows start below the header's rule
            continue
        m = re.match(r"^([a-z0-9][a-z0-9_.-]*)\s", line)
        if body and m and m.group(1) not in boards:
            boards.append(m.group(1))
    for b in boards:
        check(f"nros board show {b}", run(cli, ["board", "show", b]), tiers=True)

    # `doctor --report` prints a row per image, so it is asked about a bringup
    # with one image per board the tracked examples deploy to — derived from
    # the tree, so a new board is covered the day an example names it.
    with tempfile.TemporaryDirectory() as tmp:
        cfg = Path(tmp) / "system.toml"
        cfg.write_text(report_bringup())
        check("nros doctor --report", run(cli, ["doctor", "--report", "--config", str(cfg)]),
              tiers=True)
    return bad


def report_bringup() -> str:
    files = subprocess.run(
        ["git", "ls-files", "*system.toml"], cwd=ROOT, capture_output=True, text=True, check=True
    ).stdout.split()
    boards = set()
    for rel in files:
        for m in re.finditer(r'^board\s*=\s*"([^"]+)"', (ROOT / rel).read_text(), re.M):
            boards.add(m.group(1))
    text = '[system]\nname = "surface_probe"\nrmw = "zenoh"\ndomain_id = 0\n'
    for i, b in enumerate(sorted(boards)):
        text += f'\n[image.i{i}]\nboard = "{b}"\n'
    return text


# --- self-test --------------------------------------------------------------


def self_test() -> None:
    must = ["see phase-525 W1", "Phase 401", "issue 1043", "(#2169)", "RFC-0116", "issues #0628"]
    for s in must:
        assert ids(s), f"self-test: {s!r} not caught"
    for s in ["C# 1234", "0x1234", "nros_check_skip", "/#1234", "&#1234;", "phase 2 of boot",
              "tier 1"]:
        assert not ids(s), f"self-test: {s!r} wrongly caught"
    assert ids("tier 2 board", tiers=True), "self-test: tier not caught"
    src = (
        '// phase-100 in a comment\n'
        'fn a() { bail!("see issue 1234"); let c = \'"\'; let l: &\'a str = r#"RFC-0001"#; }\n'
        '/* #1234 */\n'
        '#[cfg(test)]\nmod tests { fn t() { let _ = "phase-200"; } }\n'
        'fn b() { eprintln!("ok \\\n   #4321"); }\n'
    )
    got = [s for _, s in rust_strings(src)]
    found = [t for s in got for t in ids(s)]
    assert found == ["issue 1234", "RFC-0001", "#4321"], f"self-test: rust strings {found}"
    page = (
        "# Title\n\nSee issue 1043.\n<!-- phase-1 RFC-0001 -->\n\n"
        "## Contributor notes\n\nphase-525 is fine here.\n\n## Use\n\n" + "x\n" * 9 + "RFC-0116 [doc](../phase-100.md)\n"
    )
    hits = book_page_hits("p.md", page)
    assert len(hits) == 2 and "p.md:3:" in hits[0] and "RFC-0116" in hits[1], f"self-test: {hits}"
    assert subcommands("Usage: x\n\nCommands:\n  new   make\n  help  h\n\nOptions:\n") == ["new"]
    assert subcommands(
        "Quick start:\n  nros new\n\nUsage: nros\n\nBuild:\n  build  b\n  sync   s\n\n"
        "Options:\n  -h, --help\n"
    ) == ["build", "sync"]


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--self-test", action="store_true")
    ap.add_argument("--cli", help="the nros binary to render (default: the in-tree build)")
    args = ap.parse_args()
    self_test()
    if args.self_test:
        print(f"{NAME}: self-test OK")
        return 0

    bad = source_hits() + book_hits()
    cli = args.cli
    if cli is None:
        from source_locate import nros_cli_bin

        cli = nros_cli_bin(checkout_only=True)
    rendered = "rendered help, board list/show and doctor --report"
    if cli:
        bad += rendered_hits(cli)
    if bad:
        print(f"{NAME}: {len(bad)} internal id(s) on the user surface:", file=sys.stderr)
        for b in bad:
            print(f"  {b}", file=sys.stderr)
        print(
            "\nThe reader has no tree to look an id up in. Help says what the verb\n"
            "does (move the id to a `//` comment above the string); a refusal names\n"
            "the next command; a chapter states the fact, or moves the citation into\n"
            "a contributor block. If the only remedy is \"read issue N\", state the\n"
            "fact and file the refusal as a front-door issue. There is no allowlist.",
            file=sys.stderr,
        )
        return 1
    if not cli:
        return check_skip.unverified(
            NAME, f"no in-tree nros CLI (`just setup-cli`); {rendered} not read"
        )
    print(f"{NAME}: OK (cmd/ strings, {BOOK_GLOB}, {rendered})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
