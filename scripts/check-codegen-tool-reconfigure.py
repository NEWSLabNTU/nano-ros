#!/usr/bin/env python3
"""A configure-time emitter must make a rebuilt `nros` re-run its configure.

Issue 1018. The chain is::

    rosidl-codegen sources  ->  the in-tree `nros` CLI  ->  generated interfaces

and the middle arrow is manual by design (`just setup-cli`; compiling at build
time is forbidden here). This gate is about the RIGHT arrow, which is not
manual and is easy to lose.

The BUILD-time generator states it outright: `cmake/NanoRosGenerateInterfaces
.cmake`'s codegen `add_custom_command` carries `DEPENDS … ${_NANO_ROS_CODEGEN
_TOOL}`, so a newer tool re-emits. A CONFIGURE-time emitter cannot express that
edge at all — `execute_process()` has already run by the time ninja is deciding
anything — so its freshness reduces to *does a configure happen*. If the tool is
not a `CMAKE_CONFIGURE_DEPENDS` of the directory that emits, the answer after a
`just setup-cli` is no, and the build dir keeps museum generated code while
every source-level dependency looks current.

`nano_ros_entry()` learned this as issue #182 and registered the binary inline,
in its own function body. That fixed the site and not the class: three sibling
configure-time emitters — the Zephyr interfaces generator, `nros_system_
generate()`, and the ESP-IDF shim's `codegen-system` — registered nothing, so a
Zephyr image inherited freshness only when it ALSO happened to call
`nano_ros_entry()`, which no single-example image does.

WHAT IT CHECKS

Every cmake file that runs an EMITTING `nros` verb at CONFIGURE time must also
call `nros_codegen_tool_reconfigure()`. `add_custom_command` / `add_custom_target`
blocks are excluded: those carry the tool in `DEPENDS`, which is the edge this
gate demands where it cannot be expressed.

The verb is matched wherever it appears as a bare token, not only next to the
tool, because the three spellings in this tree differ — a direct
`execute_process(COMMAND "${tool}" codegen-system …)`, a command built into a
variable (`set(_cmd "${tool}" codegen …)`), and arguments built into a variable
with the tool supplied at the call (`set(_args codegen entry …)`). An
adjacency-based first draft missed two of the four sites, which is why the rule
is written this way. Verbs inside double-quoted strings are prose (a
`FATAL_ERROR` naming the command) and are blanked before matching.

Run against `origin/main` this reports four files, `NanoRosEntry.cmake` among
them: that one DID register, inline, in its own function body. Demanding the
shared spelling is the point — a rule with one call site and no name is a rule
the next three emitters cannot find.

Emitting verbs are the ones whose output OUTLIVES the configure and is compiled
into the image: `codegen` (message/service/action bindings, and `codegen entry`)
and `codegen-system` (`system_config.h`). Deliberately NOT every CLI call:

* `codegen resolve-deps` writes a `.cmake` fragment that the SAME configure
  `include()`s and that every configure rewrites — it cannot be stale relative
  to the configure that produced it.
* the fact/inventory verbs (`ws entity-inventory`, `board facts`, `profile …`)
  are the same shape: read back inside their own configure.

Both of those still get the tool change the moment any configure runs, and the
emitters above are what make one run.

The check is FILE-level rather than block-level on purpose. A directory
legitimately reaches several emitters from one module, the registration is
deduplicated, and a per-block rule would demand a call beside each one for no
extra safety.

Usage::

    check-codegen-tool-reconfigure.py [--selftest]
"""

import argparse
import re
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent

HELPER = "nros_codegen_tool_reconfigure"

# Verbs whose emitted files outlive the configure. See the module docstring for
# why `resolve-deps` and the fact verbs are not here.
EMITTING_VERBS = ("codegen", "codegen-system")

# `codegen` with this argument is the same-configure fragment writer, not an
# emitter of anything a later build step compiles.
NON_EMITTING_ARG = "resolve-deps"


def strip_comments(text):
    """Drop cmake `#` comments, keeping quoted `#` (and every newline)."""
    out = []
    for line in text.splitlines(keepends=True):
        in_str = False
        cut = None
        i = 0
        while i < len(line):
            c = line[i]
            if c == "\\" and in_str:
                i += 2
                continue
            if c == '"':
                in_str = not in_str
            elif c == "#" and not in_str:
                cut = i
                break
            i += 1
        out.append(line if cut is None else line[:cut] + "\n")
    return "".join(out)


def blank_blocks(text, command):
    """Replace every paren-balanced `<command>(...)` call with blanks.

    Keeps the file's length and newlines so later offsets stay meaningful.
    """
    pat = re.compile(r"(?<![\w.-])" + re.escape(command) + r"\s*\(")
    out = list(text)
    for m in pat.finditer(text):
        depth = 1
        i = m.end()
        while i < len(text) and depth:
            if text[i] == "(":
                depth += 1
            elif text[i] == ")":
                depth -= 1
            i += 1
        for j in range(m.start(), i):
            if out[j] != "\n":
                out[j] = " "
    return "".join(out)


def blank_strings(text):
    """Blank the contents of double-quoted strings.

    A verb NAMED in a message — `nros codegen entry` inside a FATAL_ERROR — is
    prose, not an invocation. Every real invocation spells the verb as a bare
    argument, because cmake would otherwise pass it as one word.
    """
    out = list(text)
    i, in_str = 0, False
    while i < len(text):
        c = text[i]
        if in_str and c == "\\":
            out[i] = " "
            if i + 1 < len(text) and text[i + 1] != "\n":
                out[i + 1] = " "
            i += 2
            continue
        if c == '"':
            in_str = not in_str
        elif in_str and c != "\n":
            out[i] = " "
        i += 1
    return "".join(out)


VERB_RE = re.compile(
    r"(?<![\w./$-])(" + "|".join(sorted(EMITTING_VERBS, key=len, reverse=True)) + r")(?![\w./-])"
)


def emitting_verbs(text):
    """Emitting verbs invoked at CONFIGURE time in this file.

    Build-time `add_custom_command` / `add_custom_target` are excluded: those
    carry the tool in `DEPENDS`, which is the edge this gate exists to demand
    where it cannot be expressed.

    The verb is matched wherever it appears as a bare token, not only adjacent
    to the tool, because all three spellings in this tree are different:
    `execute_process(COMMAND "${tool}" codegen-system …)`,
    `set(_cmd "${tool}" codegen …)` run later, and `set(_args codegen entry …)`
    with the tool supplied at the call. Requiring adjacency missed two of four.
    """
    text = blank_strings(blank_blocks(blank_blocks(text, "add_custom_command"), "add_custom_target"))
    found = set()
    for m in VERB_RE.finditer(text):
        verb = m.group(1)
        if verb == "codegen":
            tail = text[m.end() : m.end() + 40].split()
            if tail and tail[0] == NON_EMITTING_ARG:
                continue
        found.add(verb)
    return sorted(found)


def offenders(files, read):
    """Files running an emitting verb at configure time with no registration."""
    bad = []
    for rel in files:
        text = strip_comments(read(rel))
        verbs = emitting_verbs(text)
        if verbs and HELPER + "(" not in blank_strings(text):
            bad.append((rel, verbs))
    return bad


def tracked_cmake():
    out = subprocess.run(
        ["git", "ls-files", "*.cmake", "*CMakeLists.txt", "*.cmake.in"],
        cwd=REPO,
        capture_output=True,
        text=True,
        check=True,
    ).stdout.split()
    return [p for p in out if not p.startswith("third-party/")]


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--selftest", action="store_true")
    args = ap.parse_args()
    if args.selftest:
        return selftest(verbose=True)
    # On the NORMAL path, every time — a negative control nobody runs decays
    # into a comment (AGENTS.md "a gate must run its own selftest").
    selftest()

    files = tracked_cmake()
    bad = offenders(files, lambda rel: (REPO / rel).read_text(errors="replace"))
    if bad:
        print("check-codegen-tool-reconfigure: FAILED", file=sys.stderr)
        for rel, verbs in bad:
            print(
                f"  {rel}: runs `nros {'`, `nros '.join(verbs)}` at configure time "
                f"but never calls {HELPER}()",
                file=sys.stderr,
            )
        print(
            "\nA configure-time emitter has no add_custom_command DEPENDS to carry the\n"
            "tool, so nothing re-runs it when `nros` is rebuilt and the build dir keeps\n"
            "museum generated code (issue 1018, and #182 one site over). Call\n"
            f"  {HELPER}(\"<resolved nros path>\")\n"
            "beside the emitter. Do not exempt the call site — extend this gate if a\n"
            "genuinely non-emitting verb trips it.",
            file=sys.stderr,
        )
        return 1
    print(f"check-codegen-tool-reconfigure: OK ({len(files)} cmake files)")
    return 0


def selftest(verbose=False):
    ok = fail = 0

    def chk(what, cond):
        nonlocal ok, fail
        if cond:
            ok += 1
            if verbose:
                print(f"  ok   {what}")
        else:
            fail += 1
            print(f"  FAIL {what}", file=sys.stderr)

    reg = f'{HELPER}("${{_tool}}")\n'
    emit = 'execute_process(COMMAND "${_tool}" codegen --args-file "${_a}")\n'
    emit_sys = 'execute_process(COMMAND "${_cli}" codegen-system --out "${_o}")\n'
    resolve = (
        'execute_process(COMMAND "${_tool}" codegen resolve-deps\n'
        '    --package-xml "${_x}" --output-cmake "${_o}")\n'
    )
    # The two real spellings an adjacency rule missed.
    via_cmd_var = (
        'set(_codegen_cmd "${_tool}" codegen --language cpp --args-file "${_a}")\n'
        "execute_process(COMMAND ${_codegen_cmd} RESULT_VARIABLE _rc)\n"
    )
    via_args_var = (
        "set(_cli_args\n    codegen entry\n    --lang c)\n"
        'execute_process(COMMAND "${_tool}" ${_cli_args})\n'
    )

    files = {
        "bare.cmake": emit,
        "registered.cmake": reg + emit,
        "system.cmake": emit_sys,
        "resolve.cmake": resolve,
        "custom.cmake": 'add_custom_command(OUTPUT o COMMAND "${_tool}" codegen -a x)\n',
        "commented.cmake": "# execute_process(COMMAND nros codegen --args-file x)\n",
        "varonly.cmake": 'execute_process(COMMAND "${_NANO_ROS_CODEGEN_TOOL}" --version)\n',
        "nested.cmake": (
            reg + 'execute_process(COMMAND ${CMAKE_COMMAND} -E env A=b\n'
            '    "${_tool}" codegen --args-file "${_a}"\n'
            '    RESULT_VARIABLE _rc)\n'
        ),
        "both.cmake": emit + resolve,
        "cmdvar.cmake": via_cmd_var,
        "argsvar.cmake": via_args_var,
        "prose.cmake": (
            reg.replace(HELPER, "other_fn")
            + 'message(FATAL_ERROR "`nros codegen entry` failed")\n'
        ),
    }
    run = lambda names: offenders(names, files.get)  # noqa: E731

    chk("an unregistered configure-time `codegen` FAILS",
        run(["bare.cmake"]) == [("bare.cmake", ["codegen"])])
    chk("registering it passes", run(["registered.cmake"]) == [])
    chk("`codegen-system` counts too",
        run(["system.cmake"]) == [("system.cmake", ["codegen-system"])])
    chk("`codegen-system` is not reported as `codegen`",
        run(["system.cmake"])[0][1] == ["codegen-system"])
    chk("`codegen resolve-deps` is not an emitter", run(["resolve.cmake"]) == [])
    chk("a BUILD-time add_custom_command is out of scope (it carries DEPENDS)",
        run(["custom.cmake"]) == [])
    chk("a commented-out emitter is not a finding", run(["commented.cmake"]) == [])
    chk("a variable NAMED for codegen is not a verb", run(["varonly.cmake"]) == [])
    chk("an env-prefixed, multi-line COMMAND is still seen",
        run(["nested.cmake"]) == [])
    chk("a resolve-deps sibling does not excuse a real emitter",
        run(["both.cmake"]) == [("both.cmake", ["codegen"])])
    # Mutation: the gate must not pass merely because the helper NAME appears.
    chk("the helper name in a comment does not satisfy the rule",
        offenders(["m.cmake"], {"m.cmake": f"# {HELPER}\n" + emit}.get)
        == [("m.cmake", ["codegen"])])
    chk("a command built into a VARIABLE is still an invocation",
        run(["cmdvar.cmake"]) == [("cmdvar.cmake", ["codegen"])])
    chk("arguments built into a variable are too",
        run(["argsvar.cmake"]) == [("argsvar.cmake", ["codegen"])])
    chk("a verb named inside a quoted message is prose, not an invocation",
        run(["prose.cmake"]) == [])
    chk("the whole tracked set is enumerable", len(tracked_cmake()) > 0)

    if verbose:
        print(f"\n{ok} passed, {fail} failed")
    if fail:
        print("check-codegen-tool-reconfigure self-test: FAILED", file=sys.stderr)
        raise SystemExit(1)
    return 0


if __name__ == "__main__":
    sys.exit(main())
