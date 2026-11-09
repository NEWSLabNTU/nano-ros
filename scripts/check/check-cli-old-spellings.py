#!/usr/bin/env python3
"""check-cli-old-spellings — the tree teaches the current `nros` spelling.

RFC-0117 D5 keeps every old `nros` spelling working as a hidden alias, because
out-of-tree cmake files and scripts call them and a toolchain pin can move
under those. The tree's own files move to the new spelling instead (phase-526
W4): a doc, recipe or cmake module that still types `nros ws board-facts`
teaches a spelling nothing will ever list in `--help`.

THE TABLE IS THE CLI'S. The old -> new pairs come from
`nros help --aliases --json`, run on this checkout's CLI, so this gate carries
no list of its own and cannot drift from the parser. With no CLI built the
gate exits 78 and its recipe records a skip (`nros_check_skip`), never a pass.

WHAT COUNTS AS A USE of an old spelling `<old>` (e.g. `ws board-facts`):

  * `nros <old>` anywhere in a scanned file (prose, a recipe, a comment);
  * `<tool> <old>` where `<tool>` is a shell or cmake variable reference
    (`"${_nros}" ws board-facts`, `"$cli" sdk-root`). Five old spellings are
    also ordinary words other tools use (`codegen`, `profile`, `metadata`,
    `init`, `generate`: `git init`, `cargo metadata`), so for those the
    variable's NAME must say it holds nros (`nros`, `cli`, `tool`, `bin`) and
    must not say `cargo`, `git` or `rustup`;
  * `"$(<fn>)" <old>` — the tool given as a shell command substitution
    (`"$(nros_cli_bin)" sync`), and `[<name>, "<old>", ...]` — the tool as the
    head of a Python argv list (`[cli, "sync"]`, `[cli, "ws", "board-facts"]`).
    For both, the function's or name's NAME must say it holds nros, whatever
    the spelling: there is no `nros` literal on such a line to anchor on
    (phase-530 W1, Ruling R23 — the `nros sync` sweep missed three of these).

A following `-` or word character ends no match, so `nros generate-px4-msgs`
is not `nros generate`.

SCOPE: tracked files under cmake/, just/, scripts/, zephyr/, examples/, tests/,
book/src/ and docs/ (RFC-0117 D5), the root justfile, and packages/ (issue
2347: what the CLI prints and what every package emits). In packages/' Rust
sources only a string literal counts, and packages/ is a RATCHET against
`.config/cli-old-spellings-baseline.json` (see `BASELINE`). A spelling that must
stay old is exempt by construction, not by a path list: a refusal or rename
note names the new spelling beside the old (the rule below), and a RETIRED name
is no alias row, so this gate never reads it. Exempt:

  * HISTORY — every phase doc (docs/roadmap/: each W-item is a dated record of
    what was run), archived issues and adoption-probe run reports
    (docs/development/adoption-probes/runs/). Open issues are live: their
    reproduction commands are instructions somebody will run;
  * a line that also names the row's NEW spelling — how an RFC, the book or a
    phase doc quotes an old spelling beside its replacement;
  * this file.

`--fix` rewrites every use in place, which is how the tree was migrated.
Buildless once the CLI exists; self-tests its matcher on every run.
"""

from __future__ import annotations

import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SCOPE = ["cmake", "just", "scripts", "zephyr", "examples", "book/src", "docs", "justfile", "tests",
         "packages"]
# Issue 2347 — the packages. What the CLI PRINTS teaches a spelling as surely
# as a doc does (`nros new`'s next steps said `nros sync` after RFC-0117 D5
# renamed it), and what any package EMITS (a scaffold's comments, a generated
# file's banner, an orchestration-ir header) lands in the user's tree. In their
# Rust sources only a STRING LITERAL counts: a comment there is the code's own
# history, never shown to anyone. Every other file under them (templates,
# packs, READMEs, leaf manifests, fixtures) is text a reader sees, scanned like
# the rest of SCOPE.
RUST_LITERALS_ONLY = ("packages/",)
# Adoption-probe run reports (phase-523 W3) are history too: each records what
# a released CLI printed on a dated run, verbatim.
HISTORY_PREFIXES = (
    # Committed generator OUTPUT: it follows its generator on the next
    # `just generate-interfaces`, and is never edited by hand.
    "packages/interfaces/generated/",
    "docs/roadmap/",
    "docs/issues/archived/",
    "docs/development/adoption-probes/runs/",
)
EXEMPT_FILES = {
    "scripts/check/check-cli-old-spellings.py",
    # Generated "recently resolved" history of the issue series, frozen by
    # `check-issue-index`.
    "docs/issues/README.md",
    # Maps new spellings back to old ones for gates keyed on the old verb
    # names; its self-test has to spell both.
    "scripts/lib/cli_aliases.py",
    # Its self-test fixtures quote the pre-1396 NuttX and PlatformIO shim
    # bodies VERBATIM (old spelling included): they are what that gate exists
    # to have caught.
    "scripts/check-shim-codegen-target.py",
}
AMBIGUOUS = {"codegen", "profile", "metadata", "init", "generate"}
TOOL_NAME = re.compile(r"nros|cli|tool|bin", re.I)
NOT_TOOL = re.compile(r"cargo|git|rustup", re.I)
VAR = r'"?\$\{?(?P<var>[A-Za-z_][A-Za-z0-9_]*)\}?"?'
SUBST = r'"?\$\((?P<fn>[A-Za-z_][A-Za-z0-9_]*)\)"?'
SKIP = 78


def cli_path() -> Path | None:
    # issue 1850 — `$NROS_CLI`, else this checkout's CLI; never PATH.
    sys.path.insert(0, str(ROOT / "scripts" / "lib"))
    import source_locate
    found = source_locate.nros_cli_bin(checkout_only=True)
    return Path(found) if found else None


def load_rows(cli: Path) -> list[dict]:
    out = subprocess.run([str(cli), "help", "--aliases", "--json"],
                         capture_output=True, text=True, check=True).stdout
    rows = json.loads(out)
    if not rows or not all({"old", "new", "tier"} <= set(r) for r in rows):
        raise ValueError(f"`{cli} help --aliases --json` printed no usable table")
    return rows


def argv_words(words: str) -> str:
    """`ws board-facts` as the tail of a Python argv list: `"ws", "board-facts"`."""
    return r"""["']\s*,\s*["']""".join(re.escape(w) for w in words.split())


def compile_rows(rows: list[dict]) -> list[tuple[dict, re.Pattern, re.Pattern]]:
    out = []
    for r in rows:
        old, new = re.escape(r["old"]), re.escape(r["new"])
        end = r"(?![\w-])"
        # `(?<!cargo )`: `cargo nros <x>` is cargo-nano-ros's own subcommand
        # namespace (`cargo nros metadata` in a fixture's `generated_by`), not
        # the nros CLI, whose aliases this table lists (issue 2347's sweep).
        use = re.compile(
            rf"(?<!cargo )\bnros {old}{end}|{VAR}\s+{old}{end}|{SUBST}\s+{old}{end}"
            rf"""|\[\s*(?P<py>[A-Za-z_][A-Za-z0-9_.]*)\s*,\s*(?P<q>["']){argv_words(r["old"])}(?P=q)"""
        )
        beside = re.compile(
            rf"\bnros {new}{end}|\}}\"?\s+{new}{end}|\)\"?\s+{new}{end}"
            rf"""|["']{argv_words(r["new"])}["']"""
        )
        out.append((r, use, beside))
    return out


def counts(r: dict, m: re.Match) -> bool:
    """Whether a match of `r`'s old spelling is a use of the nros CLI."""
    var, fn, py = m.group("var"), m.group("fn"), m.group("py")
    if var is not None and r["old"] in AMBIGUOUS:
        return bool(TOOL_NAME.search(var)) and not NOT_TOOL.search(var)
    name = fn or py
    if name is not None:
        return bool(TOOL_NAME.search(name)) and not NOT_TOOL.search(name)
    return True


def uses(line: str, compiled) -> list[str]:
    found = []
    for r, use, beside in compiled:
        if beside.search(line):
            continue
        for m in use.finditer(line):
            if not counts(r, m):
                continue
            found.append(r["old"])
            break
    return found


def rewrite(line: str, compiled) -> str:
    """`line` with every use of an old spelling replaced by its new one."""
    for r, use, beside in compiled:
        if beside.search(line):
            continue

        def sub(m: re.Match) -> str:
            if not counts(r, m):
                return m.group(0)
            if m.group("py") is not None:
                q = m.group("q")
                head = m.group(0)[: m.start("q") - m.start(0)]
                return head + q + f"{q}, {q}".join(r["new"].split()) + q
            return m.group(0)[: len(m.group(0)) - len(r["old"])] + r["new"]

        line = use.sub(sub, line)
    return line


def tracked() -> list[str]:
    p = subprocess.run(["git", "-C", str(ROOT), "ls-files", "-z", "--", *SCOPE],
                       capture_output=True, check=True)
    return [f for f in p.stdout.decode().split("\0") if f]


def first_words(compiled) -> re.Pattern:
    """A prefilter over every row: each `use` pattern's match contains its
    row's FIRST old word, with a non-word, non-hyphen character (or the line's
    edge) on both sides (`nros ws …`, `"${_nros}" ws …`, `["ws", …`). A line
    with none of them cannot match any row, so the per-row regexes, 2 of them
    for each of ~40 rows, never run on it. That was 140 CPU-seconds on the
    fast line's hosted runner (phase-489); measured equal on 3740 hits."""
    words = sorted({r["old"].split()[0] for r, _use, _beside in compiled})
    return re.compile(r"(?<![\w-])(?:" + "|".join(map(re.escape, words)) + r")(?![\w-])")


def rust_string_spans(text: str) -> list[tuple[int, int]]:
    """`(start, end)` offsets of every string literal's CONTENT in Rust source:
    `"…"` (escapes honoured), `b"…"`, and raw `r#"…"#` at any `#` depth, which
    is how a scaffold's multi-line file template is written. Comments (`//`,
    `/* */`) and char literals are skipped, so a `"` inside them opens nothing."""
    spans: list[tuple[int, int]] = []
    i, n = 0, len(text)
    while i < n:
        c = text[i]
        if text.startswith("//", i):
            j = text.find("\n", i)
            i = n if j < 0 else j
        elif text.startswith("/*", i):
            j = text.find("*/", i + 2)
            i = n if j < 0 else j + 2
        elif c == "'":
            # 'x' or '\x…' is a char literal; anything else is a lifetime.
            if i + 2 < n and text[i + 2] == "'":
                i += 3
            elif i + 1 < n and text[i + 1] == "\\":
                j = text.find("'", i + 2)
                i = i + 1 if j < 0 else j + 1
            else:
                i += 1
        elif c == "r" and (i == 0 or not (text[i - 1].isalnum() or text[i - 1] == "_")):
            j = i + 1
            while j < n and text[j] == "#":
                j += 1
            if j < n and text[j] == '"':
                close = '"' + text[i + 1:j]
                k = text.find(close, j + 1)
                k = n if k < 0 else k
                spans.append((j + 1, k))
                i = k + len(close)
            else:
                i += 1
        elif c == '"':
            j = i + 1
            while j < n and text[j] != '"':
                j += 2 if text[j] == "\\" else 1
            spans.append((i + 1, j))
            i = j + 1
        else:
            i += 1
    return spans


def line_uses(rel: str, text: str, compiled, pre) -> list[tuple[int, str, str]]:
    """Every `(line number, old spelling, line)` use in one file's text."""
    spans = None
    if rel.endswith(".rs") and rel.startswith(RUST_LITERALS_ONLY):
        spans = rust_string_spans(text)
    out = []
    off = 0
    for n, line in enumerate(text.split("\n"), 1):
        if pre.search(line):
            for r, use, beside in compiled:
                if beside.search(line):
                    continue
                hit = False
                for m in use.finditer(line):
                    if not counts(r, m):
                        continue
                    pos = off + m.start()
                    if spans is not None and not any(a <= pos < b for a, b in spans):
                        continue
                    hit = True
                    break
                if hit:
                    out.append((n, r["old"], line))
                    break
        off += len(line) + 1
    return out


def scan(compiled) -> list[tuple[str, int, str, str]]:
    hits = []
    pre = first_words(compiled)
    for rel in tracked():
        if rel in EXEMPT_FILES or rel.startswith(HISTORY_PREFIXES):
            continue
        try:
            text = (ROOT / rel).read_text(encoding="utf-8")
        except (UnicodeDecodeError, IsADirectoryError, FileNotFoundError):
            continue
        if "nros" not in text and "$" not in text:
            continue
        for n, old, line in line_uses(rel, text, compiled, pre):
            hits.append((rel, n, old, line.strip()))
    return hits


def self_test() -> None:
    rows = [{"old": "ws board-facts", "new": "dev image board-facts", "tier": "dev"},
            {"old": "generate", "new": "interface generate", "tier": "user"},
            {"old": "init", "new": "setup presets", "tier": "user"},
            {"old": "metadata", "new": "dev metadata", "tier": "dev"}]
    c = compile_rows(rows)
    cases = {
        "run `nros ws board-facts --deploy x`": ["ws board-facts"],
        'COMMAND "${_nros}" ws board-facts': ["ws board-facts"],
        'out="$("$c" ws board-facts)"': ["ws board-facts"],
        "nros dev image board-facts": [],
        "`nros ws board-facts` is now `nros dev image board-facts`": [],
        "nros generate-px4-msgs --out x": [],
        "nros generate rust": ["generate"],
        'git -C "$t" init -q': [],
        '"${CARGO}" metadata --format-version 1': [],
        '"${_nros_cli}" metadata --x': ["metadata"],
        "nros initialise": [],
        "nros interface generate rust": [],
        # Ruling R23 — the tool as a command substitution, and as the head of
        # a Python argv list.
        '"$(nros_cli_bin)" ws board-facts "$dir"': ["ws board-facts"],
        '( cd "$d" && "$(nros_bin)" init )': ["init"],
        '"$(git_bin)" init': [],
        '"$(nros_bin)" dev image board-facts': [],
        'subprocess.run([cli, "ws", "board-facts"], cwd=d)': ["ws board-facts"],
        "run([nros_cli, 'metadata', '--x'])": ["metadata"],
        'run([cargo, "metadata"])': [],
        'run([cli, "dev", "image", "board-facts"])': [],
        'run(["cargo", "metadata"])': [],
        # issue 2347's sweep: another tool's namespace, and runtime names that
        # only look like a verb.
        'generated_by = "cargo nros metadata"': [],
        'nros::init().expect("nros::init failed")': [],
        '"nros metadata-mode: recorder rejected node"': [],
    }
    assert rewrite('"$(nros_cli_bin)" ws board-facts "$d"', c) == \
        '"$(nros_cli_bin)" dev image board-facts "$d"'
    assert rewrite('subprocess.run([cli, "ws", "board-facts"], cwd=d)', c) == \
        'subprocess.run([cli, "dev", "image", "board-facts"], cwd=d)'
    assert rewrite("run([cli, 'init'])", c) == "run([cli, 'setup', 'presets'])"
    assert rewrite('X "${_nros}" ws board-facts --y', c) == 'X "${_nros}" dev image board-facts --y'
    assert rewrite("nros init . && git init", c) == "nros setup presets . && git init"
    # Issue 2347 — in the CLI's Rust, a literal counts and a comment does not;
    # a raw multi-line template is a literal throughout.
    rs = ('// history: `nros generate rust` was the old verb\n'
          'println!("next: nros generate rust");\n'
          'let t = r#"line one\n# run `nros init` here\n"#;\n'
          "let c = '\"'; // nros init\n")
    got = [(n, old) for n, old, _ in line_uses("packages/cli/x/src/a.rs", rs, c, first_words(c))]
    assert got == [(2, "generate"), (4, "init")], f"self-test: rust literals: {got}"
    got = [(n, old) for n, old, _ in line_uses("examples/a.rs", rs, c, first_words(c))]
    assert got == [(1, "generate"), (2, "generate"), (4, "init"), (6, "init")], \
        f"self-test: outside packages/cli a .rs file is scanned whole: {got}"
    pre = first_words(c)
    for line, want in cases.items():
        got = uses(line, c)
        assert got == want, f"self-test: {line!r}: want {want}, got {got}"
        # The prefilter may only skip a line no row matches.
        assert not want or pre.search(line), f"self-test: prefilter skips {line!r}"


# Issue 2347 — `packages/` joined SCOPE holding ~500 old spellings, and one
# mechanical rewrite of all of them collides with every session editing under
# `packages/`. So the gate lands as a RATCHET there: a committed per-file count
# a file may only shrink. A file not in the baseline holds zero, a file above
# its count fails with its hits named, and everything outside `packages/` keeps
# zero tolerance. The sweep that empties it is `--fix` then `--update-baseline`.
BASELINE = ROOT / ".config" / "cli-old-spellings-baseline.json"
RATCHETED = ("packages/",)


def load_baseline() -> dict[str, int]:
    try:
        return {k: int(v) for k, v in json.loads(BASELINE.read_text(encoding="utf-8")).items()}
    except FileNotFoundError:
        return {}


def ratcheted_counts(hits) -> dict[str, int]:
    out: dict[str, int] = {}
    for rel, _n, _old, _line in hits:
        if rel.startswith(RATCHETED):
            out[rel] = out.get(rel, 0) + 1
    return out


def write_baseline(counts: dict[str, int]) -> None:
    BASELINE.write_text(json.dumps(dict(sorted(counts.items())), indent=1) + "\n",
                        encoding="utf-8")


def ratchet(hits, baseline: dict[str, int]):
    """`(grown, shrunk)`: the hits no baseline covers -- all of a file's hits once
    it exceeds its count, so the reader sees every candidate -- and the files now
    below theirs, as `{file: (now, baseline)}`."""
    counts = ratcheted_counts(hits)
    grown = [h for h in hits
             if not h[0].startswith(RATCHETED) or counts[h[0]] > baseline.get(h[0], 0)]
    shrunk = {rel: (counts.get(rel, 0), was) for rel, was in baseline.items()
              if counts.get(rel, 0) < was}
    return grown, shrunk


def ratchet_self_test() -> None:
    h = [("packages/a.rs", 1, "sync", "x"), ("packages/a.rs", 2, "sync", "y"),
         ("packages/b.rs", 1, "init", "z"), ("docs/c.md", 1, "init", "w")]
    grown, shrunk = ratchet(h, {"packages/a.rs": 2, "packages/d.rs": 3})
    assert [g[0] for g in grown] == ["packages/b.rs", "docs/c.md"], grown
    assert shrunk == {"packages/d.rs": (0, 3)}, shrunk
    grown, _ = ratchet(h, {"packages/a.rs": 1, "packages/b.rs": 1})
    assert [g[0] for g in grown] == ["packages/a.rs", "packages/a.rs", "docs/c.md"], grown


def main() -> int:
    self_test()
    ratchet_self_test()
    cli = cli_path()
    if cli is None:
        print("no in-tree nros CLI (just setup-cli) to read the alias table from")
        return SKIP
    try:
        rows = load_rows(cli)
    except (subprocess.CalledProcessError, ValueError, OSError) as e:
        print(f"check-cli-old-spellings: cannot read the alias table: {e}")
        return 1
    compiled = compile_rows(rows)
    if "--fix" in sys.argv:
        changed = 0
        # Only the lines that HIT are rewritten, so a Rust comment that names
        # an old spelling as history keeps it (`line_uses` never counts one).
        hit_lines: dict[str, set[int]] = {}
        for rel, n, _old, _line in scan(compiled):
            hit_lines.setdefault(rel, set()).add(n)
        for rel in sorted(hit_lines):
            path = ROOT / rel
            text = path.read_text(encoding="utf-8")
            new = "\n".join(rewrite(l, compiled) if i in hit_lines[rel] else l
                            for i, l in enumerate(text.split("\n"), 1))
            if new != text:
                path.write_text(new, encoding="utf-8")
                changed += 1
        print(f"check-cli-old-spellings --fix: rewrote {changed} file(s)")
    hits = scan(compiled)
    baseline = load_baseline()
    if "--update-baseline" in sys.argv:
        write_baseline(ratcheted_counts(hits))
        baseline = load_baseline()
    grown, shrunk = ratchet(hits, baseline)
    if grown:
        print(f"check-cli-old-spellings: FAIL — {len(grown)} old nros spelling(s) in the tree:")
        new_of = {r["old"]: r["new"] for r in rows}
        for rel, n, old, line in grown:
            print(f"  {rel}:{n}: `nros {old}` -> `nros {new_of[old]}`\n      {line[:160]}")
        print("Write the new spelling (`nros help --aliases` lists every pair). An old "
              "spelling may stay only beside its new one, or in a phase doc.")
        return 1
    held = sum(baseline.values())
    for rel, (now, was) in sorted(shrunk.items()):
        print(f"  note: {rel}: {now} old spelling(s), baseline {was} -- "
              "`--update-baseline` tightens it")
    print(f"check-cli-old-spellings: OK — {len(rows)} alias(es), no NEW old spelling in "
          f"{', '.join(SCOPE)} (phase docs and archived issues are history; "
          f"{held} held in packages/ by {BASELINE.relative_to(ROOT)}, issue 2347)")
    return 0


if __name__ == "__main__":
    if "--self-test" in sys.argv:
        self_test()
        ratchet_self_test()
        print("check-cli-old-spellings self-test: OK")
        sys.exit(0)
    sys.exit(main())
