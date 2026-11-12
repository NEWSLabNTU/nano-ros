#!/usr/bin/env python3
"""check-cli-companion-packages — one list of what builds beside the CLI.

Issue 2468. The in-tree `nros` CLI needs host packages built beside it: the
runner packages (`nros run` starts an image through one) and the first-party
command packages (`nros router`, `nros px4`). `just setup-cli` and
`scripts/bootstrap.sh` each listed them, and drifted: bootstrap.sh, the
`just`-free front door the book's checkout road uses, never learned about the
runners, so that road's first `nros run` refused. The list is now ONE file,
`scripts/lib/cli-companion-packages.txt`. This gate holds four things:

1. every listed manifest is a tracked file;
2. `just setup-cli` and `scripts/bootstrap.sh` both READ the list, and neither
   spells a listed manifest itself (a second copy is the drift);
3. the by-hand block of book-internals `orientation/contributing.md` names
   every listed workspace as `cargo build --release --manifest-path <m> [args]`;
4. every tracked `package.xml` announcing
   `<nano_ros_provides kind="runner"|"command">`, outside test fixtures, sits
   in a listed cargo workspace (the nearest ancestor with a `Cargo.lock`). So a
   new runner or command package fails here until the list names it.

Buildless, well under a second. `--self-test` runs both directions of each rule
first, on every call.
"""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
from pathlib import Path, PurePosixPath

ROOT = Path(__file__).resolve().parents[2]
NAME = "check-cli-companion-packages"
LIST = "scripts/lib/cli-companion-packages.txt"
READERS = {"justfile": "setup-cli", "scripts/bootstrap.sh": None}
BOOK = "book-internals/src/orientation/contributing.md"
PROVIDES = re.compile(r'<nano_ros_provides\s+kind="(runner|command)"')
FIXTURE = re.compile(r"(^|/)(tests?|fixtures?|testing)/")
FENCE = re.compile(r"^```")


def parse_list(text: str) -> list[tuple[str, str]]:
    out = []
    for line in text.splitlines():
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        manifest, _, args = line.partition(" ")
        out.append((manifest, " ".join(args.split())))
    return out


def recipe_body(justfile: str, recipe: str) -> str:
    lines = justfile.splitlines()
    start = next((i for i, l in enumerate(lines) if l.startswith(f"{recipe}:")), None)
    if start is None:
        return ""
    body = []
    for l in lines[start + 1 :]:
        if l and not l[0].isspace():
            break
        body.append(l)
    return "\n".join(body)


def reader_problems(name: str, text: str, entries: list[tuple[str, str]]) -> list[str]:
    bad = []
    if LIST not in text and Path(LIST).name not in text:
        bad.append(f"{name} does not read {LIST}")
    for manifest, _ in entries:
        if manifest in text:
            bad.append(f"{name} spells {manifest} itself; read it from {LIST}")
    return bad


def code_lines(md: str) -> list[str]:
    out, inside = [], False
    for l in md.splitlines():
        if FENCE.match(l.strip()):
            inside = not inside
            continue
        if inside:
            out.append(" ".join(l.split()))
    return out


def book_problems(md: str, entries: list[tuple[str, str]]) -> list[str]:
    lines = set(code_lines(md))
    bad = []
    for manifest, args in entries:
        want = f"cargo build --release --manifest-path {manifest}" + (f" {args}" if args else "")
        if want not in lines:
            bad.append(f"{BOOK}: the by-hand block lacks `{want}`")
    return bad


def workspace_of(pkg_dir: PurePosixPath, locks: set[str]) -> str | None:
    for d in [pkg_dir, *pkg_dir.parents]:
        if str(d / "Cargo.lock") in locks:
            return str(d / "Cargo.toml")
    return None


def provider_problems(
    xmls: dict[str, str], locks: set[str], entries: list[tuple[str, str]]
) -> list[str]:
    listed = {m for m, _ in entries}
    bad = []
    for rel, text in sorted(xmls.items()):
        if FIXTURE.search(rel) or not PROVIDES.search(text):
            continue
        ws = workspace_of(PurePosixPath(rel).parent, locks)
        if ws is None:
            bad.append(f"{rel}: announces a runner/command but has no cargo workspace (Cargo.lock)")
        elif ws not in listed:
            bad.append(f"{rel}: its workspace {ws} is not in {LIST}")
    return bad


def tracked(*globs: str) -> list[str]:
    return subprocess.run(
        ["git", "ls-files", *globs], cwd=ROOT, capture_output=True, text=True, check=True
    ).stdout.split()


def self_test() -> None:
    entries = parse_list("# c\n\na/Cargo.toml --workspace\nb/Cargo.toml\n")
    assert entries == [("a/Cargo.toml", "--workspace"), ("b/Cargo.toml", "")], entries
    assert not reader_problems("r", f"done < {LIST}", entries)
    assert reader_problems("r", "cargo build --manifest-path a/Cargo.toml", entries)
    assert reader_problems("r", f"read {LIST}; a/Cargo.toml", entries)
    md = (
        "```sh\ncargo build --release --manifest-path a/Cargo.toml --workspace\n"
        "cargo build  --release --manifest-path b/Cargo.toml\n```\n"
    )
    assert not book_problems(md, entries), book_problems(md, entries)
    assert book_problems("```sh\ncargo build --release --manifest-path a/Cargo.toml\n```\n", entries)
    assert book_problems("cargo build --release --manifest-path b/Cargo.toml\n", [("b/Cargo.toml", "")])
    locks = {"a/Cargo.lock", "c/Cargo.lock"}
    run = '<nano_ros_provides kind="runner" name="x"/>'
    assert not provider_problems({"a/x/package.xml": run}, locks, entries)
    assert provider_problems({"c/package.xml": run}, locks, entries)
    assert provider_problems({"d/package.xml": run}, locks, entries)
    assert not provider_problems({"c/tests/fixtures/package.xml": run}, locks, entries)
    assert not provider_problems({"c/package.xml": '<nano_ros_provides kind="board"/>'}, locks, entries)


def main() -> int:
    ap = argparse.ArgumentParser(description=(__doc__ or NAME).splitlines()[0])
    ap.add_argument("--self-test", action="store_true")
    args = ap.parse_args()
    self_test()
    if args.self_test:
        print(f"{NAME}: self-test OK")
        return 0

    entries = parse_list((ROOT / LIST).read_text())
    bad = []
    if not entries:
        bad.append(f"{LIST} lists nothing")
    files = set(tracked())
    bad += [f"{LIST}: {m} is not a tracked file" for m, _ in entries if m not in files]
    for rel, recipe in READERS.items():
        text = (ROOT / rel).read_text()
        if recipe:
            text = recipe_body(text, recipe)
            if not text:
                bad.append(f"{rel}: no `{recipe}` recipe")
                continue
        bad += reader_problems(f"{rel}{' ' + recipe if recipe else ''}", text, entries)
    bad += book_problems((ROOT / BOOK).read_text(), entries)
    xmls = {rel: (ROOT / rel).read_text() for rel in tracked("*package.xml")}
    locks = {f for f in files if f.endswith("Cargo.lock")}
    bad += provider_problems(xmls, locks, entries)

    if bad:
        print(f"{NAME}: {len(bad)} problem(s):", file=sys.stderr)
        for b in bad:
            print(f"  {b}", file=sys.stderr)
        print(
            f"\nWhat builds beside the CLI is ONE list, {LIST}: add the workspace\n"
            "there, and the same `cargo build` line to the by-hand block of\n"
            f"{BOOK}.",
            file=sys.stderr,
        )
        return 1
    print(f"{NAME}: OK ({len(entries)} workspace(s), both readers, the by-hand block, every provider)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
