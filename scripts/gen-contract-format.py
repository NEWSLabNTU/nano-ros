#!/usr/bin/env python3
"""Generate book/src/reference/contract-format.md (RFC-0124 D6, phase-537 W11).

The contract format belongs to ros-launch-manifest, not to nano-ros. Its
normative table is `types/src/field_table.rs`; that project renders it as
`docs/format-reference.md` and its own test refuses a rendering that differs
from the table. nano-ros pins the crate (`ros-launch-manifest-types` in
`packages/cli/nros-cli-core/Cargo.toml`, a git tag, resolved to a commit in
`packages/cli/Cargo.lock`), and the CLI parses contracts with exactly that
table. This page is that rendering at that commit, wrapped for the book:

  * the upstream title and its "generated, do not edit" note are replaced
    by this page's own header, which names the pinned tag and commit;
  * a relative link into upstream `docs/` becomes an absolute link to the
    same file at the pinned tag;
  * upstream's own milestone ids ("phase 70", "play_launch phase 97") are
    dropped, as `book_pages.scrub_ids` drops ours from every generated user
    page: the reader has no tree to look them up in. Only the citation goes;
    a sentence that still names an id after that stops the generator;
  * a `|` inside a code span in a table row is escaped (`\|`), so the row
    keeps its cells (GitHub-flavoured tables split on it otherwise);
  * nothing else changes: every section, table row and sentence is the
    owner's.

WHERE THE SOURCE COMES FROM (the RFC-0124 open question)

The page is COMMITTED, and this script is its gate. The docs job builds no
CLI and has no reason to fetch anything, and `just check fast` must run in a
clean clone with no network. So:

  * the pinned source is cargo's own git checkout of the crate, the one
    `cargo build -p nros-cli` leaves under `$CARGO_HOME/git/checkouts/`
    (or `NROS_RLM_SRC`, a checkout at the pinned commit). When it is there,
    `--check` re-renders the page from it and refuses any byte of difference,
    and also refuses an upstream rendering whose keys differ from the
    (context, key) pairs of `field_table.rs` itself, either way;
  * when it is not there, `--check` still verifies, offline, that the page
    names the tag in `Cargo.toml` and the commit in `Cargo.lock`, so a pin
    bump without a regeneration fails everywhere; the comparison with the
    table is then NOT VERIFIED and recorded in the skip ledger, never a quiet
    pass.

Run:  python3 scripts/gen-contract-format.py [--check | --self-test]
Gate: just check contract-format
"""

from __future__ import annotations

import glob
import os
import pathlib
import re
import subprocess
import sys

HERE = pathlib.Path(__file__).resolve().parent
ROOT = HERE.parent
sys.path.insert(0, str(HERE / "lib"))
import book_pages  # noqa: E402

PAGE = "reference/contract-format.md"
CARGO_TOML = ROOT / "packages" / "cli" / "nros-cli-core" / "Cargo.toml"
CARGO_LOCK = ROOT / "packages" / "cli" / "Cargo.lock"
CRATE = "ros-launch-manifest-types"
REPO_URL = "https://github.com/NEWSLabNTU/ros-launch-manifest"
UPSTREAM_DOC = "docs/format-reference.md"
TABLE = "types/src/field_table.rs"


def fail(msg: str) -> None:
    sys.exit(f"gen-contract-format: {msg}")


# ------------------------------------------------------------------- the pin


def pinned_tag(cargo_toml: str) -> str:
    m = re.search(r'^' + re.escape(CRATE) + r'\s*=\s*\{[^}]*\btag\s*=\s*"([^"]+)"', cargo_toml, re.M)
    if not m:
        fail(f"no `{CRATE} = {{ git = ..., tag = ... }}` line in {CARGO_TOML.relative_to(ROOT)}")
    return m.group(1)


def pinned_commit(cargo_lock: str, tag: str) -> str:
    """The full commit Cargo.lock resolved the tag to."""
    for block in cargo_lock.split("[[package]]"):
        if re.search(r'^name\s*=\s*"' + re.escape(CRATE) + r'"', block, re.M):
            m = re.search(r'^source\s*=\s*"git\+[^"?]+\?tag=([^#"]+)#([0-9a-f]{40})"', block, re.M)
            if not m:
                fail(f"{CARGO_LOCK.relative_to(ROOT)}: `{CRATE}` has no git source with a tag")
            if m.group(1) != tag:
                fail(f"{CARGO_LOCK.relative_to(ROOT)} resolves `{CRATE}` at tag {m.group(1)}, but "
                     f"Cargo.toml pins {tag}: rebuild the CLI so the lock follows the pin")
            return m.group(2)
    fail(f"{CARGO_LOCK.relative_to(ROOT)} has no `{CRATE}` package")
    raise AssertionError


def pin() -> tuple[str, str]:
    tag = pinned_tag(CARGO_TOML.read_text(encoding="utf8"))
    return tag, pinned_commit(CARGO_LOCK.read_text(encoding="utf8"), tag)


def checkout(commit: str) -> pathlib.Path | None:
    """A checkout of ros-launch-manifest at exactly `commit`, or None."""
    cands = []
    if os.environ.get("NROS_RLM_SRC"):
        cands.append(pathlib.Path(os.environ["NROS_RLM_SRC"]))
    cargo_home = pathlib.Path(os.environ.get("CARGO_HOME", pathlib.Path.home() / ".cargo"))
    cands += [pathlib.Path(p) for p in sorted(glob.glob(
        str(cargo_home / "git" / "checkouts" / "ros-launch-manifest-*" / commit[:7])))]
    for c in cands:
        if not (c / UPSTREAM_DOC).is_file() or not (c / TABLE).is_file():
            continue
        head = subprocess.run(["git", "-C", str(c), "rev-parse", "HEAD"],
                              capture_output=True, text=True).stdout.strip()
        if head == commit:
            return c
        if os.environ.get("NROS_RLM_SRC") and c == pathlib.Path(os.environ["NROS_RLM_SRC"]):
            fail(f"NROS_RLM_SRC={c} is at {head or 'an unknown commit'}, not the pinned {commit}")
    return None


# ------------------------------------------------------- the owner's table


LABEL = re.compile(r"Context::(\w+)\s*=>\s*\"([^\"]+)\"")
ENTRY = re.compile(r"\b(live|removed|alias|deprecated)\(\s*\"([^\"]+)\",\s*Context::(\w+)")


def table_keys(rs: str) -> set[tuple[str, str]]:
    """(context label, key) for every row of `FIELDS` in field_table.rs."""
    labels = dict(LABEL.findall(rs))
    start = rs.find("pub const FIELDS")
    end = rs.find("\n];", start)
    if start < 0 or end < 0 or not labels:
        fail(f"{TABLE}: no `pub const FIELDS` table or no `Context::label` arms (upstream reshaped "
             "the table: update this parser in the same commit as the pin)")
    out = set()
    for _, key, ctx in ENTRY.findall(rs[start:end]):
        if ctx not in labels:
            fail(f"{TABLE}: Context::{ctx} has no label arm")
        out.add((labels[ctx], key))
    return out


def doc_keys(md: str) -> set[tuple[str, str]]:
    """(section, key) for every row of a `| key | kind | status | meaning |`
    table in the rendered reference. The Bounds and later sections use other
    columns and are not key allowlists."""
    out, section, in_keys = set(), None, False
    for line in md.splitlines():
        m = re.match(r"^## `(.+)`$", line)
        if m:
            section, in_keys = m.group(1), False
            continue
        if line.startswith("## "):
            section, in_keys = None, False
            continue
        if line.startswith("| key | kind | status | meaning |"):
            in_keys = section is not None
            continue
        m = re.match(r"^\| `([^`]+)` \|", line)
        if in_keys and m:
            out.add((section, m.group(1)))
    return out


def agree(rs: str, md: str) -> list[str]:
    t, d = table_keys(rs), doc_keys(md)
    errs = [f"in {TABLE}, not in {UPSTREAM_DOC}: `{k}` under `{c}`" for c, k in sorted(t - d)]
    errs += [f"in {UPSTREAM_DOC}, not in {TABLE}: `{k}` under `{c}`" for c, k in sorted(d - t)]
    return errs


# ---------------------------------------------------------------- the page


def page_path() -> pathlib.Path:
    """Where the map says the page lives. A new page has no seeded `old`, so
    it is found by its row's `path`; a later move is a map edit only."""
    rows = [p for p in book_pages.load() if p.book == "user" and p.path == PAGE] or [
        p for p in book_pages.load() if PAGE in p.formers]
    if not rows:
        fail(f"book/page-map.toml has no row for {PAGE}")
    return rows[0].current() or rows[0].target


def render(upstream: str, tag: str, commit: str) -> str:
    lines = upstream.splitlines()
    if not lines or not lines[0].startswith("# "):
        fail(f"{UPSTREAM_DOC} does not start with its title")
    body = lines[1:]
    # Drop the owner's "Generated from ... Do not edit" paragraph: this page
    # has its own, naming the pin.
    while body and not body[0].strip():
        body.pop(0)
    if body and body[0].startswith("**Generated from"):
        body.pop(0)
    while body and not body[0].strip():
        body.pop(0)
    text = scrub(book_pages.scrub_ids("\n".join(body).rstrip() + "\n"))
    text = "\n".join(escape_cell_pipes(l) for l in text.split("\n"))
    blob = f"{REPO_URL}/blob/{tag}/docs/"
    text = re.sub(r"\]\((?!https?:|#|/)([^)\s]+)\)", lambda m: f"]({blob}{m.group(1)})", text)
    head = [
        "# Contract format reference",
        "",
        "<!-- type: reference · level: 3 -->",
        "",
        f"<!-- GENERATED by scripts/gen-contract-format.py from ros-launch-manifest {tag}",
        f"     ({commit}), {UPSTREAM_DOC}. Do not edit by hand.",
        "     Regenerate: python3 scripts/gen-contract-format.py",
        "     Gated by:   just check contract-format -->",
        "",
        "Every key a contract file (`<stem>.contract.yaml` beside a launch file, or",
        "`system.contract.yaml` in a bringup with no `launch/`) accepts. The contract",
        "format is ros-launch-manifest's, and this page is its format reference at",
        f"[`{tag}`]({REPO_URL}/tree/{tag}), the release the `nros` CLI is built",
        f"against (commit `{commit[:12]}`). The CLI reads a contract with that",
        "release's parser, so a key this page does not list is a parse error.",
        "",
        "To write a contract for your own nodes, see",
        "[Contracts for a C or C++ workspace](../guide/contracts/cpp-workspaces.md).",
        "",
    ]
    return "\n".join(head) + "\n" + text


# Upstream's ids that `scrub_ids` (written for ours) does not reach.
UP_CITE = r"(?:play_launch )?[Pp]hase \d{1,3}(?: [A-Z]\d+)?"
ANY_ID = re.compile(r"\b[Pp]hase[- ]\d{1,4}\b|\b[Ii]ssues?[- ]#?\d{3,4}\b|\bRFC-?\d{4}\b")


def scrub(text: str) -> str:
    text = re.sub(r"\s*\(" + UP_CITE + r"\)", "", text)
    text = re.sub(r"\(" + UP_CITE + r"[,;] ", "(", text)
    text = re.sub(r"\b(Removed|Added|Renamed|Deprecated) in " + UP_CITE + r"\b", r"\1", text)
    left = sorted({m.group(0) for m in ANY_ID.finditer(text)})
    if left:
        fail(f"the upstream reference names {', '.join(left)} in a way this generator cannot drop "
             "without rewriting the sentence; extend `scrub` for the new shape")
    return text


def escape_cell_pipes(line: str) -> str:
    """In a table row, `|` inside a code span is content, not a cell edge."""
    if not line.startswith("|"):
        return line
    return re.sub(r"`[^`]*`", lambda m: re.sub(r"(?<!\\)\|", r"\\|", m.group(0)), line)


def page_tag(text: str) -> tuple[str, str] | None:
    m = re.search(r"from ros-launch-manifest (\S+)\n\s+\(([0-9a-f]{40})\)", text)
    return (m.group(1), m.group(2)) if m else None


# ---------------------------------------------------------------- self-test


def self_test() -> None:
    def refused(fn) -> bool:
        try:
            fn()
        except SystemExit:
            return True
        return False

    toml = f'{CRATE} = {{ git = "https://x/y.git", tag = "v1.2.3" }}\n'
    assert pinned_tag(toml) == "v1.2.3"
    lock = (f'[[package]]\nname = "{CRATE}"\nversion = "0.1.0"\n'
            f'source = "git+https://x/y.git?tag=v1.2.3#{"a" * 40}"\n')
    assert pinned_commit(lock, "v1.2.3") == "a" * 40
    assert refused(lambda: pinned_commit(lock, "v1.2.4")), "a lock behind the pin passed"
    assert refused(lambda: pinned_tag("other = 1\n")), "a missing pin passed"

    rs = ('impl Context { fn label(self) { match self {\n'
          ' Context::Manifest => "manifest",\n Context::Node => "nodes.<name>",\n } } }\n'
          'pub const FIELDS: &[Field] = &[\n'
          '    live("version", Context::Manifest, Kind::Meta, "v."),\n'
          '    removed(\n        "old",\n        Context::Node,\n        "gone.",\n    ),\n];\n')
    md = ("# Contract File\n\n**Generated from x. Do not edit by hand**\n\nIntro [e](extension-guide.md).\n\n"
          "## `manifest`\n\n| key | kind | status | meaning |\n|---|---|---|---|\n| `version` | meta |  | v. |\n\n"
          "## `nodes.<name>`\n\n| key | kind | status | meaning |\n|---|---|---|---|\n| `old` |  | **removed** | gone. |\n\n"
          "## Bounds\n\n| key | where |\n|---|---|\n| `max_latency` | `paths.<name>` |\n")
    assert agree(rs, md) == [], agree(rs, md)
    # A key the table has and the rendering lacks, and the reverse, are refused.
    assert agree(rs, md.replace("| `old` |", "| `older` |")) == [
        f"in {TABLE}, not in {UPSTREAM_DOC}: `old` under `nodes.<name>`",
        f"in {UPSTREAM_DOC}, not in {TABLE}: `older` under `nodes.<name>`"]
    assert scrub("Removed in phase 70 — write x. Links (play_launch phase 97): y (play_launch phase 97 P5, "
                 "not adopted).") == "Removed — write x. Links: y (not adopted)."
    assert refused(lambda: scrub("since phase 70 the key is gone")), "a remaining id passed"
    assert escape_cell_pipes("| `k` | `{ a | b }` |") == "| `k` | `{ a \\| b }` |"
    assert escape_cell_pipes("prose `a | b`") == "prose `a | b`"
    out = render(md, "v1.2.3", "a" * 40)
    assert "Do not edit by hand**" not in out and "# Contract File" not in out
    assert f"]({REPO_URL}/blob/v1.2.3/docs/extension-guide.md)" in out, out
    assert page_tag(out) == ("v1.2.3", "a" * 40)
    # A page that names another pin is caught offline.
    assert page_tag(out.replace("v1.2.3", "v1.2.2", 1)) != ("v1.2.3", "a" * 40)


# --------------------------------------------------------------------- main


def main() -> int:
    self_test()
    if "--self-test" in sys.argv:
        print("gen-contract-format self-test: OK")
        return 0
    check = "--check" in sys.argv
    out_path = page_path()
    tag, commit = pin()
    src = checkout(commit)
    if src is None:
        if not check:
            fail(f"no checkout of ros-launch-manifest at {commit} ({tag}). Build the CLI once "
                 "(`cd packages/cli && cargo build -p nros-cli`) so cargo checks it out, or set "
                 "NROS_RLM_SRC to a checkout at that commit.")
        on_disk = out_path.read_text(encoding="utf8") if out_path.exists() else ""
        got = page_tag(on_disk)
        if got != (tag, commit):
            fail(f"{out_path.relative_to(ROOT)} names {got or 'no pin'}, but the CLI pins {tag} "
                 f"({commit}). Regenerate: python3 scripts/gen-contract-format.py")
        sys.path.insert(0, str(HERE / "lib"))
        import check_skip

        print(f"contract-format: the page names the pinned {tag} ({commit[:12]})")
        return check_skip.unverified(
            "contract-format",
            f"no checkout of ros-launch-manifest at {commit[:12]}, so the page is unread against "
            f"{TABLE} — build the CLI once (cargo fetches it) or set NROS_RLM_SRC")
    upstream = (src / UPSTREAM_DOC).read_text(encoding="utf8")
    errs = agree((src / TABLE).read_text(encoding="utf8"), upstream)
    if errs:
        fail(f"{tag}'s {UPSTREAM_DOC} disagrees with its {TABLE}:\n  " + "\n  ".join(errs))
    content = render(upstream, tag, commit)
    if check:
        on_disk = out_path.read_text(encoding="utf8") if out_path.exists() else ""
        if on_disk != content:
            fail(f"STALE — {out_path.relative_to(ROOT)} differs from the field table at {tag}. "
                 "Regenerate with python3 scripts/gen-contract-format.py and commit.")
        print(f"contract-format OK ({tag}, {len(doc_keys(upstream))} keys, matches {TABLE})")
        return 0
    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text(content, encoding="utf8")
    print(f"wrote {out_path.relative_to(ROOT)} from ros-launch-manifest {tag}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
