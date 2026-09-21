#!/usr/bin/env python3
"""A capability probe may gate a METHOD. It may never change `sizeof`.

phase-417, widened by issue 1225. This MEASURES the rule rather than grepping
for it: it compiles a probe TU in several configurations and compares the
reported `sizeof`. A text scanner for "member inside an `#if`" was written first
and thrown away -- it could not tell a member from a local variable, and the
file-level include guard made every line look conditional (the same
false-negative shape that made an earlier scanner in this tree report zero).

WHY IT MATTERS. Two TUs of one image disagreeing about a capability is a
SUPPORTED state, not a misconfiguration:

  * `examples/px4/cpp/bridge/src/modules/nros_uorb_bridge/CMakeLists.txt:123`
    sets `-DNROS_CPP_STD=1` on ONE module of a larger image, deliberately.
  * `zephyr/cmake/nros_rmw_cyclonedds.cmake` adds the `cxx-compat` include dir
    for some targets only, so `__has_include` can answer differently for two TUs
    of one build.

It already shipped once. `rclcpp::Node` held
`std::vector<std::shared_ptr<detail::WallTimer>> timers_` behind
`NROS_CPP_HAS_STD_CHRONO`, reachable only through `NROS_CPP_STD`, so the px4
bridge module compiled a 3776-byte node while every other TU compiled a
3752-byte one. They linked. Each wrote the object through its own layout.

The probes are themselves unreliable -- three distinct failure modes measured in
one day: `<type_traits>` present-but-hollow on Zephyr, `NROS_CPP_STD` set by
nothing that ships, `<memory>` present-then-`#error` under `-ffreestanding` on
GCC 13. So this gate does not ask whether a probe is right. It removes the class
of bug where being wrong changes a layout.

--- WHAT THE FIRST VERSION GOT WRONG (issue 1204) ----------------------------

It had exactly ONE measurement arm: compile hosted, force each capability macro
ON with `-D`, compare against the hosted baseline. But the headers self-define
every `NROS_CPP_HAS_*` macro under `__has_include(<memory>)` and friends, and on
a hosted compiler those always succeed. Forcing an already-on macro on is a
strict no-op, so the gate compared the baseline against itself seven times per
type and reported OK. Issue 1204 proved it by mutation: an
`#ifdef NROS_CPP_HAS_SHARED_PTR`-gated `double` member added to a real type left
the gate green at exit 0. Hence the arms below where the macros are genuinely
off -- the `-nostdinc++` freestanding one, and since phase-438 W4 a hosted one
with `-DNROS_CPP_STD` withheld -- and hence the real-header mutation in
`selftest`, because a synthetic negative control that cannot fail on the real
subject is not a control over it.

--- WHAT PHASE-438 W4 ADDED -------------------------------------------------

W2 made the std surface a REQUEST, so "hosted, opt-in withheld" became a
configuration that exists. It is the one every hosted consumer that has not
opted in now compiles in, and it isolates the FLAG from the toolchain: the
freestanding arm varies `-std`, `-nostdinc++` and the shim all at once, so on
its own it cannot tell "the porting surface moved a layout" from "the two
libc++ shims disagree about a member". W4's acceptance is that
`sizeof(rclcpp::Node)` does not move between it and the baseline; measured 200
in all three arms (baseline, no-std, freestanding), over 90 derived subjects
rather than the five the arm was authored against.

--- WHAT THE SECOND VERSION GOT WRONG (issue 1225) ---------------------------

Its subject list was THREE AUTHORED NAMES, later five:

    TYPES=("rclcpp::Node" "::nros::Node" "::nros::QoS")

chosen for the `timers_` defect it was built to catch. Every other public type
had been unmeasured since, and a sweep during phase-427 found three that follow
a probe -- `nros::Timer` 24 -> 32, `nros::GuardCondition` 32 -> 40,
`nros::ComponentNode` 55784 -> 55848, all one `#ifdef NROS_CPP_STD`
`std::unique_ptr<std::function<void()>> closure_`. A gate whose coverage is
narrower than the rule it enforces is issue 0196's shape, and this is the
authored-list-drifts class `CLAUDE.md` records for the RMW parity map, whose
authored table read "no vtable slot" for 28 slots that had moved.

The list is now DERIVED. `scripts/check/cpp_capability_subjects.py` asks clang
what the umbrella header exports in our four namespaces and returns every
spelling that can appear inside a `sizeof` -- 90 of them today against the 5
that were authored, class templates included at a derived instantiation. Adding
a public type adds a subject; nobody has to remember.

--- HOW THE SIZE IS READ, AND WHY IT IS ONE TU PER CONFIGURATION -------------

`-fsyntax-only` plus an intentionally incomplete template: `nros_size_7<sizeof
(T)> v7;` makes the compiler print the number in its own diagnostic. No link, no
run, no library.

The template is named PER SUBJECT rather than a shared `ShowSize`, which is what
lets all 90 subjects share ONE compile: the index is in the diagnostic next to
the size, on both g++ and clang++, so one TU per configuration answers for every
subject at once. Ten compiles, not 900, and the ten run concurrently.

MEASURED 2026-09-09 on a 24-core host: 9.4 s wall for the whole gate over 90
subjects, against 65 s for the five-type shell version. The split is ~6.8 s of
subject derivation (clang emits a 333 MB JSON AST for the umbrella and Python
parses it) and ~2.5 s of everything else. In the `check-fast` fan-out at -P80 it
falls outside the ten slowest gates; the lane's wall is set by `check-api-parity`
at 64 s. So the wider gate is CHEAPER than the narrow one it replaces, and does
not become the fast lane's floor.

A subject missing from an arm's output is re-measured ALONE before any verdict.
A cascading parse error could in principle suppress a later diagnostic, and
"absent" is a verdict this gate acts on -- so it is confirmed rather than
inferred from a batch.

--- WHY NO BUILD ------------------------------------------------------------

The previous version compiled against `-Itarget/nros-{c,cpp}-generated`, which a
pristine tree does not have. Its probe TU therefore had 149 errors -- and
printed a `sizeof` anyway, because GCC keeps going past `#error`. It was
measuring numbers off a TU that does not compile.

`-DNROS_PLATFORM_NUTTX` selects the COMMITTED sizes header instead, the same
choice `scripts/api_parity/extract_cxx.py` makes for the same reason. The gate
needs a sizes header CONSISTENT between its arms, not a host-shaped one, so a
committed one is strictly better -- and the subject derivation refuses to run at
all on a parse error, so the "149 errors and a number anyway" state can no
longer exist.

--- THE BASELINE IS GONE; THIS GATE ASSERTS A CONSTANT (phase-456 W6) --------

`.config/cpp-capability-layout-baseline.txt` WAS a ratchet, in the shape
`.config/cpp-freestanding-includes-baseline.txt` used one directory over. It
could hold three kinds of row:

  diverges <subject>     a known size violator (issue 1225).
  hosted-only <subject>  the type does not EXIST in the freestanding arm.
  std-only <subject>     the type does not EXIST hosted without -DNROS_CPP_STD.

It now holds NONE, and may hold none: a row of any kind is a hard failure
naming this paragraph. The file stays as the record of what the ratchet
measured and why, which is the one thing deleting it would throw away.

WHY A CONSTANT RATHER THAN A RATCHET, and it is not tidiness. A ratchet is the
right instrument for debt you intend to pay: it stops the number growing while
you pay it. Every row is paid -- W1 removed the `diverges` rows in the commits
that fixed the members, and phase-442/456 removed the `hosted-only` and
`std-only` rows with the gates that recorded them -- so what the slot holds now
is not tolerance for measured debt but a place to put the NEXT violation. The
rule this gate enforces has no legitimate exception: a public type's LAYOUT may
not depend on a capability macro, because two TUs of one image may legitimately
disagree about one (px4 sets `-DNROS_CPP_STD` on a single module) and would
then link, write the object through one layout and read it through the other.
Issue 0135 is that bug, shipped. A rule with no exception should not offer a
slot for one.

WHAT IS STILL PERMITTED, so the constant is not read as more than it says: a
capability macro may gate a METHOD, and does -- `Rate`'s `std::chrono`
constructor, the whole `NROS_CPP_NODE_HOSTED` block on `rclcpp::Node`. Adding
a method changes no layout, selftest case 2 proves the gate lets it through,
and nothing here asks for those to go away. What is refused is a capability
macro reaching a MEMBER, a base class, or the existence of a public type.

If a genuine exception ever appears, the fix is to change this gate with the
reason in the commit, not to append a line nobody reviews.

Usage::

    check-cpp-capability-layout.py           # the gate
    check-cpp-capability-layout.py --report  # every subject and size, no verdict
"""

import itertools
import os
import re
import shutil
import subprocess
import sys
import tempfile
from concurrent import futures

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, "scripts", "check"))

import cpp_capability_subjects as subjects_mod  # noqa: E402

BASELINE = os.path.join(ROOT, ".config", "cpp-capability-layout-baseline.txt")
KINDS = ("diverges", "hosted-only", "std-only")

# Every capability macro the public headers define for themselves, plus the
# consumer-facing opt-in. Forcing one ON is exactly what px4 does.
#
# phase-426 W4 added `NROS_SYSTEM_PARAM_SERVICES`, the one macro on this list
# that no header defines for itself: `NanoRosCapabilities.cmake` sets it, per
# DIRECTORY (`add_compile_definitions`), when the bringup declares the
# `param_services` capability. So it is exactly the shape the rest of this list
# exists for — a macro two TUs of one image can legitimately disagree about —
# and it had never been measured against a layout, while gating the branch that
# decides whether a node has a parameter store at all. It became worth
# measuring when the C++ parameter MEMBERS went away and the facade started
# depending on the store's existence.
CAPS = (
    "NROS_CPP_STD",
    "NROS_CPP_HAS_SHARED_PTR",
    "NROS_CPP_HAS_STD_STRING",
    "NROS_CPP_HAS_STD_VECTOR",
    "NROS_CPP_HAS_STD_FUNCTION",
    "NROS_CPP_HAS_STD_CHRONO",
    "NROS_CPP_HAS_STD_SSTREAM",
    "NROS_SYSTEM_PARAM_SERVICES",
)

# Hosted is c++17 because the umbrella's `if constexpr` needs it; the
# freestanding flags are copied verbatim from the `cpp` lane's `-nostdinc++`
# header-parse arm so the two agree by construction.
#
# `-DNROS_CPP_STD` is on the hosted arm since phase-438 W2, and it is not a new
# configuration — it is the SAME one, asked for out loud. Before W2 a hosted
# compiler got the std surface because `__has_include` found the headers, so
# this arm measured the std-flavoured types without naming them. W2 deleted
# discovery; without the flag the probe TU no longer compiles and the gate
# reports "could not measure sizeof(rclcpp::Node)" instead of a size.
#
# Note what this arm is and is not. Forcing an individual `NROS_CPP_HAS_*` ON
# here is close to a no-op, because the baseline already has them all — that is
# issue 1204's finding, and the answer to it is the FREESTANDING arm below,
# which is where the macros are genuinely off.
#
# `HOSTED_NO_STD_FLAGS` is the THIRD arm, added by phase-438 W4 and not
# measurable before W2: the SAME compiler and the SAME language standard as the
# baseline, with the opt-in withheld. It is the shape a hosted consumer gets by
# default from here on, and it is the arm that isolates the FLAG from the
# toolchain — the freestanding arm changes three things at once (`-std`,
# `-nostdinc++`, the shim), so on its own it cannot tell "the porting surface
# moved the layout" from "the two libc++ shims disagree about a member's size".
HOSTED_FLAGS = ["-std=c++17", "-DNROS_CPP_STD=1"]
HOSTED_NO_STD_FLAGS = ["-std=c++17"]
THREADX_SHIM = "packages/boards/nros-board-threadx-qemu-riscv64/cxx-compat"
FREESTANDING_FLAGS = [
    "-std=c++14",
    "-ffreestanding",
    "-nostdinc++",
    "-isystem",
    os.path.join(ROOT, THREADX_SHIM),
]

PROBE = re.compile(r"nros_size_(\d+)<(\d+)")


def compile_flags():
    return ["-fsyntax-only", "-fno-exceptions", "-fno-rtti"]


def probe_source(subjects):
    """One TU declaring an incomplete template per subject and a variable of it."""
    lines = ['#include <nros/nros.hpp>']
    for i in range(len(subjects)):
        lines.append(f"template <int N> struct nros_size_{i};")
    for i, ty in enumerate(subjects):
        lines.append(f"nros_size_{i}<static_cast<int>(sizeof({ty}))> nros_probe_{i};")
    return "\n".join(lines) + "\n"


_ARM_SEQ = itertools.count()


def measure(subjects, flags, include_args, workdir):
    """{subject: size} for the subjects this configuration can name.

    The TU is named per call. The arms run CONCURRENTLY -- nine compiles that
    share nothing -- and a shared filename would have them overwrite each
    other's source, which is a race that shows up as a wrong number rather than
    as an error.
    """
    src = os.path.join(workdir, f"nros_capability_layout_probe_{next(_ARM_SEQ)}.cpp")
    with open(src, "w", encoding="utf8") as fh:
        fh.write(probe_source(subjects))
    proc = subprocess.run(
        ["c++"] + compile_flags() + list(flags) + list(include_args) + [src],
        capture_output=True,
        text=True,
    )
    out = {}
    for idx, size in PROBE.findall(proc.stdout + proc.stderr):
        i = int(idx)
        if i < len(subjects):
            out.setdefault(subjects[i], int(size))
    return out


def measure_one(subject, flags, include_args, workdir):
    """Re-measure a single subject, so `absent` is confirmed and not inferred.

    A cascading parse error could suppress a later diagnostic in the batch, and
    absence is a verdict this gate acts on. Cheap: it runs only for the handful
    of subjects a batch could not answer for.
    """
    return measure([subject], flags, include_args, workdir).get(subject)


# --------------------------------------------------------------------------
# the baseline
# --------------------------------------------------------------------------


def parse_baseline(text):
    """[problems] -- the file must contain no rows at all (phase-456 W6).

    Kept as a parser rather than a `grep -c`, because the two failures read
    very differently and the reader needs to be told which one happened: a row
    that is not even in the old `<kind> <subject>` shape is a typo, and a
    well-formed row is someone reaching for an escape hatch that is closed.
    """
    problems = []
    for lineno, raw in enumerate(text.split("\n"), 1):
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        parts = line.split()
        shape = (
            f"a `{parts[0]}` row"
            if len(parts) == 2 and parts[0] in KINDS
            else "a row that is not even in the retired `<kind> <subject>` shape"
        )
        problems.append(
            f"  baseline:{lineno}: {shape} -- `{line}`\n"
            "      This file holds no rows. phase-456 W6 turned the ratchet into a\n"
            "      CONSTANT: a public type's layout may not depend on a capability\n"
            "      macro, and that rule has no legitimate exception, so there is no\n"
            "      slot to put one in. Gating a METHOD is still fine and needs no\n"
            "      row. If you believe this is the exception, change the gate with\n"
            "      the reason in the commit rather than appending here."
        )
    return problems


def load_baseline():
    if not os.path.exists(BASELINE):
        raise SystemExit(
            f"check-cpp-capability-layout: baseline missing at {BASELINE}.\n"
            "  It is tracked, so its absence is a PATH bug. The file must EXIST and\n"
            "  be empty of rows; an absent file and an empty one are different\n"
            "  states and only one of them is checked."
        )
    with open(BASELINE, encoding="utf8") as fh:
        return parse_baseline(fh.read())


# --------------------------------------------------------------------------
# the comparison
#
# Split out from the measurement so the selftest can drive it with synthetic
# readings and prove BOTH ratchet directions without waiting on a compiler --
# in particular the one that only appears once a violator is fixed, which no
# measurement of today's tree can produce.
# --------------------------------------------------------------------------


def compare(subjects, base, forced, nostd, freestanding):
    """[errors] -- every arm, with no excuses available (phase-456 W6).

    `base` is {subject: size}; `forced` is {cap: {subject: size}}; `nostd` and
    `freestanding` are {subject: size or None}, None meaning confirmed absent.

    This used to take a `baseline` set and skip a finding that appeared in it,
    returning the entries the readings justified so the caller could catch
    stale ones. Both arms are gone with the slot they served: every divergence
    and every absence is a failure now, so there is nothing to excuse and
    nothing to go stale. Still a function rather than an inline loop, because
    the selftest drives it with synthetic readings to prove each arm says yes.
    """
    errors = []

    for ty in subjects:
        if ty not in base:
            errors.append(
                f"could not measure sizeof({ty}) in the baseline configuration.\n"
                f"      The probe TU did not compile for it; this gate would\n"
                f"      otherwise pass on absence. Fix the header or the\n"
                f"      derivation -- do not delete the subject."
            )
            continue

        # ARM 1 -- force each capability macro ON against the hosted baseline.
        # This is what px4 does to one module of an image. On a hosted compiler
        # the headers have already self-defined most of these, so most of the
        # arm is a no-op; that is exactly why arm 2 exists. It is kept because
        # it is the only arm that models the `-DNROS_CPP_STD=1` consumer, and
        # because a macro that BREAKS the compile when forced on is itself a
        # finding.
        for cap in CAPS:
            got = forced[cap].get(ty)
            if got is None:
                errors.append(
                    f"{ty} does not compile with -D{cap}=1.\n"
                    f"      px4 sets a capability macro on one module of a real image,\n"
                    f"      so this configuration has to at least build. An earlier\n"
                    f"      version treated it as 'not a layout question' (issue 1204)."
                )
                continue
            if got != base[ty]:
                errors.append(
                    f"sizeof({ty}) changes with -D{cap}=1 -- {base[ty]} vs {got}."
                )

        # ARM 2 -- hosted, WITHOUT the porting-surface opt-in (phase-438 W4).
        #
        # Same compiler, same `-std`, same headers as the baseline; the only
        # difference is that `-DNROS_CPP_STD=1` is withheld. That makes this
        # the arm that answers W4's acceptance question directly -- "does
        # asking for the std surface move a layout" -- with nothing else
        # varying. It is also the configuration EVERY hosted consumer that has
        # not opted in now compiles in, which before W2 did not exist at all:
        # the macros were discovered from the include path, so a hosted TU
        # always had them.
        without = nostd.get(ty)
        if without is None:
            errors.append(
                f"{ty} does not compile hosted WITHOUT -DNROS_CPP_STD.\n"
                f"      Since phase-438 W2 the std surface is a REQUEST, so this is\n"
                f"      the default configuration of every hosted consumer that has\n"
                f"      not opted in. A derived public type is expected to exist\n"
                f"      there, and since phase-456 W6 there is no `std-only` row to\n"
                f"      declare otherwise: a type whose EXISTENCE depends on the\n"
                f"      opt-in is the shape phase-438 W4 exists to remove."
            )
        elif without != base[ty]:
            errors.append(
                f"sizeof({ty}) changes with -DNROS_CPP_STD -- {without} without, "
                f"{base[ty]} with.\n"
                f"      The porting surface must be ADDITIVE METHODS over a fixed\n"
                f"      layout (phase-438 W4), not a second shape of the same class."
            )

        # ARM 3 -- the freestanding configuration, where the macros are
        # genuinely off because the ThreadX shim has no `<memory>` and no
        # `-DNROS_CPP_STD` is given. This is the arm with teeth: a member gated
        # on any of the seven macros is present hosted and absent here, so the
        # two sizes disagree.
        fs = freestanding.get(ty)
        if fs is None:
            errors.append(
                f"{ty} cannot be measured against the ThreadX shim.\n"
                f"      A public type is expected to exist on freestanding targets,\n"
                f"      and since phase-456 W6 there is no `hosted-only` row to\n"
                f"      declare otherwise -- RFC-0096 D1 is one API on every\n"
                f"      platform, so the ABSENCE is the defect."
            )
        elif fs != base[ty]:
            errors.append(
                f"sizeof({ty}) differs hosted vs -nostdinc++ freestanding -- "
                f"{base[ty]} vs {fs}."
            )

    return errors


# --------------------------------------------------------------------------
# the run
# --------------------------------------------------------------------------


def read_all(subjects, include_args, workdir):
    """(base, forced, nostd, freestanding) -- every arm, absences confirmed alone.

    The ten arms are independent compiles of the same subject list, so they
    run concurrently. Serial they are the gate's whole cost; concurrent the
    gate costs its slowest single arm plus the derivation.
    """
    arms = [("base", HOSTED_FLAGS)]
    arms += [(cap, HOSTED_FLAGS + [f"-D{cap}=1"]) for cap in CAPS]
    arms.append(("nostd", HOSTED_NO_STD_FLAGS))
    arms.append(("freestanding", FREESTANDING_FLAGS))

    with futures.ThreadPoolExecutor(max_workers=len(arms)) as pool:
        got = dict(
            zip(
                [name for name, _ in arms],
                pool.map(
                    lambda a: measure(subjects, a[1], include_args, workdir), arms
                ),
            )
        )

    base = got["base"]
    for ty in subjects:
        if ty not in base:
            one = measure_one(ty, HOSTED_FLAGS, include_args, workdir)
            if one is not None:
                base[ty] = one

    forced = {}
    for cap in CAPS:
        arm = got[cap]
        for ty in subjects:
            if ty in base and ty not in arm:
                one = measure_one(ty, HOSTED_FLAGS + [f"-D{cap}=1"], include_args, workdir)
                if one is not None:
                    arm[ty] = one
        forced[cap] = arm

    ns = got["nostd"]
    nostd = {}
    for ty in subjects:
        nostd[ty] = (
            ns[ty]
            if ty in ns
            else measure_one(ty, HOSTED_NO_STD_FLAGS, include_args, workdir)
        )

    fs = got["freestanding"]
    freestanding = {}
    for ty in subjects:
        freestanding[ty] = (
            fs[ty]
            if ty in fs
            else measure_one(ty, FREESTANDING_FLAGS, include_args, workdir)
        )
    return base, forced, nostd, freestanding


def run(include_args, workdir, subjects=None):
    """(errors, subjects, base) for one include configuration.

    Passing `subjects` narrows the run to a chosen few; the selftest does, to
    keep its two extra measurement passes to 2 compiles per arm instead of
    2 x 90, which is the difference between a gate that runs on the fast lane
    and one that doubles its wall clock.
    """
    if subjects is None:
        subjects, _templates, problems = subjects_mod.derive(include_args)
        if problems:
            return (
                ["cannot derive the subject list:\n      " + "\n      ".join(problems)],
                [],
                {},
            )
    base, forced, nostd, freestanding = read_all(subjects, include_args, workdir)
    errors = compare(subjects, base, forced, nostd, freestanding)
    return errors, subjects, base


# --------------------------------------------------------------------------
# NEGATIVE CONTROLS, on the normal path.
#
# The measurement can only be trusted if it is known to FAIL when a layout does
# follow a probe. Cases 1 and 2 pin the two directions of the rule -- a gated
# MEMBER must diverge, a gated METHOD must not -- in a standalone TU with no
# nros headers, which is the cheapest possible statement of what the gate
# believes.
#
# Case 3 is the one issue 1204 said was missing: the synthetic struct passed
# honestly while the real types were being compared against themselves, so the
# control has to reach a real type in a real header. It copies the whole
# `nros-cpp` include tree to a temp dir, injects a capability-gated member into
# `rclcpp::Node`, and runs the SAME `run()` the gate runs. No tracked header is
# touched.
#
# Cases 4-7 were issue 1225's ratchet arms -- a stale entry, an orphaned one,
# a `hosted-only` row excusing a hosted failure. phase-456 W6 deleted the rows
# those arms policed, so the controls that replaced them assert the STRONGER
# statement: each finding fires with no excuse available, and a row in the file
# is itself refused. They drive `compare()` and `parse_baseline()` with
# synthetic input, which is why both are functions rather than inline.
# --------------------------------------------------------------------------

SYNTHETIC = """\
struct Conditional {
    void* always;
#ifdef NROS_SELFTEST_CAP
    double gated_member;
#endif
};
struct Invariant {
    void* always;
#ifdef NROS_SELFTEST_CAP
    void gated_method();
#endif
};
template <int N> struct nros_size_0;
#ifdef NROS_SELFTEST_PICK_INVARIANT
nros_size_0<static_cast<int>(sizeof(Invariant))> probe;
#else
nros_size_0<static_cast<int>(sizeof(Conditional))> probe;
#endif
"""

ANCHOR = "    nros_cpp_node_t handle_;"

# A `std` TYPE in a public signature, which is phase-456 W6's acceptance
# wording, not a `double` standing in for one. The member is guarded on
# `NROS_CPP_STD` rather than on `NROS_CPP_HAS_SHARED_PTR` for a measured
# reason: forcing `-DNROS_CPP_HAS_SHARED_PTR=1` does not make `<memory>`
# appear, since `std_detect.hpp` includes it only under the opt-in -- so
# `std::shared_ptr` would be an undeclared name and the arm would report "does
# not compile" instead of the size divergence this control is about. Under
# `NROS_CPP_STD` the header IS included, so the mutation is exactly the defect:
# a public type whose layout grows when a consumer asks for the porting
# surface, which is what px4 does to one module of an image.
INJECTION = (
    ANCHOR
    + "\n#ifdef NROS_CPP_STD\n"
    + "    ::std::shared_ptr<int> nros_selftest_mutation_member_;\n#endif"
)


def _synthetic_size(workdir, *flags):
    src = os.path.join(workdir, "selftest.cpp")
    with open(src, "w", encoding="utf8") as fh:
        fh.write(SYNTHETIC)
    proc = subprocess.run(
        ["c++", "-fsyntax-only", "-std=c++17", *flags, src],
        capture_output=True,
        text=True,
    )
    found = PROBE.findall(proc.stdout + proc.stderr)
    return int(found[0][1]) if found else None


def selftest():
    fail = []
    with tempfile.TemporaryDirectory() as workdir:
        a = _synthetic_size(workdir)
        b = _synthetic_size(workdir, "-DNROS_SELFTEST_CAP=1")
        c = _synthetic_size(workdir, "-DNROS_SELFTEST_PICK_INVARIANT=1")
        d = _synthetic_size(
            workdir, "-DNROS_SELFTEST_PICK_INVARIANT=1", "-DNROS_SELFTEST_CAP=1"
        )
        if None in (a, b, c, d):
            fail.append("case 1/2: the size probe produced no number at all")
        else:
            if a == b:
                fail.append(
                    f"case 1: a CONDITIONAL MEMBER did not change sizeof ({a} vs {b}) -- "
                    "the measurement cannot see the defect it exists to catch"
                )
            if c != d:
                fail.append(
                    f"case 2: a gated METHOD changed sizeof ({c} vs {d}) -- the gate "
                    "would fail on the shape it is supposed to permit"
                )

        # Case 3 -- mutate a real header in a throwaway copy of the include
        # tree, with a real `std` TYPE in a public signature (phase-456 W6's
        # acceptance wording). The UNMUTATED copy runs FIRST, because it is
        # what tells the mutated run apart from a broken copy: if the gate
        # already fails on a faithful copy, whatever the mutated run reports
        # afterwards proves nothing.
        mut = os.path.join(workdir, "selftest-include")
        shutil.copytree(os.path.join(ROOT, "packages/api/nros-cpp/include"), mut)
        args = ["-I" + mut] + subjects_mod.include_args()
        # The subject is `::rclcpp::Node`, which is where the CLASS is
        # (phase-427 W7 flipped the direction; `::nros::Node` is the alias now).
        # The two are one layout either way, so this narrows the mutation to the
        # DEFINITION rather than to a name that resolves to it.
        before, subjects, _base = run(args, workdir, subjects=["::rclcpp::Node"])
        if before:
            fail.append(
                "case 3: an UNMUTATED copy of the include tree failed the gate while\n"
                "    the tracked tree is what the run below reports, so a mutation of\n"
                f"    it would be measuring the copy. First problem: {before[0]}"
            )
        else:
            node = os.path.join(mut, "nros", "node.hpp")
            with open(node, encoding="utf8") as fh:
                text = fh.read()
            if ANCHOR not in text:
                fail.append(
                    "case 3: the anchor line is gone from node.hpp, so the injection\n"
                    "    could not apply. Re-anchor it; without it the control cannot fail."
                )
            else:
                with open(node, "w", encoding="utf8") as fh:
                    fh.write(text.replace(ANCHOR, INJECTION, 1))
                after, _s, _b = run(args, workdir, subjects=subjects)
                if not after:
                    fail.append(
                        "case 3: a `std::shared_ptr<int>` MEMBER injected into\n"
                        "    ::rclcpp::Node behind NROS_CPP_STD did not fail the gate.\n"
                        "    This is issue 1204 exactly -- the measurement is comparing\n"
                        "    a configuration against itself."
                    )
                elif not any("::rclcpp::Node" in e for e in after):
                    fail.append(
                        "case 3: the gate failed on the injected member but its message\n"
                        f"    does not name the type. Reported: {after[0]}"
                    )

    # Case 4 -- a size divergence under a forced capability macro fails, with
    # NOTHING available to excuse it. This used to need a companion case
    # proving a `diverges` row COULD excuse it and a third proving a stale row
    # failed; phase-456 W6 removed the row, so one statement is the whole rule.
    errs = compare(
        ["::nros::Moves"],
        {"::nros::Moves": 24},
        {cap: {"::nros::Moves": 32 if cap == "NROS_CPP_STD" else 24} for cap in CAPS},
        {"::nros::Moves": 24},
        {"::nros::Moves": 24},
    )
    if not any("24 vs 32" in e for e in errs):
        fail.append("case 4: a size divergence under a forced macro did not fail")

    # Case 5 -- phase-438 W4's arm on its own: a layout that MOVES when the
    # porting surface is requested. Case 3 exercises this on the normal path,
    # but only where the mutation happens to diverge in every arm at once.
    errs = compare(
        ["::nros::Asks"],
        {"::nros::Asks": 32},
        {cap: {"::nros::Asks": 32} for cap in CAPS},
        {"::nros::Asks": 24},
        {"::nros::Asks": 32},
    )
    if not any("-DNROS_CPP_STD" in e and "24 without" in e for e in errs):
        fail.append(
            "case 5: a layout that MOVES when the porting surface is requested did\n"
            "    not fail. That is phase-438 W4's whole acceptance -- the std surface\n"
            "    is additive methods over a fixed layout, not a second shape."
        )

    # Case 6 -- ABSENCE fails in both arms, and this is the one phase-456 W6
    # strengthened rather than kept. A type that vanishes hosted without the
    # opt-in, and a type that vanishes freestanding, each used to be excusable
    # by a row (`std-only`, `hosted-only`). RFC-0096 D1 is one API on every
    # platform, so neither is excusable now.
    errs = compare(
        ["::nros::VanishesHosted"],
        {"::nros::VanishesHosted": 32},
        {cap: {"::nros::VanishesHosted": 32} for cap in CAPS},
        {"::nros::VanishesHosted": None},
        {"::nros::VanishesHosted": 32},
    )
    if not any("WITHOUT -DNROS_CPP_STD" in e for e in errs):
        fail.append(
            "case 6: a subject that does not compile hosted without the opt-in did\n"
            "    not fail. There is no `std-only` row to declare that any more."
        )
    errs = compare(
        ["::nros::VanishesFree"],
        {"::nros::VanishesFree": 32},
        {cap: {"::nros::VanishesFree": 32} for cap in CAPS},
        {"::nros::VanishesFree": 32},
        {"::nros::VanishesFree": None},
    )
    if not any("ThreadX shim" in e for e in errs):
        fail.append(
            "case 6: a subject absent from the freestanding arm did not fail. There\n"
            "    is no `hosted-only` row to declare that any more."
        )

    # Case 7 -- THE CONSTANT ITSELF. A row in the baseline file is refused,
    # whatever shape it is in. Without this, turning the ratchet into a
    # constant would be a claim in a docstring: an appended row would sail
    # through, the gate would still print OK, and the only thing that had moved
    # is the prose. Both spellings, because they get different messages and a
    # reader needs to be told which one happened -- plus the negative control,
    # since a parser that rejected everything would pass the first two and be
    # red on the tracked file.
    for probe, label in (
        ("diverges ::nros::Whatever", "a well-formed retired row"),
        ("this is not even a row", "a malformed row"),
    ):
        if not parse_baseline("# a comment\n\n" + probe + "\n"):
            fail.append(
                f"case 7: {label} in the baseline file was ACCEPTED. The gate is\n"
                "    still a ratchet with a docstring that says otherwise -- the next\n"
                "    capability-dependent layout can be waved through by one line."
            )
    if parse_baseline("# only comments\n\n   \n"):
        fail.append(
            "case 7: a file of comments and blanks was REJECTED. That is the state\n"
            "    the tracked file is in, so the gate would be red on a clean tree."
        )

    if fail:
        print("check-cpp-capability-layout --selftest FAILED:", file=sys.stderr)
        for f in fail:
            print(f"  - {f}", file=sys.stderr)
        raise SystemExit(1)
    print(
        "check-cpp-capability-layout --selftest: 7 case(s) OK (gated member diverges, "
        "gated method does not, a std::shared_ptr member injected into the real "
        "::rclcpp::Node is caught, a forced-macro divergence fails, a layout that "
        "moves with -DNROS_CPP_STD fails, absence fails in both the no-std and the "
        "freestanding arm, and a row in the baseline file is refused)"
    )


def main():
    os.chdir(ROOT)
    if not os.path.isdir(THREADX_SHIM):
        # The shim is the whole point of the freestanding arm, so its absence
        # must be fatal rather than a skip. Same reasoning, same spelling, as
        # the lane that parses every header against it: a gate that passes when
        # its subject is missing is issue 0232's false green.
        print(
            f"check-cpp-capability-layout: {THREADX_SHIM} is MISSING\n"
            "  The freestanding arm is the only configuration where the capability\n"
            "  macros are genuinely off. Without it this gate would pass on absence.",
            file=sys.stderr,
        )
        return 1

    problems = load_baseline()
    if problems:
        print(
            "check-cpp-capability-layout: the baseline file must hold NO rows:",
            file=sys.stderr,
        )
        for p in problems:
            print(p, file=sys.stderr)
        return 1

    selftest()

    include_args = subjects_mod.include_args()
    with tempfile.TemporaryDirectory() as workdir:
        if "--report" in sys.argv:
            subjects, templates, problems = subjects_mod.derive(include_args)
            if problems:
                for p in problems:
                    print(f"  - {p}", file=sys.stderr)
                return 1
            base, forced, nostd, freestanding = read_all(
                subjects, include_args, workdir
            )
            for ty in subjects:
                without = nostd.get(ty)
                print(
                    f"{base.get(ty, '-'):>8}  "
                    f"no-std={without if without is not None else 'absent':>8}  "
                    f"freestanding={freestanding.get(ty) if freestanding.get(ty) is not None else 'absent':>8}"
                    f"  {ty}"
                )
            print(f"\n{len(subjects)} subject(s), {len(templates)} of them templates")
            return 0

        errors, subjects, _base = run(include_args, workdir)

    if errors:
        print(f"check-cpp-capability-layout: {len(errors)} problem(s):\n", file=sys.stderr)
        for e in errors:
            print(f"  - {e}", file=sys.stderr)
        print(
            """
  A capability probe changed a LAYOUT, or a type that should exist on a
  freestanding target does not. Two TUs of one image are allowed to disagree
  about a capability macro -- px4's bridge sets -DNROS_CPP_STD on one module of
  a larger image on purpose -- so they would link and then write the object
  through one layout and read it through the other. Silent. Issues 0135, 0460.

  Fix: hold the capability-dependent thing through an UNCONDITIONAL member.
  State hides behind a pointer only when it EXCEEDS a pointer; a member that is
  itself exactly a pointer wide is taken unconditionally in both
  configurations, which costs the same 8 bytes and buys one stable layout.

  There is NO baseline slot for this (phase-456 W6). The rule has no legitimate
  exception, so .config/cpp-capability-layout-baseline.txt holds no rows and a
  row in it is itself a failure. Gating a METHOD is still fine and needs none.""",
            file=sys.stderr,
        )
        return 1

    print(
        f"check-cpp-capability-layout: OK -- {len(subjects)} DERIVED subject(s) "
        f"x {len(CAPS)} forced capability macro(s) hosted, plus "
        f"hosted-without-NROS_CPP_STD and a -nostdinc++ freestanding measurement "
        f"against the ThreadX shim"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
