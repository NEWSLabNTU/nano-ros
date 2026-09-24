#!/usr/bin/env python3
"""A dist's `system = [..]` must cover every library it actually needs.

WHY THIS EXISTS

phase-327 W4 declared `[tool.qemu] system = ["libslirp"]` because a dist's
runtime dep had reached the dynamic loader as a bare error. The declaration was
correct and NINETEEN SONAMES SHORT, and W4's own follow-up — "ldd audit of the
other dists" — stayed a sentence in a roadmap doc for a month. When it was
finally run (issue 0926) it found five more dists undeclared and two binaries
that could not start at all on a stock 22.04 host:

    openocd: error while loading shared libraries: libftdi.so.1
    arm-none-eabi-gdb: libncursesw.so.5 => not found

`system = [..]` is hand-authored, so it is only ever as complete as whoever
wrote it. This gate re-derives the truth from the dists themselves.

WHAT IT CHECKS

For every provisioned dist that matches a `[tool.<name>]`: the external ldd
closure of the ELF files the dist's PROGRAMS REACH — minus the base glibc/gcc
runtime, minus what the dist ships itself — must be covered by
`[tool.<name>] system = [..]`, via each prereq's `check.sharedlib` plus its
optional `provides = [..]`.

WHAT "REACH" MEANS, AND WHY IT IS NOT "EVERY ELF" (issue 1452)

The question this gate asks is *what must be present for this tool to RUN*.
Until issue 1452 it measured a different one — *what does any shared object in
this tree name* — and for every dist in the store those two agreed, because the
only shared objects present were ones a program loads in order to start.

A dist that bundles a CPython is the first case where they diverge. The arm64
`arm-none-eabi-gcc` dist ships a dynamic `libpython3.8.so.1.0` for `gdb`
(issue 0932), and a dynamic libpython brings a real `lib-dynload/` — ~44
optional extension modules, each an ELF naming a library of its own. Nothing
loads them at startup: CPython `import`s them opportunistically and degrades
when they are absent (`_hashlib` falls back to built-in hashes, `_decimal` to
`_pydecimal`). Measured on an arm64 host, they contributed TWELVE undeclared
sonames and turned `just doctor` red, for a toolchain that works.

Declaring them was not available as an answer. `libssl.so.1.1` /
`libcrypto.so.1.1` are unobtainable on jammy, and RFC-0099 D1's backward half
reads `system = [..]` to decide whether a prebuilt can be offered — so
declaring the pair would REFUSE THE WHOLE TOOLCHAIN on every jammy arm64 host
over two unimportable Python modules.

So the scope is derived instead:

* ROOTS are the dist's PROGRAMS — every ELF the loader can exec (`ET_EXEC`, or
  `ET_DYN` carrying a `PT_INTERP`). Not the index's `front`/`smoke` entries:
  those name a release job's smoke command, not an inventory. `smoke` for
  `arm-none-eabi-gcc` names 2 of ~40 shipped binaries, and a user runs
  `arm-none-eabi-objcopy` too. Rooting the walk in a hand-authored field would
  re-introduce, one level up, exactly the "only as complete as whoever wrote
  it" problem this gate exists to remove.
* A LIBRARY is in scope only if a root's `DT_NEEDED` chain reaches it INSIDE
  the dist. The chain is walked here, against the dist's own filenames and
  `DT_SONAME`s, rather than left to `ldd`: measured on `[tool.xrce-agent]`,
  `ldd bin/MicroXRCEAgent.real` stops at `libmicroxrcedds_agent.so.2.4 => not
  found` (the launcher supplies the path at exec time), so `libssl.so.3` and
  `libcrypto.so.3` — real requirements, three links down — are invisible to a
  walk that trusts the loader's transitivity.
* A dist with libraries and NO program is a LIBRARY dist: its libraries are the
  product, so all of them are roots. Without that clause such a dist would
  measure the empty set and print OK, which is a gate that can only pass.

WHAT THIS DELIBERATELY STOPS CATCHING

A shared object the dist ships that no program's `DT_NEEDED` chain reaches —
one loaded only by `dlopen`/`import` — no longer has to have its own
dependencies declared. If such a plug-in is REQUIRED rather than optional, a
missing library behind it now surfaces at first use as a `dlopen` failure
instead of here.

The store DOES contain required dlopen plug-ins — this is not a hypothetical
class. `arm-none-eabi-gcc` ships `libexec/.../liblto_plugin.so` and
`lib/bfd-plugins/libdep.so`, which `ld` dlopens during an LTO link; the riscv
toolchain ships those plus 75 CPython extension modules under
`lib/python3.12/lib-dynload/` — so the arm64 class is already on x86_64, and
the loss is real. It is bounded, and it was measured before it was accepted:

* Not one of the 88 out-of-scope objects names anything outside the base
  glibc/gcc runtime, because that toolchain BUNDLES what its modules need
  (`libexec/libssl.so.3`, `libsqlite3`, `libffi`, …) and the bundle subtracts
  out. So today the loss has no content. The gate SAYS SO on every run rather
  than leaving it implicit: an object that falls out of scope while naming an
  external library is reported, as information rather than as a verdict.
* The catch this gate first earned its keep on is preserved. openocd's
  `libftdi.so.1` is a `DT_NEEDED` OF THE PROGRAM (`readelf -d bin/openocd`),
  which is why it failed at the loader and not at a `dlopen` — a reachability
  walk still finds it, along with `libhidapi`, `libusb` and, through libusb,
  `libudev`.
* Across the 8 pinned dists provisioned on the author's host the scoped closure
  is IDENTICAL to what this gate measured before issue 1452 — `[tool.qemu]`'s
  `libselinux`/`libpcre2` and `[tool.xrce-agent]`'s two OpenSSL sonames
  included. Nothing that was being caught stopped being caught.
* `--include-unreached` puts every ELF back in scope, so the wider measurement
  is one flag away when a dist gains a plug-in worth auditing.

`env -u LD_LIBRARY_PATH` IS LOAD-BEARING, not hygiene. Measured with ROS on the
path, cyclonedds appeared to need four `libiceoryx_*` libs it does not: the
loader had resolved `libddsc.so.0` to ROS's own cyclonedds rather than to the
dist's copy behind `RUNPATH=$ORIGIN/../lib`. That is issue 0774's class, and an
audit inheriting the caller's environment measures the caller.

WHERE IT RUNS

NOT on the fast line. It needs a provisioned store, so under CLAUDE.md's
affordability rule (`check-lane-contracts`) it belongs only in a tier that has
one. With no store it SKIPS, loudly and by name — a gate that silently passes on
every machine lacking its input is issue 0196's shape.

WHAT IT DOES NOT CHECK

* Whether a declared package is INSTALLED. That is `nros setup --system
  --check`'s job, and it is a property of the host, not of the tree.
* Dists with no `[tool.*]` entry (zenohd is provisioned another way).
* Non-Linux hosts: `ldd` is glibc's. Skips with a reason.
* Libraries the RUSTUP TOOLCHAIN provides — see `RUSTC` below.
* A dist's glibc FLOOR. `scripts/sdk/measure-dist-floor.py` asks that, and it
  reads EVERY ELF on purpose: "can these bytes run on this host at all" is a
  property of the file, not of whether a program reaches it.

Usage:  check-dist-runtime-deps.py [--store DIR] [--include-unreached]
"""

import collections
import os
import re
import struct
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
INDEX = os.path.join(ROOT, "nros-sdk-index.toml")
DEFAULT_STORE = os.path.expanduser("~/.nros/sdk")

# The C/C++ runtime every ELF on a glibc host links. Declaring these would be
# noise: a host without libc cannot run the gate that checks for it.
BASE = re.compile(
    r"^(libc|libm|libdl|libpthread|librt|libstdc\+\+|libgcc_s|libutil|libresolv"
    r"|ld-linux.*|linux-vdso)\.so"
)

# Shared libraries that come from the RUSTUP TOOLCHAIN a dist pins, not from a
# package manager (phase-422 W2).
#
# `system = [..]` names `[prereq.*]` keys, and every one of those resolves to an
# apt/dnf/pacman/brew package. `librustc_driver-<hash>.so` has no such package on
# any distro: it ships inside `~/.rustup/toolchains/<tc>/lib`, and the dist finds
# it because its own launcher sets the library path before exec'ing the binary
# that needs it. So a bare `ldd` on such a binary always reports `not found`, on
# a host where the tool works perfectly — a permanent false positive, and the
# advice it prints ("add a `[prereq.*]` entry") names a package that cannot
# exist.
#
# `[tool.verus]` is the case: `rust_verify` links `librustc_driver-<hash>.so`,
# and `verus --version` runs fine beside it. The requirement is not undeclared —
# it is declared as the rustup toolchain, by `just verification verus`, which
# installs the exact channel the release pins.
#
# Keyed on the SHAPE rustc gives its own shared libraries — `lib<name>-<16 hex
# digits>.so` — because that hash is a rustc-internal disambiguator and nothing a
# distro ships is named that way. Narrow on purpose: a plain `libfoo.so.1` a dist
# forgot to declare still fails, which is the bug this gate exists for.
RUSTC = re.compile(r"^lib[A-Za-z0-9_]+-[0-9a-f]{16}\.so$")

# What one ELF file says about itself. `needed` is DT_NEEDED in order; `soname`
# is DT_SONAME or None. `is_program` means the loader can exec it.
ElfFacts = collections.namedtuple("ElfFacts", "is_program soname needed")

ET_EXEC, ET_DYN = 2, 3
PT_LOAD, PT_DYNAMIC, PT_INTERP = 1, 2, 3
DT_NULL, DT_NEEDED, DT_STRTAB, DT_SONAME = 0, 1, 5, 14


def elf_facts(path):
    """Read `PT_INTERP` / `DT_NEEDED` / `DT_SONAME` straight out of an ELF file.

    Returns None for anything that is not a dynamic ELF image — a script, a
    `.o`, a target-arch archive member, a truncated file.

    Parsed here rather than shelled out to `readelf` because this runs over
    every file in every provisioned dist (~2000 on a full store) and because it
    makes the classification a pure function of the bytes, which is what
    `self_test` exercises. The sibling `measure-dist-floor.py` does use
    `readelf`: it needs `.gnu.version_r`, which is a real reason to.
    """
    try:
        with open(path, "rb") as fh:
            head = fh.read(64)
            if len(head) < 64 or head[:4] != b"\x7fELF":
                return None
            cls, data = head[4], head[5]
            if cls not in (1, 2) or data not in (1, 2):
                return None
            end = "<" if data == 1 else ">"
            wide = cls == 2
            (e_type,) = struct.unpack(end + "H", head[16:18])
            if e_type not in (ET_EXEC, ET_DYN):
                return None
            if wide:
                (e_phoff,) = struct.unpack(end + "Q", head[32:40])
                e_phentsize, e_phnum = struct.unpack(end + "HH", head[54:58])
            else:
                (e_phoff,) = struct.unpack(end + "I", head[28:32])
                e_phentsize, e_phnum = struct.unpack(end + "HH", head[42:46])
            if not e_phnum or e_phnum == 0xFFFF:
                return None
            fh.seek(e_phoff)
            phdrs = fh.read(e_phentsize * e_phnum)
            if len(phdrs) < e_phentsize * e_phnum:
                return None

            loads, dynamic, interp = [], None, False
            for i in range(e_phnum):
                ph = phdrs[i * e_phentsize : (i + 1) * e_phentsize]
                (p_type,) = struct.unpack(end + "I", ph[0:4])
                if wide:
                    p_offset, p_vaddr = struct.unpack(end + "QQ", ph[8:24])
                    (p_filesz,) = struct.unpack(end + "Q", ph[32:40])
                else:
                    p_offset, p_vaddr = struct.unpack(end + "II", ph[4:12])
                    (p_filesz,) = struct.unpack(end + "I", ph[16:20])
                if p_type == PT_INTERP:
                    interp = True
                elif p_type == PT_LOAD:
                    loads.append((p_vaddr, p_filesz, p_offset))
                elif p_type == PT_DYNAMIC:
                    dynamic = (p_offset, p_filesz)

            is_program = e_type == ET_EXEC or interp
            if dynamic is None:
                return ElfFacts(is_program, None, ())

            def to_offset(vaddr):
                for base, size, off in loads:
                    if base <= vaddr < base + size:
                        return vaddr - base + off
                return None

            fh.seek(dynamic[0])
            dyn = fh.read(dynamic[1])
            step = 16 if wide else 8
            fmt = end + ("Q" if wide else "I")
            strtab, entries = None, []
            for i in range(0, len(dyn) - step + 1, step):
                (tag,) = struct.unpack(fmt, dyn[i : i + step // 2])
                (val,) = struct.unpack(fmt, dyn[i + step // 2 : i + step])
                if tag == DT_NULL:
                    break
                if tag == DT_STRTAB:
                    strtab = to_offset(val)
                elif tag in (DT_NEEDED, DT_SONAME):
                    entries.append((tag, val))
            if strtab is None:
                return ElfFacts(is_program, None, ())

            def string_at(offset):
                fh.seek(strtab + offset)
                raw = fh.read(4096)
                cut = raw.find(b"\0")
                if cut < 0:
                    return None
                try:
                    return raw[:cut].decode("utf-8")
                except UnicodeDecodeError:
                    return None

            soname, needed = None, []
            for tag, val in entries:
                name = string_at(val)
                if not name:
                    continue
                if tag == DT_SONAME:
                    soname = name
                else:
                    needed.append(name)
            return ElfFacts(is_program, soname, tuple(needed))
    except (OSError, struct.error):
        return None


def dist_scope(records):
    """Which of a dist's ELF files must have their dependencies declared.

    `records` maps a path to its `ElfFacts`. Returns `(scope, unreached)`, both
    sets of paths: `scope` is the programs plus every library a program's
    `DT_NEEDED` chain reaches inside the dist, `unreached` is the rest — the
    plug-ins nothing loads in order to start.

    PURE. Everything about *which files matter* is decided here, so the rule can
    be tested against a recorded dist shape on a host that has no such dist.
    """
    # A `DT_NEEDED` name resolves against the dist's own `DT_SONAME`s first and
    # its filenames second — `libfastrtps.so.2.14` is a soname whose only file
    # on disk is `libfastrtps.so.2.14.6`, and the symlink that would have
    # bridged them is not walked. Sorted so a basename that happens to occur in
    # two directories resolves the same way on every host.
    resolve = {}
    for path in sorted(records):
        if records[path].soname:
            resolve.setdefault(records[path].soname, path)
    for path in sorted(records):
        resolve.setdefault(os.path.basename(path), path)

    roots = {p for p, f in records.items() if f.is_program}
    if not roots:
        # A LIBRARY dist: nothing here is exec'd, so the libraries ARE the
        # product and every one of them is a root. Measuring the empty set and
        # printing OK is the failure mode this clause exists to prevent.
        roots = set(records)

    scope, queue = set(), list(roots)
    while queue:
        path = queue.pop()
        if path in scope:
            continue
        scope.add(path)
        for name in records[path].needed:
            target = resolve.get(name)
            if target is not None and target not in scope:
                queue.append(target)
    return scope, set(records) - scope


def load_index():
    try:
        import tomllib as toml
    except ModuleNotFoundError:
        import tomli as toml
    with open(INDEX, "rb") as fh:
        return toml.load(fh)


def sonames_of(prereq):
    """Every soname a prereq entry satisfies: its probe plus `provides`."""
    out = set(prereq.get("provides", []))
    probe = (prereq.get("check") or {}).get("sharedlib")
    if probe:
        out.add(probe)
    return out


def coverage_problem(soname, declared, by_soname):
    """Why `system = [..]` fails to cover this soname, or None if it covers it.

    The verdict, in one place, so `self_test` can exercise it. It used to be
    inline in `audit`, where the only thing standing for it in the self-test was
    a row that read `("an undeclared-by-tool soname is a problem", True)` — a
    literal, i.e. the `check-no-vacuous-tests` shape one layer down.
    """
    keys = by_soname.get(soname, set())
    if not keys:
        return "no [prereq.*] declares this soname"
    if not (keys & declared):
        return f"declared by [prereq.{sorted(keys)[0]}], not in system = [..]"
    return None


Measured = collections.namedtuple("Measured", "needed unreached out_of_scope")


def closure(dist_root, include_unreached=False):
    """External sonames a dist needs, minus base runtime and its own libs.

    `needed` is the verdict's closure, resolved by `ldd` over the in-scope files
    (so it includes what a HOST library pulls in transitively — qemu's
    `libpcre2` arrives only through `libselinux`).

    `unreached` counts the ELF objects no program reaches, and `out_of_scope`
    maps each external soname those objects name to the first that names it.
    That second map is read off their own `DT_NEEDED`, not off `ldd`, and is
    deliberately NOT a verdict: it exists so the narrowing issue 1452 asked for
    cannot make a finding invisible. Empty on every dist provisioned to date.
    """
    own, records = set(), {}
    # walk-ok: the subject is ~/.nros/sdk, a provisioned SDK store OUTSIDE the
    # repository. `git ls-files` cannot enumerate it — nothing here is tracked,
    # which is the whole point: the dists are what the index's declarations are
    # measured AGAINST.
    for dirpath, _, names in os.walk(dist_root):
        for n in names:
            if ".so" in n:
                own.add(n)
            path = os.path.join(dirpath, n)
            if not os.path.isfile(path) or os.path.islink(path):
                continue
            facts = elf_facts(path)
            if facts is not None:
                records[path] = facts
    # One spelling of "the dist ships this": a filename in the tree OR a
    # library's own `DT_SONAME`. The soname half matters — `libfastrtps.so.2.14`
    # is a soname with no file of that name beside it.
    own |= {f.soname for f in records.values() if f.soname}

    scope, unreached = dist_scope(records)
    if include_unreached:
        scope, unreached = set(records), set()

    def external(names):
        return [
            s
            for s in names
            if not BASE.match(s) and not RUSTC.match(s) and s not in own
        ]

    out_of_scope = {}
    for path in sorted(unreached):
        for so in external(records[path].needed):
            out_of_scope.setdefault(so, os.path.relpath(path, dist_root))

    env = {k: v for k, v in os.environ.items() if k != "LD_LIBRARY_PATH"}
    needed = set()
    for path in sorted(scope):
        try:
            out = subprocess.run(
                ["ldd", path], capture_output=True, text=True, env=env, timeout=60
            ).stdout
        except (OSError, subprocess.SubprocessError):
            continue
        for line in out.splitlines():
            line = line.strip()
            if "=>" not in line and "not found" not in line:
                continue
            so = line.split()[0]
            if BASE.match(so) or RUSTC.match(so) or so in own:
                continue
            needed.add(so)
    return Measured(needed, len(unreached), out_of_scope)


def audit(index, store, include_unreached=False):
    """What each provisioned dist needs and does not declare.

    Returns `(problems, skipped, notes)`: `problems` is the verdict —
    `[(tool, soname, reason)]` — `skipped` counts objects left out of scope, and
    `notes` is `[(tool, soname, path)]` for a soname only an out-of-scope object
    names. A note is never a failure; see `closure`.
    """
    prereqs = index.get("prereq", {})
    # soname -> the prereq keys that satisfy it. ONE mapping, derived from the
    # index; the gate keeps no table of its own.
    by_soname = {}
    for key, dep in prereqs.items():
        for so in sonames_of(dep):
            by_soname.setdefault(so, set()).add(key)

    problems, notes, skipped = [], [], 0
    for name, tool in sorted(index.get("tool", {}).items()):
        # The PINNED version, not the whole tool directory. The store
        # ACCUMULATES (issue 0500), so `<store>/<tool>/` holds every version
        # ever installed — and measuring them together is a false negative in
        # both directions: one version's bundled `lib/` counts as "shipped by
        # the dist" for another version's binaries, so a re-cut masks the older
        # release it replaced. Measured: with `arm-none-eabi-gcc` 13.2-nros1 and
        # -nros2 both present, nros2's bundled ncurses hid nros1's missing one.
        #
        # The pin is also the only version that MATTERS here: it is what
        # `nros setup` resolves and what a user runs.
        version = tool.get("version")
        root = os.path.join(store, name, version) if version else None
        if not root or not os.path.isdir(root):
            continue
        declared = set(tool.get("system", []))
        measured = closure(root, include_unreached)
        skipped += measured.unreached
        for so, where in sorted(measured.out_of_scope.items()):
            if so not in measured.needed:
                notes.append((name, so, where))
        for so in sorted(measured.needed):
            why = coverage_problem(so, declared, by_soname)
            if why:
                problems.append((name, so, why))
    return problems, skipped, notes


def _lib(needed, soname=None):
    return ElfFacts(False, soname, tuple(needed))


def _prog(needed):
    return ElfFacts(True, None, tuple(needed))


# The shape issue 1452 measured on an arm64 host, transcribed from its table:
# a gdb that links a dynamic CPython, and that CPython's `lib-dynload/`
# extension modules, each naming one library nothing execs it to reach. The
# reached side is `libcrypt.so.1`, the one soname `system = ["libcrypt1"]`
# covers. This is the SHAPE, not a capture of the artifact's every DT_NEEDED —
# what is under test is the reachability rule, and the rule is what no x86_64
# host can exercise (there gdb embeds CPython statically and loads no extension
# module at all).
ARM64_GDB_SHAPE = {
    "bin/arm-none-eabi-gcc": _prog(["libc.so.6"]),
    "bin/arm-none-eabi-gdb": _prog(["libpython3.8.so.1.0"]),
    "lib/libpython3.8.so.1.0": _lib(["libcrypt.so.1"], "libpython3.8.so.1.0"),
    "lib/python3.8/lib-dynload/_bz2.so": _lib(["libbz2.so.1.0"]),
    "lib/python3.8/lib-dynload/_lzma.so": _lib(["liblzma.so.5"]),
    "lib/python3.8/lib-dynload/_sqlite3.so": _lib(["libsqlite3.so.0"]),
    "lib/python3.8/lib-dynload/_curses.so": _lib(["libncursesw.so.6", "libtinfo.so.6"]),
    "lib/python3.8/lib-dynload/_curses_panel.so": _lib(["libpanelw.so.6"]),
    "lib/python3.8/lib-dynload/readline.so": _lib(["libreadline.so.8"]),
    "lib/python3.8/lib-dynload/_dbm.so": _lib(["libdb-5.3.so"]),
    "lib/python3.8/lib-dynload/_uuid.so": _lib(["libuuid.so.1"]),
    "lib/python3.8/lib-dynload/nis.so": _lib(["libnsl.so.1"]),
    "lib/python3.8/lib-dynload/_ssl.so": _lib(["libssl.so.1.1", "libcrypto.so.1.1"]),
    "lib/python3.8/lib-dynload/_hashlib.so": _lib(["libcrypto.so.1.1"]),
}

# `[tool.xrce-agent]` as provisioned: a launcher SCRIPT in `bin/` (not ELF, so
# not here), the real program under `lib/`, and a two-link internal chain to the
# library that names OpenSSL. `ldd` on the program cannot see past
# `libmicroxrcedds_agent.so.2.4 => not found`, which is why the chain is walked
# from DT_NEEDED rather than delegated to the loader.
XRCE_SHAPE = {
    "lib/MicroXRCEAgent.real": _prog(["libmicroxrcedds_agent.so.2.4"]),
    "lib/libmicroxrcedds_agent.so.2.4.3": _lib(
        ["libfastrtps.so.2.14.6", "libfastcdr.so.2.2.7"], "libmicroxrcedds_agent.so.2.4"
    ),
    "lib/libfastrtps.so.2.14.6": _lib(
        ["libssl.so.3", "libcrypto.so.3"], "libfastrtps.so.2.14"
    ),
    "lib/libfastcdr.so.2.2.7": _lib([], "libfastcdr.so.2.2"),
}


def _elf_probe_checks():
    """Two-sided check that `elf_facts` tells a program from a library.

    `dist_scope` is only as good as the classification feeding it, and that half
    cannot be tested from a table — it is a property of real bytes. Uses files
    every glibc host already has: this interpreter (a program) and whichever of
    its own dependencies is a plain shared object.
    """
    me = elf_facts(sys.executable)
    if me is None or not me.is_program:
        return [("this interpreter is classified as a program", False)]
    env = {k: v for k, v in os.environ.items() if k != "LD_LIBRARY_PATH"}
    try:
        out = subprocess.run(
            ["ldd", os.path.realpath(sys.executable)],
            capture_output=True,
            text=True,
            env=env,
            timeout=60,
        ).stdout
    except (OSError, subprocess.SubprocessError):
        return [("ldd is available to find a real shared library", False)]
    library = None
    for line in out.splitlines():
        if "=>" not in line:
            continue
        parts = line.split()
        if len(parts) < 3 or not parts[2].startswith("/"):
            continue
        facts = elf_facts(parts[2])
        if facts is not None and not facts.is_program and facts.soname:
            library = (parts[2], facts)
            break
    return [
        ("this interpreter is classified as a program", True),
        # The negative control. Without it the classifier could answer "program"
        # to everything and every check above would still pass.
        ("a real shared library is NOT classified as a program", library is not None),
        (
            "a real shared library reports its DT_SONAME",
            bool(library and library[1].soname),
        ),
    ]


def self_test():
    """Prove the check can fail — a negative control nobody runs is a comment."""
    index = {
        "tool": {"t": {"system": ["libfoo"]}},
        "prereq": {
            "libfoo": {"check": {"sharedlib": "libfoo.so.1"}},
            "libbar": {"check": {"sharedlib": "libbar.so.2"}},
            "libmulti": {
                "provides": ["liba.so.1", "libb.so.1"],
                "check": {"sharedlib": "liba.so.1"},
            },
        },
    }
    prereqs = index["prereq"]
    by_soname = {}
    for key, dep in prereqs.items():
        for so in sonames_of(dep):
            by_soname.setdefault(so, set()).add(key)

    arm_scope, arm_unreached = dist_scope(ARM64_GDB_SHAPE)
    arm_plugins = {p for p in ARM64_GDB_SHAPE if "lib-dynload/" in p}
    # MUTATION: the same plug-in, this time NEEDED by the program. If the
    # exclusion were keyed on the `lib-dynload` path — candidate 2 in issue
    # 1452, the one with the sharpest trap — this row would still be excluded
    # and a real requirement would go unmeasured.
    reached_plugin = dict(ARM64_GDB_SHAPE)
    reached_plugin["bin/arm-none-eabi-gdb"] = _prog(
        ["libpython3.8.so.1.0", "_ssl.so"]
    )
    reached_scope, _ = dist_scope(reached_plugin)
    # MUTATION: a genuinely undeclared dependency OF THE PROGRAM — openocd's
    # `libftdi.so.1` road, which is a DT_NEEDED and not a dlopen.
    missing_lib = dict(ARM64_GDB_SHAPE)
    missing_lib["bin/openocd"] = _prog(["libftdi.so.1"])
    missing_scope, _ = dist_scope(missing_lib)

    xrce_scope, xrce_unreached = dist_scope(XRCE_SHAPE)

    library_dist = {
        "lib/libthing.so.1": _lib(["libz.so.1"], "libthing.so.1"),
        "lib/libother.so.1": _lib(["libbz2.so.1.0"], "libother.so.1"),
    }
    library_scope, _ = dist_scope(library_dist)

    checks = [
        ("a probe soname is found", "libfoo.so.1" in sonames_of(prereqs["libfoo"])),
        (
            "`provides` widens the set",
            sonames_of(prereqs["libmulti"]) == {"liba.so.1", "libb.so.1"},
        ),
        ("base runtime is excluded", bool(BASE.match("libstdc++.so.6"))),
        ("a real lib is not excluded", not BASE.match("libftdi.so.1")),
        # phase-422 W2 — the rustup-provided shape, and its negative control.
        # Without the second row the pattern could be widened to anything and
        # still "pass", which is how an exemption turns into a blind spot.
        (
            "a rustc-hashed soname is excluded",
            bool(RUSTC.match("librustc_driver-4d71126a08f22b4a.so")),
        ),
        (
            "a distro soname is NOT excluded by the rustc shape",
            not (RUSTC.match("libftdi.so.1") or RUSTC.match("libncursesw.so.5")),
        ),
        # The bug this gate exists for: declared-somewhere but not by THIS tool.
        (
            "a soname declared-somewhere but not by THIS tool is a problem",
            coverage_problem("libbar.so.2", {"libfoo"}, by_soname) is not None,
        ),
        (
            "a soname no [prereq.*] knows at all is a problem",
            coverage_problem("libftdi.so.1", {"libfoo"}, by_soname)
            == "no [prereq.*] declares this soname",
        ),
        # The other direction, without which the two rows above would pass a
        # `coverage_problem` that simply always complains.
        (
            "a soname the tool DOES declare is not a problem",
            coverage_problem("libfoo.so.1", {"libfoo"}, by_soname) is None,
        ),
        (
            "`provides` covers a soname that is not the probe",
            coverage_problem("libb.so.1", {"libmulti"}, by_soname) is None,
        ),
        # --- issue 1452: what the walk reaches -------------------------------
        (
            "every lib-dynload plug-in is out of scope",
            arm_unreached == arm_plugins,
        ),
        (
            "the bundled libpython a program links IS in scope",
            "lib/libpython3.8.so.1.0" in arm_scope,
        ),
        (
            "every shipped program is a root",
            {"bin/arm-none-eabi-gcc", "bin/arm-none-eabi-gdb"} <= arm_scope,
        ),
        # The five rows below are the other direction: the gate must still fail
        # on a real missing library. A narrowing with no such control is issue
        # 0196 one layer down.
        (
            "a plug-in a program NEEDS is back in scope",
            "lib/python3.8/lib-dynload/_ssl.so" in reached_scope,
        ),
        (
            "a program's own undeclared dependency is still measured",
            "bin/openocd" in missing_scope,
        ),
        (
            "an internal chain ldd cannot follow is still walked",
            "lib/libfastrtps.so.2.14.6" in xrce_scope,
        ),
        ("a reachable dist leaves nothing unreached", xrce_unreached == set()),
        (
            "a dist with no program measures its libraries",
            library_scope == set(library_dist),
        ),
    ] + _elf_probe_checks()
    bad = [name for name, ok in checks if not ok]
    if bad:
        for b in bad:
            print(f"check-dist-runtime-deps self-test: FAIL {b}", file=sys.stderr)
        raise SystemExit(1)


def main():
    store = DEFAULT_STORE
    argv = sys.argv[1:]
    include_unreached = "--include-unreached" in argv
    argv = [a for a in argv if a != "--include-unreached"]
    if argv[:1] == ["--store"]:
        store = argv[1]
    if sys.platform != "linux":
        print(f"check-dist-runtime-deps: SKIP — ldd is glibc's ({sys.platform}).")
        return 0
    if not os.path.isdir(store):
        print(
            f"check-dist-runtime-deps: SKIP — no provisioned store at {store}.\n"
            "  This gate re-measures real dists, so it needs one. Run\n"
            "  `nros setup <board>` first, or pass --store."
        )
        return 0
    index = load_index()
    problems, skipped, notes = audit(index, store, include_unreached)
    rc = 0
    if problems:
        print(
            "check-dist-runtime-deps: a dist needs libraries its `system = [..]` "
            "does not name:\n",
            file=sys.stderr,
        )
        for tool, so, why in problems:
            print(f"  [tool.{tool}]  {so}  — {why}", file=sys.stderr)
        print(
            "\n  `system = [..]` is hand-authored and was 19 sonames short once "
            "already\n  (issue 0926). Add the missing key to that tool's list, and a\n"
            "  `[prereq.*]` entry if the soname has none. A prereq covering several\n"
            "  sonames lists them in `provides = [..]`.",
            file=sys.stderr,
        )
        rc = 1
    else:
        n = sum(
            1 for t in index.get("tool", {}) if os.path.isdir(os.path.join(store, t))
        )
        tail = (
            " (every ELF, --include-unreached)"
            if include_unreached
            else f"; {skipped} plug-in object(s) no program reaches, out of scope "
            "(issue 1452)"
        )
        print(
            f"check-dist-runtime-deps OK — {n} provisioned dist(s); every external "
            f"library each needs to RUN is declared{tail}."
        )
    # Information, not a verdict. These are the sonames the narrowing takes out
    # of the closure, and printing them is what keeps "out of scope" from
    # meaning "invisible". LAST, on both paths: `just doctor` renders this gate
    # by its FIRST line, so a note ahead of the verdict would displace it there.
    for tool, so, where in notes:
        print(
            f"check-dist-runtime-deps: note — [tool.{tool}] ships {where}, which "
            f"names {so} and which no program reaches.\n"
            "  Not required to be declared (issue 1452): a plug-in nothing loads "
            "to start is not\n  something the tool needs in order to RUN. If THIS "
            "one is required, declare it\n  and say why in the index comment."
        )
    return rc


if __name__ == "__main__":
    self_test()
    sys.exit(main())
