#!/usr/bin/env python3
"""The book's `[image.<id>]` key table names exactly the keys the CLI accepts.

phase-537 W11 (RFC-0124 Principle 9). `reference/system-toml` is hand-kept:
the CLI's schema is the serde struct `ImageBlock`
(`packages/cli/nros-cli-core/src/orchestration/image.rs`, `deny_unknown_fields`),
and its doc comments are written for contributors (they cite RFCs, issues and
internal readers), so they cannot be the page's prose. What CAN be held to the
owner is the key set, both ways:

  * every key the struct deserializes is on the page: in the `[image.<id>]`
    table (the first column, or named in a row as an alias, as `conf` is), or
    as its own `[image.<id>.<key>]` section (`run`, `flash`, `console`,
    `knobs`);
  * every key in the table's first column is one the struct deserializes, so
    a renamed or retired key cannot stay documented.

A field the struct marks `#[serde(skip)]` (resolved, never authored) or reads
only to refuse it by name (`deserialize_with = "refuse_*"`, the retired keys)
is not a key a user writes, and is not expected on the page.

The self-test runs first on every call.
"""

from __future__ import annotations

import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts" / "lib"))
import book_pages  # noqa: E402

NAME = "check-system-toml-keys"
SRC = ROOT / "packages/cli/nros-cli-core/src/orchestration/image.rs"
STRUCT = "ImageBlock"
PAGE = "reference/system-toml.md"
TABLE_HEADING = "## `[image.<id>]`"


def struct_keys(rs: str, name: str = STRUCT) -> set[str]:
    """The keys `name` deserializes: each field's serde name (its `rename`, or
    the field name), minus `skip` fields and refuse-by-name fields."""
    m = re.search(r"pub struct " + name + r"\s*\{", rs)
    if not m:
        sys.exit(f"{NAME}: no `pub struct {name}` in {SRC.relative_to(ROOT)}")
    depth, i = 1, m.end()
    while depth and i < len(rs):
        depth += {"{": 1, "}": -1}.get(rs[i], 0)
        i += 1
    body = rs[m.end():i - 1]
    keys, attrs = set(), []
    for line in body.splitlines():
        s = line.strip()
        if s.startswith("///") or s.startswith("//") or not s:
            continue
        if s.startswith("#["):
            attrs.append(s)
            continue
        if attrs and not attrs[-1].endswith("]") and not s.startswith("pub "):
            attrs[-1] += " " + s  # a multi-line attribute
            continue
        f = re.match(r"pub (\w+)\s*:", s)
        if f:
            a = " ".join(attrs)
            attrs = []
            serde = " ".join(re.findall(r"#\[serde\((.*?)\)\]", a, re.S))
            if re.search(r"(?<![\w_])skip(?![\w_])", serde) or 'deserialize_with = "refuse' in serde:
                continue
            r = re.search(r'rename\s*=\s*"([^"]+)"', serde)
            keys.add(r.group(1) if r else f.group(1))
    return keys


def page_keys(md: str) -> tuple[set[str], set[str]]:
    """(keys in the table's first column, every key the page names for an
    image: the table's backticked words plus its `[image.<id>.<key>]` and
    `.key` section headings)."""
    first, named = set(), set()
    lines = md.splitlines()
    try:
        start = lines.index(TABLE_HEADING)
    except ValueError:
        sys.exit(f"{NAME}: {PAGE} has no `{TABLE_HEADING}` section")
    for line in lines[start + 1:]:
        if line.startswith("## "):
            break
        if not line.startswith("| ") or line.startswith("| Key") or line.startswith("| ---"):
            continue
        cells = line.split(" | ")
        first |= set(re.findall(r"`([a-z_]+)`", cells[0]))
        named |= set(re.findall(r"`([a-z_]+)`", line))
    for h in re.findall(r"^## (.+)$", md, re.M):
        named |= set(re.findall(r"\[image\.<id>\.([a-z_]+)", h))
        named |= set(re.findall(r"`\.([a-z_]+)`", h))
    return first, named | first


def problems(rs: str, md: str) -> list[str]:
    want = struct_keys(rs)
    first, named = page_keys(md)
    out = [f"`{k}` is a key of `{STRUCT}` the page does not document" for k in sorted(want - named)]
    out += [f"`{k}` is in the `[image.<id>]` table but `{STRUCT}` has no such key" for k in sorted(first - want)]
    return out


def self_test() -> None:
    rs = """
pub struct ImageBlock {
    /// doc
    #[serde(default)]
    pub board: Option<String>,
    #[serde(
        default,
        rename = "ip",
        deserialize_with = "refuse_flat_ip",
        skip_serializing
    )]
    pub retired_ip: X,
    #[serde(skip)]
    pub net: Y,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conf: Vec<String>,
    #[serde(default, rename = "run")]
    pub run_table: Z,
}
"""
    assert struct_keys(rs) == {"board", "conf", "run"}, struct_keys(rs)
    md = ("# T\n\n## `[image.<id>]`\n\n| Key | Meaning |\n| --- | --- |\n"
          "| `board` | id |\n| `os_config` | files; `conf` is its alias |\n\n"
          "## `[image.<id>.run]`\n\nx\n")
    assert problems(rs, md) == [
        "`os_config` is in the `[image.<id>]` table but `ImageBlock` has no such key"], problems(rs, md)
    assert problems(rs, md.replace("`conf` is its alias", "")) == [
        "`conf` is a key of `ImageBlock` the page does not document",
        "`os_config` is in the `[image.<id>]` table but `ImageBlock` has no such key"]


def main() -> int:
    self_test()
    if "--self-test" in sys.argv:
        print(f"{NAME}: self-test OK")
        return 0
    page = next(p.current() or p.target for p in book_pages.load() if p.book == "user" and p.path == PAGE)
    errs = problems(SRC.read_text(encoding="utf8"), page.read_text(encoding="utf8"))
    if errs:
        print(f"{NAME}: {page.relative_to(ROOT)} and `{STRUCT}` disagree:", file=sys.stderr)
        for e in errs:
            print(f"  {e}", file=sys.stderr)
        print(f"Edit the table (the key set is the struct's, {SRC.relative_to(ROOT)}).", file=sys.stderr)
        return 1
    print(f"{NAME}: OK ({len(struct_keys(SRC.read_text(encoding='utf8')))} keys of `{STRUCT}`, "
          "each on the page, and no other)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
