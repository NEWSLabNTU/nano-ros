#!/usr/bin/env python3
"""check-user-surface-ids — the user surface speaks in actions, never internal ids.

phase-525 W1 (RFC-0116 D2). An adopter reads `nros --help`, a refusal from a
verb, and the getting-started chapters. None of those may name a phase, an
issue, a pull request or an RFC: the reader has no tree to look the id up in,
so an id where a sentence or a command belongs is a dead end. The phase or
issue that built a verb belongs in a `//` comment above its help string; a
refusal names the next command; a chapter states the fact the issue holds, or
moves the citation to the Internals book or a `## Design background` section.

Subjects, all checked on every run:

* the refusal and message strings of `packages/cli/nros-cli-core/src/cmd/`:
  every Rust string literal outside comments and `#[cfg(test)]` modules (the
  `bail!`/`eyre!`/`anyhow!` texts, `println!`/`eprintln!` lines, and anything
  a refusal is built from);
* every page of the user book (`book/page-map.toml`, RFC-0123 D3), with the
  `## Design background` allowance for RFC ids. Nothing is exempt for its
  audience: contributor material belongs in the Internals book
  `check-book-no-just` uses (`scripts/lib/book_contributor.py`, one rule for
  both gates). HTML comments do not render and are not read;
* the RENDERED help: `nros --help`, `nros help --all`, and `--help` of every
  verb and nested verb, hidden ones included (clap turns `///` doc comments
  into help text, so this is the only reading that sees what renders);
* the WARNINGS a package's or an example's build script prints (issue 2305):
  a string literal in `build.rs` (or its `build_*.rs` / `build/*.rs` modules) that starts with
  `cargo:warning=` / `cargo::warning=`, or is the text handed to a `warn(…)` /
  `.note(…)` helper (directly or through `format!`). A warning is what an
  ordinary, successful `cargo build` of a user's image prints. A build
  script's PANIC text (a literal handed to `panic!`, `unreachable!`,
  `.expect(`, or an `assert!`'s message, directly or through `format!`) is
  read too, under the same zero rule: ruling A of issue 2443 covers a
  build that stops. A warning whose text is assembled away from its print (a `Vec` of warnings
  printed later) is not seen: a known gap, stated rather than claimed;
* every string literal of the CLI LIBRARY: `nros-cli-core/src` outside
  `cmd/`, and `cargo-nano-ros/src` (issue 2443). Its refusals stop a user's
  build, and its generated files land in the user's tree. Comments, doc
  comments and test-only code (a `#[cfg(test)]` module, inline or a file of
  its own) are not read. Baseline zero, like every other subject: this
  started as a per-file counted ratchet and, once issue 2443's sweep emptied
  it, became a hard rule, so a new id fails with its file and line rather
  than waiting on a count nobody watches;
* the messages of the colcon plugin (issue 2443),
  `packages/cli/colcon-cargo-ros2/colcon_nano_ros/`: every Python string
  literal (f-string parts included) that is not a docstring. A user's
  `colcon build` prints its refusals; its docstrings and `#` comments keep
  their ids;
* the OUTPUT OF USER-RUN SHELL SCRIPTS (issue 2443): `USER_RUN_SCRIPTS`
  (install.sh, bootstrap.sh, `activate.*`, the scripts a user's build runs
  from the release SDK root), every script the user book tells a reader to
  run, and what any of them sources. Read: `echo`/`printf` text (fish's too),
  a print helper (a function whose printer passes its arguments on), a
  `cat <<EOF` body unless redirected to a file, and a variable's assigned
  value when a printer prints it. Gaps, stated: text built by a command other
  than a printer, a heredoc written to a file, a printer inside a quoted
  `$(…)` (read as its outer string), and scripts those run rather than source;
* the RUNTIME LOG AND PANIC TEXT of every package and every example (issue
  2443): `packages/` and `examples/` (an example is code a user copies out
  and runs), outside vendored, test (an example's own `tests/` too), bench and
  build trees, bindgen's `generated.rs`, build scripts (read above) and the
  CLI library and `cmd/` (read whole above): a Rust string literal handed to
  a log, print or panic macro (`log::`/`defmt::`/bare `trace!`..`error!`,
  nros_log's `log_*!`/`nros_*_throttle!`, `cpp_diag!`, `println!`/
  `eprintln!`, `panic!`/`unreachable!`/`todo!`/`unimplemented!`, `.expect(`,
  the message of an `assert*!`) or to a board print helper (the `Board`
  trait's `println` as `B::println(format_args!(…))`, `print_line`,
  `hprintln!`, `nros_nuttx_println!`), directly or through `format!`/
  `format_args!`; a C/C++ string literal inside a `printf`/`fprintf`/
  `snprintf`/`puts`/`printk`/`syslog`/`assert`, a Zephyr `LOG_*`, PX4,
  zenoh-pico `_Z_*`, rclcpp/rclc or nros log macro, a backend's own log
  helper, or a board console helper (`uart_puts`, `semihosting_write0`,
  `nros_board_log`, `nros_board_freertos_console_write`,
  `board_stderr_line`, …; `C_LOG_CALLS` lists them). The literal's innermost
  enclosing call decides, so a table entry, a compared key or an assert's
  condition is data and is not read; Rust `#[cfg(test)]` and
  `#[cfg(all(test, …))]` items are not read. Gaps, stated: calls the lists
  above do not name, C++ stream output (`std::cout << "…"`), and text
  formatted into a variable or kept in a table and printed later (filed as
  its own issue);
* COMPILE-TIME TEXT (issue 2443): what the compiler shows a user building
  nano-ros or an app against it, in `packages/`, `zephyr/` and `cmake/`
  (tracked `.in` templates read as the file they generate) outside vendored,
  test, bench, example and fixture trees, bindgen's `generated.rs` and Rust
  `#[cfg(test)]` code. C/C++: the text of an `#error` / `#warning` /
  `#pragma message` directive, quoted or not, and the strings of a
  `static_assert` / `_Static_assert`, `_Pragma`, `[[deprecated(…)]]`,
  `[[nodiscard(…)]]`, a `*_DEPRECATED*(…)` wrapper macro, and an
  `__attribute__((deprecated|unavailable|error|warning(…)))`. Rust: the
  strings of `compile_error!` (inside `macro_rules!` and `quote!` too),
  `#[deprecated]`, `#[must_use = …]`, `#[diagnostic::on_unimplemented]`, the
  message of an `assert!` in a `const` item or `const { … }` block, and, in a
  `proc-macro = true` crate, `syn::Error::new` / `new_spanned` / `custom` and
  `proc-macro-error`'s `abort!` / `emit_error!`. Baseline zero. Gaps, stated:
  a message built away from the construct (a `String` made earlier and handed
  to `Error::new`), a proc macro's `panic!`, and a `const fn`'s assert reached
  only through a const context elsewhere;
* TEXT ASSEMBLED AWAY FROM ITS PRINT (issue 2443): an error's text, which a
  caller prints later as a CLI refusal, a build-script panic or a log line.
  In every Rust crate under `packages/` (vendored, test, bench, build and
  example trees skipped; the CLI library is read whole above): every literal
  in the body of an
  `impl Display for` or `impl Error for`; the strings of thiserror's
  `#[error(…)]`; and a literal handed to `Err(…)`, `bail!`/`eyre!`/
  `anyhow!`/`ensure!`, `.context`/`.with_context`/`.wrap_err`, `.ok_or`/
  `.ok_or_else`, `.map_err` or the arena oracle's `.text(…)` builder,
  directly, through `format!`/`format_args!`/`concat!`, or through an error
  enum's variant. A `Debug` impl is developer-facing and not read. Baseline
  zero. Gaps, stated: a message formatted into a variable first and handed
  over later (`let why = format!(…)`), a table of message strings, a literal
  a function returns for its caller to print, generated-file headers, and C
  text built into a buffer by anything but `snprintf` (which the runtime
  subject reads);
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
import ast
import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts" / "lib"))
import book_contributor  # noqa: E402
import book_pages  # noqa: E402
import check_skip  # noqa: E402
import comments  # noqa: E402

NAME = "check-user-surface-ids"
CMD_DIR = "packages/cli/nros-cli-core/src/cmd"
# The user book, as the page map assigns it (RFC-0123 D3, phase-532 W3).
BOOK_SCOPE = "the user book"
# RFC-0123 D3: a page may end with one `## Design background` section listing
# RFC links. RFC ids are allowed there and nowhere else; phase, issue and PR
# ids are never allowed.
DESIGN_BACKGROUND = re.compile(r"^##\s+Design background\s*$")
RFC_ONLY = re.compile(r"^RFC-?\d{4}$")

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


# --- the CLI library ---------------------------------------------------------

# Issue 2443, user ruling 2026-11-12 (option A): a refusal names what, where
# and how to fix, never an id. These crates held ids when the subject was
# added; it ran as a per-file ratchet until the sweep reached zero, and is
# baseline zero since (no counted baseline file: one that reaches zero and
# stays a ratchet can regress unwatched).
CLI_LIBRARY = ["packages/cli/nros-cli-core/src", "packages/cli/cargo-nano-ros/src"]
TEST_MOD_DECL = re.compile(r"#\[cfg\(test\)\]\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+(\w+)\s*;")


def test_module_files(files: list[str]) -> set[str]:
    """Files compiled only under `cfg(test)`: declared `#[cfg(test)] mod x;`
    in their parent, and everything below such a module's directory."""
    roots = []
    for rel in files:
        src = (ROOT / rel).read_text()
        p = Path(rel)
        base = p.parent if p.name in ("mod.rs", "lib.rs", "main.rs") else p.parent / p.stem
        for m in TEST_MOD_DECL.finditer(src):
            roots.append(base / m.group(1))
    out = set()
    for rel in files:
        p = Path(rel)
        for r in roots:
            if p == r.with_suffix(".rs") or p == r / "mod.rs" or r in p.parents:
                out.add(rel)
    return out


def cli_library_sites() -> dict[str, list[str]]:
    files = []
    for d in CLI_LIBRARY:
        files += subprocess.run(
            ["git", "ls-files", f"{d}/*.rs", f"{d}/**/*.rs"],
            cwd=ROOT, capture_output=True, text=True, check=True,
        ).stdout.split()
    files = sorted(set(files))
    tests = test_module_files(files)
    found: dict[str, list[str]] = {}
    for rel in files:
        if rel.startswith(CMD_DIR + "/") or rel in tests:
            continue
        if Path(rel).name == "tests.rs" or "/tests/" in rel:
            continue
        for ln, s in rust_strings((ROOT / rel).read_text()):
            for tok in ids(s):
                found.setdefault(rel, []).append(
                    f"{rel}:{ln}: {tok!r} in {' '.join(s.split())[:90]!r}"
                )
    return found


def library_sites() -> dict[str, list[str]]:
    found = cli_library_sites()
    for rel, sites in build_script_panic_sites().items():
        found.setdefault(rel, []).extend(sites)
    return found


def library_hits() -> list[str]:
    """Every id in a CLI-library string or a build-script panic text, by file:line."""
    return [site for sites in library_sites().values() for site in sites]


# --- build-script warnings --------------------------------------------------

# An example's build script runs in the user's build of that example, so
# examples are read too.
BUILD_SCRIPT_GLOBS = [
    f"{d}/**/{g}" for d in ("packages", "examples") for g in ("build.rs", "build_*.rs", "build/*.rs")
]
SKIP_TREES = re.compile(r"/(third-party|tests?|testing|generated)/")
WARNING_PREFIX = re.compile(r"^cargo::?warning=")
# A literal handed to a warning helper: `warn(`, `.note(`, optionally through
# `&format!(` / `format!(`, with only whitespace between.
WARNING_CALL = re.compile(r"(?:\bwarn|\.note)\(\s*&?\s*(?:format!\(\s*)?$")


# A literal handed to a panic: `panic!(`, `unreachable!(`, `.expect(`, or an
# `assert!`'s message after its condition, optionally through `format!(`.
PANIC_CALL = re.compile(
    r"(?:\b(?:panic|unreachable|todo)!\(|\.expect\(|\bassert(?:_eq|_ne)?!\([^;]*?,)"
    r"\s*&?\s*(?:format!\(\s*)?$",
    re.S,
)


def build_script_files() -> list[str]:
    files = subprocess.run(
        ["git", "ls-files", *BUILD_SCRIPT_GLOBS], cwd=ROOT, capture_output=True, text=True,
        check=True,
    ).stdout.split()
    return [rel for rel in sorted(set(files)) if not SKIP_TREES.search("/" + rel)]


def panic_literals(src: str) -> list[tuple[int, str]]:
    out = []
    for ln, start, end in rust_strings(src, spans=True):
        before = src[max(0, start - 120) : start].rstrip('r#"b')
        if PANIC_CALL.search(before):
            out.append((ln, src[start:end]))
    return out


def build_script_panic_sites() -> dict[str, list[str]]:
    found: dict[str, list[str]] = {}
    for rel in build_script_files():
        for ln, text in panic_literals((ROOT / rel).read_text()):
            for tok in ids(text):
                found.setdefault(rel, []).append(
                    f"{rel}:{ln}: {tok!r} in {' '.join(text.split())[:90]!r}"
                )
    return found


def build_script_warning_hits() -> list[str]:
    files = subprocess.run(
        ["git", "ls-files", *BUILD_SCRIPT_GLOBS], cwd=ROOT, capture_output=True, text=True,
        check=True,
    ).stdout.split()
    bad = []
    for rel in sorted(set(files)):
        if SKIP_TREES.search("/" + rel):
            continue
        src = (ROOT / rel).read_text()
        for ln, start, end in rust_strings(src, spans=True):
            text = src[start:end]
            before = src[max(0, start - 80) : start].rstrip('r#"b')
            if not (WARNING_PREFIX.match(text) or WARNING_CALL.search(before)):
                continue
            for tok in ids(text):
                bad.append(f"{rel}:{ln}: {tok!r} in {' '.join(text.split())[:90]!r}")
    return bad


# --- the colcon plugin ------------------------------------------------------

COLCON_DIR = "packages/cli/colcon-cargo-ros2/colcon_nano_ros"


def python_message_strings(src: str) -> list[tuple[int, str]]:
    """Every string literal of `src` that is not a docstring, with its line.

    An f-string's literal parts are read one by one. A docstring is the first
    statement of a module, class or function, when that statement is a bare
    string; `#` comments are not literals at all.
    """
    tree = ast.parse(src)
    docs = set()
    for node in ast.walk(tree):
        if isinstance(node, (ast.Module, ast.ClassDef, ast.FunctionDef, ast.AsyncFunctionDef)):
            first = node.body[0] if node.body else None
            if isinstance(first, ast.Expr) and isinstance(first.value, ast.Constant):
                docs.add(id(first.value))
    found = [
        node
        for node in ast.walk(tree)
        if isinstance(node, ast.Constant) and isinstance(node.value, str) and id(node) not in docs
    ]
    found.sort(key=lambda node: (node.lineno, node.col_offset))
    return [(node.lineno, node.value) for node in found]


def colcon_hits() -> list[str]:
    files = subprocess.run(
        ["git", "ls-files", f"{COLCON_DIR}/*.py", f"{COLCON_DIR}/**/*.py"],
        cwd=ROOT, capture_output=True, text=True, check=True,
    ).stdout.split()
    bad = []
    for rel in sorted(set(files)):
        for ln, s in python_message_strings((ROOT / rel).read_text()):
            for tok in ids(s):
                bad.append(f"{rel}:{ln}: {tok!r} in {' '.join(s.split())[:90]!r}")
    return bad


# --- user-run shell scripts -------------------------------------------------

# Issue 2443, ruling A: what a script the USER runs prints names what, where
# and how to fix, never an id. CI and maintainer tooling (`scripts/check-*`,
# `scripts/probe/`, `just` recipes) is not this subject. The set is this list,
# the scripts they source, and the scripts the user book tells a reader to run:
USER_RUN_SCRIPTS = {
    # The `curl … | sh` installer the book's install chapter shows; the
    # release asset also ships it as `share/nros/install.sh`.
    "scripts/install.sh": "the release installer",
    # The from-source front door; install.sh's own refusals name it.
    "scripts/bootstrap.sh": "the from-source installer",
    # Sourced into the user's shell, every shell once bootstrap wires the rc.
    "activate.sh": "shell activation",
    "activate.fish": "shell activation",
    # Shipped in the release SDK root (`scripts/stage-sdk-root.sh`) and run by
    # a user's own build: the NuttX board overlays provision the kernel with
    # the first, `NanoRosCorrosion.cmake` asserts with the second.
    "scripts/nuttx/build-nuttx.sh": "run by a user's NuttX build",
    "scripts/check-shared-cargo-dir-used.sh": "run by a user's cmake build",
}
# Any other `activate.*` at the root is activation too.
ACTIVATE_GLOB = "activate.*"
SH_PRINTERS = {"echo", "printf"}
FISH_PRINTERS = {"echo", "printf", "set_color"}
# Words that open a command without being its name.
SH_LEADS = {"then", "do", "else", "elif", "if", "while", "until", "!", "time", "{", "}",
            "command", "builtin", "and", "or", "not", "begin", "exec"}
SH_ASSIGN = re.compile(r"[A-Za-z_]\w*=")
SH_HEREDOC = re.compile(r"<<-?\s*(['\"]?)([A-Za-z_]\w*)\1")
# A redirect that keeps a heredoc on the terminal: `>&2`, `>/dev/stderr`.
TO_TERMINAL = re.compile(r"^(?:&[12]|/dev/std(?:out|err))$")
SH_FUNC = re.compile(r"^(\s*)(?:function\s+)?([A-Za-z_][\w-]*)\s*\(\s*\)\s*\{?(.*)$")
FISH_FUNC = re.compile(r"^(\s*)function\s+([\w-]+)")
# A helper's printer passes its ARGUMENTS on: `$*`, `$@`, `$1`, `${1…}`, `$argv`.
ARG_REF = re.compile(r"\$(?:\*|@|\{?[1-9]|argv\b|\{[*@])")
VAR_REF = re.compile(r"\$\{?([A-Za-z_]\w*)")
SOURCED = re.compile(r"""^(?:source|\.)\s+["']?([^"'\s;|&]+)""")
BOOK_SCRIPT = re.compile(r"(?<![\w.-])((?:scripts/[\w./-]+|activate)\.(?:sh|fish))\b")
INLINE_CODE = re.compile(r"`([^`]+)`")


class ShellCommand:
    # A plain class: other gates load this file by path (`check-book-audience`),
    # where a dataclass cannot resolve its module.
    def __init__(self, name: str, pieces: list, heredocs: list) -> None:
        self.name = name
        self.pieces = pieces  # (start, end) of its own text, nested commands excluded
        self.heredocs = heredocs  # (start, end) of the bodies of its `<<` heredocs

    def text(self, src: str) -> str:
        return "".join(src[a:b] for a, b in self.pieces)


def shell_words(text: str) -> list[str]:
    """`text` split at unquoted whitespace (quotes kept in the words)."""
    words, cur, i, n, q = [], "", 0, len(text), ""
    while i < n:
        c = text[i]
        if q:
            if c == "\\" and q == '"':
                cur += text[i : i + 2]
                i += 2
                continue
            if c == q:
                q = ""
        elif c in "'\"":
            q = c
        elif c == "\\":
            cur += text[i : i + 2]
            i += 2
            continue
        elif c.isspace():
            if cur:
                words.append(cur)
            cur = ""
            i += 1
            continue
        cur += c
        i += 1
    return words + ([cur] if cur else [])


def shell_commands(src: str, fish: bool = False) -> list[ShellCommand]:
    """The simple commands of a shell (or fish) script, comments already
    blanked. Commands end at an unquoted newline (not `\\`-continued), `;`,
    `&`, `|`, `(`, `)` and a backtick; a `$(…)` or `(…)` nests, so the text
    around it stays its outer command's. Quotes: `'…'` (fish and `$'…'` with
    escapes), `"…"` with escapes, a `$(…)` inside it skipped as a unit. A
    heredoc body belongs to the command whose line opened it. Not a parser:
    `case` patterns read as commands, and a printer inside a quoted `$(…)` is
    read as part of its outer string."""
    n = len(src)
    out: list[ShellCommand] = []
    stack: list[ShellCommand] = []
    cur = ShellCommand("", [], [])
    seg = 0
    pending: list[tuple[ShellCommand, str, bool]] = []

    def skip_subst(j: int) -> int:  # j just past `$(`; return past its `)`
        depth = 1
        while j < n and depth:
            c = src[j]
            if c == "\\":
                j += 2
                continue
            if c in "'\"":
                j = skip_quote(j)
                continue
            depth += {"(": 1, ")": -1}.get(c, 0)
            j += 1
        return j

    def skip_quote(j: int) -> int:  # j at the opening quote; return past its close
        q = src[j]
        esc = q == '"' or fish or (j > 0 and src[j - 1] == "$")
        k = j + 1
        while k < n and src[k] != q:
            if esc and src[k] == "\\":
                k += 2
                continue
            if q == '"' and src.startswith("$(", k):
                k = skip_subst(k + 2)
                continue
            k += 1
        return min(k + 1, n)

    def finish(end: int) -> None:
        nonlocal cur
        cur.pieces.append((seg, end))
        out.append(cur)

    i = 0
    while i < n:
        c = src[i]
        if c == "\\":
            i += 2
            continue
        if c in "'\"":
            i = skip_quote(i)
            continue
        if c == "`":
            j = src.find("`", i + 1)
            i = n if j < 0 else j + 1
            continue
        if c == "<" and not fish:
            m = SH_HEREDOC.match(src, i)
            if m and not src.startswith("<<<", i):
                pending.append((cur, m.group(2), src.startswith("<<-", i)))
                i = m.end()
                continue
        if c == "\n":
            finish(i)
            i += 1
            for cmd, term, dash in pending:
                start = i
                while i < n:
                    j = src.find("\n", i)
                    j = n if j < 0 else j
                    line = src[i:j]
                    if (line.lstrip("\t") if dash else line) == term:
                        cmd.heredocs.append((start, i))
                        i = j + 1
                        break
                    i = j + 1
                else:
                    cmd.heredocs.append((start, n))
            pending.clear()
            cur, seg = ShellCommand("", [], []), i
            continue
        if c == "&" and (src[i - 1 : i] in ("<", ">") or src[i + 1 : i + 2] == ">"):
            i += 1  # `>&2`, `&>`: a redirect, not a separator
            continue
        if c in ";&|":
            finish(i)
            cur, seg = ShellCommand("", [], []), i + 1
        elif c == "(":
            cur.pieces.append((seg, i + 1))
            stack.append(cur)
            cur, seg = ShellCommand("", [], []), i + 1
        elif c == ")":
            finish(i)
            cur = stack.pop() if stack else ShellCommand("", [], [])
            seg = i
        i += 1
    finish(n)
    for cmd in out:
        words = shell_words(cmd.text(src))
        k = 0
        while k < len(words) and (words[k] in SH_LEADS or SH_ASSIGN.match(words[k])
                                  or words[k].endswith("()") or words[k] == "function"):
            k += 1
        cmd.name = words[k] if k < len(words) else ""
    return [cmd for cmd in out if cmd.text(src).strip() or cmd.heredocs]


def shell_print_helpers(src: str, fish: bool, printers: set[str]) -> set[str]:
    """Functions of `src` whose body prints their arguments (`die() { echo
    "x: $*" >&2; exit 1; }`), and those that pass theirs to one."""
    funcs: dict[str, str] = {}
    lines = src.splitlines()
    for k, line in enumerate(lines):
        m = (FISH_FUNC if fish else SH_FUNC).match(line)
        if not m:
            continue
        indent, name = m.group(1), m.group(2)
        rest = "" if fish else m.group(3)
        if rest.rstrip().rstrip(";").endswith("}"):
            funcs[name] = rest  # a one-line body
            continue
        close = "end" if fish else "}"
        body = []
        for later in lines[k + 1:]:
            if later.startswith(indent) and later.strip() in (close, close + ";"):
                break
            body.append(later)
        funcs[name] = "\n".join(body)
    found: set[str] = set()
    while True:
        known = printers | found
        new = {
            name for name, body in funcs.items() if name not in found
            and any(c.name in known and ARG_REF.search(c.text(body))
                    for c in shell_commands(body, fish))
        }
        if not new:
            return found
        found |= new


def shell_printed(src: str, fish: bool, printers: set[str]) -> tuple[str, list[tuple[int, int]]]:
    """The spans of `src` a user sees: the text of every printer command
    (`echo`, `printf`, a print helper, fish's `set_color`), whatever it is
    redirected to; the body of a `cat <<EOF` heredoc unless it is redirected
    to a file; and the value of a variable assigned in the script that a
    printer prints (`snippet="…"` … `printf '%s\\n' "$snippet"`). Returns
    `src` with its comments blanked, which the spans index."""
    src = comments.strip_comments(src, "sh")
    cmds = shell_commands(src, fish)
    spans: list[tuple[int, int]] = []
    names: set[str] = set()
    for c in cmds:
        if c.name in printers:
            spans += c.pieces
            names |= set(VAR_REF.findall(c.text(src)))
        if c.heredocs and c.name in printers | {"cat"}:
            targets = re.findall(r"(?<![<\d&])\d?>>?\s*(\S+)", c.text(src))
            if all(TO_TERMINAL.match(t.strip("\"'")) for t in targets):
                spans += c.heredocs
    for c in cmds:
        words = c.text(src).split(None, 1)
        if fish and c.name == "set":
            rest = [w for w in c.text(src).split() if not w.startswith("-")][1:]
            if rest and rest[0] in names:
                spans += c.pieces
        elif words and (m := re.match(r"(?:(?:local|export|readonly|declare)\s+(?:-\w+\s+)*)?"
                                      r"([A-Za-z_]\w*)=", c.text(src).lstrip())):
            if m.group(1) in names:
                spans += c.pieces
    return src, spans


def resolve_sourced(rel: str, word: str) -> str | None:
    """A `source`/`.` argument as a tracked path, or None (a variable alone,
    a file outside the tree)."""
    m = re.match(r"^(?:\$\{?\w+\}?)?(/?[\w./-]+\.(?:sh|fish))$", word)
    if not m:
        return None
    tail = m.group(1)
    for cand in (tail.lstrip("/"), str(Path(rel).parent / tail.lstrip("/"))):
        if (ROOT / cand).is_file():
            return str(Path(cand))
    return None


def book_run_scripts() -> dict[str, str]:
    """Scripts the user book tells a reader to run: a path in a fenced block,
    or in an inline code span that is a command (more than the bare path)."""
    found: dict[str, str] = {}
    for rel in book_pages.tracked_user_pages():
        text = HTML_COMMENT.sub(lambda m: "\n" * m.group(0).count("\n"), (ROOT / rel).read_text())
        for ln in book_contributor.walk(text.splitlines()):
            spans = [ln.text] if ln.in_fence else [
                s for s in INLINE_CODE.findall(ln.text) if not BOOK_SCRIPT.fullmatch(s.strip())
            ]
            for s in spans:
                for m in BOOK_SCRIPT.finditer(s):
                    if (ROOT / m.group(1)).is_file():
                        found.setdefault(m.group(1), f"{rel}:{ln.number}")
    return found


def user_run_scripts() -> dict[str, str]:
    """{path: why}: the listed scripts, the root's `activate.*`, the book's,
    and every tracked script any of them sources, transitively."""
    found = dict(USER_RUN_SCRIPTS)
    for p in sorted(ROOT.glob(ACTIVATE_GLOB)):
        found.setdefault(p.name, "shell activation")
    for rel, where in book_run_scripts().items():
        found.setdefault(rel, f"the book runs it ({where})")
    queue = list(found)
    while queue:
        rel = queue.pop(0)
        if not (ROOT / rel).is_file():
            continue
        src = comments.strip_comments((ROOT / rel).read_text(), "sh")
        for c in shell_commands(src, rel.endswith(".fish")):
            m = SOURCED.match(c.text(src).strip())
            dep = m and resolve_sourced(rel, m.group(1))
            if dep and dep not in found:
                found[dep] = f"sourced by {rel}"
                queue.append(dep)
    return found


def shell_lines(src: str, spans: list[tuple[int, int]]) -> list[tuple[int, str]]:
    """Every id in `spans` of `src`, once each, as (line, id)."""
    seen = {}
    for a, b in spans:
        for m in ID.finditer(src, a, b):
            seen[m.start()] = m.group(0)
    return [(src.count("\n", 0, k) + 1, tok) for k, tok in sorted(seen.items())]


def shell_hits() -> list[str]:
    scripts = user_run_scripts()
    helpers: set[str] = set()
    texts = {}
    for rel in scripts:
        if (ROOT / rel).is_file():
            texts[rel] = (ROOT / rel).read_text()
            fish = rel.endswith(".fish")
            helpers |= shell_print_helpers(
                comments.strip_comments(texts[rel], "sh"), fish,
                FISH_PRINTERS if fish else SH_PRINTERS,
            )
    bad = []
    for rel, src in sorted(texts.items()):
        fish = rel.endswith(".fish")
        printers = (FISH_PRINTERS if fish else SH_PRINTERS) | helpers
        lines = src.splitlines()
        for ln, tok in shell_lines(*shell_printed(src, fish, printers)):
            bad.append(f"{rel}:{ln}: {tok!r} in {lines[ln - 1].strip()[:90]!r}")
    return bad


# --- runtime log and panic text ---------------------------------------------

# Issue 2443, ruling A: what an image, a tool or an example prints at run
# time -- a log line, a panic or assert message -- names what, where and how
# to fix, never an id. Every package and every example: an example is code a
# user copies out and runs, so its printed text reaches them as surely as a
# library's.
RUNTIME_DIRS = ["packages", "examples"]
RUNTIME_EXTS = (".rs", ".c", ".h", ".cpp", ".hpp", ".cc")
# Not runtime text: vendored and test trees (an example's own `tests/`
# included), bindgen output, and build scripts (their panics and warnings are
# read above).
RUNTIME_SKIP = re.compile(
    r"/(third-party|vendor|vendored|tests?|testing|benches|build)/"
    r"|/generated\.rs$|/build(_[^/]*)?\.rs$|/tests\.rs$"
)
# The CLI library and `cmd/` are read whole above (every literal), so the
# runtime subject leaves them out rather than report a line twice.
RUNTIME_SKIP_PREFIXES = tuple(d + "/" for d in [*CLI_LIBRARY, CMD_DIR])
# The library subjects below (assembled text) keep examples out: an example's
# `Err(…)` is the teaching code's own, printed only through the calls above.
LIBRARY_SKIP = re.compile(
    r"/(third-party|vendor|vendored|tests?|testing|benches|examples|build)/"
    r"|/generated\.rs$|/build(_[^/]*)?\.rs$|/tests\.rs$"
)
# The calls whose string arguments are printed, matched against the call's
# last path segment (`log::warn!`, `defmt::panic!`, `nros_log::log_error!`).
# The board print helpers are named too: the `Board` trait's `println`
# (`B::println(format_args!(…))`, `Mps2An385::println(…)`, `<B as Board>::println`),
# a board's own `print_line`, cortex-m-semihosting's `hprintln!` and the
# NuttX board's `nros_nuttx_println!`.
RUST_LOG_CALLS = re.compile(
    r"(?:trace|debug|info|warn|error|log|nros_trace|log_(?:debug|info|warn|error|fatal)"
    r"|nros_(?:trace|debug|info|warn|error)_throttle(?:_at)?|cpp_diag|cpp_info"
    r"|e?println|e?print|hprintln|nros_nuttx_println|panic|unreachable|todo|unimplemented)!"
    r"|\.expect|println|print_line"
)
# An assert's message is the argument after its condition (after both
# operands for `_eq`/`_ne`); the condition's own literals are data.
RUST_ASSERT_CALLS = re.compile(r"(?:debug_)?assert(_eq|_ne)?!")
RUST_WRAPPERS = re.compile(r"(?:format|format_args|concat)!")
C_LOG_CALLS = re.compile(
    r"v?f?printf|snprintf|f?puts|printk|syslog|assert|LOG_(?:ERR|WRN|INF|DBG)"
    r"|PX4_(?:ERR|WARN|INFO|DEBUG|PANIC)|_Z_(?:ERROR|WARN|INFO|DEBUG)"
    r"|NROS_(?:LOG_SINK|DEBUG|INFO|WARN|ERROR|FATAL)|RCLCPP_(?:DEBUG|INFO|WARN|ERROR|FATAL)\w*"
    r"|RCLCPP_TRY_LOG|RCLC_LOG_\w+|RCLC_CHECK_LOG|RCUTILS_LOG_\w+|RCL_SET_ERROR_MSG"
    r"|ZPICO_HOSTED_PRINTK|xrce_log_error|graph_log|nros_threadx_stderr|nros_freertos_append"
    # The board crates' console helpers (`packages/boards/*/c`, and the
    # examples that copy them) and the Cyclone DDS backend's trace macro.
    r"|uart_puts|semiho(?:st|sting)_write0|nros_board_log(?:_write_stderr)?"
    r"|nros_board_freertos_console_write|board_stderr_line|sh_print_size|log_app_returned"
    r"|heap_peak_report|NROS_CYC_TRACE"
)
CALL_NAME = re.compile(r"([A-Za-z_][\w:]*!?|\.\s*[A-Za-z_]\w*)\s*$")
# The places the lexer stops: a comment, a string (raw and byte forms in
# Rust), a char literal or lifetime. Everything between is code.
RUST_TOKENS = re.compile(r'//|/\*|b?r#*"|b?"|\'')
C_TOKENS = re.compile(r'//|/\*|"|\'')
RAW_START = re.compile(r'b?r(#*)"')
# `#[cfg(test)]`, or `#[cfg(all(test, …))]`: compiled for tests only.
CFG_TEST = re.compile(r"#\[cfg\((?:test|all\(\s*test\b[^\]]*\))\)\]")


def code_and_literals(src: str, rust: bool) -> tuple[str, list[tuple[int, int, int]]]:
    """`src` with comments and literal contents blanked (offsets and newlines
    kept), and every string literal as (line, start, end) of its contents.

    The C reading: `//` and `/* */` comments, `\\` escapes, char literals (so
    `'"'` and `'('` are neither a string nor a paren). Adjacent C literals
    ("a" "b") stay separate spans: each is read on its own, and each sits in
    the same call, so the concatenation needs no joining to be judged. Rust
    adds raw and byte strings and tells a lifetime from a char literal, as
    `rust_strings` does, and drops the literals of a `#[cfg(test)]` or
    `#[cfg(all(test, …))]` item.
    Not a parser; a macro body is read like code.
    """
    code = list(src)
    spans: list[tuple[int, int]] = []

    def blank(a: int, b: int) -> None:
        code[a:b] = [ch if ch == "\n" else " " for ch in src[a:b]]

    tokens = RUST_TOKENS if rust else C_TOKENS
    i, n = 0, len(src)
    while True:
        m = tokens.search(src, i)
        if not m:
            break
        i, tok = m.start(), m.group(0)
        if tok == "//":
            j = src.find("\n", i)
            j = n if j < 0 else j
            blank(i, j)
            i = j
        elif tok == "/*":
            j = src.find("*/", i + 2)
            j = n if j < 0 else j + 2
            blank(i, j)
            i = j
        elif tok == "'":
            if src.startswith("\\", i + 1):
                j = src.find("'", i + 2)
                j = n if j < 0 else j + 1
            elif i + 2 < n and src[i + 2] == "'":
                j = i + 3
            else:
                i += 1  # a Rust lifetime
                continue
            blank(i, j)
            i = j
        elif "r" in tok:
            if i and (src[i - 1].isalnum() or src[i - 1] == "_"):
                i += tok.index("r") + 1  # `…r"` ends an identifier: a plain string
                continue
            close = '"' + RAW_START.match(tok).group(1)
            j = src.find(close, m.end())
            j = n if j < 0 else j
            spans.append((m.end(), j))
            blank(m.end(), j)
            i = j + len(close)
        else:  # `"` or `b"`
            j = m.end()
            # A C string ends at its line (a `\`-newline is an escape and
            # is stepped over), so an apostrophe or a stray quote in an
            # unquoted `#error` cannot swallow the file.
            while j < n and src[j] != '"' and (rust or src[j] != "\n"):
                j += 2 if src[j] == "\\" else 1
            spans.append((m.end(), j))
            blank(m.end(), j)
            i = j + 1
    masked = "".join(code)
    if rust:
        # A `#[cfg(test)]` item ends at its `;` or at the `}` matching its
        # first `{`, whichever comes first (`rust_strings`' rule).
        dead = []
        for t in CFG_TEST.finditer(masked):
            depth = 0
            for b in re.finditer(r"[{};]", masked[t.end():]):
                ch = b.group(0)
                if ch == ";":
                    if depth == 0:
                        break
                    continue
                depth += 1 if ch == "{" else -1
                if depth == 0:
                    dead.append((t.start(), t.end() + b.end()))
                    break
        spans = [s for s in spans if not any(lo <= s[0] < hi for lo, hi in dead)]
    lines = [src.count("\n", 0, spans[0][0]) + 1] if spans else []
    for k in range(1, len(spans)):
        lines.append(lines[-1] + src.count("\n", spans[k - 1][0], spans[k][0]))
    return masked, [(ln, a, b) for ln, (a, b) in zip(lines, spans)]


def enclosing_calls(code: str, at: int, levels: int = 2) -> list[tuple[str, int]]:
    """The calls around offset `at` of blanked `code`, innermost first, as
    (name, argument index). Walks back to each unmatched `(`; stops at an
    unmatched `{` or `[`, or a `;` (a block or a table holds data, not a
    call's text)."""
    out: list[tuple[str, int]] = []
    depth = commas = 0
    k, lo = at - 1, max(0, at - 4000)
    while k >= lo and len(out) < levels:
        ch = code[k]
        if ch in ")]}":
            depth += 1
        elif ch in "([{":
            if depth:
                depth -= 1
            elif ch != "(":
                break
            else:
                m = CALL_NAME.search(code, max(0, k - 80), k)
                out.append((re.sub(r"\s+", "", m.group(1)) if m else "", commas))
                commas = 0
        elif ch == "," and not depth:
            commas += 1
        elif ch == ";" and not depth:
            break
        k -= 1
    return out


def is_printed(calls: list[tuple[str, int]], rust: bool) -> bool:
    """Whether a literal inside `calls` is text a log, print, panic or assert
    call prints."""
    if not calls:
        return False
    name, arg = calls[0]
    if rust and RUST_WRAPPERS.fullmatch(name.rsplit("::", 1)[-1]) and len(calls) > 1:
        name, arg = calls[1]  # `.expect(&format!(…))`, `assert!(c, concat!(…))`
    name = name.rsplit("::", 1)[-1]
    if not rust:
        return bool(C_LOG_CALLS.fullmatch(name))
    m = RUST_ASSERT_CALLS.fullmatch(name)
    if m:
        return arg >= (2 if m.group(1) else 1)
    return bool(RUST_LOG_CALLS.fullmatch(name))


def runtime_literals(
    src: str, rust: bool, want: re.Pattern | None = None
) -> list[tuple[int, str]]:
    """Every string literal of `src` that a log, print, panic or assert call
    prints, with its line; with `want`, only the literals it matches (the
    gate asks for id-bearing ones, which skips the walk back for the rest).
    Rust literals in test-only items are not read."""
    code, spans = code_and_literals(src, rust)
    return [
        (ln, src[a:b]) for ln, a, b in spans
        if (want is None or want.search(src, a, b))
        and is_printed(enclosing_calls(code, a - 1), rust)
    ]


def runtime_files() -> list[str]:
    files = subprocess.run(
        ["git", "ls-files", "--", *RUNTIME_DIRS], cwd=ROOT, capture_output=True, text=True,
        check=True,
    ).stdout.split()
    files = [
        rel for rel in files
        if rel.endswith(RUNTIME_EXTS) and not RUNTIME_SKIP.search("/" + rel)
        and not rel.startswith(RUNTIME_SKIP_PREFIXES) and (ROOT / rel).is_file()
    ]
    tests = test_module_files([rel for rel in files if rel.endswith(".rs")])
    return [rel for rel in files if rel not in tests]


def runtime_hits() -> list[str]:
    bad = []
    for rel in runtime_files():
        src = (ROOT / rel).read_text(errors="replace")
        if not ID.search(src):
            continue  # most files hold no id at all, so the lexer never runs
        for ln, text in runtime_literals(src, rel.endswith(".rs"), ID):
            for tok in ids(text):
                bad.append(f"{rel}:{ln}: {tok!r} in {' '.join(text.split())[:90]!r}")
    return bad


# --- compile-time text ------------------------------------------------------

# Issue 2443, ruling A: what the COMPILER shows a user building nano-ros or an
# app against it -- an `#error`, a `static_assert` message, a deprecation
# note, a proc-macro's refusal -- names what, where and how to fix, never an
# id. The id stays in a `//` comment above the construct.
# Literals and comments are told apart by `code_and_literals`, the lexer the
# runtime-log subject above uses.
CT_DIRS = ["packages", "zephyr", "cmake"]
CT_C_EXTS = (".h", ".hh", ".hpp", ".hxx", ".c", ".cc", ".cpp", ".cxx", ".inc")
# Not compiled into a user's build: vendored and test trees, examples,
# bindgen output, test-only Rust files.
CT_SKIP = re.compile(
    r"/(third-party|vendor|vendored|tests?|testing|benches|examples|fixtures)/"
    r"|/generated\.rs$|/tests\.rs$"
)
# C/C++: a diagnostic directive (`#error`, `#warning`, `#pragma message`),
# read as the whole logical line, quoted or not ...
CT_C_DIRECTIVE = re.compile(r"^[ \t]*#[ \t]*(?:error|warning|pragma[ \t]+message)\b(?:\\\n|[^\n])*", re.M)
# ... and a call-shaped construct whose string arguments the compiler prints.
CT_C_CALLS = re.compile(
    r"\b(?:static_assert|_Static_assert|_Pragma|nodiscard"
    r"|(?:__)?(?:deprecated|unavailable)(?:__)?"
    r"|\w*_DEPRECATED(?:_\w*)?)\s*\("
)
# `error(…)` / `warning(…)` only inside an `__attribute__((…))`.
CT_C_ATTR = re.compile(r"\b__attribute__\s*\(\(")
CT_C_ATTR_MSG = re.compile(r"\b(?:__)?(?:error|warning)(?:__)?\s*\(")
# Rust: `compile_error!`, and the attributes whose strings rustc prints.
CT_RS_MACRO = re.compile(r"\bcompile_error!\s*[(\[{]")
CT_RS_ATTR = re.compile(
    r"#!?\[\s*(?:deprecated|must_use|diagnostic::on_unimplemented)\b"
)
# A const-evaluated assert: `const _: () = assert!(…);`, `const X: () = {…};`,
# or a `const { … }` block. Only an assert's MESSAGE is read.
CT_RS_CONST = re.compile(r"\bconst\s+(?:_|[A-Za-z_]\w*)\s*:\s*\(\s*\)\s*=|\bconst\s*\{")
CT_RS_ASSERT = re.compile(r"\b(?:assert|const_assert)!\s*\(")
# A proc-macro crate's refusals (only in a crate with `proc-macro = true`).
CT_RS_PM = re.compile(
    r"\b(?:syn::|darling::)?Error::(?:new|new_spanned|custom)\s*\("
    r"|\b(?:abort|abort_call_site|emit_error|emit_call_site_error|emit_warning"
    r"|emit_call_site_warning)!\s*[(\[{]"
)
CT_OPEN = {"(": ")", "[": "]", "{": "}"}


def ct_close(code: str, at: int) -> int:
    """The offset just past the bracket matching the opener at `at`."""
    depth = 0
    for k in range(at, len(code)):
        ch = code[k]
        if ch in "([{":
            depth += 1
        elif ch in ")]}":
            depth -= 1
            if depth == 0:
                return k + 1
    return len(code)


def ct_first_comma(code: str, a: int, b: int) -> int:
    """The first top-level comma inside the call whose `(` is at `a`."""
    depth = 0
    for k in range(a, b):
        ch = code[k]
        if ch in "([{":
            depth += 1
        elif ch in ")]}":
            depth -= 1
        elif ch == "," and depth == 1:
            return k
    return b


def compile_time_literals(src: str, rust: bool, proc_macro: bool = False) -> list[tuple[int, str]]:
    """Every piece of compile-time text in `src` with its line: a string a
    compiler diagnostic prints, or (C/C++) a diagnostic directive's text."""
    code, spans = code_and_literals(src, rust)
    regions: list[tuple[int, int]] = []

    def call(m: re.Match) -> tuple[int, int]:
        return m.start(), ct_close(code, m.end() - 1)

    out: list[tuple[int, str]] = []
    if rust:
        for m in CT_RS_MACRO.finditer(code):
            regions.append(call(m))
        for m in CT_RS_ATTR.finditer(code):
            regions.append((m.start(), ct_close(code, code.index("[", m.start()))))
        if proc_macro:
            for m in CT_RS_PM.finditer(code):
                regions.append(call(m))
        for m in CT_RS_CONST.finditer(code):
            if code[m.end() - 1] == "{":
                lo, hi = m.start(), ct_close(code, m.end() - 1)
            else:
                semi = re.compile(r";").search(code, m.end())
                lo, hi = m.start(), (semi.end() if semi else len(code))
                # `= { … };` -- the item ends after its block.
                k = m.end()
                while k < len(code) and code[k].isspace():
                    k += 1
                if k < len(code) and code[k] == "{":
                    hi = ct_close(code, k)
            for a in CT_RS_ASSERT.finditer(code, lo, hi):
                end = ct_close(code, a.end() - 1)
                regions.append((ct_first_comma(code, a.end() - 1, end), end))
    else:
        # Text of a directive: the source with comments blanked but literal
        # contents kept, so a quoted and an unquoted message read alike.
        text = list(code)
        for _, a, b in spans:
            text[a:b] = src[a:b]
        text = "".join(text)
        for m in CT_C_DIRECTIVE.finditer(code):
            out.append((src.count("\n", 0, m.start()) + 1, text[m.start():m.end()]))
        for m in CT_C_CALLS.finditer(code):
            regions.append(call(m))
        for m in CT_C_ATTR.finditer(code):
            lo, hi = m.start(), ct_close(code, m.end() - 1)
            for e in CT_C_ATTR_MSG.finditer(code, lo, hi):
                regions.append(call(e))
    for ln, a, b in spans:
        if any(lo <= a < hi for lo, hi in regions):
            out.append((ln, src[a:b]))
    return sorted(out)


def ct_lang(rel: str) -> bool | None:
    """True for Rust, False for C/C++, None for neither (a tracked template
    `x.h.in` / `ffi_lib_rs.in` reads as the file it generates)."""
    name = rel[:-3] if rel.endswith(".in") else rel
    if name.endswith(".rs") or name.endswith("_rs"):
        return True
    if name.endswith(CT_C_EXTS):
        return False
    return None


def compile_time_files() -> list[tuple[str, bool, bool]]:
    files = subprocess.run(
        ["git", "ls-files", "--", *CT_DIRS], cwd=ROOT, capture_output=True, text=True,
        check=True,
    ).stdout.split()
    files = [
        rel for rel in files
        if ct_lang(rel) is not None and not CT_SKIP.search("/" + rel) and (ROOT / rel).is_file()
    ]
    # A Rust file compiled for tests only (`#[cfg(test)] mod x;`) is not read.
    tests = test_module_files([rel for rel in files if rel.endswith(".rs")])
    # Proc-macro crates: their `syn::Error`s are a user's compile errors.
    pm_roots = []
    for toml in subprocess.run(
        ["git", "ls-files", "--", *(f"{d}/**/Cargo.toml" for d in CT_DIRS)], cwd=ROOT,
        capture_output=True, text=True, check=True,
    ).stdout.split():
        if re.search(r"^\s*proc-macro\s*=\s*true", (ROOT / toml).read_text(), re.M):
            pm_roots.append(str(Path(toml).parent) + "/")
    return [
        (rel, bool(ct_lang(rel)), any(rel.startswith(r) for r in pm_roots))
        for rel in files if rel not in tests
    ]


def compile_time_hits() -> list[str]:
    bad = []
    for rel, rust, pm in compile_time_files():
        src = (ROOT / rel).read_text(errors="replace")
        if not ID.search(src):
            continue  # most files hold no id at all, so the lexer never runs
        for ln, text in compile_time_literals(src, rust, pm):
            for tok in ids(text):
                bad.append(f"{rel}:{ln}: {tok!r} in {' '.join(text.split())[:90]!r}")
    return bad


# --- text assembled away from its print -------------------------------------

# Issue 2443, ruling A: an error's text is printed far from where it is
# written -- a `Display` impl renders it when the caller logs or returns it,
# an `Err(…)` carries it up to a CLI refusal or a build-script panic. That
# text names what, where and how to fix, never an id. Read through
# `code_and_literals`, the lexer of the subjects above.
# Every crate under `packages/` (`LIBRARY_SKIP`: no example trees), except
# the CLI library, whose every literal its hard rule above reads already.
ASM_DIRS = ["packages"]
ASM_SKIP_PREFIXES = tuple(d + "/" for d in CLI_LIBRARY)
# (a) the body of an `impl Display for` / `impl Error for` (`fmt::`,
# `core::`, `std::` paths included). `Debug` is developer-facing (`{:?}`),
# so a `Debug` impl is not read.
ASM_IMPL = re.compile(r"\bimpl\b[^{;]*?\b(?:Display|Error)\s+for\b")
# (b) thiserror's `#[error("…")]` attribute.
ASM_ERROR_ATTR = re.compile(r"#\[\s*error\s*\(")
# (c) a literal handed to an error constructor or a named message builder,
# directly, through `format!`/`format_args!`/`concat!`, or through an error
# enum's variant (`Err(PlanError::Target(format!(…)))`). `.text` is the
# arena oracle's `Refusal` builder in `nros-node`.
ASM_CALLS = re.compile(
    r"Err|bail!|eyre!|anyhow!|ensure!|\.context|\.with_context|\.wrap_err(?:_with)?"
    r"|\.ok_or(?:_else)?|\.map_err|\.text"
)
ASM_VARIANT = re.compile(r"[A-Z]\w*")


def asm_carried(calls: list[tuple[str, int]]) -> bool:
    """Whether a literal inside `calls` is text an error constructor or a
    message builder carries."""
    names = [c[0].rsplit("::", 1)[-1] for c in calls]
    while names and RUST_WRAPPERS.fullmatch(names[0]):
        names = names[1:]
    if not names:
        return False
    if ASM_CALLS.fullmatch(names[0]):
        return True
    # A variant wrapping the text: `Err(PlanError::Target(…))`.
    return (
        ASM_VARIANT.fullmatch(names[0]) is not None and names[0] not in ("Ok", "Some")
        and len(names) > 1 and ASM_CALLS.fullmatch(names[1]) is not None
    )


def assembled_literals(src: str) -> list[tuple[int, str]]:
    """Every Rust string literal of `src` that is text assembled away from its
    print, with its line. Comments and `#[cfg(test)]` items are not read."""
    code, spans = code_and_literals(src, True)
    regions: list[tuple[int, int]] = []
    for m in ASM_IMPL.finditer(code):
        b = code.find("{", m.end())
        if b >= 0:
            regions.append((b, ct_close(code, b)))
    for m in ASM_ERROR_ATTR.finditer(code):
        regions.append((m.start(), ct_close(code, m.end() - 1)))
    return [
        (ln, src[a:b]) for ln, a, b in spans
        if any(lo <= a < hi for lo, hi in regions)
        or asm_carried(enclosing_calls(code, a - 1, 4))
    ]


def assembled_files() -> list[str]:
    files = subprocess.run(
        ["git", "ls-files", "--", *(f"{d}/*.rs" for d in ASM_DIRS)], cwd=ROOT,
        capture_output=True, text=True, check=True,
    ).stdout.split()
    files = [
        rel for rel in files
        if not LIBRARY_SKIP.search("/" + rel) and not rel.startswith(ASM_SKIP_PREFIXES)
        and (ROOT / rel).is_file()
    ]
    tests = test_module_files(files)
    return [rel for rel in files if rel not in tests]


def assembled_hits() -> list[str]:
    bad = []
    for rel in assembled_files():
        src = (ROOT / rel).read_text(errors="replace")
        if not ID.search(src):
            continue  # most files hold no id at all, so the lexer never runs
        for ln, text in assembled_literals(src):
            for tok in ids(text):
                bad.append(f"{rel}:{ln}: {tok!r} in {' '.join(text.split())[:90]!r}")
    return bad


# --- the book ---------------------------------------------------------------


def book_page_hits(rel: str, text: str) -> list[str]:
    # Blank out HTML comments (they do not render), keeping line numbers.
    text = HTML_COMMENT.sub(lambda m: "\n" * m.group(0).count("\n"), text)
    bad = []
    in_background = False
    for ln in book_contributor.walk(text.splitlines()):
        if not ln.in_fence and ln.text.startswith("#"):
            in_background = bool(DESIGN_BACKGROUND.match(ln.text))
        if ln.exempt:
            continue
        for tok in ids(LINK_TARGET.sub("]", ln.text)):
            if in_background and RFC_ONLY.match(tok):
                continue
            bad.append(f"{rel}:{ln.number}: {tok!r} in {ln.text.strip()[:90]!r}")
    return bad


def book_hits() -> list[str]:
    files = book_pages.tracked_user_pages()
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
    # RFC-0123 D3: a "Contributor" heading no longer exempts its body.
    assert len(hits) == 3 and "p.md:3:" in hits[0] and "p.md:8:" in hits[1] and "RFC-0116" in hits[2], \
        f"self-test: {hits}"
    # RFC-0123 D3: `## Design background` licenses RFC ids, and nothing else.
    bg = "# T\n\n## Design background\n\n- [x](a.md) RFC-0123\n- see issue 1043\n\n## After\n\nRFC-0001\n"
    hits = book_page_hits("b.md", bg)
    assert len(hits) == 2 and "issue 1043" in hits[0] and "b.md:10:" in hits[1], f"self-test: background {hits}"
    # Issue 2305 -- a build-script warning is read; a panic and written source are not.
    bs = (
        'fn a() { println!("cargo::warning=x: drop it (issue 1352)"); }\n'
        'fn b() { warn(&format!("x: low (phase-474 I1) {}", 1)); }\n'
        'fn c() { d.note("x: assumed (RFC-0100 D6)"); }\n'
        'fn d() { panic!("x: refused (issue 1896)"); }\n'
        'fn e() { w.write_all(b"// Generated (phase-479 W5)"); }\n'
    )
    got = []
    for ln, start, end in rust_strings(bs, spans=True):
        text = bs[start:end]
        before = bs[max(0, start - 80) : start].rstrip('r#"b')
        if WARNING_PREFIX.match(text) or WARNING_CALL.search(before):
            got += [(ln, t) for t in ids(text)]
    assert got == [(1, "issue 1352"), (2, "phase-474"), (3, "RFC-0100")], f"self-test: build {got}"
    # Issue 2443 -- a public diagnostic code (phase-552, ruling C) is not an id.
    assert ids("NROS-0412: the board states no locator") == [], "self-test: NROS code"
    # Issue 2443 -- a build-script panic text is read; a plain literal is not.
    rs = (
        'fn main() {\n'
        '    panic!("bad (issue 1896)");\n'
        '    let v = x.expect(&format!("no {} (phase-474)", y));\n'
        '    assert!(n < 3, "too many (RFC-0100)");\n'
        '    let s = "data (issue 1234)";\n'
        '}\n'
    )
    got = [(ln, t) for ln, text in panic_literals(rs) for t in ids(text)]
    assert got == [(2, "issue 1896"), (3, "phase-474"), (4, "RFC-0100")], f"self-test: panic {got}"
    # Issue 2443 -- a test-only module file is recognised, a plain one is not.
    decl = "#[cfg(test)]\nmod golden;\n#[cfg(test)]\npub(crate) mod tests_c;\nmod lower;\n"
    assert [m.group(1) for m in TEST_MOD_DECL.finditer(decl)] == ["golden", "tests_c"], "self-test: cfg(test) mod"
    # Issue 2443 -- a colcon refusal is read; a docstring and a comment are not.
    py = (
        '"""Module doc, RFC-0087 D2."""\n'
        'def f(t):\n'
        '    """Function doc, phase-445."""\n'
        '    # issue 2443 in a comment\n'
        '    raise ValueError(f"bad {t} (issue 1234)" + " see RFC-0098")\n'
    )
    got = [(ln, t) for ln, s in python_message_strings(py) for t in ids(s)]
    assert got == [(5, "issue 1234"), (5, "RFC-0098")], f"self-test: colcon {got}"
    # Issue 2443 -- a user-run script's printed text is read; a comment, an
    # assignment nothing prints and a heredoc into a file are not.
    sh = (
        '#!/usr/bin/env bash # issue 1000\n'
        '# phase-100 in a comment\n'
        'die() { echo "x: $*" >&2; exit 1; }\n'
        'echo "bad (issue 1234)" >&2  # issue 1001 trailing\n'
        'plan="not printed (RFC-0001)"\n'
        'note="# kept (Phase 218.C)\nsource x"\n'
        'printf \'%s\\n\' "$note"\n'
        'cat >"$out/f" <<EOF\nwritten (issue 2000)\nEOF\n'
        'cat <<\'EOF\'\nusage (RFC-0090)\nEOF\n'
        'if [ -z "$x" ]; then die "no x (phase-431 W1)"; fi\n'
        'echo "a # (issue 1235)"\n'
    )
    helpers = shell_print_helpers(comments.strip_comments(sh, "sh"), False, SH_PRINTERS)
    assert helpers == {"die"}, f"self-test: shell helpers {helpers}"
    got = shell_lines(*shell_printed(sh, False, SH_PRINTERS | helpers))
    assert got == [(4, "issue 1234"), (6, "Phase 218"), (13, "RFC-0090"), (15, "phase-431"),
                   (16, "issue 1235")], f"self-test: shell {got}"
    fish = (
        '# issue 1563 in a comment\n'
        'if set -q X\n    echo "error: retired (RFC-0103 D6)" >&2\nend\n'
        'set -l unused "data (issue 1111)"\n'
    )
    got = shell_lines(*shell_printed(fish, True, FISH_PRINTERS))
    assert got == [(3, "RFC-0103")], f"self-test: fish {got}"
    # Issue 2443 -- runtime log and panic text is read; a comment, a data
    # string, an assert's condition and a test module are not.
    rs = (
        '// issue 1000 in a comment\n'
        'const TABLE: &[&str] = &["RFC-0001 data"];\n'
        'fn f() {\n'
        '    log::warn!("dropped (issue 1234)");\n'
        '    nros_log::log_error!(logger, "{}: full (phase-412)", n);\n'
        '    let v = x.expect(&format!("no clock (issue 0709) {}", y));\n'
        '    assert_eq!(a, "issue 1111", "mismatch (RFC-0088)");\n'
        '    let s = "data (issue 1234)"; let c = \'"\'; let l: &\'a str = "x";\n'
        '    compile_error!("not a runtime print (issue 2222)");\n'
        '}\n'
        '#[cfg(test)]\nmod tests { fn t() { panic!("phase-200"); } }\n'
        '#[cfg(all(\n    test,\n    feature = "x"\n))]\nmod t2 { fn t() { panic!("phase-201"); } }\n'
    )
    got = [(ln, t) for ln, text in runtime_literals(rs, True) for t in ids(text)]
    assert got == [(4, "issue 1234"), (5, "phase-412"), (6, "issue 0709"), (7, "RFC-0088")], \
        f"self-test: runtime rust {got}"
    c = (
        '/* printf("phase-100") in a comment */\n'
        'static const char *names[] = { "issue 1000" };\n'
        'void f(void) {\n'
        '    char q = \'"\'; // printf("issue 1001")\n'
        '    fprintf(stderr, "heap low: %d B "\n'
        '            "(issue 0803)\\n", n);\n'
        '    LOG_ERR("no slot \\"%s\\" (RFC-0074)", name);\n'
        '    strcmp(key, "phase-519");\n'
        '}\n'
    )
    got = [(ln, t) for ln, text in runtime_literals(c, False) for t in ids(text)]
    assert got == [(6, "issue 0803"), (7, "RFC-0074")], f"self-test: runtime c {got}"
    # The board print helpers: the `Board` trait's `println`, `hprintln!`, a
    # board's C console helpers; a method named `print` on a value is not one.
    rs = (
        'fn f() {\n'
        '    B::println(format_args!("boot (phase-436 B2) {}", n));\n'
        '    <B as Board>::println(format_args!("x (issue 0636)"));\n'
        '    hprintln!("y (RFC-0109)");\n'
        '    report.print("data (issue 1111)");\n'
        '}\n'
    )
    got = [(ln, t) for ln, text in runtime_literals(rs, True) for t in ids(text)]
    assert got == [(2, "phase-436"), (3, "issue 0636"), (4, "RFC-0109")], \
        f"self-test: runtime board rust {got}"
    c = (
        'void f(void) {\n'
        '    uart_puts("no link (issue 0769)\\n");\n'
        '    nros_board_freertos_console_write("slicing ON (issue 2017)\\n");\n'
        '    semihosting_write0("x (phase-372)");\n'
        '}\n'
    )
    got = [(ln, t) for ln, text in runtime_literals(c, False) for t in ids(text)]
    assert got == [(2, "issue 0769"), (3, "issue 2017"), (4, "phase-372")], \
        f"self-test: runtime board c {got}"
    # Every package and example is read; vendored trees, an example's own
    # tests, build trees and the CLI library (read whole above) are not.
    for rel, read in [
        ("examples/qemu/rust/talker/src/main.rs", True),
        ("packages/boards/nros-board-x/c/entry.c", True),
        ("examples/qemu/rust/talker/tests/smoke.rs", False),
        ("packages/x/third-party/y.c", False),
        ("packages/cli/nros-cli-core/src/lib.rs", False),
    ]:
        got = not RUNTIME_SKIP.search("/" + rel) and not rel.startswith(RUNTIME_SKIP_PREFIXES)
        assert got == read, f"self-test: runtime file {rel} read={got}"
    # Issue 2443 -- compile-time text is read; a comment, an `#if` condition,
    # a plain string and an assert's condition are not.
    c = (
        '// #error "phase-100" in a comment\n'
        '#if NROS_X < 1 /* issue 1000 */\n'
        '#error "NROS_X must be >= 1 (issue 1015)"\n'
        '#  warning can\'t \\\n    see phase-200\n'
        '#endif\n'
        'static const char *k = "RFC-0001 data";\n'
        'static_assert(sizeof(int) == 4,\n              "int (issue #229)");\n'
        '__attribute__((deprecated("renamed (phase-515 C8)"))) void f(void);\n'
        '__attribute__((section("RFC-0002"))) int g;\n'
        'NROS_CPP_DEPRECATED_MSG("old (phase 536)") void h();\n'
    )
    got = [(ln, t) for ln, text in compile_time_literals(c, False) for t in ids(text)]
    assert got == [(3, "issue 1015"), (4, "phase-200"), (9, "issue #229"), (10, "phase-515"),
                   (12, "phase 536")], f"self-test: compile-time C {got}"
    rs = (
        '// compile_error!("issue 1000")\n'
        'macro_rules! m { () => { compile_error!("bad (issue 1234)"); } }\n'
        '#[deprecated(since = "0.6.0", note = "renamed (phase-515 R8)")]\n'
        'pub type A = B;\n'
        '#[must_use = "use it (RFC-0003)"]\n'
        'const _: () = assert!(X <= 4, "too big (issue 0810)");\n'
        'const _: () = { assert!(f("issue 1111"), concat!("a ", "(phase-446)")); };\n'
        'fn g() { const { assert!(Y, "no (RFC-0088)") }; let s = "data (issue 2222)";'
        ' let e = Error::new(span, "no (issue 1234)"); }\n'
        '#[cfg(test)]\nmod t { #[deprecated(note = "phase-999")] fn x() {} }\n'
    )
    got = [(ln, t) for ln, text in compile_time_literals(rs, True) for t in ids(text)]
    assert got == [(2, "issue 1234"), (3, "phase-515"), (5, "RFC-0003"), (6, "issue 0810"),
                   (7, "phase-446"), (8, "RFC-0088")], f"self-test: compile-time rust {got}"
    pm = 'fn f() -> syn::Error { syn::Error::new_spanned(x, format!("bad {} (issue 1043)", y)) }\n'
    got = [t for _, text in compile_time_literals(pm, True, proc_macro=True) for t in ids(text)]
    assert got == ["issue 1043"], f"self-test: compile-time proc-macro {got}"
    # Issue 2443 -- text assembled away from its print is read: a `Display`
    # body, `#[error]`, an `Err(…)`, the `.text(…)` builder; a `Debug` body, a
    # data `format!`, a comment and a test module are not.
    rs = (
        '// Err("issue 1000") in a comment\n'
        'impl fmt::Display for E {\n'
        '    fn fmt(&self, f: &mut fmt::Formatter<\'_>) -> fmt::Result {\n'
        '        write!(f, "too big (issue 1234)")\n'
        '    }\n'
        '}\n'
        'impl fmt::Debug for E { fn fmt(&self, f: &mut F) -> R { write!(f, "E (phase-100)") } }\n'
        '#[derive(thiserror::Error)]\n'
        'enum G { #[error("renamed (phase-390)")] Old }\n'
        'fn f() -> Result<(), String> {\n'
        '    let key = format!("data (RFC-0001) {}", 1);\n'
        '    let r = R::new().text("nros arena oracle (phase-412): ");\n'
        '    x.ok_or(format!("no port (RFC-0118 D6) {}", y))?;\n'
        '    Err(PlanError::Target("one only (RFC-0118 D1)".into()))\n'
        '}\n'
        '#[cfg(test)]\nmod tests { fn t() -> Result<(), String> { Err("phase-200".into()) } }\n'
    )
    got = [(ln, t) for ln, text in assembled_literals(rs) for t in ids(text)]
    assert got == [(4, "issue 1234"), (9, "phase-390"), (12, "phase-412"), (13, "RFC-0118"),
                   (14, "RFC-0118")], f"self-test: assembled {got}"
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

    bad = (
        source_hits() + library_hits() + book_hits() + build_script_warning_hits()
        + colcon_hits() + shell_hits() + runtime_hits() + compile_time_hits() + assembled_hits()
    )
    cli = args.cli
    if cli is None:
        from source_locate import nros_cli_bin

        cli = nros_cli_bin(checkout_only=True)
    rendered = "rendered help, board list/show and doctor --report"
    if cli:
        bad += rendered_hits(cli)
    if bad:
        print(f"{NAME}: internal ids on the user surface ({len(bad)} line(s)):", file=sys.stderr)
        for b in bad:
            print(f"  {b}", file=sys.stderr)
        print(
            "\nThe reader has no tree to look an id up in. Help says what the verb\n"
            "does (move the id to a comment above the string: `//`, or `#` in shell); a refusal names\n"
            "the next command; a chapter states the fact, moves contributor material\n"
            "to the Internals book, or lists an RFC under `## Design background`.\n"
            "If the only remedy is \"read issue N\", state the\n"
            "fact and file the refusal as a front-door issue. There is no allowlist.",
            file=sys.stderr,
        )
        return 1
    if not cli:
        return check_skip.unverified(
            NAME, f"no in-tree nros CLI (`just setup-cli`); {rendered} not read"
        )
    print(
        f"{NAME}: OK (cmd/ strings, CLI library, build-script panics, build-script warnings, colcon "
        f"messages, user-run shell scripts, runtime log and panic text, compile-time text, "
        f"assembled error text, {BOOK_SCOPE}, "
        f"{rendered})"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
