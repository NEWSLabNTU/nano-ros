#!/usr/bin/env python3
"""phase-429 W3 — `NROS_CODEGEN_VERSION` must move when the version surface does.

WHY THIS GATE EXISTS

RFC-0090 D1 settles the compatibility token as an AUTHORED integer, and D5 puts
the runtime's accept window on `NROS_CODEGEN_VERSION` / `_MIN`. An authored
integer is the right token — a reader can order two of them, a hash it cannot —
but it inherits the failure every authored mirror in this tree has already had:
`check-rmw-api-parity`'s map is authored, and it drifted by 25 symbols while
reading green. The same drift here is worse than a wrong table. If someone
changes what generated code requires of the runtime and does not bump the
constant, every generated tree already on disk keeps stamping the old number and
a runtime that can no longer run them accepts them all. The check passes and the
failure moves to wherever the missing item is finally named.

So the hash below is EVIDENCE, not the token. It is never compiled into
anything, never compared at run time, and never shown to a user of nano-ros. It
exists so that a maintainer who moves the surface is told, at push time, that
the integer beside it has not moved.

WHAT THE VERSION SURFACE IS

    the DECLARATION, in the runtime, of every runtime identifier that the
    codegen packs name.

Both halves are extracted, neither is authored:

  DEMAND  — `packages/cli/rosidl-codegen/{packs,templates}/**`, comment-stripped
            (jinja `{# #}`, `//`, `/* */`), harvested for `nros_core::…` /
            `nros_serdes::…` paths, `use nros_*::{…}` lists, `nros_*` / `NROS_*`
            C identifiers and `nros::…` C++ paths. An identifier written
            immediately before a `{{` interpolation (`nros_cdr_{{ method }}`) is
            a PREFIX and pulls in every declaration that starts with it — that
            is the one place a literal harvest would otherwise go blind.

  SUPPLY  — the matching declaration in
              packages/core/nros-core/src/**.rs
              packages/core/nros-serdes/src/**.rs
              packages/api/nros-c/include/nros/**.h
              packages/api/nros-cpp/include/nros/**.hpp

IN, precisely:

  * Rust — the item header (kind, name, generics, supertraits, where-clause) of
    each demanded trait/struct/enum/union/type/mod, plus its associated item
    signatures, its `pub` fields, its enum variants, the `pub fn` signatures of
    its inherent `impl` blocks, and the HEADER of every `impl <demanded trait>
    for T` (which types implement it is part of what generated code may assume).
    A demanded lowercase name resolves only as a module-level `pub fn`/`pub
    const`/`pub mod` — never as a method — so `new`, `from`, `push` and `len`
    resolve to nothing instead of dragging in every same-named method in the
    crate.
  * Rust — the `pub use` line that publishes a demanded name. Generated code
    writes `nros_core::CdrReader`; moving that re-export is a surface change
    even when the item it names did not move.
  * C — the prototype of a demanded function, the full `typedef`/`struct`/`enum`
    declaration (fields ARE the signature of a type generated code fills in),
    and the NAME plus parameter list of a demanded macro.
  * C++ — the declaration of a demanded class/struct/alias/function, with public
    member signatures; for a demanded `A::B` path, member `B` of `A` ALONE.

OUT, and why each:

  * Function and method BODIES, all comments, all doc comments, and all
    formatting. This is the property that keeps the gate from crying wolf, and
    it is the same argument D1 makes against a hash as the token: a token that
    moves for a reflowed comment stops meaning anything. String LITERAL contents
    are blanked for the same reason (a literal is a value, not a signature).
  * Everything generated code does not name — the rest of `nros-core`
    (`Node`, `Executor`, params, timers) and the rest of the C/C++ API headers.
    Those are the HAND-WRITTEN API: break them and the compiler reports it at
    the call site the same day. `NROS_CODEGEN_VERSION` exists for the code no
    human wrote, so the surface is exactly the code no human wrote names. For
    the one demanded member of a hand-written class (`nros::Node::create_
    publisher`) only that member is taken, never the class.
  * Private items. Generated code cannot name them.
  * `#[cfg(test)]` items and `mod tests`.
  * Attributes that cannot change what may be named or how it is called —
    `inline`, `must_use`, `allow`/`warn`/`deny`/`expect`, `cold`,
    `track_caller`, `doc`, `rustfmt`. This is a DENYLIST, so an attribute nobody
    anticipated is KEPT and the gate fires: unknown means fail-closed.
  * A macro's replacement text. Generated C names `NROS_PUB_BUFFER_SIZE`; what
    it expands to is a per-configure knob value, and hashing it would put every
    knob edit through the codegen token.
  * `INVENTORY_SCHEMA_VERSION` / `ENTITY_INVENTORY_SCHEMA_VERSION` and the
    `bounds.rs` inventory shape. VERIFIED and deliberately excluded: those
    already carry their own authored integer, checked where they are consumed.
    Putting them here too would make one change answerable to two tokens, and
    the second one would be the one nobody bumps.

WHAT A RED MEANS

  surface moved, `NROS_CODEGEN_VERSION` did not   -> the failure this exists for.
                                                     Bump it (and `_MIN` too if
                                                     old generated trees can no
                                                     longer run), then rerun with
                                                     --write-baseline.
  surface moved AND the version was bumped        -> benign; refresh the baseline.
  version bumped, surface unchanged               -> benign; refresh the baseline.
                                                     (Legitimate: codegen can
                                                     change what it EMITS without
                                                     changing what it NAMES.)

Usage::

    check-codegen-version-surface.py                  # the gate
    check-codegen-version-surface.py --audit          # print the surface
    check-codegen-version-surface.py --write-baseline # after a deliberate move

Dependency-free on purpose — the house pattern for gates
(`scripts/check-board-tiers.py`, `scripts/build/fixtures-manifest.py`): CI hosts
here run Python 3.10, so no `tomllib` and no third-party parser.
"""

import hashlib
import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
BASELINE = os.path.join(ROOT, ".config", "codegen-version-surface.txt")
VERSION_RS = os.path.join(ROOT, "packages/core/nros-core/src/codegen_version.rs")

TEMPLATE_DIRS = [
    "packages/cli/rosidl-codegen/packs",
    "packages/cli/rosidl-codegen/templates",
]
# The templates are only HALF the emitter. `nros::HeapString`, `nros::Span<T>`
# and `nros::HeapSequence<T>` are never written in a `.jinja` — they are built
# in `types.rs` / `generator/common.rs` and interpolated as `{{ field | cpp_type
# }}`, so a template-only harvest saw one C++ name where there are seven. Here
# the identifiers live INSIDE string literals, which is the exact opposite of
# how a runtime source is read.
EMITTER_SRC = "packages/cli/rosidl-codegen/src"
RUST_RUNTIME = [
    ("nros_core", "packages/core/nros-core/src"),
    ("nros_serdes", "packages/core/nros-serdes/src"),
]
C_RUNTIME = "packages/api/nros-c/include/nros"
CPP_RUNTIME = "packages/api/nros-cpp/include/nros"

# Attributes that cannot change what generated code may name or how it calls it.
# A DENYLIST: anything not here survives into the digest, so an attribute nobody
# anticipated fires the gate rather than slipping past it.
ATTR_IGNORED = {
    "inline", "must_use", "allow", "warn", "deny", "expect", "cold",
    "track_caller", "doc", "rustfmt", "no_mangle", "used",
}


# ─────────────────────────── sanitizing ───────────────────────────
#
# One pass removes comments and blanks string/char literal CONTENTS, so every
# later regex and brace-walk runs on text where no `{`, `}` or `;` can be hiding
# inside a literal. Positions are preserved (removed text becomes spaces) only
# where it costs nothing; callers use the sanitized text for everything.

_RAW_OPEN = re.compile(r'r(#*)"')
# Bounded on purpose: a char literal is at most `'\u{1F600}'`. Unbounded, the
# first alternative scans to end-of-file and backtracks at every apostrophe,
# which is quadratic — and every Rust lifetime is an apostrophe.
_CHAR_LIT = re.compile(r"'(?:\\.|[^'\\])'")


def sanitize(text, rust, collect=None):
    """Strip comments and blank string/char literal contents.

    `rust` enables raw strings. `collect`, when a list, receives each string
    literal's CONTENT — which is how the codegen crate's own source is read:
    there the runtime identifiers live inside `format!` strings, so blanking
    them (right for a runtime source) would blank the whole demand set.
    """
    out = []
    i, n = 0, len(text)
    while i < n:
        c = text[i]
        nxt = text[i + 1] if i + 1 < n else ""
        if c == "/" and nxt == "/":
            j = text.find("\n", i)
            i = n if j < 0 else j
            continue
        if c == "/" and nxt == "*":
            j = text.find("*/", i + 2)
            i = n if j < 0 else j + 2
            out.append(" ")
            continue
        if rust and c == "r" and nxt in ('"', "#"):
            m = _RAW_OPEN.match(text, i)
            if m:
                close = '"' + m.group(1)
                j = text.find(close, m.end())
                if collect is not None:
                    collect.append(text[m.end():j if j >= 0 else n])
                i = n if j < 0 else j + len(close)
                out.append('""')
                continue
        if c == '"':
            j = i + 1
            while j < n:
                if text[j] == "\\":
                    j += 2
                    continue
                if text[j] == '"':
                    break
                j += 1
            if collect is not None:
                collect.append(text[i + 1:j])
            i = j + 1
            out.append('""')
            continue
        if c == "'":
            # Rust lifetimes ('a, 'static) are not literals; a char literal is
            # at most `'\u{1F600}'`, so a quote within a few chars decides it.
            m = _CHAR_LIT.match(text, i)
            if m:
                i = m.end()
                out.append("' '")
                continue
        out.append(c)
        i += 1
    return "".join(out)


def norm(text):
    """Collapse whitespace — formatting is never a surface change."""
    return re.sub(r"\s+", " ", text).strip()


def match_brace(text, i):
    """Index just past the `}` matching the `{` at `i` (sanitized text)."""
    depth = 0
    while i < len(text):
        if text[i] == "{":
            depth += 1
        elif text[i] == "}":
            depth -= 1
            if depth == 0:
                return i + 1
        i += 1
    return len(text)


def strip_attrs(text):
    """Drop contract-irrelevant attributes; keep every other one."""
    def keep(m):
        name = re.match(r"[A-Za-z_][A-Za-z0-9_]*", m.group(1).strip())
        return "" if name and name.group(0) in ATTR_IGNORED else m.group(0)
    return re.sub(r"#!?\[([^\[\]]*(?:\[[^\[\]]*\][^\[\]]*)*)\]", keep, text)


def strip_bodies(text):
    """Elide `{...}` that follows a function signature — C/C++ header-inline."""
    out, i = [], 0
    while i < len(text):
        if text[i] == "{":
            before = text[:i].rstrip()
            if re.search(r"(\)|\bconst|\bnoexcept|\boverride|\bfinal|->\s*[\w:<>,&*\s]+)$",
                         before):
                out.append("{...}")
                i = match_brace(text, i)
                continue
        out.append(text[i])
        i += 1
    return "".join(out)


# ─────────────────────────── demand ───────────────────────────

JINJA_COMMENT = re.compile(r"\{#.*?#\}", re.S)


def tracked_files(rel, suffix):
    """Tracked files under `rel` ending in `suffix`, in sorted order.

    `git ls-files`, not a filesystem walk: `check-no-tracked-file-find` refuses
    a walk used to locate tracked files, and it is right to. A walk also sees
    build output and untracked scratch, either of which would silently join the
    surface and make this gate fire on something nobody committed.
    """
    out = subprocess.run(
        ["git", "ls-files", "-z", "--", rel],
        cwd=ROOT, capture_output=True, text=True, check=True,
    ).stdout
    return sorted(
        os.path.join(ROOT, f)
        for f in out.split("\0")
        if f and f.endswith(suffix)
    )


def template_text():
    """Every emitter template, comment-stripped, concatenated."""
    chunks = []
    for rel in TEMPLATE_DIRS:
        for path in tracked_files(rel, ".jinja"):
                with open(path, encoding="utf8") as fh:
                    body = JINJA_COMMENT.sub(" ", fh.read())
                # `//` and `/* */` in a template are comments in the OUTPUT, and
                # a generated comment is not a surface. Stripping them is also
                # what keeps `nros_cpp_*` prose out of the demand set.
                chunks.append(sanitize(body, rust=False))
    for path in tracked_files(EMITTER_SRC, ".rs"):
            with open(path, encoding="utf8") as fh:
                lits = []
                sanitize(fh.read(), rust=True, collect=lits)
            chunks.extend(lits)
    return "\n".join(chunks)


def demand(text=None):
    """(rust, c, cpp, prefixes) — the names generated code writes."""
    text = template_text() if text is None else text
    rust, c, cpp, prefixes = set(), set(), set(), set()

    for m in re.finditer(r"use\s+(?:::)?(nros_core|nros_serdes)::\{([^}]*)\}", text):
        for part in m.group(2).split(","):
            part = part.strip()
            if part:
                rust.add(part)
    for m in re.finditer(r"use\s+(?:::)?(nros_core|nros_serdes)::([A-Za-z_][A-Za-z0-9_]*)\s*;", text):
        rust.add(m.group(2))
    for m in re.finditer(r"(?:::)?(?:nros_core|nros_serdes)::((?:[A-Za-z_][A-Za-z0-9_]*::)*[A-Za-z_][A-Za-z0-9_]*)",
                         text):
        for seg in m.group(1).split("::"):
            rust.add(seg)

    for m in re.finditer(r"\bnros::((?:[A-Za-z_][A-Za-z0-9_]*::)*[A-Za-z_][A-Za-z0-9_]*)", text):
        cpp.add(m.group(1))

    for m in re.finditer(r"\b(nros_[A-Za-z0-9_]*|NROS_[A-Z0-9_]*)(\{)?", text):
        name, interp = m.group(1), m.group(2)
        if interp:
            prefixes.add(name)
        else:
            c.add(name)
    # A name that is only ever a prefix is not itself an identifier.
    c -= prefixes
    rust.discard("")
    return rust, c, cpp, prefixes


# ─────────────────────────── Rust supply ───────────────────────────

RUST_ITEM = re.compile(
    r"^[ \t]*pub(?:\s*\([^)]*\))?\s+(trait|struct|enum|union|type|const|static|fn|mod)\s+"
    r"([A-Za-z_][A-Za-z0-9_]*)", re.M)
RUST_IMPL = re.compile(r"^[ \t]*impl(\s*<[^{]*?>)?\s+([^{;]*?)\s*\{", re.M)
RUST_ASSOC = re.compile(
    r"^[ \t]*(pub(?:\s*\([^)]*\))?\s+)?(fn|const|type)\s+([A-Za-z_][A-Za-z0-9_]*)", re.M)
RUST_TESTS = re.compile(r"^[ \t]*#\[cfg\(test\)\][ \t]*\n[ \t]*(pub\s+)?mod\s+\w+\s*\{", re.M)


def _decl_end(text, start):
    """(end, brace_index or None) for the item beginning at `start`."""
    i, depth = start, 0
    while i < len(text):
        ch = text[i]
        if ch in "<([":
            depth += 1
        elif ch in ">)]":
            depth -= 1
        elif depth <= 0 and ch == ";":
            return i + 1, None
        elif depth <= 0 and ch == "{":
            return i, i
        i += 1
    return len(text), None



ATTR_LINE = re.compile(r"(?:^[ \t]*#!?\[[^\n]*\][ \t]*\n)+\Z", re.M)


def _attrs_before(text, start):
    """The attribute lines immediately above an item.

    A capture that begins at the `pub` keyword silently drops EVERY attribute,
    which would have made the documented denylist a lie in the unsafe direction:
    `#[non_exhaustive]`, `#[repr(C)]` and a `#[cfg(feature = ...)]` gate all
    change what generated code may name, and none of them would have been seen.
    """
    head = text[:start]
    nl = head.rfind("\n")
    line_start = nl + 1
    m = ATTR_LINE.search(text[:line_start])
    return text[m.start():line_start] if m else ""


def _top_level(text, pos):
    """True when `pos` sits at brace depth 0 of `text` (sanitized).

    Without this, `RUST_ASSOC` matches a `fn` nested inside a method BODY and a
    private helper reads as public surface.
    """
    return text.count("{", 0, pos) == text.count("}", 0, pos)


def _drop_test_mods(text):
    while True:
        m = RUST_TESTS.search(text)
        if not m:
            return text
        text = text[:m.start()] + text[match_brace(text, m.end() - 1):]


def rust_files(rel):
    return tracked_files(rel, ".rs")


def rust_surface(wanted, prefixes):
    """Declarations in the Rust runtime for the names generated code writes."""
    out = {}
    for crate, rel in RUST_RUNTIME:
        for path in rust_files(rel):
            with open(path, encoding="utf8") as fh:
                text = _drop_test_mods(sanitize(fh.read(), rust=True))

            impls = []  # (target, header, start, end)
            for m in RUST_IMPL.finditer(text):
                head = norm(m.group(0)[:-1])
                end = match_brace(text, m.end() - 1)
                target = m.group(2).strip()
                impls.append((target, head, m.end(), end))

            def inside_impl(pos):
                return any(s <= pos < e for _, _, s, e in impls)

            for m in RUST_ITEM.finditer(text):
                kind, name = m.group(1), m.group(2)
                if inside_impl(m.start()):
                    continue
                hit = name in wanted or any(name.startswith(p) for p in prefixes)
                if not hit:
                    continue
                # A lowercase demand resolves only as a module-level item, which
                # is what keeps `new`/`from`/`push` from matching methods.
                end, brace = _decl_end(text, m.start())
                header = strip_attrs(_attrs_before(text, m.start())
                                     + text[m.start():end])
                body = ""
                if brace is not None:
                    inner = text[brace + 1:match_brace(text, brace) - 1]
                    if kind == "trait":
                        body = " ".join(
                            norm(strip_attrs(_attrs_before(inner, a.start())
                                             + inner[a.start():_decl_end(inner, a.start())[0]]))
                            for a in RUST_ASSOC.finditer(inner)
                            if _top_level(inner, a.start()))
                    elif kind in ("struct", "union"):
                        body = " ".join(norm(strip_attrs(ln)) for ln in inner.split(",")
                                        if ln.strip().startswith("pub "))
                    elif kind == "enum":
                        body = norm(strip_attrs(inner))
                    # `mod`: its own `pub use`/items are picked up in their own
                    # right by the passes below; the header alone is the surface.
                key = f"{crate}::{name}"
                out[f"rust|{kind}|{key}"] = norm(header) + (" { " + body + " }" if body else "")

                if kind in ("struct", "enum", "union", "type"):
                    for target, head, s, e in impls:
                        if "for" in target.split():
                            continue
                        if re.sub(r"<.*", "", target).strip() != name:
                            continue
                        seg = text[s:e]
                        for a in RUST_ASSOC.finditer(seg):
                            if not a.group(1) or not _top_level(seg, a.start()):
                                continue  # private, or nested inside a body
                            sig = (_attrs_before(seg, a.start())
                                   + seg[a.start():_decl_end(seg, a.start())[0]])
                            out[f"rust|method|{key}::{a.group(3)}"] = norm(strip_attrs(sig))

            # `impl <demanded trait> for T` — which types implement it.
            for target, head, _, _ in impls:
                parts = target.split(" for ")
                if len(parts) != 2:
                    continue
                trait = re.sub(r"<.*", "", parts[0]).strip().split("::")[-1]
                if trait in wanted:
                    out[f"rust|traitimpl|{crate}|{norm(head)}"] = norm(head)

            # The re-export that publishes a demanded name at the path
            # generated code writes.
            for m in re.finditer(r"^[ \t]*pub use [^;]*;", text, re.M):
                line = norm(m.group(0))
                names = set(re.findall(r"[A-Za-z_][A-Za-z0-9_]*", line))
                if names & wanted:
                    # Keyed on the SOURCE path, not the whole line, so adding a
                    # name to an existing re-export reads as `~ changed` rather
                    # than as an unrelated remove-plus-add.
                    src = re.match(r"pub use ([A-Za-z_][A-Za-z0-9_:]*)", line)
                    out[f"rust|reexport|{crate}|{src.group(1) if src else line}"] = line
    return out


# ─────────────────────────── C supply ───────────────────────────

def c_files(rel, exts):
    for ext in ((exts,) if isinstance(exts, str) else exts):
        yield from tracked_files(rel, ext)


C_DEFINE = re.compile(r"^[ \t]*#[ \t]*define[ \t]+([A-Za-z_][A-Za-z0-9_]*)(\([^)]*\))?", re.M)
C_TAGGED = re.compile(r"\b(typedef|struct|enum|union)\b")
C_PROTO = re.compile(
    r"(?:^|[;}])\s*((?:[A-Za-z_][A-Za-z0-9_]*[\s*]+)+?)([A-Za-z_][A-Za-z0-9_]*)\s*\(", re.M)


def c_surface(wanted, prefixes):
    out = {}
    for path in c_files(C_RUNTIME, (".h",)):
        # Keyed by HEADER, not by name alone: `nros_cdr_write_u32` is declared
        # in both `cdr.h` and the cbindgen aggregate `nros_generated.h`, and a
        # name-only key let the second silently shadow the first — a C-side
        # mutation went unreported while the Rust one beside it fired. Two
        # entries also means a mirror that drifts from its source is a red,
        # which is the 0088 family's whole lesson.
        hdr = os.path.relpath(path, os.path.join(ROOT, C_RUNTIME))
        with open(path, encoding="utf8") as fh:
            text = sanitize(fh.read(), rust=False)

        for m in C_DEFINE.finditer(text):
            name = m.group(1)
            if name in wanted or any(name.startswith(p) for p in prefixes):
                # Name + parameters only. The replacement text is a knob value.
                out[f"c|macro|{hdr}|{name}"] = norm(name + (m.group(2) or ""))

        for m in C_TAGGED.finditer(text):
            start = m.start()
            i, depth = start, 0
            while i < len(text):
                if text[i] == "{":
                    depth += 1
                elif text[i] == "}":
                    depth -= 1
                elif text[i] == ";" and depth == 0:
                    break
                i += 1
            decl = text[start:i + 1]
            names = re.findall(r"[A-Za-z_][A-Za-z0-9_]*", decl)
            hits = [n for n in names
                    if n in wanted or any(n.startswith(p) for p in prefixes)]
            if hits:
                out[f"c|type|{hdr}|{sorted(hits)[0]}"] = norm(decl)

        for m in C_PROTO.finditer(text):
            name = m.group(2)
            if name in ("if", "for", "while", "switch", "return", "sizeof", "defined"):
                continue
            if not (name in wanted or any(name.startswith(p) for p in prefixes)):
                continue
            j, depth = m.end() - 1, 0
            while j < len(text):
                if text[j] == "(":
                    depth += 1
                elif text[j] == ")":
                    depth -= 1
                    if depth == 0:
                        break
                j += 1
            tail = text[j + 1:j + 40]
            if "{" in tail.split(";")[0]:
                continue  # a definition in a header, not the declaration
            out[f"c|fn|{hdr}|{name}"] = norm(m.group(1) + name + text[m.end() - 1:j + 1])
    return out


# ─────────────────────────── C++ supply ───────────────────────────

CPP_TYPE = re.compile(
    r"^[ \t]*(?:template\s*<[^>]*>\s*)?(class|struct|using)\s+([A-Za-z_][A-Za-z0-9_]*)", re.M)
# A free function is searched for BY NAME, one name at a time. The obvious
# single regex — an alternation of declaration keywords, each followed by `\s+`,
# under a `+` — backtracks catastrophically on a real header (it did not finish
# in 100 s on `packages/api/nros-cpp/include/nros`), and a gate that hangs is a
# gate nobody keeps.
CPP_DECL_KEYWORD = re.compile(
    r"\b(?:template|constexpr|inline|static|using|auto|struct|class|"
    r"[a-z_][a-z0-9_]*_t|void|bool|size_t)\b")


def cpp_surface(wanted):
    """`wanted` holds `nros::` paths — `Span`, `Node::create_publisher`, …"""
    # Every segment is a type candidate: `detail::size_bound_dependent_false`
    # is a struct in a NAMESPACE, not a member of a class called `detail`.
    types = {seg for p in wanted for seg in p.split("::")}
    members = {}
    for p in wanted:
        segs = p.split("::")
        if len(segs) >= 2:
            members.setdefault(segs[-2], set()).add(segs[-1])
    plain = {p for p in wanted if "::" not in p}

    out = {}
    for path in c_files(CPP_RUNTIME, (".hpp", ".h")):
        hdr = os.path.relpath(path, os.path.join(ROOT, CPP_RUNTIME))
        with open(path, encoding="utf8") as fh:
            text = sanitize(fh.read(), rust=False)

        for m in CPP_TYPE.finditer(text):
            kind, name = m.group(1), m.group(2)
            if name not in types:
                continue
            end, brace = _decl_end(text, m.start()), None
            if kind == "using":
                j = text.find(";", m.start())
                out[f"cpp|using|{hdr}|{name}"] = norm(text[m.start():j + 1])
                continue
            brace = text.find("{", m.start())
            semi = text.find(";", m.start())
            if brace < 0 or (0 <= semi < brace):
                continue  # forward declaration
            body = text[brace + 1:match_brace(text, brace) - 1]
            head = norm(text[m.start():brace])
            if name in plain:
                # A type generated code uses wholesale: its public members are
                # part of what generated code may call.
                out[f"cpp|{kind}|{hdr}|{name}"] = head + " { " + norm(strip_bodies(body)) + " }"
            else:
                out[f"cpp|{kind}|{hdr}|{name}"] = head
            # A demanded MEMBER of a hand-written class: that member alone.
            for want in members.get(name, ()):
                for mm in re.finditer(
                        r"^[ \t]*[^;{}\n]*?\b" + re.escape(want) + r"\s*\([^;{}]*\)[^;{}]*",
                        body, re.M):
                    out[f"cpp|member|{hdr}|{name}::{want}"] = norm(strip_bodies(mm.group(0)))

        for name in sorted(plain):
            if any(f"cpp|{k}|{hdr}|{name}" in out for k in ("struct", "class", "using")):
                continue
            for m in re.finditer(r"\b" + re.escape(name) + r"\s*\(", text):
                start = max((text.rfind(ch, 0, m.start()) for ch in ";}{"), default=-1) + 1
                head = text[start:m.start()]
                if not CPP_DECL_KEYWORD.search(head):
                    continue  # a call site, not a declaration
                j = match_paren(text, m.end() - 1)
                out[f"cpp|fn|{hdr}|{name}"] = norm(strip_bodies(text[start:j]))
    return out


def match_paren(text, i):
    depth = 0
    while i < len(text):
        if text[i] == "(":
            depth += 1
        elif text[i] == ")":
            depth -= 1
            if depth == 0:
                return i + 1
        i += 1
    return len(text)


# ─────────────────────────── the surface ───────────────────────────

def build_surface():
    rust, c, cpp, prefixes = demand()
    out = {}
    out.update(rust_surface(rust, {p for p in prefixes if not p.startswith("NROS")}))
    out.update(c_surface(c, prefixes))
    out.update(cpp_surface(cpp))
    return out


def digest(value):
    return hashlib.sha256(value.encode("utf8")).hexdigest()[:16]


VERSION_RE = re.compile(
    r"pub const (NROS_CODEGEN_VERSION(?:_MIN)?)\s*:\s*u32\s*=\s*(\d+)")


def read_versions(path=None, errors=None):
    path = path or VERSION_RS
    if not os.path.exists(path):
        (errors if errors is not None else []).append(
            f"missing {os.path.relpath(path, ROOT)} — phase-429 W1 declares "
            "NROS_CODEGEN_VERSION there; this gate has nothing to bind without it")
        return None, None
    with open(path, encoding="utf8") as fh:
        found = dict(VERSION_RE.findall(sanitize(fh.read(), rust=True)))
    cur = int(found["NROS_CODEGEN_VERSION"]) if "NROS_CODEGEN_VERSION" in found else None
    low = int(found["NROS_CODEGEN_VERSION_MIN"]) if "NROS_CODEGEN_VERSION_MIN" in found else None
    return cur, low


HEADER = """\
# phase-429 W3 — the version-surface stamp for `NROS_CODEGEN_VERSION`.
#
# EVIDENCE, NOT THE TOKEN. Nothing compiles this file, nothing compares these
# hashes at run time, and no user of nano-ros ever sees one. RFC-0090 D1 keeps
# the compatibility token an authored integer precisely so a reader can order
# two of them; this file exists only so that an authored integer cannot drift
# away from what it claims to describe.
#
# `version` is the value of NROS_CODEGEN_VERSION when the surface below was
# recorded. `check-codegen-version-surface` fails when the surface moves and
# that integer does not.
#
# Regenerate after a DELIBERATE surface move (bump the constant first):
#     python3 scripts/check-codegen-version-surface.py --write-baseline
"""


def write_baseline(surface, version, path=None):
    path = path or BASELINE
    lines = [HEADER, f"version {version}\n"]
    for key in sorted(surface):
        lines.append(f"{digest(surface[key])} {key}\n")
    with open(path, "w", encoding="utf8") as fh:
        fh.writelines(lines)
    print(f"wrote {os.path.relpath(path, ROOT)}: version {version}, "
          f"{len(surface)} surface item(s)")
    return 0


def read_baseline(path=None, errors=None):
    path = path or BASELINE
    errors = errors if errors is not None else []
    if not os.path.exists(path):
        errors.append(
            f"missing {os.path.relpath(path, ROOT)} — regenerate with "
            "--write-baseline. A missing baseline is a FAILURE, not an empty "
            "surface: an empty one reads as 'nothing to protect' and the gate "
            "goes green on the very drift it exists to catch")
        return None, {}
    version, items = None, {}
    with open(path, encoding="utf8") as fh:
        for line in fh:
            line = line.strip()
            if not line or line.startswith("#"):
                continue
            if line.startswith("version "):
                version = int(line.split()[1])
                continue
            h, _, key = line.partition(" ")
            items[key] = h
    if version is None:
        errors.append(f"{os.path.relpath(path, ROOT)} has no `version` line")
    return version, items


def compare(surface, base_items, base_version, version, low):
    """(errors, notes) — the whole verdict, as data, so self_test can drive it."""
    errors, notes = [], []
    if version is None:
        return ["NROS_CODEGEN_VERSION not found"], notes
    if low is not None and low > version:
        errors.append(
            f"NROS_CODEGEN_VERSION_MIN ({low}) exceeds NROS_CODEGEN_VERSION "
            f"({version}) — the accept window is empty, so no generated tree "
            "can ever satisfy it")
    cur = {k: digest(v) for k, v in surface.items()}
    added = sorted(set(cur) - set(base_items))
    removed = sorted(set(base_items) - set(cur))
    changed = sorted(k for k in set(cur) & set(base_items) if cur[k] != base_items[k])
    moved = added or removed or changed

    if base_version is not None and version < base_version:
        errors.append(
            f"NROS_CODEGEN_VERSION went BACKWARDS ({base_version} -> {version}). "
            "The token is monotone: a lower number re-uses an identity that is "
            "already on disk somewhere, stamped by a different surface")

    def listing():
        out = []
        for k in added:
            out.append(f"    + {k}")
        for k in removed:
            out.append(f"    - {k}")
        for k in changed:
            out.append(f"    ~ {k}")
        return out

    if moved and base_version is not None and version == base_version:
        errors.append(
            "the codegen version surface MOVED and NROS_CODEGEN_VERSION did not "
            f"(still {version}). Every generated tree on disk still stamps "
            f"{version}, so a runtime that can no longer run them accepts them "
            "all.\n  what moved (+ added, - removed, ~ changed):\n"
            + "\n".join(listing())
            + f"\n  Fix: bump NROS_CODEGEN_VERSION in "
            f"{os.path.relpath(VERSION_RS, ROOT)} (and NROS_CODEGEN_VERSION_MIN "
            "too, if a tree generated against the old surface can no longer "
            "run), then rerun with --write-baseline.")
    elif moved:
        errors.append(
            f"the version surface moved and NROS_CODEGEN_VERSION was bumped "
            f"({base_version} -> {version}) — benign, but the baseline still "
            "records the old surface. Refresh it:\n"
            "    python3 scripts/check-codegen-version-surface.py --write-baseline\n"
            "  what moved:\n" + "\n".join(listing()))
    elif base_version is not None and version != base_version:
        errors.append(
            f"NROS_CODEGEN_VERSION moved ({base_version} -> {version}) with the "
            "surface unchanged — legitimate (codegen can change what it EMITS "
            "without changing what it NAMES), but the baseline must record it:\n"
            "    python3 scripts/check-codegen-version-surface.py --write-baseline")
    else:
        notes.append(f"version {version} (min {low}), {len(cur)} surface item(s)")
    return errors, notes


# ─────────────────────────── self-test ───────────────────────────

def self_test(quiet=False):
    """Negative controls. Run on the NORMAL path — a control nobody runs decays
    into a comment (`check-board-tiers.py`), and `check-gate-selftests` binds it.

    The pair that actually tests the SURFACE DEFINITION is the last one: a
    cosmetic edit must NOT move a digest, and a signature edit MUST.
    """
    base = {"rust|trait|nros_core::Serialize": digest("pub trait Serialize { fn serialize; }")}
    surf = {"rust|trait|nros_core::Serialize": "pub trait Serialize { fn serialize; }"}

    errors, notes = compare(surf, base, 1, 1, 1)
    assert not errors and notes, f"an unchanged surface must pass: {errors}"

    # THE failure this gate exists for: surface moved, integer did not.
    moved = {"rust|trait|nros_core::Serialize": "pub trait Serialize { fn serialize2; }"}
    errors, _ = compare(moved, base, 1, 1, 1)
    assert errors and "did not" in errors[0], "a moved surface at a fixed version must FAIL"
    assert "~ rust|trait|nros_core::Serialize" in errors[0], \
        "the failure must NAME what moved, not merely that something did"

    # ... and the escape is bumping the integer, not editing the baseline.
    errors, _ = compare(moved, base, 1, 2, 1)
    assert errors and "benign" in errors[0], "a bumped version must read as benign"
    assert "did not" not in errors[0], "a bumped version must not report the loud failure"
    errors, _ = compare(moved, {k: digest(v) for k, v in moved.items()}, 2, 2, 1)
    assert not errors, "baseline refreshed at the new version must pass"

    # Additions and removals are named too, with their own sign.
    errors, _ = compare({**surf, "c|fn|cdr.h|nros_cdr_write_u32": "int nros_cdr_write_u32(void)"},
                        base, 1, 1, 1)
    assert "+ c|fn|cdr.h|nros_cdr_write_u32" in errors[0], "an ADDED item must be named"
    errors, _ = compare({}, base, 1, 1, 1)
    assert "- rust|trait|nros_core::Serialize" in errors[0], "a REMOVED item must be named"

    # The token is monotone, and its accept window must be inhabitable.
    errors, _ = compare(surf, base, 2, 1, 1)
    assert any("BACKWARDS" in e for e in errors), "a version rewind must fail"
    errors, _ = compare(surf, base, 1, 1, 5)
    assert any("accept window is empty" in e for e in errors), "MIN > CURRENT must fail"

    # A missing baseline must FAIL, never read as an empty surface — an empty
    # one is the strictest-looking and most useless reading: it protects nothing
    # while reporting green.
    errs = []
    assert read_baseline(os.path.join(ROOT, "does-not-exist.txt"), errs) == (None, {}) and errs, \
        "a missing baseline must be an error"
    errs = []
    read_versions(os.path.join(ROOT, "does-not-exist.rs"), errs)
    assert errs, "a missing codegen_version.rs must be an error, not version None"

    # ── the cry-wolf control ──────────────────────────────────────────────
    # D1 rejects a hash as the TOKEN because a token that moves for a reflowed
    # comment stops meaning anything. The same argument binds the gate: these
    # two must produce the SAME digest.
    a = """
        /// Serialize this value to the CDR writer.
        #[inline]
        #[must_use]
        pub fn write_u32(&mut self, v: u32) -> Result<(), SerError> {
            self.align(4)?;
            Ok(())
        }
    """
    b = """
        // reworded, reformatted, reimplemented — none of it a surface change
        pub fn write_u32(&mut self,   v: u32)
            -> Result<(), SerError>
        {
            /* different body entirely */
            self.pad(4)?; self.push(v); Ok(())
        }
    """
    def sig(src):
        t = sanitize(src, rust=True)
        m = RUST_ASSOC.search(t)
        return norm(strip_attrs(_attrs_before(t, m.start())
                                + t[m.start():_decl_end(t, m.start())[0]]))
    assert sig(a) == sig(b), f"cosmetic edits must not move the surface: {sig(a)!r} vs {sig(b)!r}"

    # ...and a real signature change MUST move it.
    c = b.replace("v: u32", "v: u64")
    assert sig(a) != sig(c), "an argument-type change must move the surface"
    d = b.replace("write_u32", "write_uint32")
    assert sig(a) != sig(d), "a rename must move the surface"

    # An attribute nobody anticipated is KEPT — unknown means fail-closed.
    e = b.replace("        pub fn", "        #[non_exhaustive]\n        pub fn")
    assert sig(a) != sig(e), "an unrecognised attribute must move the surface"

    # A macro contributes its NAME and parameters, never its replacement text,
    # so a knob's VALUE cannot travel through the codegen token.
    one = c_surface({"NROS_PUB_BUFFER_SIZE"}, set())
    m1 = C_DEFINE.search("#define NROS_PUB_BUFFER_SIZE 1024")
    m2 = C_DEFINE.search("#define NROS_PUB_BUFFER_SIZE 4096")
    assert m1 and m2 and norm(m1.group(1) + (m1.group(2) or "")) == \
        norm(m2.group(1) + (m2.group(2) or "")), "a macro's value must not be surface"
    del one

    # Literal contents are values, not signatures.
    assert sanitize('const X: &str = "abc";', rust=True) == \
        sanitize('const X: &str = "xyz";', rust=True), "string contents must not be surface"

    # Comment-stripping the templates is what keeps prose out of the demand set.
    r, cc, cp, pre = demand(sanitize("// nros_cpp_ghost is prose\nnros_cdr_{{ m }}(x);", False))
    assert "nros_cpp_ghost" not in cc, "a name only mentioned in a comment is not demand"
    assert "nros_cdr_" in pre, "a name before an interpolation is a PREFIX"

    if not quiet:
        print("check-codegen-version-surface self-test: OK")
    return 0


# ─────────────────────────── main ───────────────────────────

def main():
    if "--selftest" in sys.argv or "--self-test" in sys.argv:
        return self_test()
    self_test(quiet=True)

    surface = build_surface()

    if "--audit" in sys.argv:
        for key in sorted(surface):
            print(f"{digest(surface[key])} {key}\n    {surface[key][:200]}")
        print(f"\n{len(surface)} surface item(s)")
        return 0

    errors = []
    version, low = read_versions(errors=errors)
    if errors:
        for e in errors:
            print(f"FAIL: {e}")
        return 1

    if "--write-baseline" in sys.argv:
        return write_baseline(surface, version)

    base_version, base_items = read_baseline(errors=errors)
    if errors:
        for e in errors:
            print(f"FAIL: {e}")
        return 1

    errs, notes = compare(surface, base_items, base_version, version, low)
    for n in notes:
        print(f"codegen version surface: OK — {n}")
    for e in errs:
        print(f"FAIL: {e}")
    return 1 if errs else 0


if __name__ == "__main__":
    sys.exit(main())
