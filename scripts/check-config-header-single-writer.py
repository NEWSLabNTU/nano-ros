#!/usr/bin/env python3
"""`*config_generated.h` has exactly ONE writer: the mirror script. Issue 0985.

The per-build sizes headers (`nros_config_generated.h`,
`nros_cpp_config_generated.h`) are mirrored into the in-tree include dirs by
`scripts/build/mirror-generated-header.sh`, which since issue 0978 knows the
precedence: prefer the leaf-INDEPENDENT copy in the shared cargo target dir,
fall back to the leaf's own. That precedence exists because a shared
`--target-dir` makes cargo run a build script once per (crate, feature set),
not once per leaf, so the leaf's own copy is present and arbitrarily old for
every leaf after the first.

A second writer does not merely duplicate that logic — it DEFEATS it. Issue
0985: `nros-cpp/CMakeLists.txt` healed the mirror at configure time with a
plain `file(COPY_FILE)` from `${CMAKE_CURRENT_BINARY_DIR}`, i.e. exactly the
stale source 0978 stopped trusting. It wrote a museum header over a correct one
AND stamped it with a new mtime, so ninja found the mirror's OUTPUT newer than
its trigger and skipped the command that would have fixed it. A repair for
drift that caused drift and then suppressed its own fix.

The whole 0088 -> 0114 -> 0122 -> 0123 -> 0245 -> 0268 -> 0978 -> 0985 family is
one shape: two ways to answer "which bytes are the current sizes header". This
gate keeps the answer to one.

Allowed: any invocation of `mirror-generated-header.sh` (that IS the writer),
and comments.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path
import sys as _w3_sys  # noqa: E402
from pathlib import Path as _W3Path  # noqa: E402
_w3_sys.path.insert(0, str(_W3Path(__file__).resolve().parent / "lib"))
import comments  # noqa: E402  phase-472 W3 — the one comment stripper

REPO = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(REPO / "scripts" / "lib"))
from tracked import tracked  # noqa: E402 — issue 0721: index lookup, not a walk
HEADER = "config_generated"
WRITER = "mirror-generated-header.sh"

# cmake commands that write a file somewhere.
_WRITE_CMD = re.compile(
    r"\b(configure_file|file|add_custom_command|add_custom_target|execute_process)\s*\(", re.I)
# issue 1615 (W6): `cmake -E copy_if_different` inside a custom command writes a
# file exactly as `file(COPY_FILE)` does, and only `configure_file`/`file` were
# read. A command-running call counts as a write when it runs one of these.
_E_WRITE = re.compile(
    r"-E\s+(copy|copy_if_different|create_symlink|create_hardlink|rename|cat)\b")
_RUNS = ("add_custom_command", "add_custom_target", "execute_process")


def strip_comments(text: str) -> str:
    """Blank out cmake `#` comments, preserving line numbering."""
    # phase-472 W3 — the shared stripper (scripts/lib/comments.py).
    return comments.strip_comments(text, "cmake")


# `foreach(V "a_config_generated.h" ...)` / `set(V "...config_generated.h")` —
# a variable that CARRIES the header name. Issue 0985's own code named the
# header only through such a variable, so a literal-only scan passed on the
# very code it was written to catch. (Measured: the first version of this gate
# did exactly that.)
_BINDS = re.compile(r"\b(?:foreach|set)\s*\(\s*([A-Za-z_][A-Za-z0-9_]*)([^)]*)\)", re.I | re.S)


def header_vars(stripped: str) -> set[str]:
    return {m.group(1) for m in _BINDS.finditer(stripped) if HEADER in m.group(2)}


def offenders(text: str) -> list[tuple[int, str]]:
    """(line number, snippet) for every write command touching a sizes header."""
    stripped = strip_comments(text)
    hvars = header_vars(stripped)
    found = []
    for m in _WRITE_CMD.finditer(stripped):
        # Take the balanced argument span, capped so a runaway never scans the
        # whole file.
        depth, i, end = 0, m.end() - 1, None
        while i < len(stripped) and i < m.end() + 4000:
            if stripped[i] == "(":
                depth += 1
            elif stripped[i] == ")":
                depth -= 1
                if depth == 0:
                    end = i
                    break
            i += 1
        span = stripped[m.start(): (end + 1) if end else m.end() + 400]
        if m.group(1).lower() in _RUNS and not _E_WRITE.search(span):
            continue
        names_header = HEADER in span or any(
            f"${{{v}}}" in span for v in hvars
        )
        if names_header and WRITER not in span:
            line = stripped.count("\n", 0, m.start()) + 1
            found.append((line, " ".join(span.split())[:110]))
    return found


# issue 1735 — a SHELL writer is the same second writer: `cp … nros_cpp_config_
# generated.h` in a script or recipe passed, because the population was
# `.cmake` + `packages/**/CMakeLists.txt` only ("173 cmake file(s)"). A shell /
# just / make line that WRITES a sizes header — `cp`/`install`/`mv`/`ln`/
# `rsync`/`tee` or a `>` redirection whose DESTINATION names it — counts. A
# destination rooted in a TEMP dir (`$tmp`, `$TMP`, `$d`, …) is a selftest's
# synthetic fixture, not the in-tree mirror, and is not a writer of it.
_SH_VERB = re.compile(r"(?:^|[;&|({]\s*|\s)(cp|install|mv|ln|rsync|tee)\s")
_SH_REDIR = re.compile(r">>?\s*(\"?[^\s;|&)]*config_generated\.h\"?)")
_SH_DEST = re.compile(r"\"?[^\s;|&)\"]*config_generated\.h\"?")
_TEMP_ROOT = re.compile(r'^"?\$\{?(?:tmp|TMP|tmpdir|TMPDIR|d|work|scratch)\b')


def shell_offenders(text: str, lang: str = "sh") -> list[tuple[int, str]]:
    """(line, snippet) for every shell-ish line that writes a sizes header.

    The VERB / `>` must itself be CODE (`comments.code_mask` with strings): a
    heredoc body or an echoed string that SHOWS `<…>/nros_config_generated.h`
    writes nothing.
    """
    found = []
    code = comments.strip_comments(text, lang)
    mask = comments.code_mask(text, lang, strings=True)
    off = 0
    for n, line in enumerate(code.split("\n"), 1):
        base, off = off, off + len(line) + 1
        if HEADER not in line or WRITER in line:
            continue
        dests = [m.group(1) for m in _SH_REDIR.finditer(line) if mask[base + m.start()]]
        if any(mask[base + m.start(1)] for m in _SH_VERB.finditer(line)):
            dests += _SH_DEST.findall(line)[-1:]
        if any(not _TEMP_ROOT.match(d) for d in dests):
            found.append((n, " ".join(line.split())[:110]))
    return found


def self_test() -> None:
    """Runs on the NORMAL path — `check-gate-selftests`."""
    bad = 'file(COPY_FILE "${D}/nros_config_generated.h" "${E}/nros_config_generated.h")'
    assert offenders(bad), "a bare COPY_FILE of the sizes header must be caught"
    # Issue 0985's ACTUAL shape: the header is named through a loop variable, so
    # the write command's own text never contains `config_generated`. The first
    # version of this gate passed on this and had to be fixed — keep the case.
    via_var = ('foreach(_h "nros_cpp_config_generated.h" "nros_config_generated.h")\n'
               '  file(COPY_FILE "${B}/${_h}" "${I}/nros/${_h}" ONLY_IF_DIFFERENT)\n'
               'endforeach()')
    assert offenders(via_var), "a header named through a variable must be caught"
    # The real writer is allowed.
    ok = ('execute_process(COMMAND bash "${SH}/mirror-generated-header.sh" '
          '"${A}/nros_config_generated.h" "${B}" gen nros_config_generated.h "${C}")')
    assert not offenders(ok), "the mirror script itself must be allowed"
    # A comment naming the old spelling must not fire (issue 0985 leaves several).
    assert not offenders('# file(COPY_FILE) of nros_config_generated.h, retired'), \
        "a comment is not a writer"
    assert offenders('add_custom_command(OUTPUT h COMMAND ${CMAKE_COMMAND} -E '
                     'copy_if_different s.h ${B}/nros/nros_config_generated.h)'), \
        "a `cmake -E copy_if_different` of the header must be flagged (issue 1615)"
    assert not offenders('add_custom_command(OUTPUT h COMMAND gen nros_config_generated.h)'), \
        "a custom command that does not COPY is not this rule"
    # An unrelated copy must not fire.
    assert not offenders('file(COPY "${X}/src" DESTINATION "${Y}")'), \
        "unrelated file(COPY) must not be flagged"
    # issue 1735 — the shell spellings.
    assert shell_offenders('cp "$out/nros_cpp_config_generated.h" "$inc/nros/nros_cpp_config_generated.h"\n'), \
        "a shell `cp` of the sizes header must be caught"
    assert shell_offenders('    install -m644 a.h include/nros/nros_config_generated.h\n', "just"), \
        "a recipe `install` of the sizes header must be caught"
    assert shell_offenders('gen > "$inc/nros/nros_config_generated.h"\n'), \
        "a redirection into the sizes header must be caught"
    assert not shell_offenders('printf x > "$tmp/nros/nros_config_generated.h"\n'), \
        "a selftest's synthetic header under $tmp is not a writer"
    assert not shell_offenders('bash scripts/build/mirror-generated-header.sh a nros_config_generated.h b\n'), \
        "the mirror script itself must be allowed"
    assert not shell_offenders('# cp x nros_config_generated.h — retired\n'), \
        "a comment is not a writer"
    assert not shell_offenders('ninja -C b -t query include/nros/nros_config_generated.h\n'), \
        "a READ of the header is not a write"
    assert not shell_offenders("cat <<EOF\n  ninja -t query <…>/include/nros/nros_config_generated.h\nEOF\n"), \
        "a heredoc body that SHOWS a path is not a write"


def main() -> int:
    self_test()

    # The INDEX, not a filesystem glob. The rule is about AUTHORED cmake, which
    # is tracked by definition; `REPO.glob("packages/**/...")` walked every cargo
    # `target/` and build dir under `packages/` (20 G on disk against 5.6 MB
    # tracked). Measured on the walk it replaced: 714 files returned, 461 of
    # them (65%) generated build output this gate had no business reading, and
    # 37 minutes inside a loaded `check fast` — the tail of every push — for a
    # scan the index answers instantly. `check-no-tracked-file-find` missed it
    # because its regex required the `**` to lead the pattern.
    # issue 1735 — every CMake file by KIND (`file_kinds`), not four
    # directories; plus the shell / just / make files that can hold a `cp`.
    import file_kinds
    files = [REPO / f for f in file_kinds.files_of_kind("cmake", repo=REPO)]
    sh_files = [(REPO / f, file_kinds.kind_of(f)) for f in
                file_kinds.files_of_kind("shell", "just", "make", repo=REPO)]
    # A gate that scanned nothing must not read as a pass (see git history:
    # its sibling `check-ret-code-citations` was found that way).
    if not files or not sh_files:
        print(
            "check-config-header-single-writer: scanned NO cmake (or shell) files -- "
            "this gate would pass vacuously.",
            file=sys.stderr,
        )
        return 1
    hits = []
    for f in files:
        try:
            text = f.read_text(encoding="utf-8")
        except OSError:
            continue
        for line, snippet in offenders(text):
            hits.append((f.relative_to(REPO), line, snippet))
    for f, kinds in sh_files:
        rel = f.relative_to(REPO)
        if f.name == WRITER:
            continue  # THE writer
        try:
            text = f.read_text(encoding="utf-8")
        except (OSError, UnicodeDecodeError):
            continue
        for line, snippet in shell_offenders(text, "just" if "just" in kinds else "sh"):
            hits.append((rel, line, snippet))

    if not hits:
        print(f"check-config-header-single-writer: OK — {len(files)} cmake file(s) + "
              f"{len(sh_files)} shell/just/make file(s), the mirror script is the only writer.")
        return 0

    print("check-config-header-single-writer: a SECOND writer of the per-build "
          "sizes header.", file=sys.stderr)
    for path, line, snippet in hits:
        print(f"  {path}:{line}: {snippet}", file=sys.stderr)
    print("", file=sys.stderr)
    print("  Route it through scripts/build/mirror-generated-header.sh, which "
          "knows the", file=sys.stderr)
    print("  precedence (issue 0978: the leaf's own copy is present and "
          "arbitrarily old once", file=sys.stderr)
    print("  leaves share a cargo target dir). A second writer does not just "
          "duplicate that", file=sys.stderr)
    print("  logic — it stamps the mirror's OUTPUT with a new mtime and so "
          "suppresses the", file=sys.stderr)
    print("  ninja edge that would have corrected it (issue 0985).", file=sys.stderr)
    return 1


if __name__ == "__main__":
    sys.exit(main())
