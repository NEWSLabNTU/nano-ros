#!/usr/bin/env python3
"""A user chapter of the book never assumes a nano-ros checkout — phase-517 W2.

WHAT THIS ASSERTS

Users install the `nros` CLI from a release (`scripts/install.sh`); they do
not clone this repository into or beside their project. So no code block in a
USER chapter may

  * clone this repository (`git clone …/nano-ros`, `git submodule add …/nano-ros`),
  * run `bootstrap.sh`,
  * source `activate.sh` / `activate.fish`,
  * name `NROS_REPO_DIR`, or
  * run `just` (a contributor tool a user does not have), or
  * `cd` into the checkout's `examples/` tree, which a release does not ship
    (link the example on GitHub as reading material, or `nros new` one).

The book cloned `--branch nros-v0.6.0`, a tag 663 commits behind `main` at
which the next documented step (`nros setup linux`) did not exist, and the
probe never noticed because it substituted the branch under test for the tag.
That class goes away when no user step names a ref of this repository.

WHICH CHAPTERS ARE "USER"

Decided by SECTION, never by a list of files: a page is a user chapter when it
lives in one of the user directories of `book/src/` (`USER_DIRS`). Contributor
material lives in `internals/` (the "Internals" and "Contributing" parts of
`SUMMARY.md`), which this gate does not read. A chapter the table of contents
marks `(…, dormant)` or `(…, retired)` — the parked ESP32 pages, issue 1525 —
is kept unchanged on purpose and is skipped, with the same derivation
`check-book-links.py` and `check-book-board-names.py` use.

Only FENCED code is read: prose may say "you do not need `activate.sh`". The
`just` and `cd` rules read shell fences only (`sh`, `bash`, … or no info string).

Run:  python3 scripts/check-book-no-checkout.py [--selftest]

The self-test runs on every invocation and must FAIL a planted
`git clone …/nano-ros.git` in `first-project.md`.
"""

import argparse
import pathlib
import re
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent

# The user sections of the book, as directories under book/src/.
USER_DIRS = ("getting-started", "start-here", "user-guide", "platform-guides", "porting")

FENCE = re.compile(r"^\s*(?:```|~~~)\s*([A-Za-z0-9_+-]*)")
# A fence with one of these info strings (or none) holds shell a reader runs.
# The `just` and `cd` rules read only those: in a Rust or C block, "…; just arm
# a flag" is English inside a comment.
QUOTE = re.compile(r"^\s*(?:>\s?)+")
SHELL_LANGS = {"", "sh", "bash", "shell", "console", "zsh", "fish"}
PARKED = re.compile(r"\[[^\]]*\((?:[^)]*,\s*)?(?:dormant|retired)\)\]\(\./([^)]+)\)", re.I)

# (what, pattern, shell_only) — each pattern is applied to one fenced line.
RULES = [
    ("clones this repository",
     re.compile(r"\bgit\s+(?:clone|submodule\s+add)\b[^\n]*nano-ros", re.I), False),
    ("runs bootstrap.sh", re.compile(r"\bbootstrap\.sh\b"), False),
    ("sources activate.sh", re.compile(r"\bactivate\.(?:sh|fish)\b"), False),
    ("names NROS_REPO_DIR", re.compile(r"\bNROS_REPO_DIR\b"), False),
    # `just <recipe>` in command position: line start, after `$ `, after a
    # shell separator, or after `# or:` in a comment a reader copies.
    ("runs just",
     re.compile(r"(?:^\s*|[;&|(]\s*|\$\s+|#\s*(?:or:?\s*)?)just\s+[a-z_-]"), True),
    # A checkout's example tree is reading material (a GitHub link), never a
    # directory a user steps into — the release does not ship `examples/`.
    ("cds into the checkout's examples/",
     re.compile(r"(?:^|[;&|]\s*|\$\s+)\s*cd\s+\S*\bexamples/"), True),
]


def parked_pages(summary: str) -> set[str]:
    return {f"book/src/{p}" for p in PARKED.findall(summary)}


def is_user_page(rel: str) -> bool:
    parts = rel.split("/")
    return len(parts) >= 3 and parts[:2] == ["book", "src"] and parts[2] in USER_DIRS


def scan_page(rel: str, text: str) -> list[str]:
    errors = []
    in_fence = False
    lang = ""
    for n, raw in enumerate(text.splitlines(), 1):
        # A fence inside a blockquote (`> ```bash`) is still a code block a
        # reader copies from; strip the quote markers before reading it.
        line = QUOTE.sub("", raw)
        m = FENCE.match(line)
        if m:
            in_fence = not in_fence
            lang = m.group(1).lower() if in_fence else ""
            continue
        if not in_fence:
            continue
        for what, pat, shell_only in RULES:
            if shell_only and lang not in SHELL_LANGS:
                continue
            if pat.search(line):
                errors.append(f"{rel}:{n}: {what}: {line.strip()[:90]}")
    return errors


def run(pages: dict[str, str]) -> list[str]:
    parked = parked_pages(pages.get("book/src/SUMMARY.md", ""))
    errors: list[str] = []
    for rel, text in sorted(pages.items()):
        if is_user_page(rel) and rel not in parked:
            errors += scan_page(rel, text)
    return errors


def selftest(verbose: bool = False) -> int:
    first = "book/src/getting-started/first-project.md"
    cases = [
        ("planted clone in first-project.md fails",
         {first: "```sh\ngit clone https://github.com/NEWSLabNTU/nano-ros.git\n```\n"}, True),
        ("bootstrap.sh fails", {first: "```sh\n./scripts/bootstrap.sh\n```\n"}, True),
        ("source activate.sh fails", {first: "```sh\nsource ./activate.sh\n```\n"}, True),
        ("NROS_REPO_DIR fails", {first: "```cmake\nadd_subdirectory(${NROS_REPO_DIR})\n```\n"}, True),
        ("just recipe fails", {first: "```sh\njust zenohd\n```\n"}, True),
        ("just after && fails", {first: "```sh\ncd x && just build\n```\n"}, True),
        ("just in an `or:` comment fails",
         {first: "```sh\nros2 run rmw_zenoh_cpp rmw_zenohd   # or: just zenohd\n```\n"}, True),
        ("submodule add fails",
         {first: "```sh\ngit submodule add https://github.com/NEWSLabNTU/nano-ros x\n```\n"}, True),
        ("installed path passes",
         {first: "```sh\ncurl -fsSL https://x/install.sh | sh\nnros new a --workspace\n```\n"}, False),
        ("bootstrap.sh in a blockquoted fence fails",
         {first: "> ```sh\n> ./scripts/bootstrap.sh\n> ```\n"}, True),
        ("prose mention passes", {first: "You do not need `activate.sh` or `just`.\n"}, False),
        ("another repo's clone passes",
         {first: "```sh\ngit clone https://github.com/zephyrproject-rtos/zephyr\n```\n"}, False),
        ("English 'just' in a fence passes", {first: "```text\nit just works\n```\n"}, False),
        ("'; just' in a Rust comment passes",
         {first: "```rust\n// decide; just arm a flag\n```\n"}, False),
        ("cd into examples/ fails", {first: "```bash probe=40\ncd examples/linux/c/talker\n```\n"}, True),
        ("cd into a scaffold passes", {first: "```sh\nnros new t && cd t\n```\n"}, False),
        ("contributor section is exempt",
         {"book/src/internals/contributing.md": "```sh\n./scripts/bootstrap.sh\n```\n"}, False),
        ("dormant chapter is exempt",
         {"book/src/SUMMARY.md": "- [ESP32 (esp-hal, dormant)](./getting-started/esp32.md)\n",
          "book/src/getting-started/esp32.md": "```sh\njust esp32 build\n```\n"}, False),
        ("a user page not marked dormant is read",
         {"book/src/SUMMARY.md": "- [ESP32 (esp-hal, dormant)](./getting-started/esp32.md)\n",
          "book/src/getting-started/zephyr.md": "```sh\njust zephyr build\n```\n"}, True),
    ]
    failed = 0
    for name, pages, want_fail in cases:
        got = bool(run(pages))
        if got != want_fail:
            failed += 1
            print(f"check-book-no-checkout: SELFTEST FAILED: {name}", file=sys.stderr)
        elif verbose:
            print(f"  ok: {name}")
    if failed:
        return 1
    if verbose:
        print("check-book-no-checkout: selftest OK")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--selftest", action="store_true")
    args = ap.parse_args()
    if args.selftest:
        return selftest(verbose=True)
    if selftest():
        return 1

    listed = subprocess.run(
        ["git", "ls-files", "book/src/**/*.md", "book/src/*.md"],
        cwd=ROOT, capture_output=True, text=True, check=True,
    ).stdout.split()
    pages = {rel: (ROOT / rel).read_text() for rel in listed}
    user = [rel for rel in pages if is_user_page(rel)]
    if not user:
        print("check-book-no-checkout: found NO user chapters — USER_DIRS is stale",
              file=sys.stderr)
        return 1
    errors = run(pages)
    if errors:
        print(f"check-book-no-checkout: {len(errors)} user-chapter step(s) assume a checkout:",
              file=sys.stderr)
        for e in errors:
            print(f"  {e}", file=sys.stderr)
        print(
            "\nA user has the installed `nros` (install.sh puts it in ~/.nros/bin), not"
            "\na clone. Use `nros new` / `nros sdk-root` / the `nros` command a `just`"
            "\nrecipe wraps, or link the example on GitHub as reading material."
            "\nContributor material belongs in book/src/internals/.",
            file=sys.stderr,
        )
        return 1
    print(f"check-book-no-checkout: OK ({len(user)} user chapter(s))")
    return 0


if __name__ == "__main__":
    sys.exit(main())
