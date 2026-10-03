#!/usr/bin/env python3
"""Per-ITEM matching — phase-472 W6.

A rule about EVERY item (every definition, every loop, every emitted header,
every spawned process, every struct, every target) was being checked by a
search that stops at the FIRST match, or accepts ANY match in the file. Each
such gate passes a file in which one item is right and the next is wrong:

* `check-config-header-producers` / `check-config-fallback-macros` read the
  FIRST `#define NROS_CODEGEN_VERSION`; the compiler keeps the LAST (issue 1540);
* `check-board-cargo-config-shape` read the first `cargo_config` blob of a
  descriptor that has two;
* `check-tier-spin-gap` accepted a file-level `extern` prototype of the gap
  helper as the gap of every loop in the file;
* `check-literal-domain-id` truncated at the first `mod tests` SUBSTRING — so a
  `mod tests;` declaration hid the 57 lines of shipped code after it.

So this module splits a text into its ITEMS and hands each one to the rule.
Every function works on text whose comments (and, where noted, string contents)
are already blanked by `comments.strip_comments` — same length, same newlines —
so an offset still indexes the original and a brace in prose is not structure.

API
    line_of(text, idx)               1-based line of an offset
    block_end(code, open_idx)        offset just past the `}` matching `{` at open_idx
    blocks(code, head_re)            [(head_match, open_idx, end_idx)] — each head's body
    segments(text, start_re)         [(match, segment)] — each match to the next one
    call_args(text, name)            [(offset, first_argument)] of every `name(` call
    c_defines(text)                  [Define] — EVERY `#define`, with its conditional depth
    duplicate_defines(defines)       {name: [Define, …]} for names defined twice unconditionally
    rust_cfg_test_blank(code)        `code` with every `#[cfg(test)]` ITEM blanked
    cmake_calls(code)                [(command, args, line)] — EVERY invocation, nested too
    cmake_args(args)                 the argument tokens of one invocation, quotes dropped
    cmake_keyword_items(args, kw, keywords)  EVERY item under keyword `kw` (e.g. OUTPUT)

CLI (for shell gates)
    python3 scripts/lib/per_item.py call-args NAME FILE   one first-argument per line
    python3 scripts/lib/per_item.py                       self-test
"""

from __future__ import annotations

import re
import sys
from typing import NamedTuple


def line_of(text: str, idx: int) -> int:
    return text.count("\n", 0, idx) + 1


def block_end(code: str, open_idx: int) -> int:
    """Offset just past the `}` that closes the `{` at `open_idx`.

    An unbalanced block runs to the end of the text — the item is then as large
    as it can be, which makes a "must contain X" rule easier to satisfy, never
    harder; callers that care check `code[end - 1] == "}"`.
    """
    assert code[open_idx] == "{", (open_idx, code[open_idx: open_idx + 10])
    depth = 0
    for i in range(open_idx, len(code)):
        ch = code[i]
        if ch == "{":
            depth += 1
        elif ch == "}":
            depth -= 1
            if depth == 0:
                return i + 1
    return len(code)


def blocks(code: str, head_re) -> list:
    """[(head_match, open_idx, end_idx)] for every head FOLLOWED BY a body.

    The body is the first `{` after the head with no `;` in between — a
    declaration (`fn f();`, `mod tests;`) has no body and is not an item here.
    """
    head_re = re.compile(head_re) if isinstance(head_re, str) else head_re
    out = []
    for m in head_re.finditer(code):
        i = m.end()
        while i < len(code) and code[i] not in "{;":
            i += 1
        if i < len(code) and code[i] == "{":
            out.append((m, i, block_end(code, i)))
    return out


def segments(text: str, start_re) -> list:
    """[(match, segment)] — each match of `start_re` through to the next one.

    For a rule of the form "every X is followed by its Y": a Y after the SECOND
    X must not count for the first.
    """
    start_re = re.compile(start_re) if isinstance(start_re, str) else start_re
    ms = list(start_re.finditer(text))
    return [(m, text[m.start(): ms[k + 1].start() if k + 1 < len(ms) else len(text)])
            for k, m in enumerate(ms)]


def call_args(text: str, name: str) -> list:
    """[(offset, first argument)] for every call `name(ARG …` — `name` bounded,
    so `nano_ros_add_executable(` is not an `add_executable(` call."""
    pat = re.compile(rf"(?<![A-Za-z0-9_]){re.escape(name)}\s*\(\s*([^\s,()]+)")
    return [(m.start(), m.group(1)) for m in pat.finditer(text)]


class Define(NamedTuple):
    line: int
    name: str
    value: str | None  # None for a function-like macro
    conditional: bool  # inside an #if/#ifdef/#ifndef arm (the include guard excepted)


_DIRECTIVE = re.compile(r"^[ \t]*#[ \t]*([a-z]+)\b[ \t]*(.*)$")


def c_defines(text: str) -> list:
    """EVERY `#define` in file order — never just the first.

    The outermost `#ifndef` opened before any define is the include guard, and
    its body is NOT conditional. A function-like macro's value is None.
    """
    out = []
    depth = 0
    guard_open = False
    for lineno, line in enumerate(text.split("\n"), 1):
        m = _DIRECTIVE.match(line)
        if not m:
            continue
        kw, rest = m.group(1), m.group(2)
        if kw in ("if", "ifdef", "ifndef"):
            if depth == 0 and not guard_open and kw == "ifndef" and not out:
                guard_open = True
                continue
            depth += 1
        elif kw == "endif":
            if depth > 0:
                depth -= 1
            else:
                guard_open = False
        elif kw == "define":
            dm = re.match(r"([A-Za-z_][A-Za-z0-9_]*)(\()?[ \t]*(\S*)", rest)
            if dm:
                value = None if dm.group(2) else (dm.group(3) or "")
                out.append(Define(lineno, dm.group(1), value, depth > 0))
    return out


def duplicate_defines(defines) -> dict:
    """{name: [Define, …]} for every name defined more than once UNCONDITIONALLY
    — the case where the language makes the last one win silently."""
    seen: dict = {}
    for d in defines:
        if not d.conditional:
            seen.setdefault(d.name, []).append(d)
    return {n: ds for n, ds in seen.items() if len(ds) > 1}


# `#[cfg(test)]` and `#[cfg(all(test, …))]` — both compile ONLY under test.
# `any(test, …)` does not (it ships whenever the other arm holds), so it is not here.
_CFG_TEST = re.compile(r"#\s*\[\s*cfg\s*\(\s*(?:all\s*\(\s*)?test\b[^\]]*\]")


def rust_cfg_test_blank(code: str, cfg_re=None) -> str:
    """`code` with each `#[cfg(test)]` ITEM blanked (newlines kept).

    The item is the attribute plus what follows it up to its `;` (a
    declaration such as `mod tests;`) or through its brace-matched body —
    never "everything after the first `mod tests`".
    """
    out = list(code)
    for m in (cfg_re or _CFG_TEST).finditer(code):
        i = m.end()
        while i < len(code) and code[i] not in "{;":
            i += 1
        end = block_end(code, i) if i < len(code) and code[i] == "{" else min(i + 1, len(code))
        for k in range(m.start(), end):
            if out[k] != "\n":
                out[k] = " "
    return "".join(out)


_USE = re.compile(r"\buse\s+")


def rust_use_paths(code: str) -> list:
    """[(path, offset)] for every path a `use` imports, use-trees EXPANDED.

    phase-472 W6 / issue 1615: `use eyre::{\n    Context,\n};` imports
    `eyre::Context` across three lines, and a line grep for `use eyre::{...Context`
    never saw it. Pass COMMENT-STRIPPED code. `as` aliases are dropped (the path
    is what is imported); `self` resolves to its parent.
    """
    out = []
    for m in _USE.finditer(code):
        end = code.find(";", m.end())
        if end < 0:
            continue
        tree = re.sub(r"\s+", " ", code[m.end():end])
        tree = re.sub(r"\s*(::|[{},])\s*", r"\1", tree).strip()

        def expand(prefix, t):
            t = t.strip()
            if not t:
                return
            if "{" not in t:
                path = prefix + t.split(" as ", 1)[0].strip()
                if path.endswith("::self"):
                    path = path[: -len("::self")]
                out.append((path, m.start()))
                return
            head, rest = t.split("{", 1)
            inner = rest[: rest.rfind("}")]
            depth, cur, parts = 0, "", []
            for ch in inner:
                if ch == "," and depth == 0:
                    parts.append(cur)
                    cur = ""
                    continue
                depth += (ch == "{") - (ch == "}")
                cur += ch
            parts.append(cur)
            for part in parts:
                expand(prefix + head, part)

        expand("", tree)
    return out


_CMAKE_IDENT = re.compile(r"[A-Za-z_][A-Za-z0-9_]*")


def cmake_calls(code: str) -> list:
    """[(command, argument-text, line)] for EVERY CMake command invocation.

    Pass COMMENT-STRIPPED code. Scanning continues INSIDE an argument list, so a
    command nested in an `if(...)`/`foreach(...)` body is seen as well.
    """
    out, n, i = [], len(code), 0
    while True:
        m = _CMAKE_IDENT.search(code, i)
        if not m:
            return out
        k = m.end()
        while k < n and code[k] in " \t\n":
            k += 1
        if k < n and code[k] == "(":
            depth, p = 0, k
            while p < n:
                if code[p] == "(":
                    depth += 1
                elif code[p] == ")":
                    depth -= 1
                    if depth == 0:
                        break
                p += 1
            if depth != 0:
                return out
            out.append((m.group(0), code[k + 1: p], code.count("\n", 0, m.start()) + 1))
            i = k + 1
        else:
            i = m.end()


def cmake_args(args: str) -> list:
    """The argument tokens of one invocation: whitespace-split, a quoted
    argument kept whole and unquoted."""
    return [a if a else b for a, b in re.findall(r'"([^"]*)"|(\S+)', args) if (a or b)]


def cmake_keyword_items(args: str, kw: str, keywords) -> list:
    """EVERY item following keyword `kw` up to the next keyword — never only the
    first. `add_custom_command(OUTPUT a.c b.c COMMAND …)` has two outputs."""
    toks, out, on = cmake_args(args), [], False
    for t in toks:
        if t == kw:
            on = True
        elif t in keywords:
            on = False
        elif on:
            out.append(t)
    return out


def self_test() -> None:
    calls = cmake_calls("if(X)\n  add_library(a ${s})\nendif()\n")
    assert [(c, ln) for c, _a, ln in calls] == [("if", 1), ("add_library", 2), ("endif", 3)], calls
    assert cmake_args('a "b c" ${d}') == ["a", "b c", "${d}"]
    assert cmake_keyword_items("OUTPUT x.c y.h COMMAND gen x.c DEPENDS z", "OUTPUT",
                               {"OUTPUT", "COMMAND", "DEPENDS"}) == ["x.c", "y.h"]
    paths = [p for p, _ in rust_use_paths(
        "use eyre::{\n    Context,\n    Result as R,\n};\nuse a::{b::{c, d as e}, self};\n")]
    assert paths == ["eyre::Context", "eyre::Result", "a::b::c", "a::b::d", "a"], paths
    code = "fn a() { if x { y(); } }\nfn b();\nfn c() { z(); }\n"
    got = [(m.group(0), code[o:e]) for m, o, e in blocks(code, r"\bfn \w+\(\)")]
    assert got == [("fn a()", "{ if x { y(); } }"), ("fn c()", "{ z(); }")], got
    segs = [s for _m, s in segments("D 1\nG\nD 2\nH\n", re.compile(r"^D", re.M))]
    assert segs == ["D 1\nG\n", "D 2\nH\n"], segs
    assert [a for _o, a in call_args(
        "add_executable(a x.c)\nnano_ros_add_executable(b y.c)\nadd_executable( c )", "add_executable")] \
        == ["a", "c"]
    defs = c_defines(
        "#ifndef G_H\n#define G_H\n#define V 8\n#if A\n#define S 1\n#else\n#define S 2\n#endif\n"
        "#define F(x) x\n#define V 9\n#endif\n")
    assert [(d.name, d.value, d.conditional) for d in defs] == [
        ("G_H", "", False), ("V", "8", False), ("S", "1", True), ("S", "2", True),
        ("F", None, False), ("V", "9", False)], defs
    dup = duplicate_defines(defs)
    assert list(dup) == ["V"] and [d.value for d in dup["V"]] == ["8", "9"], dup
    rs = "fn ship() {}\n#[cfg(test)]\nmod tests;\nfn also_ships() { x.with_domain(0) }\n" \
         "#[cfg(test)]\nmod t { fn f() { y.with_domain(1) } }\n"
    blanked = rust_cfg_test_blank(rs)
    assert "also_ships" in blanked and "with_domain(0)" in blanked, blanked
    assert "with_domain(1)" not in blanked and blanked.count("\n") == rs.count("\n"), blanked
    assert "q()" not in rust_cfg_test_blank("#[cfg(all(test, feature = \"x\"))]\nfn t() { q() }\n")
    assert "q()" in rust_cfg_test_blank("#[cfg(any(test, feature = \"x\"))]\nfn t() { q() }\n")


def main(argv) -> int:
    self_test()
    if argv[:1] == ["call-args"] and len(argv) >= 3:
        name = argv[1]
        for path in argv[2:]:
            with open(path, encoding="utf-8", errors="replace") as fh:
                for _o, arg in call_args(fh.read(), name):
                    print(arg)
        return 0
    if argv:
        print(f"per_item: unknown arguments {argv}", file=sys.stderr)
        return 2
    print("per_item self-test: OK")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
