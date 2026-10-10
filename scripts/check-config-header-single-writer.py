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


# Issue 1746 -- WHEN the writer runs is half the rule. The mirror script picks
# the header out of `<build>/cargo/*`, a symlink whose target is chosen by KEY,
# and that key is final only after `_nros_entity_facts_flush` re-keys it at the
# end of the configure (issue 1700). A configure-time `execute_process` of the
# script anywhere else read a PROVISIONAL key's header, stamped the mirror's
# output newer than the archive, and suppressed the build-time edge -- the
# first build after every re-configure of a native C++ leaf failed to link.
# So a configure-time run must be the one deferred flush; a caller queues it
# with `nros_config_header_heal(...)`.
HEAL_FLUSH = "_nros_config_header_heal_flush"
_FUNC = re.compile(r"\b(function|macro)\s*\(\s*([A-Za-z_][A-Za-z0-9_]*)", re.I)
_ENDFUNC = re.compile(r"\bend(function|macro)\s*\(", re.I)
_EXEC = re.compile(r"\bexecute_process\s*\(", re.I)


def immediate_heals(text: str) -> list[tuple[int, str]]:
    """Configure-time runs of the mirror script outside the deferred flush."""
    stripped = strip_comments(text)
    # The script is usually named through a variable (`_NROS_CPP_MIRROR_SH`),
    # which is how the pre-fix site spelled it -- the 0985 lesson again.
    writer_vars = {m.group(1) for m in _BINDS.finditer(stripped) if WRITER in m.group(2)}
    found = []
    for m in _EXEC.finditer(stripped):
        depth, i, end = 0, m.end() - 1, len(stripped)
        while i < len(stripped):
            if stripped[i] == "(":
                depth += 1
            elif stripped[i] == ")":
                depth -= 1
                if depth == 0:
                    end = i
                    break
            i += 1
        span = stripped[m.start(): end + 1]
        if WRITER not in span and not any(f"${{{v}}}" in span for v in writer_vars):
            continue
        before = stripped[: m.start()]
        opens = list(_FUNC.finditer(before))
        enclosing = None
        if opens and len(opens) > len(_ENDFUNC.findall(before)):
            enclosing = opens[-1].group(2)
        if enclosing != HEAL_FLUSH:
            line = stripped.count("\n", 0, m.start()) + 1
            found.append((line, " ".join(span.split())[:110]))
    return found


# Issue 1783 -- WHAT the build-time writer keys on is the third half. The
# mirror's source is a file cargo writes as a side effect, which no CMake rule
# produces, so 0268 keyed the mirror (and 0740's per-consumer stamp) on a PROXY:
# `$<TARGET_FILE:nros_{c,cpp}-static>`. A codegen-version bump rewrites the
# header and leaves the archive byte-identical, Corrosion's `copy_if_different`
# keeps its mtime, and every message TU compiled against the museum mirror. The
# rules, all over the same cmake population:
#
#   E1  the build-time mirror rule (an `add_custom_command` running the writer)
#       is spelled ONLY in `nros_config_header_mirror`;
#   E2  no custom command keys on the staticlib proxy;
#   E3  the mirror and the stamp each re-run every build -- an input on the
#       always-out-of-date node (`_nros_config_header_rerun_node`) -- and have
#       ONE output each (Make `touch_nocreate`s every output after the first,
#       which would re-stamp the header on every build);
#   E4  a consumer reaches the mirrors only through `nros_config_header_files`
#       / `nros_config_header_object_depends`, so no consumer can stamp ONE
#       crate's mirror while its include path resolves the OTHER's (a C message
#       library linked through `NanoRosCpp` did exactly that);
#   E5  the helpers exist -- otherwise E1..E4 hold vacuously.
#   E6  every image/library CREATOR that compiles a TU including the mirror
#       still calls the consumer helper, as often as it creates such a target.
#       E1..E4 police HOW a consumer spells its edge and are silent on one that
#       has no edge at all: commenting out the message library's call left the
#       gate green (measured). The creators are named below with why, and a
#       creator that disappears or loses a call fails, so the table cannot
#       quietly go stale toward OK.
CONSUMERS = {
    # creator function: (calls required, what it compiles)
    "nros_generate_interfaces": (1, "the generated <pkg>__nano_ros_c library"),
    "nano_ros_entry": (2, "the entry's generated TU and its app sources"),
    "nano_ros_node_register": (2, "both ThreadX carrier arms"),
}
MIRROR_FN = "nros_config_header_mirror"
STAMP_FN = "_nros_config_header_stamp"
RERUN_FN = "_nros_config_header_rerun_node"
FILES_FN = "nros_config_header_files"
DEPENDS_FN = "nros_config_header_object_depends"
_ACC = re.compile(r"\badd_custom_command\s*\(", re.I)
_PROXY = re.compile(r"\$<TARGET_FILE:nros_c(?:pp)?-static>")
_HDR_PROP = re.compile(r"\bget_property\s*\([^)]*\bNROS_C(?:PP)?_CONFIG_HEADER_FILE\b", re.I)
_STAMP_CALL = re.compile(r"\b" + STAMP_FN + r"\s*\(")
_KEYWORDS = r"(?:COMMAND|DEPENDS|BYPRODUCTS|COMMENT|VERBATIM|WORKING_DIRECTORY|MAIN_DEPENDENCY|IMPLICIT_DEPENDS|DEPFILE|JOB_POOL|USES_TERMINAL|APPEND|COMMAND_EXPAND_LISTS)"
_OUTPUTS = re.compile(r"\bOUTPUT\s+(.*?)(?=\b" + _KEYWORDS + r"\b|\)\s*$)", re.S)


def _span(stripped: str, start: int) -> tuple[str, int]:
    """The balanced `name( ... )` call starting at `start` (inclusive)."""
    depth, i = 0, stripped.index("(", start)
    begin = start
    while i < len(stripped):
        if stripped[i] == "(":
            depth += 1
        elif stripped[i] == ")":
            depth -= 1
            if depth == 0:
                return stripped[begin: i + 1], i + 1
        i += 1
    return stripped[begin:], len(stripped)


def _enclosing(stripped: str, pos: int) -> str | None:
    before = stripped[:pos]
    opens = list(_FUNC.finditer(before))
    if opens and len(opens) > len(_ENDFUNC.findall(before)):
        return opens[-1].group(2)
    return None


def _defines(stripped: str, name: str) -> bool:
    return any(m.group(2) == name for m in _FUNC.finditer(stripped))


def consumer_calls(text: str) -> dict[str, int]:
    """{creator: number of DEPENDS_FN calls in its body} for the CONSUMERS
    this file defines (E6)."""
    stripped = strip_comments(text)
    out = {}
    for m in _FUNC.finditer(stripped):
        if m.group(2) not in CONSUMERS:
            continue
        end = re.compile(r"\bend" + m.group(1) + r"\s*\(", re.I).search(stripped, m.end())
        body = stripped[m.end(): end.start() if end else len(stripped)]
        out[m.group(2)] = len(re.findall(r"\b" + DEPENDS_FN + r"\s*\(", body))
    return out


def edge_offenders(text: str) -> list[tuple[int, str]]:
    """(line, reason) for every 1783-class edge defect in one cmake file."""
    stripped = strip_comments(text)
    writer_vars = {m.group(1) for m in _BINDS.finditer(stripped) if WRITER in m.group(2)}
    # Both pre-1783 sites spelled the proxy through a VARIABLE -- the mirror as
    # `set(_trigger "$<TARGET_FILE:nros_c-static>")`, the stamp as
    # `foreach(_lib nros_c-static ...)` + `$<TARGET_FILE:${_lib}>` -- so a
    # literal-only E2 passed on both (measured: the first draft of this rule).
    proxy_vars = {m.group(1) for m in _BINDS.finditer(stripped)
                  if _PROXY.search(m.group(2))}
    lib_vars = {m.group(1) for m in _BINDS.finditer(stripped)
                if re.search(r"\bnros_c(?:pp)?-static\b", m.group(2))}
    found = []

    def hit(pos: int, why: str) -> None:
        found.append((stripped.count("\n", 0, pos) + 1, why))

    for m in _ACC.finditer(stripped):
        span, _ = _span(stripped, m.start())
        fn = _enclosing(stripped, m.start())
        runs_writer = WRITER in span or any(f"${{{v}}}" in span for v in writer_vars)
        if runs_writer and fn != MIRROR_FN:
            hit(m.start(), f"E1 build-time mirror rule outside {MIRROR_FN}(): "
                + " ".join(span.split())[:90])
        if (_PROXY.search(span) or any(f"${{{v}}}" in span for v in proxy_vars)
                or any(f"$<TARGET_FILE:${{{v}}}>" in span for v in lib_vars)):
            hit(m.start(), "E2 custom command keyed on the staticlib proxy "
                "(a header can change while the archive does not): "
                + " ".join(span.split())[:90])
        if fn in (MIRROR_FN, STAMP_FN):
            if "${_rerun}" not in span:
                hit(m.start(), f"E3 {fn}() command does not depend on the "
                    f"always-out-of-date node ({RERUN_FN}), so it can skip a "
                    "header that moved")
            o = _OUTPUTS.search(span)
            if o and len(o.group(1).split()) != 1:
                hit(m.start(), f"E3 {fn}() command has {len(o.group(1).split())} "
                    "OUTPUTs; Make touch_nocreate's every output after the first")
    for fn in (MIRROR_FN, STAMP_FN):
        for m in _FUNC.finditer(stripped):
            if m.group(2) != fn:
                continue
            body, _ = _span(stripped, m.start())
            end = stripped.find("endfunction", m.start())
            body = stripped[m.start(): end if end > 0 else len(stripped)]
            if RERUN_FN + "(" not in body:
                hit(m.start(), f"E3 {fn}() never declares the rerun node ({RERUN_FN})")
    for m in _HDR_PROP.finditer(stripped):
        if _enclosing(stripped, m.start()) != FILES_FN:
            hit(m.start(), f"E4 reads a mirror-header property directly; use "
                f"{FILES_FN}() / {DEPENDS_FN}() so BOTH crates' mirrors are named")
    for m in _STAMP_CALL.finditer(stripped):
        line_start = stripped.rfind("\n", 0, m.start()) + 1
        if stripped[line_start: m.start()].strip().lower().startswith("function("):
            continue  # the definition
        if _enclosing(stripped, m.start()) != DEPENDS_FN:
            hit(m.start(), f"E4 {STAMP_FN}() called directly; use {DEPENDS_FN}()")
    return found


def self_test_edges() -> None:
    good = (
        'function(_nros_config_header_rerun_node _o _p)\nendfunction()\n'
        'function(nros_config_header_mirror _t _a)\n'
        '  _nros_config_header_rerun_node(_rerun "${X}")\n'
        '  add_custom_command(OUTPUT "${_dest}" COMMAND bash "${_NROS_CFG_MIRROR_SH}" a\n'
        '      DEPENDS "${_rerun}" ${_after_dep} VERBATIM)\n'
        'endfunction()\n'
        'set(_NROS_CFG_MIRROR_SH "${D}/scripts/build/mirror-generated-header.sh")\n'
        'function(nros_config_header_files _o)\n'
        '  get_property(_c GLOBAL PROPERTY NROS_C_CONFIG_HEADER_FILE)\n'
        'endfunction()\n'
        'function(nros_config_header_object_depends _o)\n'
        '  _nros_config_header_stamp(_s "${_o}" ${_h})\n'
        'endfunction()\n'
        'function(_nros_config_header_stamp _v _o)\n'
        '  _nros_config_header_rerun_node(_rerun "${D}/rerun")\n'
        '  add_custom_command(OUTPUT "${_stamp}" COMMAND x DEPENDS "${_rerun}" ${_deps})\n'
        'endfunction()\n')
    assert not edge_offenders(good), edge_offenders(good)
    # 0268's spelling, verbatim in shape: the mirror keyed on the archive, at
    # package scope.
    pre = ('add_custom_command(OUTPUT "${H}" COMMAND bash "${_NROS_C_MIRROR_SH}" a b c d "${H}"\n'
           '    DEPENDS cargo-build_nros_c $<TARGET_FILE:nros_c-static> VERBATIM)\n'
           'set(_NROS_C_MIRROR_SH "${D}/scripts/build/mirror-generated-header.sh")\n')
    rules = {w.split()[0] for _, w in edge_offenders(pre)}
    assert rules == {"E1", "E2"}, f"0268's mirror must be E1+E2, got {rules}"
    # The pre-1783 stamp: proxy input, no rerun node.
    stamp = ('function(_nros_config_header_stamp _v _o)\n'
             '  add_custom_command(OUTPUT "${_stamp}" COMMAND x\n'
             '      DEPENDS ${_deps} "$<TARGET_FILE:nros_cpp-static>")\nendfunction()\n')
    rules = {w.split()[0] for _, w in edge_offenders(stamp)}
    assert rules == {"E2", "E3"}, f"the pre-1783 stamp must be E2+E3, got {rules}"
    # ... and both through a variable, which is how the tree actually had them.
    via_set = ('set(_trig "$<TARGET_FILE:nros_c-static>")\n'
               'add_custom_command(OUTPUT h COMMAND x DEPENDS ${_trig})\n')
    assert any(w.startswith("E2") for _, w in edge_offenders(via_set)), \
        "a proxy bound through set() must be E2"
    via_loop = ('foreach(_lib nros_c-static nros_cpp-static)\n'
                '  list(APPEND _deps "$<TARGET_FILE:${_lib}>")\nendforeach()\n'
                'add_custom_command(OUTPUT h COMMAND x DEPENDS "$<TARGET_FILE:${_lib}>")\n')
    assert any(w.startswith("E2") for _, w in edge_offenders(via_loop)), \
        "a proxy bound through a foreach() must be E2"
    two = ('function(nros_config_header_mirror _t _a)\n'
           '  _nros_config_header_rerun_node(_rerun "${X}")\n'
           '  add_custom_command(OUTPUT "${a}" "${b}" COMMAND bash mirror-generated-header.sh\n'
           '      DEPENDS "${_rerun}")\nendfunction()\n')
    assert any("OUTPUTs" in w for _, w in edge_offenders(two)), "two outputs must be E3"
    # The pre-1783 C message library: ONE crate's property, stamped directly.
    lib = ('function(nros_generate_interfaces t)\n'
           '  get_property(_h GLOBAL PROPERTY NROS_C_CONFIG_HEADER_FILE)\n'
           '  _nros_config_header_stamp(_s "${t}" "${_h}")\nendfunction()\n')
    rules = [w.split()[0] for _, w in edge_offenders(lib)]
    assert rules == ["E4", "E4"], f"a direct property read + stamp must be E4 twice, got {rules}"
    # E6 -- a creator whose edge call is gone (commented out) counts zero.
    gone = ('function(nros_generate_interfaces t)\n'
            '  # nros_config_header_object_depends(${t} ${s})\nendfunction()\n')
    assert consumer_calls(gone) == {"nros_generate_interfaces": 0}, consumer_calls(gone)
    kept = gone.replace("  # nros", "  nros")
    assert consumer_calls(kept) == {"nros_generate_interfaces": 1}, consumer_calls(kept)


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
    # issue 1746 -- the pre-fix nros-cpp heal, verbatim in shape: an immediate
    # configure-time run at the package's own scope.
    pre_1746 = ('set(_NROS_CPP_MIRROR_SH "${D}/scripts/build/mirror-generated-header.sh")\n'
                'foreach(_h "nros_cpp_config_generated.h;nros-cpp-generated")\n'
                '  execute_process(COMMAND bash "${_NROS_CPP_MIRROR_SH}" "${B}/${_h}"\n'
                '      "${CMAKE_BINARY_DIR}" gen "${_h}" "${I}/nros/${_h}"\n'
                '      RESULT_VARIABLE rc OUTPUT_QUIET ERROR_QUIET)\n'
                'endforeach()\n')
    assert immediate_heals(pre_1746), "issue 1746's immediate heal must be caught"
    in_flush = ('function(_nros_config_header_heal_flush)\n'
                '  execute_process(COMMAND bash "${_sh}/mirror-generated-header.sh" a b c d e)\n'
                'endfunction()\n')
    assert not immediate_heals(in_flush), "the deferred flush is the allowed site"
    in_other = ('function(heal_now)\n'
                '  execute_process(COMMAND bash "${S}/mirror-generated-header.sh" a b c d e)\n'
                'endfunction()\n'
                'function(_nros_config_header_heal_flush)\nendfunction()\n')
    assert immediate_heals(in_other), "another function is not the flush"
    assert not immediate_heals('add_custom_command(OUTPUT h COMMAND bash '
                               '"${S}/mirror-generated-header.sh" a b c d e)'), \
        "the BUILD-time mirror rule is not a configure-time run"


def main() -> int:
    self_test()
    self_test_edges()

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
    edge_hits = []
    defined: set[str] = set()
    calls: dict[str, int] = {}
    for f in files:
        try:
            text = f.read_text(encoding="utf-8")
        except OSError:
            continue
        for line, snippet in offenders(text):
            hits.append((f.relative_to(REPO), line, snippet))
        for line, why in edge_offenders(text):
            edge_hits.append((f.relative_to(REPO), line, why))
        defined |= {n for n in (MIRROR_FN, STAMP_FN, RERUN_FN, FILES_FN, DEPENDS_FN)
                    if _defines(strip_comments(text), n)}
        for fn, n in consumer_calls(text).items():
            calls[fn] = calls.get(fn, 0) + n
        for line, snippet in immediate_heals(text):
            hits.append((f.relative_to(REPO), line,
                         "configure-time mirror run outside the deferred heal "
                         "(issue 1746; use nros_config_header_heal): " + snippet))
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

    # E5 -- the rules above hold vacuously if the helpers they name are gone.
    for n in sorted({MIRROR_FN, STAMP_FN, RERUN_FN, FILES_FN, DEPENDS_FN} - defined):
        edge_hits.append((Path("cmake"), 0, f"E5 no cmake file defines {n}() -- the "
                          "1783 edge rules would hold vacuously"))
    # E6 -- a creator with no edge is the defect E1..E4 cannot see.
    for fn, (want, what) in sorted(CONSUMERS.items()):
        if fn not in calls:
            edge_hits.append((Path("cmake"), 0, f"E6 no cmake file defines {fn}() -- "
                              "remove it from CONSUMERS if it is gone, or the rule "
                              "holds vacuously"))
        elif calls[fn] < want:
            edge_hits.append((Path("cmake"), 0, f"E6 {fn}() calls {DEPENDS_FN}() "
                              f"{calls[fn]}x, needs {want} ({what}): a TU it compiles "
                              "would read a mirror nothing orders it after"))
    if edge_hits:
        print("check-config-header-single-writer: the per-build sizes-header MIRROR "
              "has an edge that can go stale (issue 1783).", file=sys.stderr)
        for path, line, why in edge_hits:
            print(f"  {path}:{line}: {why}", file=sys.stderr)
        print("", file=sys.stderr)
        print("  The mirror's source is a file cargo writes as a side effect; no "
              "CMake rule", file=sys.stderr)
        print("  produces it, so no proxy (the staticlib) tracks it. Use "
              "cmake/NanoRosConfigHeaderMirror.cmake:", file=sys.stderr)
        print(f"  {MIRROR_FN}() to produce, {DEPENDS_FN}() to consume.",
              file=sys.stderr)
        if not hits:
            return 1

    if not hits:
        print(f"check-config-header-single-writer: OK — {len(files)} cmake file(s) + "
              f"{len(sh_files)} shell/just/make file(s), the mirror script is the only writer "
              "and its build-time rule re-runs every build (issue 1783).")
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
