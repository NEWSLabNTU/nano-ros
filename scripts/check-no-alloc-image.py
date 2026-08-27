#!/usr/bin/env python3
"""Link-time allocation gate for a built nano-ros image — issue 0816.

The book promises heap-free operation in four places (`--claims` prints them
with their line numbers). Nothing checked any of them: no lane `nm`s an image
and asserts the allocator is absent. Cargo feature gates are what the tree has
instead, and they are necessary without being sufficient —

  * a vendored C dependency reaches the allocator with no Cargo feature
    involved at all (zenoh-pico calls `z_malloc` from 42 sites; cyclonedds and
    the lwIP/NetX stacks each have their own),
  * a weak-symbol fallback links a default implementation when nothing stronger
    is present, and no feature changed,
  * `extern crate alloc` behind a default-on feature of a *transitive* crate is
    invisible in the leaf manifest.

Issue 0817 is the worked example: sixteen `k_malloc` sites in the Zephyr
platform port bypassed `nros_platform_alloc`, compiled, linked, ran, and passed
every lane. A source grep eventually found them, and a source grep cannot see
vendored C. The linker can. This tool asks the linker.

What counts as an allocation symbol, and why
--------------------------------------------
Four families, each a *different* way the heap arrives, so each is reported
separately rather than as one undifferentiated blob:

`c-heap`
    `malloc` / `calloc` / `realloc` / `free` and the aligned and duplicating
    variants (`aligned_alloc`, `posix_memalign`, `strdup`, ...), plus the libc
    spellings that alias them (`__libc_malloc`, picolibc/newlib's reentrant
    `_malloc_r`). The duplicating ones are here because `strdup` allocates:
    listing only the four canonical names would let a C dependency keep the
    heap and pass.

`rust-alloc-shim`
    `__rust_alloc` / `__rust_dealloc` / `__rust_realloc` /
    `__rust_alloc_zeroed`, the `__rg_*` symbols a `#[global_allocator]`
    expands to, the `__rdl_*` default-allocator shim, and
    `__rust_no_alloc_shim_is_unstable`. This family is the load-bearing one for
    a Rust image: it is present if and only if the `alloc` crate is actually
    linked, whatever the feature flags say.

    It is also the one family matched as a SUBSTRING rather than a whole name,
    and the exception is deliberate. Current rustc emits these inside a
    synthetic `__rustc` crate, so a real image spells them
    `__rustc::__rust_alloc` demangled and `_RNvCs..._7___rustc12___rust_alloc`
    mangled — three underscores, a length prefix, and a crate path around the
    name. A whole-name set matched NONE of them; the first run of this gate
    against `examples/native/rust/talker` reported five `c-heap` symbols and
    zero Rust ones from a binary that plainly contains eight. The names in this
    family are distinctive enough that a substring carries none of the
    `free`/`freeaddrinfo` risk that makes substring matching wrong for
    `c-heap`. The shim also gained a `_v2` suffix, so it is matched by prefix.

`cxx-operator-new`
    `operator new` / `operator new[]` / `operator delete` / `operator delete[]`
    including the sized and `align_val_t` overloads. Matched demangled when a
    demangler ran and by Itanium mangling prefix (`_Znw`, `_Zna`, `_Zdl`,
    `_Zda`) when it did not, because a `nm` without `-C` support would
    otherwise silently report a C++ image as clean.

`rtos-heap`
    `k_malloc`/`k_free` (Zephyr), `pvPortMalloc`/`vPortFree` (FreeRTOS),
    `tx_byte_allocate`/`tx_byte_release` (ThreadX), `kmm_*` (NuttX),
    `heap_caps_*` (ESP-IDF). These never appear in a `c-heap` scan — an RTOS
    heap is not libc's — and they are exactly what issue 0817 found.

Matching is on the WHOLE symbol name after stripping any `@GLIBC_2.2.5`
version suffix, never on a substring (the one documented exception is the Rust
family above). `freeaddrinfo` and `freeifaddrs` are real symbols in this tree's
images and neither allocates from the general heap; a substring match on `free`
would report both, and a gate that cries wolf on its first run is a gate that
gets an unconditional allowlist.

The last `::` segment of a demangled path is matched too, so `std::free` in a
C++ image is not hidden by its namespace. That direction can misfire — a Rust
`mycrate::pool::free` is not libc's — and it is the right way round for a DENY
gate: a false positive is loud and is retired with
`--allow free=<why>`, while a false negative is exactly the silence this issue
was filed about.

Why the mere presence of an allocator is not automatically the finding
---------------------------------------------------------------------
This tree deliberately has exactly ONE `#[global_allocator]`, in
`nros-platform` (issue 0594, phase 391). A hosted native image links it on
purpose. So "an allocation symbol exists" is not a defect by itself — the
defect is an allocation symbol in an image that CLAIMS to have none. The claim
is therefore an INPUT: you name the tier, and the tier names the rule.

    tier `heap-free`   deny every allocation symbol, no exceptions.
                       This is the tier the book's promises are written at.

    tier `unified`     allocation is permitted, but only from the platform
                       backend — RFC-0034 D6's single-funnel rule. That is a
                       statement about WHICH OBJECT references the allocator,
                       and a linked image has thrown that away: after `ld` runs
                       there is one `U malloc` for the whole program and no
                       record of who wanted it. So `--tier unified` reads
                       OBJECTS (`--objects`), not the ELF, and refuses an ELF
                       rather than returning a green it cannot justify.

Usage
-----
    scripts/check-no-alloc-image.py <elf> [<elf>...]          # tier heap-free
    scripts/check-no-alloc-image.py <elf> --json
    scripts/check-no-alloc-image.py --objects <dir-or-file>... --tier unified
    scripts/check-no-alloc-image.py --claims                  # the book roster
    scripts/check-no-alloc-image.py --selftest                # prove it fails

A verdict is only as fresh as the artifact
------------------------------------------
This reads a BUILT thing, so it inherits the tree's museum-binary hazard
wholesale: an object from before a fix still contains the bypass, and this tool
will faithfully report it. The first real `--tier unified` run here flagged
`calloc`/`free` in a cyclonedds `service.o` dated three weeks before the source
that stopped calling them. Every report therefore prints the artifact's mtime —
compare it against the source you think you are checking, and rebuild the
fixture rather than filing the finding.

Exit codes: 0 clean, 1 violation, 2 the check could not be run (no symbol
table, no usable `nm`, a path that matched nothing, a tier/mode mismatch). 2 is
never reported as a pass — a check with nothing to check reads as coverage and
is worse than no check at all, which is the same rule
`scripts/nros-mem-report.py --check` and `check-no-vacuous-tests` enforce.
"""

import argparse
import contextlib
import datetime
import fnmatch
import io
import json
import os
import re
import shutil
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# ---------------------------------------------------------------------------
# Symbol families. Exact whole-name matches unless noted; see the docstring for
# why each member is here.
# ---------------------------------------------------------------------------

C_HEAP = {
    # the canonical four
    "malloc",
    "calloc",
    "realloc",
    "free",
    # sized/aligned variants — a C dependency that only uses these still heaps
    "reallocarray",
    "aligned_alloc",
    "posix_memalign",
    "memalign",
    "valloc",
    "pvalloc",
    # allocating string/format helpers
    "strdup",
    "strndup",
    "asprintf",
    "vasprintf",
    # glibc internal aliases (a static link can bind these directly)
    "__libc_malloc",
    "__libc_calloc",
    "__libc_realloc",
    "__libc_free",
    "__libc_memalign",
    # picolibc / newlib reentrant spellings — the ones a Zephyr or bare-metal
    # image actually links, and the ones a libc-name-only list would miss
    "_malloc_r",
    "_calloc_r",
    "_realloc_r",
    "_free_r",
    "_memalign_r",
}

# `__rust_*` (the compiler's allocator ABI), `__rg_*` (what
# `#[global_allocator]` expands to) and `__rdl_*` (the default shim used when
# no `#[global_allocator]` is declared). Matched as a substring — see the
# docstring for why this family, and only this family, gets that.
RUST_ALLOC_RE = re.compile(
    r"__(?:rust|rg|rdl)_"
    r"(?:alloc_error_handler|alloc_zeroed|dealloc|realloc|alloc|oom)"
    r"(?![A-Za-z0-9_])"
)
# `__rust_no_alloc_shim_is_unstable`, which is now `..._v2`. Prefix, so the
# next rename does not silently un-check it.
RUST_SHIM_RE = re.compile(r"__rust_no_alloc_shim_is_unstable")

# Demangled C++ operator names. `nm -C` renders these with the argument list,
# so match on the prefix of the demangled name rather than the whole string.
CXX_DEMANGLED_PREFIXES = (
    "operator new",
    "operator delete",
)
# Itanium mangling, for when no demangler ran. `_Znw`/`_Zna` are new/new[],
# `_Zdl`/`_Zda` are delete/delete[]; the suffix encodes the overload.
CXX_MANGLED_PREFIXES = ("_Znw", "_Zna", "_Zdl", "_Zda")

RTOS_HEAP = {
    # Zephyr
    "k_malloc",
    "k_calloc",
    "k_free",
    "k_aligned_alloc",
    "k_heap_alloc",
    "k_heap_aligned_alloc",
    "k_heap_free",
    "sys_heap_alloc",
    "sys_heap_free",
    # FreeRTOS
    "pvPortMalloc",
    "pvPortCalloc",
    "vPortFree",
    "pvPortMallocStack",
    "vPortFreeStack",
    # ThreadX
    "tx_byte_allocate",
    "tx_byte_release",
    "_txe_byte_allocate",
    "_txe_byte_release",
    "_tx_byte_allocate",
    "_tx_byte_release",
    # NuttX
    "kmm_malloc",
    "kmm_calloc",
    "kmm_realloc",
    "kmm_zalloc",
    "kmm_memalign",
    "kmm_free",
    # ESP-IDF
    "heap_caps_malloc",
    "heap_caps_calloc",
    "heap_caps_realloc",
    "heap_caps_free",
}

FAMILIES = (
    ("c-heap", C_HEAP),
    ("rtos-heap", RTOS_HEAP),
)

# RFC-0034 D6's funnel. Not an allocation symbol to deny — it is the symbol the
# `unified` tier expects everything else to go through.
FUNNEL = {
    "nros_platform_alloc",
    "nros_platform_dealloc",
    "nros_platform_realloc",
}

# `--tier unified`: object paths permitted to reference an allocator directly.
# Each carries the reason, and the reason is PRINTED, so an allowance is
# visible in the green output rather than buried in this file.
PLATFORM_OBJECT_ALLOWLIST = (
    (
        "*nros[-_]platform*",
        "the platform backend itself — RFC-0034 D6 makes this the one funnel",
    ),
    (
        "*zpico_platform_aliases*",
        (
            "zenoh-pico's `z_malloc`/`z_free` shim, which forwards to "
            "`nros_platform_alloc` (verified: its only undefined alloc symbol "
            "is the funnel)"
        ),
    ),
)

# ---------------------------------------------------------------------------
# The book's promises. Kept here rather than in a report so the answer to
# "which claim is backed by a check?" ages with the tool instead of with a
# markdown file. `backed_by` is the image a `heap-free` run would have to pass
# for the claim to stop being a claim; None means no such image is built in any
# lane today.
# ---------------------------------------------------------------------------
BOOK_CLAIMS = (
    {
        "where": "book/src/user-guide/embassy-integration.md:81",
        "text": "The no-alloc contract.",
        "backed_by": None,
        "why_not": "no Embassy example exists in examples/ and no fixtures.toml "
        "row builds one, so there is no image to point the gate at",
    },
    {
        "where": "book/src/user-guide/embassy-integration.md:336",
        "text": "stays fully no-alloc",
        "backed_by": None,
        "why_not": "same — the Embassy path is documented but unbuilt",
    },
    {
        "where": "book/src/internals/dispatch-strategy.md:171",
        "text": "the no-alloc + framework-task-routed contract",
        "backed_by": None,
        "why_not": "the RTIC entries that would carry it "
        "(examples/qemu-arm-baremetal/rust/*-rtic) all enable "
        '`nros` feature "alloc" in their Cargo.toml, so they are '
        "`unified`-tier images, not `heap-free` ones",
    },
    {
        "where": "book/src/concepts/no-std.md:146",
        "text": "The core ParameterServer API works without alloc",
        "backed_by": None,
        "why_not": "an API-shape claim; backing it needs an image that uses "
        "ParameterServer and links no allocator, and no example is "
        "configured that way",
    },
)


NM_LINE = re.compile(r"^(?:([0-9a-fA-F]+))?\s*([A-Za-z?])\s+(.*)$")
VERSIONED = re.compile(r"@+[A-Za-z0-9_.]+$")
ARGLIST = re.compile(r"\(.*\)(?:\s*const)?\s*$")


def pick_nm(path):
    """An `nm` that can read THIS file.

    Same reasoning as `scripts/nros-mem-report.py`: GNU nm is built for one
    target family and refuses a foreign ELF with "File format not recognized",
    which is most of what we want to check here — the images that promise
    no-alloc are the cross-built ones. llvm-nm reads them all, so prefer it.

    Ordering differs from that tool's on purpose: the host `nm` comes SECOND,
    ahead of the cross-prefixed ones. Some binutils builds carry several BFD
    targets, so `riscv64-unknown-elf-nm` happily reads an x86-64 ELF — and then
    demangles Rust v0 symbols not at all, so a real image's ten
    `__rustc::__rust_alloc`-family symbols arrive as `_RNvCs..._12___rust_alloc`
    instead. That is survivable here (the Rust family is matched on the mangled
    spelling too, which is half of why it is matched as a substring) but it
    makes the report unreadable, so pick a demangler that works when one is
    available. A host nm still REFUSES a genuinely foreign ELF, so this
    ordering cannot cost us a cross-built image.
    """
    candidates = ["llvm-nm", "nm", "arm-none-eabi-nm", "riscv64-unknown-elf-nm"]
    tried = []
    for name in candidates:
        tool = shutil.which(name)
        if not tool:
            continue
        tried.append(name)
        probe = subprocess.run([tool, path], capture_output=True, text=True)
        # rc 1 with "no symbols" still means the tool UNDERSTOOD the file; a
        # format rejection is what disqualifies it.
        if probe.returncode == 0 or "no symbols" in probe.stderr.lower():
            return tool
    raise Vacuous(
        f"no usable nm for {path} — tried {', '.join(tried) or 'nothing'}. "
        "Install llvm-nm (llvm package) for cross-built images."
    )


class Vacuous(Exception):
    """The check could not be run. Never reported as a pass — exit 2."""


def mtime(path):
    """When the artifact was built, so a stale verdict is visible as stale."""
    try:
        return datetime.datetime.fromtimestamp(os.path.getmtime(path)).isoformat(
            " ", "seconds"
        )
    except OSError:
        return "unknown"


def normalise(name):
    """Whole symbol name, minus any `@GLIBC_2.2.5` / `@@GLIBC_2.2.5` suffix."""
    return VERSIONED.sub("", name.strip())


def classify(name):
    """The allocation family a symbol belongs to, or None.

    Whole-name for `c-heap`/`rtos-heap` (a substring `free` would report
    `freeaddrinfo`), last-path-segment as well, so a demangled Rust or C++
    path does not hide a bare libc name, and substring for the Rust allocator
    ABI, whose spellings the compiler decorates.
    """
    n = normalise(name)
    # `nm -C` renders a C++ symbol with its argument list, so `std::free`
    # arrives as `std::free(void*)`. Drop the parameter list before matching,
    # or the namespace check above is defeated by the signature.
    base = ARGLIST.sub("", n).strip()
    leaf = base.rsplit("::", 1)[-1]
    for family, members in FAMILIES:
        if base in members or leaf in members:
            return family
    if RUST_ALLOC_RE.search(n) or RUST_SHIM_RE.search(n):
        return "rust-alloc-shim"
    if base.startswith(CXX_DEMANGLED_PREFIXES) or leaf.startswith(
        CXX_DEMANGLED_PREFIXES
    ):
        return "cxx-operator-new"
    if n.startswith(CXX_MANGLED_PREFIXES):
        return "cxx-operator-new"
    return None


def read_symbols(path):
    """[(type_letter, name)] for every symbol, DEFINED AND UNDEFINED.

    Undefined ones are the whole point and are the one thing
    `nros-mem-report.py` deliberately drops: it passes `--size-sort`, which
    omits sizeless symbols, and a call into libc's `malloc` is precisely a
    sizeless `U malloc`. A no-alloc gate that only looked at defined symbols
    would pass every dynamically linked image in the tree.
    """
    tool = pick_nm(path)
    syms = []
    for extra in ([], ["-D"]):
        out = subprocess.run(
            [tool, "-C", "--radix=d", *extra, path], capture_output=True, text=True
        )
        for line in out.stdout.splitlines():
            m = NM_LINE.match(line)
            if not m:
                continue
            _addr, typ, name = m.groups()
            syms.append((typ, name.strip()))
        if syms:
            return syms
        # Nothing from the static table — a stripped binary still has a dynamic
        # one, so try that before giving up.
    raise Vacuous(
        f"{path} has no readable symbol table ({tool} found none, with or "
        "without -D). A stripped image cannot be checked, and reporting it "
        "clean would be a lie — build with symbols, or check the pre-strip "
        "artifact."
    )


def check_image(path, tier, allows):
    """tier heap-free: no allocation symbol may appear, defined or referenced."""
    syms = read_symbols(path)
    findings = []
    funnel_present = False
    for typ, name in syms:
        n = normalise(name)
        if n in FUNNEL and typ not in "Uw":
            funnel_present = True
        family = classify(name)
        if not family:
            continue
        if n in allows:
            continue
        findings.append(
            {
                "symbol": n,
                "family": family,
                # `U`/`w` = the image CALLS it and the definition comes from
                # outside; anything else = the image CONTAINS it. Both are
                # violations at `heap-free`, and the distinction is what tells
                # an operator whether to look at a dependency or at the link.
                "linkage": "referenced" if typ in "Uw" else "defined",
                "nm_type": typ,
            }
        )
    findings.sort(key=lambda f: (f["family"], f["symbol"]))
    return {
        "path": os.path.relpath(path, ROOT) if path.startswith(ROOT) else path,
        "tier": tier,
        "built": mtime(path),
        "symbols_read": len(syms),
        "funnel_present": funnel_present,
        "allowed": sorted(allows),
        "findings": findings,
    }


def expand_objects(paths):
    """Every `.o` and `.a` under the given files/dirs, sorted, deduped.

    The caller passes a BUILD directory (a cargo `build/<pkg>-*/out`, a cmake
    object dir). Object files are untracked build output, so `git ls-files`
    cannot see them and a walk is the only way to enumerate them --- which is
    the case check-no-tracked-file-find explicitly permits, provided the walk is
    scoped to a build dir rather than to packages/ or examples/. Scope is the
    caller's: pass an output directory, never a source tree.
    """
    out = []
    for p in paths:
        if os.path.isdir(p):
            # walk-ok: enumerating UNTRACKED .o/.a build output under a
            # caller-supplied build dir; git ls-files cannot see these.
            for base, _dirs, files in os.walk(p):
                for f in files:
                    if f.endswith((".o", ".a", ".obj", ".lib")):
                        out.append(os.path.join(base, f))
        else:
            out.append(p)
    return sorted(set(out))


def dedupe_archive_members(rows):
    """Collapse `foo.o` and `libx.a:foo.o` from the SAME directory.

    A cargo `out/` dir holds both the loose objects and the archive built from
    them, so every finding is reported twice and the operator reads twenty
    violations where there are ten. Keyed on the containing directory as well
    as the member basename, so two different archives that happen to contain a
    `platform.o` each are still reported separately — merging those would be a
    false negative, which is the one failure mode this gate cannot afford.
    """
    seen = {}
    for r in rows:
        member = r["object"].rsplit(":", 1)[-1]
        holder = r["object"].rsplit(":", 1)[0] if ":" in r["object"] else r["object"]
        key = (
            os.path.dirname(holder),
            os.path.basename(member),
            r["family"],
            r["symbol"],
        )
        # Prefer the archive spelling: it is the artifact the link consumes.
        if key not in seen or ":" in r["object"]:
            seen[key] = r
    return list(seen.values())


def object_allowance(member):
    for pattern, reason in PLATFORM_OBJECT_ALLOWLIST:
        if fnmatch.fnmatch(member, pattern):
            return reason
    return None


def check_objects(paths, allows):
    """tier unified: only platform objects may reference an allocator.

    Reads UNDEFINED symbols per object, which is the reference edge a linked
    image no longer has. `nm -o` prefixes each line with `<archive>:<member>:`,
    so the object identity survives into the report.
    """
    files = expand_objects(paths)
    if not files:
        raise Vacuous(
            f"--objects matched no .o/.a under {', '.join(paths)}. A check with "
            "nothing to check reads as coverage; build the fixture first."
        )
    findings = []
    allowed_rows = []
    scanned = 0
    for f in files:
        try:
            tool = pick_nm(f)
        except Vacuous:
            continue
        out = subprocess.run(
            [tool, "-o", "-C", "--undefined-only", f], capture_output=True, text=True
        )
        if not out.stdout.strip():
            continue
        scanned += 1
        for line in out.stdout.splitlines():
            # `<path>[:<member>]:<addr?> U <name>`  — split off the trailing
            # ` U <name>` first, everything before it is the object identity.
            m = re.match(r"^(.*?):\s*([Uw])\s+(\S.*)$", line)
            if not m:
                continue
            member, typ, name = m.groups()
            n = normalise(name)
            family = classify(name)
            if not family or n in allows:
                continue
            member = member.rstrip(":").rstrip()
            reason = object_allowance(member)
            row = {
                "object": os.path.relpath(member, ROOT)
                if member.startswith(ROOT)
                else member,
                "symbol": n,
                "family": family,
                "nm_type": typ,
            }
            if reason:
                row["allowed_because"] = reason
                allowed_rows.append(row)
            else:
                findings.append(row)
    if not scanned:
        raise Vacuous(
            f"none of the {len(files)} object(s) yielded a symbol table — "
            "stripped, or not object files at all."
        )
    findings = dedupe_archive_members(findings)
    allowed_rows = dedupe_archive_members(allowed_rows)
    findings.sort(key=lambda r: (r["object"], r["family"], r["symbol"]))
    allowed_rows.sort(key=lambda r: (r["object"], r["family"], r["symbol"]))
    return {
        "path": ", ".join(paths),
        "tier": "unified",
        "newest": max((mtime(f) for f in files), default="unknown"),
        "objects_scanned": scanned,
        "objects_found": len(files),
        "findings": findings,
        "allowed_rows": allowed_rows,
    }


def report_image(res):
    lines = []
    add = lines.append
    add(f"# no-alloc gate — {res['path']}  (tier {res['tier']})")
    add("")
    add(f"artifact built: {res['built']}")
    add(f"symbols read: {res['symbols_read']:,}")
    if not res["findings"]:
        add("")
        add(f"OK — no allocation symbol is present in {res['path']}.")
        return "\n".join(lines)
    by_family = {}
    for f in res["findings"]:
        by_family.setdefault(f["family"], []).append(f)
    add("")
    add(
        f"FAIL — {len(res['findings'])} allocation symbol(s) in an image "
        f"declared tier `{res['tier']}`."
    )
    for family, rows in sorted(by_family.items()):
        add("")
        add(f"  {family}:")
        for r in rows:
            add(f"    {r['nm_type']}  {r['symbol']:<32} ({r['linkage']})")
    add("")
    if res["funnel_present"]:
        add(
            "`nros_platform_alloc` IS defined in this image, so it is a "
            "`unified`-tier image mislabelled `heap-free`, not a bypass."
        )
    else:
        add(
            "`nros_platform_alloc` is NOT defined here, so these reach the "
            "heap outside RFC-0034 D6's funnel entirely."
        )
    return "\n".join(lines)


def report_objects(res):
    lines = []
    add = lines.append
    add(f"# single-funnel gate (tier unified) — {res['path']}")
    add("")
    add(f"newest object: {res['newest']}")
    add(
        f"objects scanned: {res['objects_scanned']:,} of {res['objects_found']:,} found"
    )
    if res["allowed_rows"]:
        add("")
        add("  permitted (platform backend):")
        seen = set()
        for r in res["allowed_rows"]:
            key = (r["object"], r["allowed_because"])
            if key in seen:
                continue
            seen.add(key)
            add(f"    {r['object']}")
            add(f"      — {r['allowed_because']}")
    if not res["findings"]:
        add("")
        add("OK — every allocator reference comes from the platform backend.")
        return "\n".join(lines)
    add("")
    add(
        f"FAIL — {len(res['findings'])} allocator reference(s) bypass "
        "`nros_platform_alloc` (RFC-0034 D6, issue 0817's class):"
    )
    add("")
    by_obj = {}
    for r in res["findings"]:
        by_obj.setdefault(r["object"], []).append(r)
    for obj, rows in sorted(by_obj.items()):
        add(f"  {obj}")
        for r in rows:
            add(f"    {r['family']:<18} {r['symbol']}")
    return "\n".join(lines)


def report_claims():
    lines = ["# book claims this gate exists to back", ""]
    for c in BOOK_CLAIMS:
        state = c["backed_by"] or "UNBACKED"
        lines.append(f"  {c['where']}")
        lines.append(f'    "{c["text"]}"')
        lines.append(f"    {state}")
        if not c["backed_by"]:
            lines.append(f"    reason: {c['why_not']}")
        lines.append("")
    backed = sum(1 for c in BOOK_CLAIMS if c["backed_by"])
    lines.append(f"{backed} of {len(BOOK_CLAIMS)} backed by a built image.")
    lines.append("")
    lines.append(
        "This is a LISTING, not a check — it always exits 0. Wiring it as a "
        "gate would red-line every lane until a no-alloc fixture row exists, "
        "which is issue 0816's remaining half, not this tool's."
    )
    return "\n".join(lines)


def selftest():
    """The gate has to be able to FAIL, or a green means nothing.

    Both directions, plus the two ways it can be vacuous. Runs on synthetic
    symbol tables rather than on a compiled fixture on purpose: a selftest that
    needs a toolchain is a selftest that gets skipped on the host where the
    gate is about to be trusted.
    """

    def img(syms, tier="heap-free", allows=frozenset()):
        # `check_image` minus the `nm` call, so the classification and the
        # verdict are what is under test.
        findings = []
        funnel = False
        for typ, name in syms:
            n = normalise(name)
            if n in FUNNEL and typ not in "Uw":
                funnel = True
            fam = classify(name)
            if fam and n not in allows:
                findings.append(
                    {
                        "symbol": n,
                        "family": fam,
                        "linkage": "referenced" if typ in "Uw" else "defined",
                        "nm_type": typ,
                    }
                )
        return {
            "path": "synthetic",
            "tier": tier,
            "symbols_read": len(syms),
            "funnel_present": funnel,
            "allowed": sorted(allows),
            "findings": findings,
        }

    clean = [("T", "main"), ("t", "nros_core::init"), ("U", "memcpy")]
    assert not img(clean)["findings"], "a genuinely heap-free image must pass"

    # Each family, on its own, must be enough to fail.
    for typ, sym, family in (
        ("U", "malloc", "c-heap"),
        ("U", "free@GLIBC_2.2.5", "c-heap"),
        ("T", "_malloc_r", "c-heap"),
        ("T", "__rust_alloc", "rust-alloc-shim"),
        ("D", "__rust_no_alloc_shim_is_unstable", "rust-alloc-shim"),
        ("T", "__rg_dealloc", "rust-alloc-shim"),
        # The spellings a REAL rustc emits, all four of which a whole-name
        # match missed on this gate's first run against a real binary.
        ("T", "__rustc::__rust_alloc", "rust-alloc-shim"),
        ("t", "__rustc::__rdl_dealloc", "rust-alloc-shim"),
        ("T", "__rustc::__rust_no_alloc_shim_is_unstable_v2", "rust-alloc-shim"),
        ("T", "_RNvCs9wFQrvczXsK_7___rustc12___rust_alloc", "rust-alloc-shim"),
        ("t", "_RNvCs9wFQrvczXsK_7___rustc11___rdl_alloc", "rust-alloc-shim"),
        ("W", "operator new(unsigned long)", "cxx-operator-new"),
        ("W", "operator delete[](void*, std::align_val_t)", "cxx-operator-new"),
        ("U", "std::free(void*)", "c-heap"),
        ("U", "_ZdlPv", "cxx-operator-new"),
        ("U", "k_malloc", "rtos-heap"),
        ("U", "pvPortMalloc", "rtos-heap"),
        ("U", "tx_byte_allocate", "rtos-heap"),
    ):
        res = img(clean + [(typ, sym)])
        assert res["findings"], f"{sym} must FAIL a heap-free image"
        assert res["findings"][0]["family"] == family, (
            f"{sym} must be classified {family}, got {res['findings'][0]['family']}"
        )

    # ... and the near-misses must NOT fire, or the first real run drowns the
    # signal and earns itself a blanket allowlist.
    for sym in (
        "freeaddrinfo",
        "freeifaddrs",
        "z_free",
        "_z_list_free",
        "nros_platform_alloc",
        "nros_platform_dealloc",
        "free_list_head",
        "mallocinfo",
        "core::ptr::drop_in_place",
        "_z_string_preallocate",
        "_z_slice_is_alloced",
        "heapless::pool::Node<T>::new",
    ):
        assert not img(clean + [("U", sym)])["findings"], (
            f"{sym} does not allocate from the general heap and must not fire"
        )

    # linkage is reported, because "we call it" and "we contain it" send an
    # operator to different places.
    assert img(clean + [("U", "malloc")])["findings"][0]["linkage"] == "referenced"
    assert img(clean + [("T", "malloc")])["findings"][0]["linkage"] == "defined"

    # An explicit allowance suppresses exactly one symbol and nothing else.
    two = clean + [("U", "malloc"), ("U", "k_malloc")]
    assert len(img(two, allows={"malloc"})["findings"]) == 1

    # The two vacuous shapes must be exit-2, never a green.
    def raises(fn):
        try:
            fn()
        except Vacuous:
            return True
        return False

    assert raises(
        lambda: check_objects([os.path.join(ROOT, "does-not-exist")], set())
    ), "an --objects path that matches nothing must be vacuous, not clean"
    empty = os.path.join(ROOT, "scripts", "check-no-alloc-image.py")
    assert raises(lambda: read_symbols(empty)), (
        "a file with no symbol table must be vacuous, not clean"
    )

    # And `main()` must turn each of those into the right exit code.
    def quiet(argv):
        buf = io.StringIO()
        with contextlib.redirect_stdout(buf), contextlib.redirect_stderr(buf):
            return main(argv)

    assert quiet([empty]) == 2, "unreadable image must exit 2, not 0"
    assert quiet(["--objects", os.path.join(ROOT, "nope"), "--tier", "unified"]) == 2
    assert quiet([empty, "--tier", "unified"]) == 2, (
        "tier unified on an ELF must refuse, not green"
    )

    print(
        "selftest: ok — clean images pass, all four families fail, "
        "near-miss symbols do not fire, and every vacuous shape exits 2"
    )
    return 0


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("elf", nargs="*", help="built image(s) to check")
    ap.add_argument(
        "--tier",
        choices=["heap-free", "unified"],
        default="heap-free",
        help="heap-free: no allocation symbol at all (default). "
        "unified: allocation only from the platform backend — needs --objects.",
    )
    ap.add_argument(
        "--objects",
        nargs="+",
        default=None,
        help="object files or dirs to scan for the single-funnel rule",
    )
    ap.add_argument(
        "--allow",
        action="append",
        default=[],
        metavar="SYM=REASON",
        help="permit one symbol, with a written reason (required)",
    )
    ap.add_argument("--json", action="store_true", help="machine-readable output")
    ap.add_argument(
        "--claims", action="store_true", help="print the book claims this gate backs"
    )
    ap.add_argument("--selftest", action="store_true", help="prove the check can fail")
    args = ap.parse_args(argv)

    if args.selftest:
        return selftest()
    if args.claims:
        print(report_claims())
        return 0

    allows = set()
    for spec in args.allow:
        if "=" not in spec:
            print(
                f"--allow {spec}: needs a reason (`--allow malloc=<why>`). An "
                "exemption without a written reason is how a gate stops "
                "meaning anything.",
                file=sys.stderr,
            )
            return 2
        allows.add(spec.split("=", 1)[0].strip())

    try:
        if args.tier == "unified":
            if args.elf:
                raise Vacuous(
                    "tier `unified` is a rule about WHICH OBJECT reaches the "
                    "allocator, and a linked image has already discarded that: "
                    "after ld there is one `U malloc` for the whole program. "
                    "Pass --objects <build-dir> instead of an ELF. Returning a "
                    "green here would be a check that cannot fail."
                )
            if not args.objects:
                raise Vacuous("tier `unified` needs --objects")
            results = [check_objects(args.objects, allows)]
        else:
            if args.objects:
                raise Vacuous(
                    "--objects is the `unified` tier's input; pass "
                    "--tier unified with it."
                )
            if not args.elf:
                ap.error("give at least one ELF, or --selftest / --claims")
            missing = [e for e in args.elf if not os.path.exists(e)]
            if missing:
                raise Vacuous(
                    "these images do not exist: "
                    + ", ".join(missing)
                    + ". A gate that finds no candidate image must fail rather "
                    "than pass — build the fixture, then re-run."
                )
            results = [
                check_image(os.path.abspath(e), args.tier, allows) for e in args.elf
            ]
    except Vacuous as exc:
        print(f"check-no-alloc-image: CANNOT CHECK — {exc}", file=sys.stderr)
        return 2

    if args.json:
        print(json.dumps(results if len(results) > 1 else results[0], indent=2))
        return 1 if any(r["findings"] for r in results) else 0

    rc = 0
    for res in results:
        print(report_objects(res) if args.tier == "unified" else report_image(res))
        print("")
        if res["findings"]:
            rc = 1
    return rc


if __name__ == "__main__":
    sys.exit(main())
