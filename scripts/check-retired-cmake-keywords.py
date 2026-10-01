#!/usr/bin/env python3
"""issues 1033 + 1554 — a RETIRED cmake API keyword survives nowhere but its tombstone.

# The failure this prevents

Our public cmake verbs retire a keyword by raising a `FATAL_ERROR` naming the
replacement (a TOMBSTONE), so an old caller fails loudly instead of having the
keyword and its values fall into `UNPARSED_ARGUMENTS` and become source files
nobody can find. That is the right shape — but it only fires when somebody
CONFIGURES the caller, and this repo has callers that only a Zephyr SDK + west
lane configures. Nothing on the `pull_request` lane does.

So phase-412 retired `nano_ros_node_register(... ENTITIES ...)`, migrated the
six `examples/workspaces/cpp` packages, and MISSED the six standalone
`examples/zephyr/cpp` leaves that issue 1033 had taught to declare the day
before. All six could not CONFIGURE on `main` (issue 1033).

Issue 1554 is the same retirement read from the other side. The callers were
clean, and three of the build's OWN user-facing strings kept telling the reader
to write `ENTITIES` for 24 days — one of them the remedy attached to a
FATAL_ERROR, read exactly when the reader is already stuck. This gate could not
see them: it exempted `cmake/` WHOLESALE, because the tombstone lives there.
The exemption's reach (a directory) was wider than its reason (one function),
which is issue 0196's shape the other way round.

# What it checks

The retired set is DISCOVERED from the cmake sources, never hand-listed: every
`"<fn>(...): <KEYWORD> was retired|removed"` — or `"${<var>}: <KEYWORD> was …"`,
the shape a tombstone SHARED between verbs uses — contributes one retirement,
and its TOMBSTONE is the `function()`/`macro()` that contains it. Then, over
every tracked cmake file outside `tests/` and `third-party/`, comment-stripped
by the shared stripper (`scripts/lib/comments.py`, so a `#` INSIDE a string —
the text a generated file's comment is written from — is still read):

  1. A keyword that is FULLY retired (no live `cmake_parse_arguments` grammar
     outside its tombstones still declares it) may appear as a token NOWHERE
     outside its tombstone function: not in a caller, not in a `message()`
     remedy, not in a `string(APPEND)` written into a generated file, not in a
     revived grammar. `#` comments explaining the retirement are free.
  2. A keyword retired from ONE function but live in others (`MODEL`) may not
     be passed by a caller outside `cmake/` (the original 1033 rule), nor
     inside a call to the function that retired it anywhere.
  3. A tombstone's own remedy may name, besides the keyword it retires, only
     keywords its function still ACCEPTS. `nano_ros_entry`'s `HOST` tombstone
     told the reader to "point MODEL at the per-host SystemModel" for months
     after phase-405 W4 took `MODEL` out of `nano_ros_entry`'s grammar — the
     sibling tombstone in `nano_ros_add_executable` had been corrected, this
     one had not. A remedy that names a dead keyword is a second tombstone.

A retirement whose wording this gate cannot parse is a failure, not a skip:
1554's own wave moved the `ENTITIES` refusal to `"${_call}: ENTITIES was
retired"`, the old pattern stopped matching it, and the gate kept printing OK —
it still found `HOST` and `MODEL`, so its "found nothing" guard never fired.
Every `<KEYWORD> was retired|removed` in non-comment `cmake/` code must now be
attributed.
"""

import os
import re
import shutil
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, "scripts", "lib"))
import comments  # noqa: E402  phase-472 W3 — the one comment stripper

# `"nano_ros_node_register(${_NRC_NAME}): ENTITIES was retired (phase-412)"`,
# `"nano_ros_entry(${_NRA_NAME}): HOST was removed (phase-326 …"` and the
# shared-tombstone shape `"${_call}: ENTITIES was retired …"`.
RETIREMENT = re.compile(
    r'"(?:[A-Za-z_][A-Za-z0-9_]*\([^"]*\)|\$\{[A-Za-z_][A-Za-z0-9_]*\}):\s*'
    r'(?P<kw>[A-Z][A-Z0-9_]*)\s+was\s+(?:retired|removed)\b'
)

# Every way a keyword retirement can be WORDED, attributable or not. A hit this
# finds and RETIREMENT does not is a tombstone the gate is blind to.
RETIREMENT_ANY = re.compile(
    r'(?<![A-Za-z0-9_${])(?P<kw>[A-Z][A-Z0-9_]*)\s+was\s+(?:retired|removed)\b'
)

# The gate harnesses exercise refusals on purpose; vendored cmake is not ours.
EXEMPT_PREFIXES = ("tests/", "third-party/")

TOKEN = r"(?<![A-Za-z0-9_]){}(?![A-Za-z0-9_])"
CALL = re.compile(r"(?<![A-Za-z0-9_$])([A-Za-z_][A-Za-z0-9_]*)\s*\(")
KEYWORD = re.compile(r"^[A-Z][A-Z0-9_]*$")


class Tombstone:
    def __init__(self, kw, fn, rel, span, msg_span):
        self.kw = kw          # the retired keyword
        self.fn = fn          # the function/macro the refusal lives in
        self.rel = rel        # its file
        self.span = span      # (start, end) of that function's body
        self.msg_span = msg_span  # (start, end) of the message() call


def cmake_files(root):
    """Tracked cmake (CMakeLists.txt, *.cmake), minus tests/ and vendored trees."""
    out = subprocess.run(
        ["git", "-C", root, "ls-files", "*CMakeLists.txt", "*.cmake"],
        capture_output=True, text=True,
    )
    if out.returncode == 0:
        paths = out.stdout.split()
    else:
        paths = []
        # walk-ok: the self-test builds a synthetic tree with no git index.
        for dirpath, dirnames, filenames in os.walk(root):
            dirnames[:] = [d for d in dirnames if d not in (".git", "build", "target")]
            for fn in filenames:
                if fn == "CMakeLists.txt" or fn.endswith(".cmake"):
                    paths.append(os.path.relpath(os.path.join(dirpath, fn), root))
    return sorted(
        p for p in paths
        if not p.startswith(EXEMPT_PREFIXES) and "/third-party/" not in p
    )


def read(root, rel):
    try:
        with open(os.path.join(root, rel), encoding="utf8", errors="replace") as fh:
            return fh.read()
    except OSError:
        return None


class Source:
    """One cmake file: raw text, code (comments blanked), and structure."""

    def __init__(self, rel, text):
        self.rel = rel
        self.text = text
        self.code = comments.strip_comments(text, "cmake")
        # Strings blanked too: parens and keywords inside a string are not
        # structure.
        self.shape = comments.strip_comments(text, "cmake", strings=True)
        self.calls = list(self._calls())
        self.functions = self._functions()

    def _calls(self):
        """(name, open, close) for every command invocation, balanced on the
        string-blanked text so a `)` inside a message is not the end."""
        s = self.shape
        for m in CALL.finditer(s):
            depth, i = 0, m.end() - 1
            while i < len(s):
                if s[i] == "(":
                    depth += 1
                elif s[i] == ")":
                    depth -= 1
                    if depth == 0:
                        break
                i += 1
            yield m.group(1), m.end() - 1, i

    def _functions(self):
        """[(name, start, end)] for every function()/macro() definition."""
        out, stack = [], []
        for name, o, c in self.calls:
            low = name.lower()
            if low in ("function", "macro"):
                args = self.code[o + 1:c].split()
                stack.append((args[0] if args else "?", o))
            elif low in ("endfunction", "endmacro") and stack:
                fname, start = stack.pop()
                out.append((fname, start, c))
        return out

    def enclosing_function(self, off):
        best = None
        for name, a, b in self.functions:
            if a <= off <= b and (best is None or a > best[1]):
                best = (name, a, b)
        return best

    def enclosing_call(self, off, name=None):
        best = None
        for n, a, b in self.calls:
            if a <= off <= b and (name is None or n == name):
                if best is None or a > best[1]:
                    best = (n, a, b)
        return best

    def grammars(self):
        """[(owning function, offset, {keywords})] for each literal
        `cmake_parse_arguments` keyword list in this file."""
        out = []
        for name, o, c in self.calls:
            if name != "cmake_parse_arguments":
                continue
            kws = set()
            for lit in re.findall(r'"([^"]*)"', self.code[o:c + 1]):
                for tok in lit.split(";"):
                    if KEYWORD.match(tok):
                        kws.add(tok)
            fn = self.enclosing_function(o)
            out.append((fn[0] if fn else None, o, kws))
        return out

    def line(self, off):
        return self.code.count("\n", 0, off) + 1


def load(root):
    sources = []
    for rel in cmake_files(root):
        text = read(root, rel)
        if text is not None:
            sources.append(Source(rel, text))
    return sources


def tombstones(sources):
    """Every attributable retirement, plus every one this gate cannot read."""
    found, blind = [], []
    for src in sources:
        if not src.rel.startswith("cmake/"):
            continue
        attributed = set()
        for m in RETIREMENT.finditer(src.code):
            off = m.start("kw")
            attributed.add(off)
            fn = src.enclosing_function(off)
            msg = src.enclosing_call(off, "message")
            if fn is None or msg is None:
                blind.append((src.rel, src.line(off), m.group("kw"),
                              "not inside a function()/macro() message()"))
                continue
            found.append(Tombstone(m.group("kw"), fn[0], src.rel,
                                   (fn[1], fn[2]), (msg[1], msg[2])))
        for m in RETIREMENT_ANY.finditer(src.code):
            if m.start("kw") not in attributed:
                blind.append((src.rel, src.line(m.start("kw")), m.group("kw"),
                              "wording this gate cannot attribute to a function"))
    return found, blind


def check(root):
    """Return a list of problem strings (empty == pass)."""
    sources = load(root)
    found, blind = tombstones(sources)
    problems = []
    if not found:
        # Never pass because the pattern stopped matching: that is a blind gate
        # reporting success, and the thing it guards configures on no PR lane.
        return [
            "no retirement guards found under cmake/ — this gate looks for\n"
            '  message(FATAL_ERROR "<fn>(...): <KEYWORD> was retired|removed ...")\n'
            "  and found none. Either the wording moved (fix the pattern) or the\n"
            "  guards were deleted. Do not delete this check."
        ]
    for rel, ln, kw, why in blind:
        problems.append(
            f"{rel}:{ln} retires {kw} in {why}. A tombstone this gate cannot\n"
            f"    read is a retirement nothing polices — issue 1554's own wave moved\n"
            f'    one to "${{_call}}: ENTITIES was retired" and the gate stayed green.\n'
            f'    Word it "<fn>(...): {kw} was retired|removed" or "${{<var>}}: {kw} '
            f'was …"\n    inside the function()/macro() that refuses it.'
        )

    by_kw = {}
    for t in found:
        by_kw.setdefault(t.kw, []).append(t)

    def in_tombstone(src, off, kw):
        return any(t.rel == src.rel and t.span[0] <= off <= t.span[1]
                   for t in by_kw[kw])

    # The functions that REFUSE a keyword: each tombstone's own, plus every
    # function that hands its ARGN to a shared tombstone. A parse list in one of
    # those declares the keyword only to refuse it — not a live grammar.
    refusers = {kw: {t.fn for t in stones} for kw, stones in by_kw.items()}
    for kw, names in refusers.items():
        tomb_fns = set(names)
        for src in sources:
            for name, o, _c in src.calls:
                if name in tomb_fns:
                    fn = src.enclosing_function(o)
                    if fn is not None:
                        names.add(fn[0])

    # Live grammar: a parse list declaring the keyword in a function that does
    # not refuse it.
    live = {kw: set() for kw in by_kw}
    union = set()
    fn_grammar = {}
    for src in sources:
        for fn, off, kws in src.grammars():
            union |= kws
            if fn is not None:
                fn_grammar.setdefault(fn, set()).update(kws)
            for kw in by_kw:
                if kw in kws and fn not in refusers[kw]:
                    live[kw].add(f"{fn or '?'} ({src.rel}:{src.line(off)})")

    for kw, stones in sorted(by_kw.items()):
        rx = re.compile(TOKEN.format(re.escape(kw)))
        retired_fns = refusers[kw]
        where = ", ".join(sorted({f"{t.fn}() in {t.rel}" for t in stones}))
        for src in sources:
            for m in rx.finditer(src.code):
                off = m.start()
                if in_tombstone(src, off, kw):
                    continue
                ln = src.line(off)
                api = src.rel.startswith("cmake/")
                if live[kw]:
                    # Partially retired: only a call into a function that
                    # retired it (anywhere), or any use by a caller outside the
                    # API (the 1033 rule, unchanged), is decidable.
                    call = src.enclosing_call(off)
                    into_retirer = call is not None and call[0] in retired_fns
                    if api and not into_retirer:
                        continue
                if api:
                    problems.append(
                        f"{src.rel}:{ln} names {kw} outside its tombstone "
                        f"({where}). {kw} was retired, so a message() remedy, a "
                        f"string written into a generated file, or a grammar "
                        f"that still names it sends the reader to a FATAL_ERROR "
                        f"(issue 1554). Name the live replacement the tombstone "
                        f"gives instead; a `#` comment may explain the retirement."
                    )
                else:
                    problems.append(
                        f"{src.rel}:{ln} passes {kw}, which {where} RETIRED. "
                        f"Configuring this file is a FATAL_ERROR — and nothing "
                        f"on the pull_request lane configures it, so the error "
                        f"reports to nobody. Delete the argument and follow the "
                        f"replacement the guard names."
                    )

    # Rule 3 — a tombstone's remedy names only keywords its verb accepts.
    #
    # "Its verb" is the tombstone's own function, or — for a SHARED tombstone
    # with no grammar of its own — the verbs that hand it their ARGN, since
    # the reader is told to edit THAT call. A capitalised word counts as an
    # instruction to write a keyword only in KEYWORD POSITION: inside
    # backticks, or followed by a value (`<dir>`, `${x}`, a path, `k=v`). This
    # repo's prose capitalises for emphasis ("ONCE PER SYSTEM, in a contract"),
    # and a rule that fires on emphasis is a rule somebody switches off.
    retired_in = {}
    for t in found:
        retired_in.setdefault(t.fn, set()).add(t.kw)
    src_by_rel = {s.rel: s for s in sources}
    for t in found:
        src = src_by_rel[t.rel]
        body = src.code[t.msg_span[0]:t.msg_span[1] + 1]
        # The message as the reader sees it: string contents joined, every
        # `${...}` a value.
        shown = " ".join(re.findall(r'"((?:[^"\\]|\\.)*)"', body))
        shown = re.sub(r"\$\{[^}]*\}", "VALUE/", shown).replace("\\n", " ")
        verbs = {t.fn} if fn_grammar.get(t.fn) else (refusers[t.kw] - {t.fn})
        accepts = set()
        for v in verbs:
            accepts |= fn_grammar.get(v, set()) - retired_in.get(v, set())
        named = set()
        for seg in re.findall(r"`([^`]*)`", shown):
            named |= set(re.findall(r"(?<![A-Za-z0-9_])[A-Z][A-Z0-9_]*(?![A-Za-z0-9_])", seg))
        named |= set(re.findall(
            r"(?<![A-Za-z0-9_])([A-Z][A-Z0-9_]*)\s+(?=[<$]|[^\s,;)]*[/=:.][^\s,;)]*)", shown))
        for tok in sorted(named):
            if tok == t.kw or tok not in union or tok in accepts:
                continue
            problems.append(
                f"{t.rel}:{src.line(t.msg_span[0])} the {t.kw} tombstone in "
                f"{t.fn}() tells the reader to write {tok}, which "
                f"{'/'.join(sorted(verbs)) or t.fn}() does not accept (live keywords: "
                f"{', '.join(sorted(accepts)) or 'none'}). A remedy naming a dead "
                f"keyword is a second tombstone (issue 1554)."
            )
    return problems


def _write(root, files):
    for rel, text in files.items():
        os.makedirs(os.path.join(root, os.path.dirname(rel)), exist_ok=True)
        with open(os.path.join(root, rel), "w", encoding="utf8") as fh:
            fh.write(text)


# A verb that retired ENTITIES through a SHARED tombstone (the 1554 shape), and
# a verb that retired HOST in place while keeping its live grammar.
SHARED = (
    "function(_nros_entities_retired _call)\n"
    '    if(NOT "ENTITIES" IN_LIST ARGN)\n'
    "        return()\n"
    "    endif()\n"
    "    message(FATAL_ERROR\n"
    '        "${_call}: ENTITIES was retired (phase-412).\\n"\n'
    '        "  Delete the ENTITIES argument; state it in the contract sidecar.")\n'
    "endfunction()\n"
    "function(nano_ros_node_register)\n"
    '    cmake_parse_arguments(_NRC "" "NAME" "SOURCES" ${ARGN})\n'
    '    _nros_entities_retired("nano_ros_node_register(${_NRC_NAME})" ${ARGN})\n'
    "endfunction()\n"
)
IN_PLACE = (
    "function(nano_ros_entry)\n"
    '    cmake_parse_arguments(_NRA "" "NAME;BRINGUP;LAUNCH;HOST" "LAUNCH_ARGS" ${ARGN})\n'
    "    # HOST is kept solely to fail loudly for a pre-phase-326 caller.\n"
    "    if(_NRA_HOST)\n"
    "        message(FATAL_ERROR\n"
    '            "nano_ros_entry(${_NRA_NAME}): HOST was removed (phase-326) — "\n'
    '            "pass BRINGUP <dir> LAUNCH <f> LAUNCH_ARGS host=${_NRA_HOST}")\n'
    "    endif()\n"
    "endfunction()\n"
)
# MODEL retired from one verb, live in another.
PARTIAL = (
    "function(nano_ros_add_executable name)\n"
    '    cmake_parse_arguments(_NRE "" "BRINGUP;MODEL" "" ${ARGN})\n'
    "    if(_NRE_MODEL)\n"
    "        message(FATAL_ERROR\n"
    '            "nano_ros_add_executable(${name}): MODEL was removed — pass BRINGUP")\n'
    "    endif()\n"
    "endfunction()\n"
    "function(nros_entity_inventory)\n"
    '    cmake_parse_arguments(_E "" "MODEL" "" ${ARGN})\n'
    "endfunction()\n"
)


def self_test():
    """Every probe asserts a failure this gate must catch, plus the clean case,
    so a gate that stopped matching anything cannot report success."""
    api = "cmake/NanoRos.cmake"
    leaf = "examples/leaf/CMakeLists.txt"
    base = SHARED + IN_PLACE + PARTIAL
    remedy_bad = (
        "function(_report)\n"
        '    message(FATAL_ERROR "Fix the ENTITIES argument of nano_ros_node_register().")\n'
        "endfunction()\n"
    )
    generated_bad = (
        "function(_emit out)\n"
        '    string(APPEND _k "# declare ENTITIES on every component to narrow them\\n")\n'
        "endfunction()\n"
    )
    comment_ok = (
        "# phase-412 retired ENTITIES; the contract states it now.\n"
        "function(_emit out)\n"
        '    string(APPEND _k "# declare entities in the contract sidecar\\n")  # was ENTITIES\n'
        "endfunction()\n"
    )
    # The shape 1554's wave removed: a verb that still PARSES the keyword it
    # hands to the shared tombstone.
    grammar_revived = (
        "function(nros_components_register_node)\n"
        '    cmake_parse_arguments(_C "" "EXECUTABLE" "ENTITIES" ${ARGN})\n'
        '    _nros_entities_retired("nros_components_register_node()" ${ARGN})\n'
        "endfunction()\n"
    )
    cases = [
        ({api: base, leaf: "nano_ros_node_register(NAME l)\n"}, 0,
         "the tombstones alone, and a clean caller"),
        ({api: base + remedy_bad}, 1,
         "a message() remedy naming ENTITIES outside its tombstone (1554)"),
        ({api: base + generated_bad}, 1,
         "a `#` INSIDE a string written into a generated file is still read"),
        ({api: base + comment_ok}, 0,
         "`#` comments explaining the retirement pass"),
        ({api: base + grammar_revived}, 1,
         "a grammar that re-declares a fully retired keyword"),
        ({api: base, leaf: "nano_ros_node_register(NAME l\n    ENTITIES sub:a/msg/B)\n"}, 1,
         "a caller still passing ENTITIES (1033)"),
        ({api: base, leaf: "nano_ros_node_register(NAME l ENTITIES)\n"}, 1,
         "a caller passing the BARE keyword"),
        ({api: base, leaf: "set(BUDGET_IDENTITIES 4)\nset(X_MAX_ENTITIES 4)\n"}, 0,
         "IDENTITIES / MAX_ENTITIES must not match as substrings"),
        ({api: base, leaf: "nano_ros_entry(NAME app HOST alpha)\n"}, 1,
         "a caller passing HOST"),
        ({api: base, leaf: "nano_ros_add_executable(app MODEL m.yaml)\n"}, 1,
         "a caller passing MODEL to the verb that retired it"),
        ({api: base + "function(_x)\n    nros_entity_inventory(MODEL \"${m}\")\nendfunction()\n"}, 0,
         "MODEL passed to a verb where it is still LIVE"),
        ({api: base + "function(_x)\n    nano_ros_add_executable(app MODEL m)\nendfunction()\n"}, 1,
         "MODEL passed to the verb that retired it, from inside the API"),
        ({api: base.replace("pass BRINGUP <dir> LAUNCH <f> LAUNCH_ARGS host=${_NRA_HOST}",
                            "e.g. MODEL config/multihost_${_NRA_HOST}_model.yaml")}, 1,
         "a tombstone remedy naming a keyword its function does not accept"),
        ({api: base.replace("pass BRINGUP <dir>", "pass `MODEL` and BRINGUP <dir>")}, 1,
         "the same, written in backticks"),
        ({api: base.replace("state it in the contract sidecar",
                            "state it ONCE PER SYSTEM, in the contract sidecar")
          + "function(nros_ws_meta)\n    cmake_parse_arguments(_W \"\" \"SYSTEM\" \"\" ${ARGN})\nendfunction()\n"}, 0,
         "prose emphasis that happens to spell some verb's keyword is not an instruction"),
        ({api: base.replace("state it in the contract sidecar",
                            "or pass SOURCES <file> instead")}, 0,
         "a shared tombstone may name a keyword its CALLING verb accepts"),
        ({api: base.replace('"${_call}: ENTITIES was retired', '"${_call} -- ENTITIES was retired')}, 1,
         "a retirement worded so the gate cannot attribute it (1554's blind spot)"),
        ({api: "# no retirement guard at all\n", leaf: "nano_ros_entry(NAME a)\n"}, 1,
         "the guard wording moved and this gate went blind"),
    ]
    failures = 0
    tmp = tempfile.mkdtemp()
    try:
        for files, want, label in cases:
            root = os.path.join(tmp, "t")
            shutil.rmtree(root, ignore_errors=True)
            _write(root, files)
            got = check(root)
            if (1 if got else 0) != want:
                sys.stderr.write(f"  self-test FAIL: {label} — got {got}, want {'a failure' if want else 'none'}\n")
                failures += 1
    finally:
        shutil.rmtree(tmp, ignore_errors=True)
    if failures:
        sys.stderr.write(f"check-retired-cmake-keywords: {failures} self-test failure(s)\n")
        return 1
    print(f"check-retired-cmake-keywords: self-test OK ({len(cases)} cases)")
    return 0


def main():
    # On the NORMAL path, not behind a flag: a negative control nobody runs
    # decays into a comment (`check-gate-selftests`).
    if self_test():
        return 1
    if "--self-test" in sys.argv:
        return 0
    problems = check(ROOT)
    if problems:
        sys.stderr.write("check-retired-cmake-keywords FAILED:\n")
        for p in problems:
            sys.stderr.write(f"  {p}\n")
        return 1
    found, _ = tombstones(load(ROOT))
    names = ", ".join(sorted({f"{t.kw} ({t.fn})" for t in found}))
    print(f"check-retired-cmake-keywords OK: each retired keyword lives only in its tombstone: {names}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
