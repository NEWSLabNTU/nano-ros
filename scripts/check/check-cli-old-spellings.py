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
book/src/ and docs/ (RFC-0117 D5), and the root justfile. Exempt:

  * HISTORY — every phase doc (docs/roadmap/: each W-item is a dated record of
    what was run) and archived issues. Open issues are live: their
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
SCOPE = ["cmake", "just", "scripts", "zephyr", "examples", "book/src", "docs", "justfile", "tests"]
HISTORY_PREFIXES = ("docs/roadmap/", "docs/issues/archived/")
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
        use = re.compile(
            rf"\bnros {old}{end}|{VAR}\s+{old}{end}|{SUBST}\s+{old}{end}"
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


def scan(compiled) -> list[tuple[str, int, str, str]]:
    hits = []
    for rel in tracked():
        if rel in EXEMPT_FILES or rel.startswith(HISTORY_PREFIXES):
            continue
        try:
            text = (ROOT / rel).read_text(encoding="utf-8")
        except (UnicodeDecodeError, IsADirectoryError, FileNotFoundError):
            continue
        if "nros" not in text and "$" not in text:
            continue
        for n, line in enumerate(text.splitlines(), 1):
            for old in uses(line, compiled):
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
        'run(["git", "init"])': [],
    }
    assert rewrite('"$(nros_cli_bin)" ws board-facts "$d"', c) == \
        '"$(nros_cli_bin)" dev image board-facts "$d"'
    assert rewrite('subprocess.run([cli, "ws", "board-facts"], cwd=d)', c) == \
        'subprocess.run([cli, "dev", "image", "board-facts"], cwd=d)'
    assert rewrite("run([cli, 'init'])", c) == "run([cli, 'setup', 'presets'])"
    assert rewrite('X "${_nros}" ws board-facts --y', c) == 'X "${_nros}" dev image board-facts --y'
    assert rewrite("nros init . && git init", c) == "nros setup presets . && git init"
    for line, want in cases.items():
        got = uses(line, c)
        assert got == want, f"self-test: {line!r}: want {want}, got {got}"


def main() -> int:
    self_test()
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
        for rel in sorted({h[0] for h in scan(compiled)}):
            path = ROOT / rel
            text = path.read_text(encoding="utf-8")
            new = "\n".join(rewrite(l, compiled) for l in text.split("\n"))
            if new != text:
                path.write_text(new, encoding="utf-8")
                changed += 1
        print(f"check-cli-old-spellings --fix: rewrote {changed} file(s)")
    hits = scan(compiled)
    if hits:
        print(f"check-cli-old-spellings: FAIL — {len(hits)} old nros spelling(s) in the tree:")
        new_of = {r["old"]: r["new"] for r in rows}
        for rel, n, old, line in hits:
            print(f"  {rel}:{n}: `nros {old}` -> `nros {new_of[old]}`\n      {line[:160]}")
        print("Write the new spelling (`nros help --aliases` lists every pair). An old "
              "spelling may stay only beside its new one, or in a phase doc.")
        return 1
    print(f"check-cli-old-spellings: OK — {len(rows)} alias(es), no old spelling in "
          f"{', '.join(SCOPE)} (phase docs and archived issues are history)")
    return 0


if __name__ == "__main__":
    if "--self-test" in sys.argv:
        self_test()
        print("check-cli-old-spellings self-test: OK")
        sys.exit(0)
    sys.exit(main())
