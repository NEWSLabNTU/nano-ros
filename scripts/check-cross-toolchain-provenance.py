#!/usr/bin/env python3
"""A cross build must SAY which compiler it picked and where it came from.

Issue 1117. `nros-sdk-index.toml` pins `[tool.arm-none-eabi-gcc]` at ARM's
13.2.rel1. On a host whose SDK store had never been provisioned, `nros build
freertos` compiled with Ubuntu's `/usr/bin/arm-none-eabi-g++` 10.3.1 — three
major versions below the pin — and printed nothing about the substitution. The
resulting compile error was diagnosed twice as a nano-ros codegen bug
(docs/issues/archived/1113-*.md, retracted) and nearly produced a change to the
entry emitter to accommodate a compiler this project does not ship.

The fallback itself is DELIBERATE — activate.sh says so in as many words: "A
system cross-gcc (e.g. /usr/bin/arm-none-eabi-gcc) still resolves when the store
has none." A contributor with a working distro toolchain should not need a
150 MB download. So the rule is not "refuse the fallback"; it is the issue-0500
remedy one layer down, where `nano-ros: Corrosion <ver> via <origin>` made the
choice legible: RECORD IT.

WHAT IT CHECKS

For every toolchain file under `cmake/toolchain/`:

  R1  a file that sets CMAKE_C_COMPILER / CMAKE_CXX_COMPILER includes
      NanoRosCrossToolchain.cmake and calls BOTH resolve() and report()
  R2  the compiler is set from a ${variable}, not a literal `<prefix>-gcc`
  R3  the TOOL named is a real `[tool.<name>]` in nros-sdk-index.toml
  R4  resolve() and report() in one file agree — same TOOL, same OVERRIDE_VAR,
      and report()'s PREFIX/ORIGIN read the variables resolve() wrote

  R5  (issue 1936) the module RECORDS a below-floor compiler for the C++
      refusal: run under `cmake -P` against a stub gcc, a 10.x sets the
      `_NROS_CT_BELOW_FLOOR` global property — one string naming
      `std::this_thread` and `nros setup --tool <tool>` — and a 13.x sets none
  R6  (issue 1936) `nano_ros_entry` turns that record into a FATAL_ERROR for a
      C++ entry: the read sits inside `if(_NRA_LANG STREQUAL "cpp")`

R4 is the one that is not obvious, and it is the reason this gate is worth
having rather than a grep for the helper name. A file can call both helpers,
pass every argument, look entirely correct — and report a DIFFERENT tool than
the one it configures, or read a stale variable some earlier version wrote. That
mutant is shape-valid: cmake accepts it, the configure prints a provenance line,
and the line is about the wrong compiler. A gate that only asks "is the helper
called" gives that mutant a pass, which is the same failure the rmw parity map
had — an authored claim agreeing with nothing.

R3 is the same idea pointed outward: the TOOL string is a name in another file,
so it can drift when the index is renamed. Checking it against the index means
the map cannot claim a territory that does not exist.

SIBLINGS OUTSIDE cmake/toolchain/, and why they are a listed gap

Four more places in this tree select a cross compiler and are NOT under
cmake/toolchain/. They have the identical defect and are not fixed here (they
belong to other owners); each is listed in EXEMPT with issue 1117 in the reason,
which is the only form of tolerated gap this repo accepts. The list can only
shrink: the gate fails if an exempt path stops existing or stops setting a
compiler, so a file that gets fixed must leave the list rather than sit in it
looking like coverage.

Usage::

    check-cross-toolchain-provenance.py            # the gate (self-tests first)
    check-cross-toolchain-provenance.py --selftest # the negative control alone
"""

import argparse
import os
import re
import subprocess
import sys
import sys as _w3_sys  # noqa: E402
from pathlib import Path as _W3Path  # noqa: E402
_w3_sys.path.insert(0, str(_W3Path(__file__).resolve().parent / "lib"))
import comments  # noqa: E402  phase-472 W3 — the one comment stripper

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
TOOLCHAIN_DIR = "cmake/toolchain"
MODULE = "NanoRosCrossToolchain.cmake"
INDEX = os.path.join(ROOT, "nros-sdk-index.toml")

# Files outside cmake/toolchain/ that select a cross compiler. Every entry
# names issue 1117: an unreported gap is indistinguishable from coverage.
EXEMPT = {
}
# A stand-in entry for the ratchet's own self-test.
SELFTEST_EXEMPT = {"x/tc.cmake": "issue 1117 — self-test stand-in"}

# `CMAKE_C_COMPILER_LAUNCHER` and friends are not compiler selection.
SET_COMPILER = re.compile(
    r"""(?:set|find_program)\s*\(\s*(CMAKE_(?:C|CXX|ASM)_COMPILER)\b(?!_)\s*([^\s\)]*)""")
INCLUDE_MODULE = re.compile(r"include\s*\([^)]*" + re.escape(MODULE))
TOOL_SECTION = re.compile(r"^\[tool\.([^\]]+)\]", re.M)


def strip_comments(text):
    """Drop `#` comments. Nothing in these files quotes a `#`."""
    # phase-472 W3 — the shared stripper (scripts/lib/comments.py).
    return comments.strip_comments(text, "cmake")


# The keyword names each helper takes. A KNOWN set, not "any all-caps token":
# `NROS_ARM_NONE_EABI_PREFIX` and `_P` are also all-caps, and treating them as
# keywords silently split every call into nonsense — a parser that reads the
# compliant file as broken would have made this gate unlandable and, worse,
# could have been "fixed" by loosening the rule instead of the parser.
RESOLVE_KEYS = {"TOOL", "PREFIXES", "OVERRIDE_VAR", "OUT_PREFIX", "OUT_ORIGIN"}
REPORT_KEYS = {"TOOL", "PREFIX", "ORIGIN", "OVERRIDE_VAR"}


def call_args(text, fname, keys):
    """Parsed `KEY value…` pairs of the first `fname(...)` call, or None."""
    m = re.search(re.escape(fname) + r"\s*\(", text)
    if not m:
        return None
    depth, i = 1, m.end()
    while i < len(text) and depth:
        if text[i] == "(":
            depth += 1
        elif text[i] == ")":
            depth -= 1
        i += 1
    args, key = {}, None
    for tok in text[m.end(): i - 1].split():
        if tok in keys:
            key = tok
            args[key] = []
        elif key:
            args[key].append(tok.strip('"'))
    return args


def one(args, key):
    """The single value of `key`, or "" — never an IndexError."""
    vals = args.get(key) or [""]
    return vals[0]


def index_tools(read_index):
    return set(TOOL_SECTION.findall(read_index()))


def offenders(files, read, tools):
    """(path, message) for every rule broken. `files` are repo-relative."""
    bad = []
    for rel in sorted(files):
        raw = read(rel)
        text = strip_comments(raw)
        sets = SET_COMPILER.findall(text)
        if not sets:
            continue

        # R1 — the helper is used at all.
        if not INCLUDE_MODULE.search(text):
            bad.append((rel, f"sets a cross compiler but never includes {MODULE}"))
            continue
        resolve = call_args(text, "nros_cross_toolchain_resolve", RESOLVE_KEYS)
        report = call_args(text, "nros_cross_toolchain_report", REPORT_KEYS)
        if resolve is None or report is None:
            missing = "resolve()" if resolve is None else "report()"
            bad.append((rel, f"includes {MODULE} but never calls "
                             f"nros_cross_toolchain_{missing[:-2]}"))
            continue

        # R2 — the compiler comes from the resolved variable, not a literal.
        for var, value in sets:
            if "${" not in value:
                bad.append((rel, f"{var} is set to the literal `{value}` — a "
                                 f"resolved compiler must come from "
                                 f"${{{one(resolve, 'OUT_PREFIX')}}}"))

        # R3 — the tool name exists in the index.
        tool = one(resolve, "TOOL")
        if tool not in tools:
            bad.append((rel, f"resolve(TOOL {tool}) names no [tool.{tool}] in "
                             f"nros-sdk-index.toml"))

        # R4 — the two calls are wired to each other, not merely both present.
        rep_tool = one(report, "TOOL")
        if rep_tool != tool:
            bad.append((rel, f"report(TOOL {rep_tool}) disagrees with "
                             f"resolve(TOOL {tool}) — the provenance line would "
                             f"name a compiler this file does not configure"))
        r_ov = one(resolve, "OVERRIDE_VAR")
        p_ov = one(report, "OVERRIDE_VAR")
        if r_ov != p_ov:
            bad.append((rel, f"report(OVERRIDE_VAR {p_ov}) disagrees with "
                             f"resolve(OVERRIDE_VAR {r_ov}) — the remedy would "
                             f"name a knob that does nothing"))
        for arg, out in (("PREFIX", "OUT_PREFIX"), ("ORIGIN", "OUT_ORIGIN")):
            want = one(resolve, out)
            got = one(report, arg)
            if got != "${%s}" % want:
                bad.append((rel, f"report({arg} {got}) does not read "
                                 f"resolve({out} {want})"))
    return bad


def exempt_still_needed(read, exists, exempt=None):
    """An exempt path that no longer needs the exemption must leave the list.

    `exempt` defaults to EXEMPT. The self-test passes its own table, so the
    ratchet stays tested once the real list is empty (it is, since issue 1117's
    four siblings were fixed or deleted).
    """
    bad = []
    for rel, why in sorted((EXEMPT if exempt is None else exempt).items()):
        if not exists(rel):
            bad.append((rel, "exempt path no longer exists — delete the EXEMPT entry"))
            continue
        if not SET_COMPILER.search(strip_comments(read(rel))):
            bad.append((rel, "exempt path no longer selects a cross compiler — "
                             "delete the EXEMPT entry"))
        if "1117" not in why:
            bad.append((rel, "exemption reason names no tracked issue id"))
    return bad


def tracked_toolchain_files():
    """Every CMake file that SELECTS a cross compiler, plus `cmake/toolchain/`.

    issue 1614 (W5): the population was the directory, so a board overlay that
    sets `CMAKE_C_COMPILER` itself (`/usr/bin/arm-none-eabi-gcc`) sat outside
    the rule it breaks. A file selecting a compiler anywhere is in, by KIND
    (`scripts/lib/file_kinds.py`); the listed gaps stay in EXEMPT.
    """
    import file_kinds

    out = []
    for p in file_kinds.files_of_kind("cmake", repo=ROOT):
        if os.path.basename(p) == MODULE or p in EXEMPT:
            continue
        if p.startswith(TOOLCHAIN_DIR + "/") and p.endswith(".cmake"):
            out.append(p)
            continue
        try:
            with open(os.path.join(ROOT, p), encoding="utf8", errors="replace") as fh:
                if SET_COMPILER.search(strip_comments(fh.read())):
                    out.append(p)
        except OSError:
            continue
    return out


ENTRY = "cmake/NanoRosEntry.cmake"
FLOOR_PROPERTY = "_NROS_CT_BELOW_FLOOR"


def probe_floor_record(version, workdir):
    """R5 — what the module records for a cross gcc reporting `version`.

    Behavioural, not a grep: the report function runs under `cmake -P` with a
    stub compiler, so a refusal text that is built but never stored, or stored
    as a cmake LIST (one `;` per argument — the first draft did exactly that),
    is caught here. Returns the property's value, or None if cmake is absent.
    """
    import shutil
    import tempfile
    if not shutil.which("cmake"):
        return None
    with tempfile.TemporaryDirectory(dir=workdir) as tmp:
        stub = os.path.join(tmp, "stub-gcc")
        with open(stub, "w") as fh:
            fh.write(f"#!/bin/sh\necho {version}\n")
        os.chmod(stub, 0o755)
        script = os.path.join(tmp, "probe.cmake")
        with open(script, "w") as fh:
            fh.write(
                f'set(CMAKE_BINARY_DIR "{tmp}")\n'
                f'include("{ROOT}/{TOOLCHAIN_DIR}/{MODULE}")\n'
                f'nros_cross_toolchain_report(TOOL arm-none-eabi-gcc PREFIX "{tmp}/stub" '
                f'ORIGIN "override NROS_ARM_NONE_EABI_PREFIX" '
                f'OVERRIDE_VAR NROS_ARM_NONE_EABI_PREFIX)\n'
                f'get_property(_p GLOBAL PROPERTY {FLOOR_PROPERTY})\n'
                f'message(STATUS "@@FLOOR@@${{_p}}@@END@@")\n')
        out = subprocess.run(["cmake", "-P", script], capture_output=True, text=True,
                             cwd=tmp, check=False)
        m = re.search(r"@@FLOOR@@(.*?)@@END@@", out.stdout + out.stderr, re.S)
        if not m:
            return f"<probe printed no record; rc {out.returncode}: {out.stderr[-400:]}>"
        return m.group(1)


def floor_record_offenders(probe):
    """R5 over the two sides of the floor."""
    bad = []
    below = probe("10.3.1")
    if below is None:
        return [(MODULE, "R5 not measured: no `cmake` on PATH")]
    if "this_thread" not in below or "nros setup --tool arm-none-eabi-gcc" not in below:
        bad.append((MODULE, f"R5 a GCC 10.3.1 cross compiler records no C++ refusal naming "
                            f"`std::this_thread` and `nros setup --tool` (got {below!r})"))
    if ";" in below:
        bad.append((MODULE, "R5 the recorded refusal is a cmake LIST (contains `;`): "
                            "build it with string(CONCAT) before set_property"))
    at_pin = probe("13.2.1")
    if at_pin:
        bad.append((MODULE, f"R5 a GCC 13.2.1 cross compiler records a refusal: {at_pin!r}"))
    return bad


def entry_refusal_offenders(entry_text):
    """R6 — the entry refuses a C++ image on the record (comments stripped)."""
    text = strip_comments(entry_text)
    m = re.search(r'if\(\s*_NRA_LANG\s+STREQUAL\s+"cpp"\s*\)(.*?)\n\s*endif\(\)\s*\n\s*endif\(\)',
                  text, re.S)
    if not m or FLOOR_PROPERTY not in m.group(1) or "FATAL_ERROR" not in m.group(1):
        return [(ENTRY, f"R6 nano_ros_entry does not refuse a C++ entry on "
                        f"`{FLOOR_PROPERTY}` (a `get_property(... GLOBAL PROPERTY "
                        f"{FLOOR_PROPERTY})` + FATAL_ERROR inside "
                        f"`if(_NRA_LANG STREQUAL \"cpp\")`) — issue 1936")]
    return []


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--selftest", action="store_true")
    args = ap.parse_args()
    if args.selftest:
        return selftest(verbose=True)
    # On the NORMAL path, every time: a negative control nobody runs decays
    # into a comment (AGENTS.md, "a gate must run its own selftest").
    if selftest():
        return 1

    def read(rel):
        with open(os.path.join(ROOT, rel), encoding="utf8", errors="replace") as fh:
            return fh.read()

    with open(INDEX, encoding="utf8") as fh:
        index_text = fh.read()
    tools = index_tools(lambda: index_text)
    if not tools:
        print("check-cross-toolchain-provenance: parsed ZERO [tool.*] from "
              "nros-sdk-index.toml — the R3 check would be vacuous", file=sys.stderr)
        return 1

    files = tracked_toolchain_files()
    if not files:
        print(f"check-cross-toolchain-provenance: found no toolchain files under "
              f"{TOOLCHAIN_DIR}/ — this gate would examine nothing", file=sys.stderr)
        return 1

    bad = offenders(files, read, tools) + exempt_still_needed(
        read, lambda rel: os.path.exists(os.path.join(ROOT, rel)))
    bad += floor_record_offenders(lambda v: probe_floor_record(v, None))
    bad += entry_refusal_offenders(read(ENTRY))
    if bad:
        print("check-cross-toolchain-provenance: FAILED", file=sys.stderr)
        for rel, msg in bad:
            print(f"  {rel}: {msg}", file=sys.stderr)
        print(
            "\nA cross build that does not name its compiler is issue 1117: the pin in\n"
            "nros-sdk-index.toml said 13.2.rel1, the build used a distro 10.3.1, and the\n"
            "failure read as a codegen bug for two diagnoses. The fallback is fine; the\n"
            "silence is not. Wire the file through cmake/toolchain/" + MODULE + ":\n"
            "    include(\"${CMAKE_CURRENT_LIST_DIR}/" + MODULE + "\")\n"
            "    nros_cross_toolchain_resolve(TOOL <index tool> PREFIXES <p>\n"
            "        OVERRIDE_VAR <NROS_..._PREFIX> OUT_PREFIX _P OUT_ORIGIN _O)\n"
            "    nros_cross_toolchain_report(TOOL <index tool> PREFIX \"${_P}\"\n"
            "        ORIGIN \"${_O}\" OVERRIDE_VAR <NROS_..._PREFIX>)\n"
            "    set(CMAKE_C_COMPILER ${_P}-gcc)   # etc\n",
            file=sys.stderr)
        return 1
    print(f"check-cross-toolchain-provenance: OK ({len(files)} toolchain file(s), "
          f"{len(EXEMPT)} listed gap(s))")
    return 0


# ---------------------------------------------------------------------------
GOOD = """
set(CMAKE_SYSTEM_NAME Generic)
include("${CMAKE_CURRENT_LIST_DIR}/NanoRosCrossToolchain.cmake")
nros_cross_toolchain_resolve(
    TOOL         arm-none-eabi-gcc
    PREFIXES     arm-none-eabi
    OVERRIDE_VAR NROS_ARM_NONE_EABI_PREFIX
    OUT_PREFIX   _P
    OUT_ORIGIN   _O)
nros_cross_toolchain_report(
    TOOL   arm-none-eabi-gcc
    PREFIX "${_P}"
    ORIGIN "${_O}"
    OVERRIDE_VAR NROS_ARM_NONE_EABI_PREFIX)
set(CMAKE_C_COMPILER   ${_P}-gcc)
set(CMAKE_CXX_COMPILER ${_P}-g++)
"""

TOOLS = {"arm-none-eabi-gcc", "riscv-none-elf-gcc"}


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

    def run(body):
        return offenders(["t.cmake"], lambda _rel: body, TOOLS)

    chk("the compliant shape passes", run(GOOD) == [])

    # A file that selects no compiler is not this gate's business.
    chk("a non-toolchain cmake file is ignored",
        run("include(\"x.cmake\")\nadd_library(a b.c)\n") == [])

    # R1
    chk("R1 no module include",
        any("never includes" in m for _, m in run(
            "set(CMAKE_C_COMPILER arm-none-eabi-gcc)\n")))
    chk("R1 include but no report()",
        any("never calls nros_cross_toolchain_report" in m for _, m in run(
            GOOD.replace("nros_cross_toolchain_report", "nros_unrelated_call"))))

    # R2 — the wire-crossing that keeps the helper call and hardcodes anyway.
    chk("R2 helper called AND compiler hardcoded",
        any("literal `arm-none-eabi-gcc`" in m for _, m in run(
            GOOD.replace("set(CMAKE_C_COMPILER   ${_P}-gcc)",
                         "set(CMAKE_C_COMPILER   arm-none-eabi-gcc)"))))

    # R3
    chk("R3 tool absent from the index",
        any("names no [tool." in m for _, m in run(
            GOOD.replace("TOOL         arm-none-eabi-gcc",
                         "TOOL         arm-none-eabi-gcc-13"))))

    # R4 — every mutant below keeps the SHAPE valid: cmake accepts it, a
    # provenance line still prints, and it is about the wrong thing.
    chk("R4 report names a different TOOL than resolve",
        any("disagrees with resolve(TOOL" in m for _, m in run(
            GOOD.replace("    TOOL   arm-none-eabi-gcc",
                         "    TOOL   riscv-none-elf-gcc"))))
    chk("R4 report names a different OVERRIDE_VAR",
        any("name a knob that does nothing" in m for _, m in run(
            GOOD.replace("    OVERRIDE_VAR NROS_ARM_NONE_EABI_PREFIX)",
                         "    OVERRIDE_VAR NROS_RISCV64_PREFIX)"))))
    chk("R4 report reads a variable resolve never wrote",
        any("does not read resolve(OUT_PREFIX" in m for _, m in run(
            GOOD.replace('PREFIX "${_P}"', 'PREFIX "${_STALE}"'))))
    chk("R4 report's ORIGIN reads the PREFIX variable",
        any("does not read resolve(OUT_ORIGIN" in m for _, m in run(
            GOOD.replace('ORIGIN "${_O}"', 'ORIGIN "${_P}"'))))

    # R6 — the wiring, both ways.
    entry_ok = ('if(_NRA_LANG STREQUAL "cpp")\n'
                '    get_property(_b GLOBAL PROPERTY _NROS_CT_BELOW_FLOOR)\n'
                '    if(NOT "${_b}" STREQUAL "")\n'
                '        message(FATAL_ERROR "x")\n'
                '    endif()\n'
                'endif()\n')
    chk("R6 the C++ refusal passes", entry_refusal_offenders(entry_ok) == [])
    chk("R6 a WARNING instead of a refusal is reported",
        entry_refusal_offenders(entry_ok.replace("FATAL_ERROR", "WARNING")) != [])
    chk("R6 a refusal on the C entry instead is reported",
        entry_refusal_offenders(entry_ok.replace('"cpp"', '"c"')) != [])
    chk("R6 a commented-out read is reported",
        entry_refusal_offenders(entry_ok.replace("    get_property", "    # get_property"))
        != [])

    # R5's judge, over canned probe answers (the probe itself runs on the
    # normal path).
    good_rec = "x 10.3.1 std::this_thread ... nros setup --tool arm-none-eabi-gcc"
    chk("R5 a correct record passes",
        floor_record_offenders(lambda v: good_rec if v.startswith("10") else "") == [])
    chk("R5 no record below the floor is reported",
        floor_record_offenders(lambda v: "") != [])
    chk("R5 a LIST-valued record is reported",
        floor_record_offenders(lambda v: good_rec.replace(" ...", ";") if v.startswith("10")
                               else "") != [])
    chk("R5 a record AT the pin is reported",
        floor_record_offenders(lambda v: good_rec) != [])

    # The EXEMPT ratchet: an entry that stops being needed must be removed.
    chk("exempt path that vanished is reported",
        any("no longer exists" in m for _, m in exempt_still_needed(
            lambda _r: "", lambda _r: False, SELFTEST_EXEMPT)))
    chk("exempt path that stopped selecting a compiler is reported",
        any("no longer selects" in m for _, m in exempt_still_needed(
            lambda _r: "# nothing here\n", lambda _r: True, SELFTEST_EXEMPT)))
    chk("exempt reason naming no issue is reported",
        any("names no tracked issue" in m for _, m in exempt_still_needed(
            lambda _r: "set(CMAKE_C_COMPILER gcc)\n", lambda _r: True,
            {"x/tc.cmake": "board-owned"})))

    if verbose:
        print(f"check-cross-toolchain-provenance --selftest: {ok} ok, {fail} failed")
    if fail:
        print(f"check-cross-toolchain-provenance: SELFTEST FAILED ({fail})",
              file=sys.stderr)
    return 1 if fail else 0


if __name__ == "__main__":
    sys.exit(main())
