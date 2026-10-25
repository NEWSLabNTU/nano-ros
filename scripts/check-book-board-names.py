#!/usr/bin/env python3
"""Every board the book names is a board `nros` accepts — phase-517 W1.

WHAT THIS ASSERTS

A board word in the book is a command a reader types. The book named
`native`/`posix` as the host board for weeks after phase-500 made both a
refusal, listed a `stm32f4` key the index never had, and its CLI reference
carried a hand-kept "full set" that had drifted from the index — and nothing
failed, because every one of those is prose.

So every board word in a code block or inline code span must be one of

  * an index `[board.<key>]` key (`nros-sdk-index.toml`) — the ONLY thing
    `nros setup <board>` resolves, so that position accepts nothing else; or
  * a `names` alias of a tracked board descriptor
    (`packages/boards/**/nros-board.toml`) — what `[image.*] board` and
    `nros new --image … --board` resolve; or
  * a name the SAME page declares in a `names = [...]` row (a chapter that
    writes its own descriptor), or a placeholder (`<board>`, `my-…`).

Both sets are DERIVED from those files on every run, never listed here.

A chapter the book's table of contents marks `(…, dormant)` or `(…, retired)`
(the ESP32 pages, issue 1525) is parked text kept unchanged on purpose, and is
not read — derived from `SUMMARY.md`, the same rule `check-book-links.py` uses.

And the retired host-board words (`RETIRED_BOARD_KEYS` in
`packages/cli/nros-entry-lower/src/lib.rs`, harvested, not copied) appear as a
board nowhere except `book/src/reference/retired-names.md`, the one page whose
job is to say they are retired. "As a board" means: in a board position above,
as `nros new --platform <word>`, or as an inline `` `<word>` `` on a line that
also says "board".

A "board word" is the argument of `nros setup [board] <word>`, of `--board
<word>` on an `nros` command, and the value of a `board = "<word>"` row.
`west build -b`, `nros new entry --board` and other ZEPHYR board ids are not
nano-ros boards and are not read.

BOARD LISTS

A list of every board is generated, never typed: a page may hold

    <!-- board-names:begin … -->
    …
    <!-- board-names:end -->

and the text between the markers must equal the index keys, rendered here.
`--write` rewrites every such block.

Run:  python3 scripts/check-book-board-names.py [--write] [--selftest]

The self-test runs on every invocation (it is cheap), and must FAIL a planted
`nros setup native` — a gate that cannot see the defect it names is a print.
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
INDEX = ROOT / "nros-sdk-index.toml"
RETIRED_SRC = ROOT / "packages/cli/nros-entry-lower/src/lib.rs"
RETIRED_PAGE = "book/src/reference/retired-names.md"

BEGIN = re.compile(r"<!-- board-names:begin[^>]*-->\n")
END = "<!-- board-names:end -->"

FENCE = re.compile(r"^(\s*)(```|~~~)")
INLINE = re.compile(r"`([^`\n]+)`")

WORD = r"[A-Za-z0-9_][A-Za-z0-9_.-]*"
SETUP = re.compile(r"\bnros\s+setup\s+(board\s+)?(" + WORD + r")")
BOARD_FLAG = re.compile(r"\bnros\s+(?!new\s+entry\b)[^#\n]*?--board[ =](" + WORD + r")")
BOARD_ROW = re.compile(r"^\s*board\s*=\s*\"([^\"]*)\"")
PLATFORM_FLAG = re.compile(r"\bnros\s+new\b[^#\n]*?--platform[ =](" + WORD + r")")
NAMES_ROW = re.compile(r"^\s*names\s*=\s*\[([^\]]*)\]", re.M)
QUOTED = re.compile(r"\"([^\"]+)\"")

# `nros setup <flag>` and `nros setup board` (the subcommand) take no board here.
SETUP_NOT_BOARD = {"board"}


def index_keys(text: str) -> list[str]:
    return sorted(tomllib.loads(text).get("board", {}).keys())


def descriptor_names() -> set[str]:
    out = subprocess.run(
        ["git", "ls-files", "packages/boards/**/nros-board.toml"],
        cwd=ROOT, capture_output=True, text=True, check=True,
    ).stdout.split()
    names: set[str] = set()
    for rel in out:
        data = tomllib.loads((ROOT / rel).read_text())
        for entry in data.get("board", []) if isinstance(data.get("board"), list) else []:
            names.update(entry.get("names", []))
    return names


def retired_words(text: str) -> set[str]:
    body = re.search(r"RETIRED_BOARD_KEYS[^=]*=\s*&\[(.*?)\];", text, re.S)
    if not body:
        raise SystemExit("check-book-board-names: cannot find RETIRED_BOARD_KEYS")
    return set(re.findall(r"\(\s*\"([^\"]+)\"", body.group(1)))


# A SUMMARY.md entry whose title ends "(…, dormant)" or "(…, retired)" is parked
# text (ESP32, issue 1525) and kept unchanged on purpose — the same derivation
# `check-book-links.py` uses, read from the book's own table of contents.
PARKED = re.compile(r"\[[^\]]*\((?:[^)]*,\s*)?(?:dormant|retired)\)\]\(\./([^)]+)\)", re.I)


def parked_pages(summary: str) -> set[str]:
    return {f"book/src/{p}" for p in PARKED.findall(summary)}


def render_list(keys: list[str]) -> str:
    return ", ".join(f"`{k}`" for k in keys) + ".\n"


def is_placeholder(word: str) -> bool:
    return word.startswith(("<", "$", "my-", "your-")) or not word


def code_lines(text: str):
    """(lineno, text, in_fence) for each line; inline spans are found later."""
    in_fence = False
    for n, line in enumerate(text.splitlines(), 1):
        if FENCE.match(line):
            in_fence = not in_fence
            continue
        yield n, line, in_fence


def code_lines_keep(text: str):
    """Like `code_lines`, but fence lines are kept (as fenced) so line numbers hold."""
    in_fence = False
    for n, line in enumerate(text.splitlines(), 1):
        if FENCE.match(line):
            in_fence = not in_fence
            yield n, line, True
            continue
        yield n, line, in_fence


def prose_patterns(retired: set[str]) -> list[re.Pattern]:
    """Prose that names a retired word AS a board: "the `native` board", "board is
    not `native`", "name the board instead of `native`". Deliberately narrow — a
    sentence saying `posix` is NOT a board, or a history note about a rename,
    must stay legal."""
    w = "|".join(sorted(map(re.escape, retired)))
    return [
        re.compile(r"`(" + w + r")`\s+(?:host\s+)?boards?\b"),
        re.compile(r"\bboards?\s+(?:is\s+(?:not\s+)?|named\s+|=\s*)?`(" + w + r")`"),
        re.compile(r"\bboards?\s+instead\s+of\s+`(" + w + r")`"),
    ]


def scan_page(rel: str, text: str, allowed: set[str], retired: set[str],
              setup_keys: set[str]) -> list[str]:
    local = {w for row in NAMES_ROW.findall(text) for w in QUOTED.findall(row)}
    ok = allowed | local
    exempt_retired = rel == RETIRED_PAGE
    errors = []

    def judge(n: int, word: str, how: str, accepted: set[str] = ok) -> None:
        if is_placeholder(word):
            return
        if word in retired:
            if not exempt_retired:
                errors.append(
                    f"{rel}:{n}: `{word}` is a RETIRED board word ({how}) — write `linux`; "
                    f"only {RETIRED_PAGE} may name it"
                )
            return
        if word not in accepted:
            what = ("not an index [board.*] key — `nros setup` resolves only those"
                    if accepted is setup_keys else
                    "not an index [board.*] key nor a descriptor `names` alias")
            errors.append(f"{rel}:{n}: `{word}` names no board ({how}) — {what}")

    for n, line, in_fence in code_lines(text):
        spans = [line] if in_fence else INLINE.findall(line)
        for span in spans:
            for m in SETUP.finditer(span):
                if m.group(1):  # `nros setup board <name>`: a board crate's name
                    judge(n, m.group(2), "nros setup board")
                elif m.group(2) not in SETUP_NOT_BOARD:
                    judge(n, m.group(2), "nros setup", setup_keys)
            for m in BOARD_FLAG.finditer(span):
                judge(n, m.group(1), "--board")
            if in_fence:
                m = BOARD_ROW.match(span)
                if m:
                    judge(n, m.group(1), 'board = "…"')
            for m in PLATFORM_FLAG.finditer(span):
                if m.group(1) in retired and not exempt_retired:
                    judge(n, m.group(1), "nros new --platform")
        # A retired word in a board command's comment (`nros setup <board>  # e.g. `native``).
        if in_fence and not exempt_retired and re.search(r"\bnros\s+(setup\b|.*--board)", line):
            for span in INLINE.findall(line):
                if span.strip() in retired:
                    judge(n, span.strip(), "beside a board command")
    if not exempt_retired:
        prose = "\n".join("" if fenced else line for _, line, fenced in code_lines_keep(text))
        for pattern in prose_patterns(retired):
            for m in pattern.finditer(prose):
                judge(prose.count("\n", 0, m.start()) + 1, m.group(1), "named as a board in prose")
    return errors


def check_lists(rel: str, text: str, rendered: str, write: bool) -> tuple[str, list[str]]:
    errors = []
    out = []
    pos = 0
    for m in BEGIN.finditer(text):
        end = text.find(END, m.end())
        if end < 0:
            errors.append(f"{rel}: board-names:begin with no board-names:end")
            return text, errors
        out.append(text[pos:m.end()])
        if text[m.end():end] != rendered and not write:
            n = text.count("\n", 0, m.end()) + 1
            errors.append(
                f"{rel}:{n}: generated board list is stale — run "
                "`python3 scripts/check-book-board-names.py --write`"
            )
        out.append(rendered)
        pos = end
    out.append(text[pos:])
    return "".join(out), errors


def run(pages: dict[str, str], allowed: set[str], retired: set[str], keys: list[str],
        write: bool = False) -> tuple[list[str], dict[str, str]]:
    parked = parked_pages(pages.get("book/src/SUMMARY.md", ""))
    errors: list[str] = []
    rewritten: dict[str, str] = {}
    rendered = render_list(keys)
    for rel, text in sorted(pages.items()):
        new, list_errors = check_lists(rel, text, rendered, write)
        errors += list_errors
        if new != text:
            rewritten[rel] = new
        if rel not in parked:
            errors += scan_page(rel, new, allowed, retired, set(keys))
    return errors, rewritten


def selftest(verbose: bool = False) -> int:
    keys = ["linux", "zephyr"]
    allowed = set(keys) | {"freertos"}
    retired = {"native", "posix"}

    def errs(page: str, rel: str = "book/src/x.md") -> list[str]:
        return run({rel: page}, allowed, retired, keys)[0]

    cases = [
        ("planted `nros setup native` fails", "```sh\nnros setup native\n```\n", True),
        ("planted `nros setup posix` inline fails", "Run `nros setup posix --rmw zenoh`.\n", True),
        ("unknown key fails", "```sh\nnros setup stm32f4\n```\n", True),
        ("`board = \"native\"` fails", "```toml\n[image.x]\nboard = \"native\"\n```\n", True),
        ("`--platform native` on nros new fails", "```sh\nnros new a --platform native\n```\n", True),
        ("retired word named as a board in prose fails", "Name the board instead of\n`native`.\n", True),
        ("`the native host board` fails", "builds only the `native` host board.\n", True),
        ("retired word in a setup comment fails",
         "```sh\nnros setup <board>   # e.g. `native`\n```\n", True),
        ("saying posix is not a board passes", "`posix` names the platform, not the board.\n", False),
        ("index key passes", "```sh\nnros setup linux --rmw zenoh\nnros setup --list\n```\n", False),
        ("descriptor alias passes", "```toml\nboard = \"freertos\"\n```\n", False),
        ("descriptor alias is not a setup key", "```sh\nnros setup freertos\n```\n", True),
        ("placeholder passes", "```sh\nnros setup <board>\n```\n`board = \"my-board\"`\n", False),
        ("page-local names pass", "```toml\nnames = [\"acme-x\"]\n```\n```toml\nboard = \"acme-x\"\n```\n", False),
        ("zephyr entry --board is not a nano-ros board",
         "```sh\nnros new entry e --board native_sim/native/64\n```\n", False),
        ("posix as a platform passes", "`nros config explain --platform posix`\n", False),
        ("stale generated list fails",
         "<!-- board-names:begin -->\n`linux`.\n<!-- board-names:end -->\n", True),
        ("fresh generated list passes",
         "<!-- board-names:begin -->\n`linux`, `zephyr`.\n<!-- board-names:end -->\n", False),
    ]
    failed = 0
    for name, page, want_fail in cases:
        got = bool(errs(page))
        if got != want_fail:
            failed += 1
            print(f"check-book-board-names: SELFTEST FAILED: {name}", file=sys.stderr)
        elif verbose:
            print(f"  ok: {name}")
    parked = run({"book/src/SUMMARY.md": "- [X (y, dormant)](./p.md)\n",
                  "book/src/p.md": "```sh\nnros setup esp-x\n```\n"},
                 allowed, retired, keys)[0]
    if parked:
        failed += 1
        print("check-book-board-names: SELFTEST FAILED: a dormant chapter is skipped",
              file=sys.stderr)
    if errs("```sh\nnros setup native\n```\n", rel=RETIRED_PAGE):
        failed += 1
        print("check-book-board-names: SELFTEST FAILED: retired-names page is exempt",
              file=sys.stderr)
    if failed:
        return 1
    if verbose:
        print("check-book-board-names: selftest OK")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--write", action="store_true", help="regenerate board-names blocks")
    ap.add_argument("--selftest", action="store_true")
    args = ap.parse_args()
    if args.selftest:
        return selftest(verbose=True)
    if selftest():
        return 1

    keys = index_keys(INDEX.read_text())
    allowed = set(keys) | descriptor_names()
    retired = retired_words(RETIRED_SRC.read_text())
    if not keys or not retired:
        print("check-book-board-names: derived an EMPTY board or retired set", file=sys.stderr)
        return 1
    # A word both retired and accepted would make the rule contradict itself.
    clash = allowed & retired
    if clash:
        print(f"check-book-board-names: retired word(s) still accepted as boards: {sorted(clash)}",
              file=sys.stderr)
        return 1

    listed = subprocess.run(
        ["git", "ls-files", "book/src/**/*.md", "book/src/*.md"],
        cwd=ROOT, capture_output=True, text=True, check=True,
    ).stdout.split()
    pages = {rel: (ROOT / rel).read_text() for rel in listed}
    errors, rewritten = run(pages, allowed, retired, keys, write=args.write)
    if args.write:
        for rel, text in rewritten.items():
            (ROOT / rel).write_text(text)
            print(f"check-book-board-names: rewrote {rel}")
    if errors:
        print(f"check-book-board-names: {len(errors)} board word(s) the CLI would refuse:",
              file=sys.stderr)
        for e in errors:
            print(f"  {e}", file=sys.stderr)
        print(
            "\nA board word is a command a reader types. Use an index `[board.*]` key"
            "\n(`nros setup`) or a descriptor `names` alias (`[image.*] board`); the"
            "\nhost board is `linux`.",
            file=sys.stderr,
        )
        return 1
    print(f"check-book-board-names: OK ({len(pages)} page(s), {len(keys)} index key(s), "
          f"{len(allowed)} accepted name(s))")
    return 0


if __name__ == "__main__":
    sys.exit(main())
