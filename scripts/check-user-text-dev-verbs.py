#!/usr/bin/env python3
"""No user is TOLD to run a dev-tier verb (RFC-0117 D1/D3, RFC-0124 D8; phase-542 W3).

RFC-0117 splits the CLI into a user tier (what `nros --help` shows) and a
hidden dev tier, `nros dev <noun> <verb>`, whose callers are build tools:
cmake, west, build.rs and `just`. A build tool CALLING a dev verb is the
design -- `NanoRosEntry.cmake`, the PX4 module glue and the shared leaf root
block all do it. What the user tier must never do is have a dev verb typed or
told: a dev verb is hidden from help, carries no promise to users and has no
user page. `check-book-audience` holds the book to that; this gate holds the
rest of what a user reads.

SPELLINGS. `nros dev <word>`, and every OLD spelling whose new spelling is a
dev verb (`sdk-root` for `dev sdk root`, `codegen-system` for `dev codegen system`, ...),
read from the CLI's own `ALIASES` tables through `lib/cli_aliases.py`, so a
new alias is covered without editing this gate. Some old spellings are also
English (`metadata`, `profile`), so an old spelling counts only
where a command starts: after a backtick, a quote, `$ ` or at line start.

SCOPE AND RULE -- what a user reads, never what a build tool executes:

  A. CMake (`*.cmake`, `CMakeLists.txt`, `*.cmake.in`, repo-wide): the text
     of `message(...)`, every mode. Only ADVICE is refused (see below): a
     FATAL_ERROR that names the dev call that FAILED ("`nros dev profile dir
     x` failed", "nros dev codegen: ...", "not what `nros dev codegen system`
     produced") is a diagnosis, and stays. A cmake COMMENT is out, and so is
     every other command: `execute_process(COMMAND ...)` and
     `add_custom_command(COMMAND ...)` are build tools calling the verb.
  B. Markdown in the user trees -- `examples/`, `integrations/`, the scaffold
     templates `nros new` writes (`packages/cli/cargo-nano-ros/templates/`)
     -- and the root `README.md`: EVERY hit, fenced or not, advice or not. A
     README is what a user reads and pastes. Contributor READMEs (`docs/`,
     `packages/**` outside the templates) are contributor audience, like
     `book-internals/`, and are not scanned.
  C. Every other text file in those user trees, comments stripped: EVERY hit
     (a Kconfig `help` text, a template's string literal). Shell, Python,
     `just` and make files are out: they are build tools, and a dev verb in
     one is a call.
  D. The CLI's own Rust string literals (`packages/cli/**/*.rs`, test modules
     and `tests/` excluded): only ADVICE, as in A. Most of them name the dev
     verb that prints them (`nros dev image sizing-descriptor: ...`) or label
     a call (`"nros dev image entity-inventory"`); neither is advice. A dev
     verb's own refusal speaks to its build-tool caller, and names a sibling
     by its noun path without the program name (`dev sdk path x`, "this verb
     with `--build`"), which is not a spelling this gate reads.

ADVICE is a dev-verb mention that is not a diagnosis (followed by `:`,
`failed`, `could not`, `reported`, `printed`, `produced`, `returned`,
`exited`, `unavailable`) and that either follows an imperative or a means
(`run`, `use`, `take`, `regenerate`, `try`, `invoke`, `call`, `with`, `via`),
is followed by `answers` / `prints`, or starts a line of the message as a
command to type (unquoted, with arguments or text after it).

MARKER. Advice with no user-tier verb to point at yet carries
`dev-verb-advice-ok: issue NNNN` in a comment from three lines above the
`message(` call or the string literal that holds it, down to the line of the
hit. NNNN must be a TRACKED OPEN issue (`docs/issues/
NNNN-*.md`, `status: open`), like a gap row of the RFC parity map: a marker
naming a missing or closed issue is refused, and so is a marker that excuses
nothing. Markdown and Kconfig take no marker.

The self-test runs on every invocation, on the normal path. Exit 0 when clean;
1 with one line per finding otherwise.
"""

from __future__ import annotations

import pathlib
import re
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "scripts" / "lib"))
import cli_aliases  # noqa: E402

USER_TREES = ("examples/", "integrations/", "packages/cli/cargo-nano-ros/templates/")
ROOT_DOCS = ("README.md",)
CLI_RUST = "packages/cli/"
SCRIPT_SUFFIXES = (".sh", ".bash", ".py", ".just", ".mk", ".ps1", ".bat")
SCRIPT_NAMES = ("justfile", "Justfile", "Makefile", "makefile", "GNUmakefile")
BINARY_SUFFIXES = (".png", ".jpg", ".jpeg", ".gif", ".svg", ".ico", ".pdf", ".bin",
                   ".elf", ".a", ".so", ".o", ".gz", ".zip", ".lock")
C_LIKE = (".c", ".h", ".cc", ".cpp", ".hpp", ".hh", ".cxx", ".rs", ".template",
          ".jinja", ".ld", ".js", ".ts", ".dts", ".dtsi", ".overlay")
XML_LIKE = (".xml", ".launch", ".html")
MARKER = re.compile(r"dev-verb-advice-ok:\s*issue\s+(\d{4})")
CMAKE_MODES = {"FATAL_ERROR", "SEND_ERROR", "WARNING", "AUTHOR_WARNING", "DEPRECATION",
               "NOTICE", "STATUS", "VERBOSE", "DEBUG", "TRACE", "CHECK_START",
               "CHECK_PASS", "CHECK_FAIL", "CONFIGURE_LOG"}

# The CLI is spelled "nros" + " dev" in the fixtures below, so neither this
# gate nor `check-cli-old-spellings` reads its own fixtures as findings.
NROS = "nros"

DIAG_WORDS = r"failed|could not|couldn't|reported|printed|produced|returned|exited|" \
             r"unavailable|errored|did not"
DIAG_QUOTED = re.compile(rf"\s*\)?\s*(?::|\b(?:{DIAG_WORDS})\b)")
DIAG_BARE = re.compile(rf"[^`\n]{{0,80}}?(?::(?:\s|$)|\b(?:{DIAG_WORDS})\b)")
ADVICE_BEFORE = re.compile(
    r"\b(?:run|rerun|re-run|use|take|regenerate|re-generate|try|invoke|call|with|via)"
    r"(?:\s+(?:it|this|one|them|again|a new one))?\s*$", re.I)
COMMAND_TAIL = re.compile(r"(?:^|\s)(?:--?[A-Za-z]|<[^>]+>|\{[^}]*\}|#)")
ADVICE_AFTER = re.compile(r"\s*(?:answers|prints|will answer)\b", re.I)


def build_pattern(olds: list[str]) -> re.Pattern[str]:
    old = "|".join(re.escape(o) for o in sorted(olds, key=len, reverse=True))
    alts = [rf"(?<![\w-]){NROS} dev [a-z][\w-]*"]
    if old:
        starts = "|".join([r"(?<=[`'\"])", r"(?<=\$ )", r"(?<=^)"]
                          + [rf"(?<=^[ \t>*-]{{{k}}})" for k in range(1, 9)])
        alts.append(rf"(?:{starts}){NROS} (?:{old})(?![\w-])")
    return re.compile("|".join(alts), re.M)


def dev_olds() -> list[str]:
    return [old for old, new in cli_aliases.load(ROOT) if new.startswith("dev ")]


def is_advice(text: str, start: int, end: int) -> bool:
    """`text` is the message as the user sees it (escapes resolved)."""
    line_start = text.rfind("\n", 0, start) + 1
    before = text[line_start:start]
    quoted = before.endswith("`")
    if quoted:
        close = text.find("`", end)
        after = text[close + 1:] if close >= 0 else text[end:]
        if DIAG_QUOTED.match(after.split("\n", 1)[0]):
            return False
    else:
        after = text[end:]
        if DIAG_BARE.match(after.split("\n", 1)[0]):
            return False
    if ADVICE_BEFORE.search(before.rstrip("`").rstrip()):
        return True
    if ADVICE_AFTER.match(after.split("\n", 1)[0]):
        return True
    # A command at the start of a line, with a flag, a placeholder or a shell
    # comment after it: one to type. Command words alone are a label
    # (`"nros dev image entity-inventory"`, an error prefix's argument).
    if not quoted and before.strip() in ("", "$") and COMMAND_TAIL.search(after.split("\n", 1)[0]):
        return True
    return False


# --- markers ------------------------------------------------------------------

_ISSUE_STATUS: dict[str, str | None] = {}


def issue_status(nnnn: str) -> str | None:
    """`open`, another status, or None when no open-tree issue file exists."""
    if nnnn not in _ISSUE_STATUS:
        found = sorted((ROOT / "docs" / "issues").glob(f"{nnnn}-*.md"))
        status = None
        if found:
            m = re.search(r"^status:\s*(\S+)", found[0].read_text(errors="replace"), re.M)
            status = m.group(1) if m else "unknown"
        _ISSUE_STATUS[nnnn] = status
    return _ISSUE_STATUS[nnnn]


def marker_in(lines: list[str], lo: int, hi: int) -> tuple[int, str] | None:
    """(line, issue) of the first marker on 1-based lines lo..hi."""
    for n in range(max(lo, 1), min(hi, len(lines)) + 1):
        m = MARKER.search(lines[n - 1])
        if m:
            return n, m.group(1)
    return None


def judge(rel: str, line: int, spelled: str, lines: list[str], lo: int,
          used: set[int], status) -> list[str]:
    mk = marker_in(lines, lo, line)
    if mk is None:
        return [f"{rel}:{line}: tells the user to run `{spelled}`, a dev-tier verb -- name a "
                "user-tier verb or action, or mark a gap `dev-verb-advice-ok: issue NNNN` "
                "(RFC-0117 D3)"]
    used.add(mk[0])
    st = status(mk[1])
    if st != "open":
        why = "names no tracked issue" if st is None else f"names issue {mk[1]}, which is `{st}`"
        return [f"{rel}:{mk[0]}: dev-verb-advice-ok marker {why} -- a marker needs a tracked "
                "OPEN issue"]
    return []


def stale_markers(rel: str, lines: list[str], used: set[int]) -> list[str]:
    return [f"{rel}:{n}: dev-verb-advice-ok marker excuses nothing -- remove it"
            for n, ln in enumerate(lines, 1) if MARKER.search(ln) and n not in used]


# --- A. cmake message() -------------------------------------------------------

def cmake_strip_comments(text: str) -> str:
    """Blank out `#` comments (line and `#[[ ]]` bracket), keeping strings and
    line structure."""
    out, i, n, in_str = [], 0, len(text), False
    while i < n:
        c = text[i]
        if in_str:
            if c == "\\" and i + 1 < n:
                out.append(text[i:i + 2]); i += 2; continue
            if c == '"':
                in_str = False
            out.append(c); i += 1; continue
        if c == '"':
            in_str = True; out.append(c); i += 1; continue
        if c == "#":
            m = re.match(r"#\[(=*)\[", text[i:])
            if m:
                end = text.find("]" + m.group(1) + "]", i)
                end = n if end < 0 else end + len(m.group(1)) + 2
            else:
                end = text.find("\n", i)
                end = n if end < 0 else end
            out.append(re.sub(r"[^\n]", " ", text[i:end])); i = end; continue
        out.append(c); i += 1
    return "".join(out)


MESSAGE = re.compile(r"(?<![\w])message\s*\(", re.I)
ESCAPES = {"n": "\n", "t": "\t", "r": "\r", '"': '"', "\\": "\\", ";": ";"}


def message_calls(clean: str) -> list[tuple[int, str, list[int]]]:
    """(offset of `message(`, joined text, source offset per char) per call.
    Arguments are concatenated as cmake does for message(); the mode keyword
    is dropped and escapes resolved."""
    calls = []
    for m in MESSAGE.finditer(clean):
        i, depth, chars, offs, first = m.end(), 1, [], [], True
        while i < len(clean) and depth:
            c = clean[i]
            if c == '"':
                i += 1
                while i < len(clean) and clean[i] != '"':
                    if clean[i] == "\\" and i + 1 < len(clean):
                        if clean[i + 1] == "\n":
                            i += 2; continue
                        chars.append(ESCAPES.get(clean[i + 1], clean[i + 1])); offs.append(i)
                        i += 2; continue
                    chars.append(clean[i]); offs.append(i); i += 1
                i += 1; first = False; continue
            if c == "(":
                depth += 1
            elif c == ")":
                depth -= 1
                if not depth:
                    break
            if c.isspace() or c in "()":
                i += 1; continue
            j = i
            while j < len(clean) and not clean[j].isspace() and clean[j] not in '()"':
                j += 1
            tok = clean[i:j]
            if not (first and tok.upper() in CMAKE_MODES):
                chars.extend(tok); offs.extend(range(i, j))
            first = False
            i = j
        calls.append((m.start(), "".join(chars), offs))
    return calls


def cmake_findings(rel: str, text: str, pat: re.Pattern[str], status) -> list[str]:
    clean = cmake_strip_comments(text)
    lines = text.splitlines()
    out, used = [], set()
    for at, msg, offs in message_calls(clean):
        call_line = clean.count("\n", 0, at) + 1
        for m in pat.finditer(msg):
            if not is_advice(msg, m.start(), m.end()):
                continue
            line = clean.count("\n", 0, offs[m.start()]) + 1
            out += judge(rel, line, m.group(0), lines, call_line - 3, used, status)
    return out + stale_markers(rel, lines, used)


# --- B / C. user-tree text ----------------------------------------------------

def every_hit(rel: str, text: str, pat: re.Pattern[str]) -> list[str]:
    out = []
    for n, line in enumerate(text.splitlines(), 1):
        for m in pat.finditer(line):
            out.append(f"{rel}:{n}: `{m.group(0)}` is a dev-tier verb in user-facing text "
                       "-- name a user-tier verb or action instead (RFC-0117 D3)")
    return out


def strip_comments(rel: str, text: str) -> str:
    """Comments blanked, line structure kept. A comment marker counts at line
    start or after whitespace, which is how every template here writes one."""
    blank = lambda m: re.sub(r"[^\n]", " ", m.group(0))  # noqa: E731
    suffix = pathlib.PurePath(rel).suffix.lower()
    if suffix in C_LIKE:
        text = re.sub(r"/\*.*?\*/", blank, text, flags=re.S)
        text = re.sub(r"(^|\s)//[^\n]*", lambda m: m.group(1), text)
        if suffix == ".jinja":
            text = re.sub(r"\{#.*?#\}", blank, text, flags=re.S)
        return text
    if suffix in XML_LIKE:
        return re.sub(r"<!--.*?-->", blank, text, flags=re.S)
    return re.sub(r"(^|\s)#[^\n]*", lambda m: m.group(1), text)


# --- D. CLI Rust string literals ----------------------------------------------

def rust_strings(text: str) -> list[tuple[str, list[int]]]:
    """(content, source offset per char) of every string literal outside
    comments. Handles escapes, backslash-newline continuations, raw strings
    and char literals (so a quote char literal opens nothing)."""
    out, i, n = [], 0, len(text)
    while i < n:
        c = text[i]
        if text.startswith("//", i):
            j = text.find("\n", i); i = n if j < 0 else j; continue
        if text.startswith("/*", i):
            j = text.find("*/", i + 2); i = n if j < 0 else j + 2; continue
        if c == "'":
            m = re.match(r"'(?:\\(?:u\{[0-9a-fA-F]+\}|x[0-9a-fA-F]{2}|.)|[^\\'])'", text[i:])
            i += m.end() if m else 1
            continue
        m = re.match(r'b?r(#*)"', text[i:])
        if m and (i == 0 or not (text[i - 1].isalnum() or text[i - 1] == "_")):
            close = '"' + m.group(1)
            s = i + m.end()
            j = text.find(close, s)
            j = n if j < 0 else j
            out.append((text[s:j], list(range(s, j))))
            i = j + len(close); continue
        if c == '"':
            i += 1
            chars, offs = [], []
            while i < n and text[i] != '"':
                if text[i] == "\\" and i + 1 < n:
                    if text[i + 1] == "\n":
                        i += 2
                        while i < n and text[i] in " \t":
                            i += 1
                        continue
                    chars.append({"n": "\n", "t": "\t"}.get(text[i + 1], text[i + 1]))
                    offs.append(i); i += 2; continue
                chars.append(text[i]); offs.append(i); i += 1
            out.append(("".join(chars), offs)); i += 1; continue
        i += 1
    return out


def rust_without_tests(text: str) -> str:
    """The text with everything from the first `#[cfg(test)]` module on blanked."""
    m = re.search(r"^\s*#\[cfg\(test\)\]\s*\n\s*(?:pub(?:\([^)]*\))?\s+)?mod\s", text, re.M)
    return text if not m else text[:m.start()] + re.sub(r"[^\n]", " ", text[m.start():])


def rust_findings(rel: str, text: str, pat: re.Pattern[str], status) -> list[str]:
    body = rust_without_tests(text)
    lines = text.splitlines()
    out, used = [], set()
    for s, offs in rust_strings(body):
        for m in pat.finditer(s):
            if not is_advice(s, m.start(), m.end()):
                continue
            line = body.count("\n", 0, offs[m.start()]) + 1
            first = body.count("\n", 0, offs[0]) + 1
            out += judge(rel, line, m.group(0), lines, first - 3, used, status)
    return out + stale_markers(rel, lines, used)


# --- scope ---------------------------------------------------------------------

def is_cmake(rel: str) -> bool:
    name = pathlib.PurePath(rel).name
    return name == "CMakeLists.txt" or name.endswith(".cmake") or name.endswith(".cmake.in")


def classify(rel: str) -> str | None:
    """'cmake', 'md', 'text', 'rust', or None (out of scope)."""
    if rel.startswith("third-party/") or "/third-party/" in rel:
        return None
    if is_cmake(rel):
        return "cmake"
    p = pathlib.PurePath(rel)
    suffix = p.suffix.lower()
    in_user_tree = rel.startswith(USER_TREES)
    if rel in ROOT_DOCS or (in_user_tree and suffix == ".md"):
        return "md"
    if in_user_tree:
        if suffix in SCRIPT_SUFFIXES or p.name in SCRIPT_NAMES or suffix in BINARY_SUFFIXES:
            return None
        return "text"
    if rel.startswith(CLI_RUST) and suffix == ".rs" and "/tests/" not in f"/{rel}" \
            and not rel.endswith("_tests.rs"):
        return "rust"
    return None


def findings(rel: str, text: str, pat: re.Pattern[str], status=issue_status) -> list[str]:
    kind = classify(rel)
    if kind == "cmake":
        return cmake_findings(rel, text, pat, status)
    if kind == "md":
        return every_hit(rel, text, pat)
    if kind == "text":
        return every_hit(rel, strip_comments(rel, text), pat)
    if kind == "rust":
        return rust_findings(rel, text, pat, status)
    return []


def self_test() -> None:
    """Negative controls on the normal path -- a rule that never fires proves nothing."""
    pat = build_pattern(["sdk-root", "metadata", "ws board-facts", "codegen-system"])
    st = {"0001": "open", "0002": "resolved"}.get
    F = lambda rel, text: findings(rel, text, pat, st)  # noqa: E731
    dv = f"{NROS} dev sdk root"
    # A. advice fires, in every mode.
    for mode in ("FATAL_ERROR", "WARNING", "STATUS", "SEND_ERROR", "NOTICE", "AUTHOR_WARNING"):
        cm = f'message({mode}\n  "put it on PATH -- `{dv}` answers this.")\n'
        assert len(F("x/CMakeLists.txt", cm)) == 1, mode
    for advice in (f'"Regenerate it with `{NROS} dev codegen`."', f'"then run `{dv}`"',
                   f'"Build it:\\n"\n "    {NROS} dev lock central-patch --x y\\n"',
                   f'"take a new one with `{NROS} dev image entity-census take`"',
                   f'"run `{NROS} sdk-root` first"'):
        assert len(F("a.cmake", f"message(FATAL_ERROR {advice})")) == 1, advice
    # A. a diagnosis naming the FAILED call does not fire.
    for diag in (f'"nano-ros: `{NROS} dev profile dir ${{p}}` failed"',
                 f'"{NROS} dev codegen failed for ${{t}} (exit ${{rc}})"',
                 f'"{NROS} dev codegen: the regenerated tree is STILL old"',
                 f'"${{ctx}}: `{NROS} dev codegen source-language` could not name it"',
                 f'"resolved (`{NROS} dev image board-facts`):\\n${{err}}"',
                 f'"not what `{NROS} dev codegen system` produced"',
                 f'"`{NROS} dev image entity-census path` unavailable"',
                 f'"{NROS} dev codegen system produced no header"'):
        assert F("a.cmake", f"message(FATAL_ERROR {diag})") == [], diag
    # The diagnosis wins over an advice word before it.
    assert F("a.cmake", f'message(FATAL_ERROR "called with `{NROS} dev codegen x` failed")') == []
    assert F("a.cmake", f'message(FATAL_ERROR "with {NROS} dev codegen x: failed")') == []
    # Description is not advice either.
    assert F("a.cmake", f'message(STATUS "the entry passes it to `{NROS} dev image x`.")') == []
    # Comments and build-tool calls are out.
    assert F("x/a.cmake", f"# run `{dv}` -- it answers this.\nmessage(STATUS hi)\n") == []
    assert F("x/a.cmake", f'# message(STATUS "then run `{dv}`")\nset(A 1)\n') == []
    assert F("x/a.cmake", f'execute_process(COMMAND "${{N}}" dev sdk root)\n'
             f'add_custom_command(OUTPUT o COMMAND {dv} x)\n') == []
    # A `)` or `#` inside the string neither ends nor comments it; lines are mapped.
    assert len(F("a.cmake", f'message(FATAL_ERROR "(x) # y: then run `{dv}`")')) == 1
    f = F("a.cmake", f'set(A 1)\nmessage(STATUS\n "a"\n "then run `{dv}`")\n')
    assert f and f[0].startswith("a.cmake:4:"), f
    # Old dev spellings at a command start only; the English word does not fire.
    assert F("a.cmake", f'message(STATUS "with the {NROS} metadata of x")') == []
    assert F("a.cmake", 'message(STATUS "with nano-ros dev build")') == []
    # Markers: an open issue excuses; a closed or missing one, or a stale one, is refused.
    adv = f'message(FATAL_ERROR\n "then run `{dv}`")\n'
    assert F("a.cmake", "# dev-verb-advice-ok: issue 0001\n" + adv) == []
    assert len(F("a.cmake", "# dev-verb-advice-ok: issue 0002\n" + adv)) == 1
    assert len(F("a.cmake", "# dev-verb-advice-ok: issue 0003\n" + adv)) == 1
    assert len(F("a.cmake", "# dev-verb-advice-ok: issue 0001\nmessage(STATUS hi)\n")) == 1
    # B. markdown: every hit, in user trees only; a marker does not excuse it.
    md = f"Run it:\n```bash\n{dv}\n```\nit (`{NROS} dev codegen x`) failed\n"
    assert len(F("examples/x/README.md", md)) == 2
    assert len(F("integrations/px4/README.md",
                 f"<!-- dev-verb-advice-ok: issue 0001 -->\n`{NROS} codegen-system --x`")) == 1
    assert len(F("README.md", f"use `{dv}`")) == 1
    assert F("packages/cli/README.md", md) == [] and F("docs/design/README.md", md) == []
    # C. Kconfig help fires; its comment does not; a script is a call.
    kc = f"# `{dv}` writes this.\nconfig X\n\thelp\n\t  Emitted by `{NROS} codegen-system`.\n"
    assert len(F("integrations/px4/x/Kconfig", kc)) == 1
    tpl = f"// `{dv}` renders this.\n/* {dv} */\nconst char *s = \"{dv}\";\n"
    assert len(F("integrations/px4/x/a.cpp.template", tpl)) == 1
    assert F("examples/x/run.sh", f"{dv}\n") == [] and F("examples/x/gen.py", f"print('{dv}')") == []
    assert F("third-party/x/CMakeLists.txt", f'message(STATUS "then run `{dv}`")') == []
    # D. CLI Rust: advice fires; prefixes, labels, comments, tests and char literals do not.
    r = "packages/cli/nros-cli-core/src/a.rs"
    assert len(F(r, f'fn f() {{ println!("  {NROS} dev metadata --build   # record it"); }}')) == 1
    assert len(F(r, f'let s = format!("{NROS} dev image entity-census take --image {{b}}");')) == 1
    assert len(F(r, f'bail!("hint: then run `{NROS} dev metadata --build` \\\n     to make it.");')) == 1
    assert F(r, f'eprintln!("{NROS} dev image sizing-descriptor: {{n}}");') == []
    assert F(r, f'Verb::Codegen => "{NROS} dev codegen",') == []
    assert F(r, f'Verb::GenerateRust => "{NROS} dev codegen rust",') == []
    assert F(r, f'report(&inv, "{NROS} dev image entity-inventory")?;') == []
    assert F(r, f'// run `{dv}`\n/// then run `{dv}`\nlet q = \'"\';') == []
    assert F(r, f'"# Generated by `{NROS} dev codegen system` -- do not edit."') == []
    assert F(r, f'#[cfg(test)]\nmod tests {{\n  const S: &str = "then run `{dv}`";\n}}\n') == []
    assert F("packages/cli/x/tests/t.rs", f'"then run `{dv}`"') == []
    assert F(r, f'// dev-verb-advice-ok: issue 0001\nlet s = "then run `{dv}`";') == []
    assert F(r, f'// dev-verb-advice-ok: issue 0001\nf(\n  x,\n  "a \\\n   b \\\n   c \\\n  then run `{dv}`");') == []
    assert len(F(r, f'let s = r#"then run `{dv}`"#;')) == 1
    assert len(F(r, f'let q = \'"\'; let s = "then run `{dv}`";')) == 1
    assert len(F(r, f'let p = r"C:\\"; let s = "then run `{dv}`";')) == 1


def main() -> int:
    self_test()
    if "--self-test" in sys.argv:
        print("check-user-text-dev-verbs self-test: OK")
        return 0
    try:
        olds = dev_olds()
    except (OSError, ValueError) as e:
        sys.exit(f"check-user-text-dev-verbs: cannot read the CLI alias tables: {e}")
    if not olds:
        sys.exit("check-user-text-dev-verbs: the CLI alias tables name NO dev-tier spelling "
                 "-- an empty table would pass vacuously.")
    pat = build_pattern(olds)
    r = subprocess.run(["git", "-C", str(ROOT), "ls-files", "-z"],
                       capture_output=True, text=True, check=False)
    if r.returncode != 0:
        sys.exit(f"check-user-text-dev-verbs: `git ls-files` failed:\n  {r.stderr.strip()}")
    errors, scanned = [], {"cmake": 0, "md": 0, "text": 0, "rust": 0}
    for rel in r.stdout.split("\0"):
        kind = classify(rel) if rel else None
        if kind is None:
            continue
        path = ROOT / rel
        if not path.is_file() or path.is_symlink():
            continue
        try:
            text = path.read_text(encoding="utf-8")
        except (UnicodeDecodeError, OSError):
            continue
        scanned[kind] += 1
        errors += findings(rel, text, pat)
    for kind, n in scanned.items():
        if n == 0:
            sys.exit(f"check-user-text-dev-verbs: scanned NO {kind} files "
                     "-- an empty scan passes vacuously.")
    if errors:
        print("\n".join(errors))
        print(f"check-user-text-dev-verbs: {len(errors)} finding(s). A build tool may CALL a "
              "dev verb; user text never has one typed or told (RFC-0117 D3, RFC-0124 D8).",
              file=sys.stderr)
        return 1
    print(f"check-user-text-dev-verbs: OK ({scanned['cmake']} cmake, {scanned['md']} markdown, "
          f"{scanned['text']} other user-tree, {scanned['rust']} CLI Rust files)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
