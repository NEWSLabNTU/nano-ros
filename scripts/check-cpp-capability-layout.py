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
the gate green at exit 0. Hence ARM 2 below -- the `-nostdinc++` freestanding
configuration is the only place the capability macros are genuinely off -- and
hence the real-header mutation in `selftest`, because a synthetic negative
control that cannot fail on the real subject is not a control over it.

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
subject at once. Nine compiles, not 810, and the nine run concurrently.

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

--- THE BASELINE ------------------------------------------------------------

`.config/cpp-capability-layout-baseline.txt` is a RATCHET, not an allowlist, in
the shape `.config/cpp-freestanding-includes-baseline.txt` already uses one
directory over. It records two things the gate must tolerate today:

  diverges <subject>     a known size violator. Issue 1225; owned by PR #755's
                         wave, which removes the line in the commit that fixes
                         the member.
  hosted-only <subject>  the type does not EXIST in the freestanding arm. Its
                         absence is not a layout divergence -- no freestanding
                         TU can name it, so no object of it crosses the
                         boundary -- but it is declared rather than inferred,
                         because "the type was supposed to exist there" is a
                         real defect wearing the same clothes (issue 1204).

Both directions fail: an unlisted violation cannot land, and a listed one that
is FIXED must lose its line. A ratchet that tolerates stale entries has stopped
ratcheting.

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
KINDS = ("diverges", "hosted-only")

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
# which is where the macros are genuinely off. A THIRD arm is now measurable
# and was not before W2: hosted WITHOUT the opt-in, i.e. the shape a hosted
# consumer gets by default from here on. It belongs with phase-438 W4, whose
# acceptance is that `sizeof(rclcpp::Node)` does not move between it and this
# one.
HOSTED_FLAGS = ["-std=c++17", "-DNROS_CPP_STD=1"]
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
    """({(kind, subject)}, [problems]). Sorted, no duplicates, known kinds."""
    entries = []
    problems = []
    for lineno, raw in enumerate(text.split("\n"), 1):
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        parts = line.split()
        if len(parts) != 2 or parts[0] not in KINDS:
            problems.append(
                f"  baseline:{lineno}: `{line}` is not `<{'|'.join(KINDS)}> <subject>`"
            )
            continue
        entries.append((parts[0], parts[1]))
    if entries != sorted(entries):
        problems.append("  baseline: not sorted; sort it so two additions cannot collide")
    dupes = sorted({e for e in entries if entries.count(e) > 1})
    if dupes:
        problems.append(f"  baseline: listed twice: {dupes}")
    return set(entries), problems


def load_baseline():
    if not os.path.exists(BASELINE):
        raise SystemExit(
            f"check-cpp-capability-layout: baseline missing at {BASELINE}.\n"
            "  It is tracked, so its absence is a PATH bug, not an empty ratchet."
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


def compare(subjects, base, forced, freestanding, baseline, check_orphans=True):
    """(errors, seen) -- `seen` is the baseline entries the readings justify.

    `base` is {subject: size}; `forced` is {cap: {subject: size}};
    `freestanding` is {subject: size or None}, None meaning confirmed absent.

    `check_orphans` is off only when the caller deliberately narrowed the
    subject list (the selftest does, to keep its two extra measurement passes
    cheap): every baseline entry would then look orphaned, which would turn a
    real ratchet arm into noise the control has to ignore.
    """
    errors = []
    seen = set()

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
        diverged = False
        for cap in CAPS:
            got = forced[cap].get(ty)
            if got is None:
                if ("hosted-only", ty) in baseline:
                    seen.add(("hosted-only", ty))
                    continue
                errors.append(
                    f"{ty} does not compile with -D{cap}=1.\n"
                    f"      px4 sets a capability macro on one module of a real image,\n"
                    f"      so this configuration has to at least build. An earlier\n"
                    f"      version treated it as 'not a layout question' (issue 1204)."
                )
                continue
            if got != base[ty]:
                diverged = True
                if ("diverges", ty) in baseline:
                    seen.add(("diverges", ty))
                    continue
                errors.append(
                    f"sizeof({ty}) changes with -D{cap}=1 -- {base[ty]} vs {got}."
                )

        # ARM 2 -- the freestanding configuration, where the macros are
        # genuinely off because `__has_include(<memory>)` and its siblings
        # answer NO against the ThreadX shim. This is the arm with teeth: a
        # member gated on any of the seven macros is present hosted and absent
        # here, so the two sizes disagree.
        fs = freestanding.get(ty)
        if fs is None:
            if ("hosted-only", ty) in baseline:
                seen.add(("hosted-only", ty))
            else:
                errors.append(
                    f"{ty} cannot be measured against the ThreadX shim and is not\n"
                    f"      declared hosted-only. A public type is expected to exist on\n"
                    f"      freestanding targets; if this one legitimately does not, add\n"
                    f"      `hosted-only {ty}` to the baseline with the reason in its\n"
                    f"      header. If it is not, the ABSENCE is the defect."
                )
        elif ("hosted-only", ty) in baseline:
            # The stale-exemption ratchet. A reason stops being true, nobody
            # re-reads the list, and the type keeps its skip forever.
            errors.append(
                f"{ty} is declared hosted-only but DOES measure freestanding ({fs}).\n"
                f"      Remove its baseline line; the exemption is stale."
            )
        elif fs != base[ty]:
            diverged = True
            if ("diverges", ty) in baseline:
                seen.add(("diverges", ty))
            else:
                errors.append(
                    f"sizeof({ty}) differs hosted vs -nostdinc++ freestanding -- "
                    f"{base[ty]} vs {fs}."
                )

        if ("diverges", ty) in baseline and not diverged:
            # The other ratchet direction, and the one a fix produces. A
            # baseline that tolerates entries for subjects that no longer
            # violate has stopped ratcheting -- the next real divergence in that
            # subject would be absorbed by the stale line.
            errors.append(
                f"{ty} is baselined as `diverges` but its layout is now INVARIANT.\n"
                f"      Delete its line in the commit that fixed it."
            )

    if check_orphans:
        known = set(subjects)
        for kind, ty in sorted(baseline):
            if ty not in known:
                errors.append(
                    f"{ty} is in the baseline as `{kind}` but is no longer a derived\n"
                    f"      subject. Delete the line -- a stale entry is inert while\n"
                    f"      reading as tracked debt (the issue-0743 class)."
                )
    return errors, seen


# --------------------------------------------------------------------------
# the run
# --------------------------------------------------------------------------


def read_all(subjects, include_args, workdir):
    """(base, forced, freestanding) -- every arm, absences confirmed alone.

    The nine arms are independent compiles of the same subject list, so they
    run concurrently. Serial they are the gate's whole cost; concurrent the
    gate costs its slowest single arm plus the derivation.
    """
    arms = [("base", HOSTED_FLAGS)]
    arms += [(cap, HOSTED_FLAGS + [f"-D{cap}=1"]) for cap in CAPS]
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

    fs = got["freestanding"]
    freestanding = {}
    for ty in subjects:
        freestanding[ty] = (
            fs[ty]
            if ty in fs
            else measure_one(ty, FREESTANDING_FLAGS, include_args, workdir)
        )
    return base, forced, freestanding


def run(include_args, workdir, baseline, subjects=None):
    """(errors, subjects, base) for one include configuration.

    `baseline` is passed in already parsed, so a FORMAT problem in the file is
    reported once by `main` rather than surfacing inside the selftest's case 3
    as "the unmutated copy failed" -- which is a true statement about a
    completely different cause.

    Passing `subjects` narrows the run to a chosen few; the baseline's orphan
    arm is then off, since every other entry would read as orphaned.
    """
    narrowed = subjects is not None
    if not narrowed:
        subjects, _templates, problems = subjects_mod.derive(include_args)
        if problems:
            return (
                ["cannot derive the subject list:\n      " + "\n      ".join(problems)],
                [],
                {},
            )
    base, forced, freestanding = read_all(subjects, include_args, workdir)
    errors, _seen = compare(
        subjects, base, forced, freestanding, baseline, check_orphans=not narrowed
    )
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
# `nros::Node`, and runs the SAME `run()` the gate runs. No tracked header is
# touched.
#
# Cases 4 and 5 are issue 1225's: a RATCHET that tolerates a stale entry has
# stopped ratcheting, and neither direction can be demonstrated by measuring
# today's tree -- one needs a violator that has been fixed, the other a subject
# that no longer exists. Both drive `compare()` with synthetic readings, which
# is why the comparison is a function rather than inlined in the loop.
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
INJECTION = (
    ANCHOR
    + "\n#ifdef NROS_CPP_HAS_SHARED_PTR\n"
    + "    double nros_selftest_mutation_member_;\n#endif"
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


def selftest(baseline):
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
        # tree. The UNMUTATED copy runs FIRST, because it is what tells the
        # mutated run apart from a broken copy: if the gate already fails on a
        # faithful copy, whatever the mutated run reports afterwards proves
        # nothing.
        mut = os.path.join(workdir, "selftest-include")
        shutil.copytree(os.path.join(ROOT, "packages/api/nros-cpp/include"), mut)
        args = ["-I" + mut] + subjects_mod.include_args()
        # Narrowed to the one subject the mutation reaches. The measurement
        # code path is identical; what it costs is 2 compiles per arm instead
        # of 2 x 90, which is the difference between a gate that runs on the
        # fast lane and one that doubles its wall clock.
        #
        # The subject is `::nros::Node` and not `rclcpp::Node` for the reason
        # in the header comment: `rclcpp::Node` is an ALIAS of it, so the two
        # are one layout, and a mutation there would be measuring the alias.
        before, subjects, _base = run(
            args, workdir, baseline, subjects=["::nros::Node"]
        )
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
                after, _s, _b = run(args, workdir, baseline, subjects=subjects)
                if not after:
                    fail.append(
                        "case 3: a capability-gated MEMBER injected into ::nros::Node did\n"
                        "    not fail the gate. This is issue 1204 exactly -- the\n"
                        "    measurement is comparing a configuration against itself."
                    )
                elif not any("::nros::Node" in e for e in after):
                    fail.append(
                        "case 3: the gate failed on the injected member but its message\n"
                        f"    does not name the type. Reported: {after[0]}"
                    )

    # Case 4 -- a baselined violator that has been FIXED must lose its line.
    # This is the direction PR #755's wave will hit, and no measurement of
    # today's tree can produce it.
    errs, _ = compare(
        ["::nros::Fixed"],
        {"::nros::Fixed": 8},
        {cap: {"::nros::Fixed": 8} for cap in CAPS},
        {"::nros::Fixed": 8},
        {("diverges", "::nros::Fixed")},
    )
    if not any("INVARIANT" in e for e in errs):
        fail.append(
            "case 4: a `diverges` entry whose subject no longer diverges did not fail.\n"
            "    A ratchet that tolerates stale entries has stopped ratcheting -- the\n"
            "    next real divergence there would be absorbed by the dead line."
        )

    # Case 5 -- an entry naming a subject the derivation no longer produces.
    errs, _ = compare(
        ["::nros::Real"],
        {"::nros::Real": 8},
        {cap: {"::nros::Real": 8} for cap in CAPS},
        {"::nros::Real": 8},
        {("hosted-only", "::nros::Gone")},
    )
    if not any("::nros::Gone" in e for e in errs):
        fail.append(
            "case 5: a baseline entry for a subject that no longer exists did not fail."
        )

    # Case 6 -- and the gate must still FIND a divergence with an empty
    # baseline. Cases 4 and 5 only prove the ratchet arms; this proves the
    # comparison they wrap still says yes.
    errs, _ = compare(
        ["::nros::Moves"],
        {"::nros::Moves": 24},
        {cap: {"::nros::Moves": 32 if cap == "NROS_CPP_STD" else 24} for cap in CAPS},
        {"::nros::Moves": 24},
        set(),
    )
    if not any("24 vs 32" in e for e in errs):
        fail.append("case 6: an unbaselined size divergence did not fail the comparison")

    if fail:
        print("check-cpp-capability-layout --selftest FAILED:", file=sys.stderr)
        for f in fail:
            print(f"  - {f}", file=sys.stderr)
        raise SystemExit(1)
    print(
        "check-cpp-capability-layout --selftest: 6 case(s) OK (gated member diverges, "
        "gated method does not, real ::nros::Node caught when mutated, a fixed "
        "baseline entry fails, a stale one fails, an unbaselined divergence fails)"
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

    baseline, problems = load_baseline()
    if problems:
        print(
            "check-cpp-capability-layout: the baseline file is malformed:",
            file=sys.stderr,
        )
        for p in problems:
            print(p, file=sys.stderr)
        return 1

    selftest(baseline)

    include_args = subjects_mod.include_args()
    with tempfile.TemporaryDirectory() as workdir:
        if "--report" in sys.argv:
            subjects, templates, problems = subjects_mod.derive(include_args)
            if problems:
                for p in problems:
                    print(f"  - {p}", file=sys.stderr)
                return 1
            base, forced, freestanding = read_all(subjects, include_args, workdir)
            for ty in subjects:
                std = forced["NROS_CPP_STD"].get(ty)
                print(
                    f"{base.get(ty, '-'):>8}  std={std if std is not None else '-':>8}  "
                    f"freestanding={freestanding.get(ty) if freestanding.get(ty) is not None else 'absent':>8}"
                    f"  {ty}"
                )
            print(f"\n{len(subjects)} subject(s), {len(templates)} of them templates")
            return 0

        errors, subjects, _base = run(include_args, workdir, baseline)

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

  Known violations live in .config/cpp-capability-layout-baseline.txt, which
  may only shrink.""",
            file=sys.stderr,
        )
        return 1

    print(
        f"check-cpp-capability-layout: OK -- {len(subjects)} DERIVED subject(s) "
        f"x {len(CAPS)} forced capability macro(s) hosted, plus a -nostdinc++ "
        f"freestanding measurement against the ThreadX shim"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
