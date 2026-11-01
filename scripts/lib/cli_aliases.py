"""The `nros` alias table, read from the CLI's own sources (RFC-0117 D5).

`nros help --aliases --json` prints the same table from the built binary, and
`check-cli-old-spellings` reads it there. A buildless gate that already reads
the CLI's Rust sources (as `check-build-tool-verbs-exempt` does) reads the
two `ALIASES` constants instead, so it can map a new spelling back to the old
one the guard tables are keyed on, without a second list of its own.
"""

from __future__ import annotations

import re
from pathlib import Path

SOURCES = (
    "packages/cli/nros-cli-core/src/cmd/dev.rs",
    "packages/cli/nros-cli-core/src/cmd/user.rs",
)
_TABLE = re.compile(r"pub const ALIASES: &\[\(&str, &str\)\] = &\[(.*?)\];", re.S)
_ROW = re.compile(r'\(\s*"([^"]+)",\s*"([^"]+)"\s*\)')


def parse(text: str) -> list[tuple[str, str]]:
    m = _TABLE.search(text)
    return _ROW.findall(m.group(1)) if m else []


def load(root: str | Path) -> list[tuple[str, str]]:
    """Every (old, new) pair, dev tier then user tier. Raises if a source has
    no table: an empty map would make every lowering a silent no-op."""
    rows: list[tuple[str, str]] = []
    for rel in SOURCES:
        found = parse((Path(root) / rel).read_text(encoding="utf-8"))
        if not found:
            raise ValueError(f"{rel}: no `pub const ALIASES` table")
        rows += found
    return rows


def to_old(words: list[str], rows: list[tuple[str, str]]) -> list[str]:
    """`words` (a command path and its arguments) with a leading NEW spelling
    replaced by its old one; the longest matching new spelling wins. Words
    that start no new spelling come back unchanged."""
    best = None
    for old, new in rows:
        n = new.split()
        if words[: len(n)] == n and (best is None or len(n) > len(best[1])):
            best = (old.split(), n)
    if best is None:
        return list(words)
    return best[0] + list(words[len(best[1]):])


def normalize(text: str, rows: list[tuple[str, str]]) -> str:
    """`text` with every `nros dev …` spelling rewritten to the old spelling it
    replaced, so a gate keyed on old verb names reads either. Dev-tier rows
    only: a user-tier new spelling (`list`, `clean`) is an ordinary word."""
    dev = sorted((r for r in rows if r[1].startswith("dev ")),
                 key=lambda r: len(r[1]), reverse=True)
    for old, new in dev:
        text = re.sub(rf"(?<![\w-]){re.escape(new)}(?![\w-])", old, text)
    return text


_ROWS: list[tuple[str, str]] | None = None


def normalize_repo(text: str, root: str | Path) -> str:
    """[`normalize`] with the table read once from `root`'s CLI sources."""
    global _ROWS
    if _ROWS is None:
        _ROWS = load(root)
    return normalize(text, _ROWS)


def _self_test() -> None:
    rows = [("ws board-facts", "dev image board-facts"), ("codegen", "dev codegen"),
            ("ws list", "list")]
    assert parse('pub const ALIASES: &[(&str, &str)] = &[\n    ("a b", "dev c"),\n];') == [("a b", "dev c")]
    assert to_old(["dev", "image", "board-facts", "--x"], rows) == ["ws", "board-facts", "--x"]
    assert to_old(["dev", "codegen", "resolve-deps"], rows) == ["codegen", "resolve-deps"]
    assert to_old(["list"], rows) == ["ws", "list"]
    assert to_old(["build"], rows) == ["build"]
    rows2 = rows + [("codegen-system", "dev codegen system")]
    assert normalize('COMMAND "${t}" dev codegen system --x; dev image board-facts', rows2) \
        == 'COMMAND "${t}" codegen-system --x; ws board-facts'
    assert normalize("list(APPEND x)", rows2) == "list(APPEND x)"


_self_test()
