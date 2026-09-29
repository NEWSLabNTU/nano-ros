"""A comment is not code — phase-472 W3.

A gate that matches raw text lets a COMMENT satisfy "the code does X". The
2026-09-28 audit found nine gates doing it, and one was a live mask: a doc
comment in `nros-rmw-xrce-cffi/src/lib.rs` counted as the xrce registration, so
deleting both real calls passed `check-entry-rmw-vocabulary`. The tree also
carried some fifty PRIVATE strippers, each with its own blind spot (`//` inside
a string literal, a trailing comment, a `/* */` spanning lines, a raw string).

So there is ONE stripper per language family, here:

* C family — `c`, `cpp`, `rust`: `//` (incl. `///`, `//!`), `/* */` (incl.
  `/** */`; NESTED in Rust, not in C/C++), C/C++ backslash-continued `//`.
  Respects string and char literals (a `"//"` is code), Rust raw strings
  (`r"…"`, `r#"…"#`, `br`/`cr`), Rust lifetimes (`'a` is not a char literal),
  C++ raw strings (`R"d(…)d"`) and C++14/C23 digit separators (`1'000`).
* `#` family — `sh`, `just`, `python`, `toml`, `yaml`, `cmake`. Respects each
  language's quotes; shell/just `#` only at a WORD start (`$#`, `${#x}` are
  code) and heredoc bodies (text, never comments); YAML `#` only after
  whitespace, quotes only at a scalar's start (`don't` is not a quote), block
  scalars (`run: |` bodies are CONTENT — strip them as `sh` separately);
  CMake bracket comments `#[[…]]` / `#[=[…]=]` and bracket arguments `[[…]]`.

The output has the SAME LENGTH as the input and every newline kept: comment text
becomes spaces, so offsets and line numbers still point into the original file.
`strings=True` also blanks the CONTENTS of string literals (delimiters kept) and
of heredoc bodies — for a gate asking about code TOKENS, where a string holding
`{` or `nros_foo(` must not count either.

An UNTERMINATED block comment runs to end of file (the compiler rejects it; a
gate that read what follows as code would credit text the build never sees),
so the failure direction is "less evidence", which fails closed.

Not modelled, deliberately: preprocessor-disabled code (`#if 0`) and `cfg`d-out
Rust are code a gate may or may not want — that is a per-gate question, not a
comment. `$(…)` nested inside a shell double-quoted string is scanned as shell.

Shell gates: `python3 scripts/lib/comments.py [--strings] [--lang L] FILE…`
prints the stripped text (language from the suffix unless `--lang`).
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

C_FAMILY = ("c", "cpp", "rust")
HASH_FAMILY = ("sh", "just", "python", "toml", "yaml", "cmake")
LANGS = C_FAMILY + HASH_FAMILY

_SUFFIX = {
    ".rs": "rust",
    ".c": "c",
    ".h": "c",
    ".cpp": "cpp",
    ".cc": "cpp",
    ".cxx": "cpp",
    ".hpp": "cpp",
    ".hh": "cpp",
    ".hxx": "cpp",
    ".ipp": "cpp",
    ".sh": "sh",
    ".bash": "sh",
    ".py": "python",
    ".toml": "toml",
    ".yml": "yaml",
    ".yaml": "yaml",
    ".just": "just",
    ".cmake": "cmake",
}
_NAME = {"justfile": "just", "Justfile": "just", ".justfile": "just", "CMakeLists.txt": "cmake"}


def lang_for(path) -> str | None:
    """The language family of a path, or None if this module has no stripper for it."""
    p = Path(path)
    return _NAME.get(p.name) or _SUFFIX.get(p.suffix)


def strip_comments(text: str, lang: str, *, strings: bool = False) -> str:
    """`text` with every comment blanked (same length, newlines kept)."""
    return _run(text, lang, strings, list(text))


def code_mask(text: str, lang: str, *, strings: bool = True) -> list[bool]:
    """Per offset: True where the character is CODE — not inside a comment
    (nor, with `strings`, a string literal's contents). Unlike comparing the
    stripped text with the original, WHITESPACE inside a comment or a string is
    False too, so a `^\\s*fn` match that starts in a string's indentation is
    not mistaken for code."""
    out = _Recorder(text)
    _run(text, lang, strings, out)
    return [not h for h in out.hit]


class _Recorder(list):
    """An `out` buffer that remembers which offsets a scanner blanked."""

    def __init__(self, text):
        super().__init__(text)
        self.hit = [False] * len(text)

    def __setitem__(self, k, v):
        self.hit[k] = True
        super().__setitem__(k, v)


def _run(text: str, lang: str, strings: bool, out: list) -> str:
    if lang in C_FAMILY:
        return _strip_c(text, lang, strings, out)
    if lang in ("sh", "just"):
        # `just` strips a recipe's indentation before the shell sees it, so a
        # heredoc terminator is indented IN THE FILE and must match stripped.
        return _strip_sh(text, strings, indented=lang == "just", out=out)
    if lang == "python":
        return _strip_quoted_hash(text, strings, escapes_in_single=True, out=out)
    if lang == "toml":
        # TOML literal strings (single-quoted) have no escapes.
        return _strip_quoted_hash(text, strings, escapes_in_single=False, out=out)
    if lang == "yaml":
        return _strip_yaml(text, strings, out)
    if lang == "cmake":
        return _strip_cmake(text, strings, out)
    raise ValueError(f"comments.strip_comments: no stripper for language {lang!r} (know: {LANGS})")


def strip_file(path, *, lang: str | None = None, strings: bool = False) -> str:
    """Read `path` and strip it by its suffix. An unknown suffix is an ERROR,
    never a pass-through: silently returning raw text is the hole this closes."""
    lang = lang or lang_for(path)
    if lang is None:
        raise ValueError(f"comments.strip_file: no language for {path} — pass lang=")
    return strip_comments(Path(path).read_text(errors="replace"), lang, strings=strings)


# --------------------------------------------------------------------------
# helpers


def _blank(out: list, a: int, b: int) -> None:
    for k in range(a, min(b, len(out))):
        if out[k] != "\n":
            out[k] = " "


def _is_ident(ch: str) -> bool:
    return ch.isalnum() or ch == "_"


def _eol(text: str, i: int) -> int:
    j = text.find("\n", i)
    return len(text) if j < 0 else j


# --------------------------------------------------------------------------
# C family

_RUST_RAW = re.compile(r"(?<![A-Za-z0-9_])(?:br|cr|r)(#*)\"")
_CPP_RAW = re.compile(r"(?<![A-Za-z0-9_])(?:u8|u|U|L)?R\"([^ ()\\\t\v\f\n\"]{0,16})\(")


def _strip_c(text: str, lang: str, strings: bool, out: list) -> str:
    rust = lang == "rust"
    n = len(text)
    i = 0
    while i < n:
        c = text[i]
        if c == "/" and text.startswith("//", i):
            j = _eol(text, i)
            # C/C++ line splicing: a `//` comment ending in `\` eats the next line.
            while not rust and j < n and text[i:j].rstrip("\r").endswith("\\"):
                j = _eol(text, j + 1)
            _blank(out, i, j)
            i = j
            continue
        if c == "/" and text.startswith("/*", i):
            depth, j = 1, i + 2
            while j < n and depth:
                if text.startswith("*/", j):
                    depth -= 1
                    j += 2
                elif rust and text.startswith("/*", j):
                    depth += 1
                    j += 2
                else:
                    j += 1
            _blank(out, i, j)  # unterminated: to EOF
            i = j
            continue
        if rust and c in "brc":
            m = _RUST_RAW.match(text, i)
            if m:
                body = m.end()
                close = '"' + m.group(1)
                e = text.find(close, body)
                end = n if e < 0 else e
                if strings:
                    _blank(out, body, end)
                i = n if e < 0 else e + len(close)
                continue
        if lang == "cpp" and c in "uULR":
            m = _CPP_RAW.match(text, i)
            if m:
                body = m.end()
                close = ")" + m.group(1) + '"'
                e = text.find(close, body)
                end = n if e < 0 else e
                if strings:
                    _blank(out, body, end)
                i = n if e < 0 else e + len(close)
                continue
        if c == '"':
            j = i + 1
            while j < n and text[j] != '"':
                if text[j] == "\\":
                    j += 2
                    continue
                if text[j] == "\n" and not rust:
                    break  # an unterminated C string ends at the line
                j += 1
            j = min(j, n)
            if strings:
                _blank(out, i + 1, j)
            i = j + 1 if j < n and text[j] == '"' else j
            continue
        if c == "'":
            j = _char_literal_end(text, i, rust)
            if j is None:
                i += 1  # a lifetime / label / digit separator: code
                continue
            if strings:
                _blank(out, i + 1, j - 1)
            i = j
            continue
        i += 1
    return "".join(out)


def _char_literal_end(text: str, i: int, rust: bool):
    """End (exclusive) of the char literal opening at `i`, or None if `'` is not one."""
    n = len(text)
    if rust:
        if i + 1 < n and text[i + 1] == "\\":
            e = text.find("'", i + 3)
            nl = text.find("\n", i)
            if e < 0 or (0 <= nl < e) or e - i > 12:
                return None
            return e + 1
        if i + 2 < n and text[i + 2] == "'" and text[i + 1] != "\n":
            return i + 3
        return None  # lifetime or label
    # C/C++: a `'` inside a NUMBER is a digit separator (C++14, C23).
    k = i
    while k > 0 and (_is_ident(text[k - 1]) or text[k - 1] in ".'"):
        k -= 1
    if k < i and text[k].isdigit():
        return None
    j = i + 1
    while j < n and text[j] not in "'\n":
        j += 2 if text[j] == "\\" else 1
    return j + 1 if j < n and text[j] == "'" else min(j, n)


# --------------------------------------------------------------------------
# shell / just

_HEREDOC = re.compile(r"<<(-?)\s*(['\"]?)([A-Za-z_][A-Za-z0-9_]*)\2")
_SH_WORD_START = " \t\n;&|()<>"


def _strip_sh(text: str, strings: bool, indented: bool = False, out: list | None = None) -> str:
    out = list(text) if out is None else out
    n = len(text)
    pending: list[tuple[str, bool]] = []  # heredocs opened on this line

    def dq(i: int) -> int:
        """Scan a "…" string opening at i; return index after its close."""
        j = i + 1
        while j < n and text[j] != '"':
            if text[j] == "\\":
                j += 2
                continue
            if text.startswith("$(", j) and not text.startswith("$((", j):
                j = code(j + 2, close=")")
                continue
            j += 1
        if strings:
            _blank_keep_subst(out, text, i + 1, min(j, n))
        return min(j + 1, n)

    def code(i: int, close: str | None = None) -> int:
        depth = 0
        while i < n:
            c = text[i]
            if c == "\n":
                i += 1
                while pending:
                    term, dash = pending.pop(0)
                    i = heredoc_body(i, term, dash)
                continue
            if c == "\\":
                i += 2
                continue
            if c == "#" and (i == 0 or text[i - 1] in _SH_WORD_START):
                j = _eol(text, i)
                _blank(out, i, j)
                i = j
                continue
            if c == "'":
                ansi = i > 0 and text[i - 1] == "$"
                j = i + 1
                while j < n and text[j] != "'":
                    j += 2 if (ansi and text[j] == "\\") else 1
                if strings:
                    _blank(out, i + 1, min(j, n))
                i = min(j + 1, n)
                continue
            if c == '"':
                i = dq(i)
                continue
            if c == "<" and text.startswith("<<", i) and not text.startswith("<<<", i):
                m = _HEREDOC.match(text, i)
                if m:
                    pending.append((m.group(3), m.group(1) == "-"))
                    i = m.end()
                    continue
            if close is not None:
                if c == "(":
                    depth += 1
                elif c == ")":
                    if depth == 0:
                        return i + 1
                    depth -= 1
            i += 1
        return i

    def heredoc_body(i: int, term: str, dash: bool) -> int:
        start = i
        while i < n:
            j = _eol(text, i)
            line = text[i:j]
            if (line.strip() if indented else line.lstrip("\t") if dash else line) == term:
                if strings:
                    _blank(out, start, i)
                return j
            i = j + 1
        if strings:
            _blank(out, start, n)
        return n

    code(0)
    return "".join(out)


def _blank_keep_subst(out: list, text: str, a: int, b: int) -> None:
    """Blank a double-quoted string's text but keep `$(…)` — that is code."""
    k = a
    while k < b:
        if text.startswith("$(", k):
            depth, j = 0, k + 1
            while j < b:
                if text[j] == "(":
                    depth += 1
                elif text[j] == ")":
                    depth -= 1
                    if depth == 0:
                        break
                j += 1
            k = j + 1
            continue
        if out[k] != "\n":
            out[k] = " "
        k += 1


# --------------------------------------------------------------------------
# python / toml

_PY_PREFIX = re.compile(r"(?<![A-Za-z0-9_])[rRbBuUfF]{0,2}('''|\"\"\"|'|\")")


def _strip_quoted_hash(text: str, strings: bool, *, escapes_in_single: bool, out: list) -> str:
    """Python and TOML: `#` outside a string is a comment, anywhere on the line."""
    n = len(text)
    i = 0
    while i < n:
        c = text[i]
        if c == "#":
            j = _eol(text, i)
            _blank(out, i, j)
            i = j
            continue
        if c in "'\"":
            q = text[i : i + 3] if text[i : i + 3] in ("'''", '"""') else c
            escapes = c == '"' or escapes_in_single
            j = i + len(q)
            while j < n and not text.startswith(q, j):
                if escapes and text[j] == "\\":
                    j += 2
                    continue
                if len(q) == 1 and text[j] == "\n":
                    break
                j += 1
            j = min(j, n)
            if strings:
                _blank(out, i + len(q), j)
            i = j + len(q) if text.startswith(q, j) else j
            continue
        i += 1
    return "".join(out)


# --------------------------------------------------------------------------
# yaml

_YAML_BLOCK = re.compile(r"(?:^\s*|[:?-]\s+)[|>][-+0-9]*$")


def _strip_yaml(text: str, strings: bool, out: list) -> str:
    n = len(text)
    i = 0
    block_indent = None  # parent line's indent while inside a block scalar
    content_indent = None
    while i < n:
        j = _eol(text, i)
        line = text[i:j]
        indent = len(line) - len(line.lstrip(" "))
        if block_indent is not None:
            if not line.strip():
                if strings:
                    _blank(out, i, j)
                i = j + 1
                continue
            if content_indent is None and indent > block_indent:
                content_indent = indent
            if content_indent is not None and indent >= content_indent:
                if strings:
                    _blank(out, i, j)
                i = j + 1
                continue
            block_indent = content_indent = None
        end = _yaml_line(text, out, i, j, strings)
        code_part = "".join(out[text.rfind("\n", 0, end) + 1 : end]).rstrip()
        if _YAML_BLOCK.search(code_part):
            block_indent, content_indent = indent, None
        i = end + 1
    return "".join(out)


def _yaml_line(text: str, out: list, i: int, j: int, strings: bool) -> int:
    """Strip one YAML line (quoted scalars may run past it); return where scanning stopped."""
    start = i
    n = len(text)
    while i < j:
        c = text[i]
        if c == "#" and (i == start or text[i - 1] in " \t"):
            _blank(out, i, j)
            return j
        if c in "'\"":
            prev = text[start:i].rstrip()
            if prev == "" or prev[-1] in ":-[{,?":
                k = i + 1
                while k < n:
                    if c == '"' and text[k] == "\\":
                        k += 2
                        continue
                    if text[k] == c:
                        if c == "'" and text.startswith("''", k):
                            k += 2
                            continue
                        break
                    k += 1
                k = min(k, n)
                if strings:
                    _blank(out, i + 1, k)
                if k >= j:  # a quoted scalar spanning lines
                    i = k + 1
                    j = _eol(text, i) if i < n else n
                    start = text.rfind("\n", 0, i) + 1
                    continue
                i = k + 1
                continue
        i += 1
    return j


# --------------------------------------------------------------------------
# cmake

_CMAKE_BRACKET_OPEN = re.compile(r"\[(=*)\[")


def _strip_cmake(text: str, strings: bool, out: list) -> str:
    n = len(text)
    i = 0
    while i < n:
        c = text[i]
        if c == "#":
            m = _CMAKE_BRACKET_OPEN.match(text, i + 1)
            if m:
                close = "]" + m.group(1) + "]"
                e = text.find(close, m.end())
                j = n if e < 0 else e + len(close)
            else:
                j = _eol(text, i)
            _blank(out, i, j)
            i = j
            continue
        if c == "\\":
            i += 2
            continue
        if c == '"':
            j = i + 1
            while j < n and text[j] != '"':
                j += 2 if text[j] == "\\" else 1
            j = min(j, n)
            if strings:
                _blank(out, i + 1, j)
            i = j + 1
            continue
        if c == "[" and (i == 0 or text[i - 1] in " \t\n("):
            m = _CMAKE_BRACKET_OPEN.match(text, i)
            if m:
                close = "]" + m.group(1) + "]"
                e = text.find(close, m.end())
                end = n if e < 0 else e
                if strings:
                    _blank(out, m.end(), end)
                i = n if e < 0 else e + len(close)
                continue
        i += 1
    return "".join(out)


# --------------------------------------------------------------------------
# the stripper's own negative controls


def self_test() -> int:
    """Every case here is a way a stripper has been, or could be, wrong.

    Run on the NORMAL path of every gate that imports this module (phase-472
    W9): a gate whose stripper regressed must not keep printing OK.
    """

    def code(lang, src, strings=False):
        return " ".join(strip_comments(src, lang, strings=strings).split())

    # -- C family --
    assert code("rust", "a(); // b()") == "a();"
    assert code("rust", "/// doc b()\n//! inner c()\nd();") == "d();"
    assert code("rust", "a /* b */ c") == "a c"
    assert code("rust", "a /* x /* nested */ still */ c") == "a c", "Rust block comments NEST"
    assert code("c", "a /* x /* not nested */ c */") == "a c */", "C block comments do NOT nest"
    assert code("rust", "a /* unterminated\nb();\n") == "a", "unterminated block runs to EOF"
    assert code("c", 'f("http://x"); // y') == 'f("http://x");', "// inside a string is code"
    assert code("c", 'f("a /* b */ c");') == 'f("a /* b */ c");', "/* inside a string is code"
    assert code("c", r'f("\"//"); // z') == r'f("\"//");', "escaped quote does not close"
    assert code("c", "c = '\"'; // q") == "c = '\"';", "a char literal holding a quote"
    assert code("c", "c = '/'; d = '/'; // q") == "c = '/'; d = '/';"
    assert code("cpp", "int x = 1'000; // it's") == "int x = 1'000;", "digit separator"
    assert code("cpp", 'R"(a // b)" + x; // c') == 'R"(a // b)" + x;', "C++ raw string"
    assert code("cpp", 'u8R"d(a )" // b)d" x // c') == 'u8R"d(a )" // b)d" x'
    assert code("c", "a(); // cont \\\nb();\nc();") == "a(); c();", "C line splice"
    assert code("rust", "a(); // no splice \\\nb();") == "a(); b();", "Rust has no splice"
    assert code("rust", 'r#"a // "b" c"# x // y') == 'r#"a // "b" c"# x', "Rust raw string"
    assert code("rust", 'br"//" x') == 'br"//" x'
    assert code("rust", "fn f<'a>(x: &'a str) {} // c") == "fn f<'a>(x: &'a str) {}", "lifetimes"
    assert code("rust", "let c = '\"'; g(\"//\"); // c") == "let c = '\"'; g(\"//\");"
    assert code("rust", "let c = '\\''; // c") == "let c = '\\'';"
    assert code("rust", "let s = \"a\nb // c\"; // d") == 'let s = "a b // c";', "multi-line string"
    assert code("rust", "let r#type = 1; // c") == "let r#type = 1;", "raw identifier"
    assert code("rust", 'f("x"); // g("y")', strings=True) == 'f(" ");', "strings=True blanks contents"
    assert code("c", "/**/x") == "x"
    assert code("c", "a /* b\n c */ d") == "a d"
    # -- shell / just --
    assert code("sh", "just x # source ./activate.sh") == "just x"
    assert code("sh", "echo $# ${#arr} a#b") == "echo $# ${#arr} a#b", "# mid-word is code"
    assert code("sh", "echo '# not' \"# not\" # yes") == "echo '# not' \"# not\""
    assert code("sh", "echo \\# x") == "echo \\# x", "escaped #"
    assert code("sh", "echo \"$(foo \"a\" # c\n)\" z") == 'echo "$(foo "a" )" z', "# inside $( ) inside quotes"
    assert code("sh", "cat <<EOF\n# body\nEOF\njust x # c") == "cat <<EOF # body EOF just x", \
        "a heredoc body is text, not a comment"
    assert code("sh", "cat <<'EOF'\njust y\nEOF\nz", strings=True) == "cat <<'EOF' EOF z"
    assert code("sh", "cat <<-EOF\n\tjust y\n\tEOF\nz", strings=True) == "cat <<-EOF EOF z"
    assert code("sh", "echo 'a' \"b $(c)\"", strings=True) == "echo ' ' \" $(c)\"", "$() survives strings=True"
    assert code("sh", "x=$'a\\'b' # c") == "x=$'a\\'b'", "ANSI-C quoting has escapes"
    assert code("just", "foo := \"a#b\" # c\nrecipe:\n    echo hi # d") == 'foo := "a#b" recipe: echo hi'
    assert code("just", "r:\n    cat <<EOF\n    # text\n    EOF\n    x # c") == "r: cat <<EOF # text EOF x", \
        "a just recipe's heredoc terminator is indented in the file"
    assert code("sh", "cat <<EOF\n  EOF\nx # c") == "cat <<EOF EOF x # c", \
        "in plain sh an indented terminator does NOT end the heredoc"
    # -- python / toml --
    assert code("python", "x = '#' # c") == "x = '#'"
    assert code("python", 'x = """\n# not\n""" # c') == 'x = """ # not """'
    assert code("python", "x = 'it\\'s' # c") == "x = 'it\\'s'"
    assert code("python", "x = 'a' # c", strings=True) == "x = ' '"
    assert code("toml", "a = \"#\" # c\nb = '''\n# x\n'''") == "a = \"#\" b = ''' # x '''"
    assert code("toml", "a = 'C:\\' # c") == "a = 'C:\\'", "TOML literal strings have no escapes"
    # -- yaml --
    assert code("yaml", "a: b # c\nd: e#f") == "a: b d: e#f", "# needs whitespace before it"
    assert code("yaml", "a: 'x # y' # z") == "a: 'x # y'"
    assert code("yaml", "a: don't # c") == "a: don't", "an apostrophe mid-scalar is not a quote"
    assert code("yaml", "a: 'it''s # x' # c") == "a: 'it''s # x'"
    assert code("yaml", "run: |\n  # shell comment\n  just x\nb: c # d") == \
        "run: | # shell comment just x b: c", "a block scalar's body is content"
    assert code("yaml", "- run: |\n    a # b\n  env: x # y") == "- run: | a # b env: x", \
        "a block ends at the first line indented less than its content"
    # -- cmake --
    assert code("cmake", "set(A b) # c") == "set(A b)"
    assert code("cmake", "#[[ block\nset(X y)\n]] set(Z w)") == "set(Z w)", "bracket comment"
    assert code("cmake", "#[==[ a ]] b ]==] c") == "c"
    assert code("cmake", 'set(A "x # y") # z') == 'set(A "x # y")'
    assert code("cmake", "set(A [[ # not ]]) # z") == "set(A [[ # not ]])", "bracket argument"
    assert code("cmake", "set(A x\\#y) # z") == "set(A x\\#y)"
    assert code("cmake", "#[[ unterminated\nset(X y)") == ""
    # -- the mask --
    m = code_mask('let s = "\n    fn x"; // c\nfn y() {}', "rust")
    src = 'let s = "\n    fn x"; // c\nfn y() {}'
    assert not m[src.index("    fn x")], "whitespace inside a string is not code"
    assert not m[src.index("fn x")] and not m[src.index("// c")]
    assert m[src.index("fn y")] and m[src.index("let")]
    # -- dispatch --
    assert lang_for("a/b.rs") == "rust" and lang_for("CMakeLists.txt") == "cmake"
    assert lang_for("justfile") == "just" and lang_for("x.unknown") is None
    try:
        strip_comments("x", "cobol")
    except ValueError:
        pass
    else:
        raise AssertionError("an unknown language must be an error, not a pass-through")
    return 0


def main(argv: list[str]) -> int:
    import argparse

    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--self-test", action="store_true")
    ap.add_argument("--strings", action="store_true", help="also blank string contents")
    ap.add_argument("--lang", choices=LANGS)
    ap.add_argument("files", nargs="*")
    a = ap.parse_args(argv)
    self_test()
    if a.self_test:
        print("comments self-test: OK")
        return 0
    for f in a.files:
        sys.stdout.write(strip_file(f, lang=a.lang, strings=a.strings))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
