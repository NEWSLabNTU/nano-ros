#!/usr/bin/env python3
"""Static-memory report for a built nano-ros image — phase 392 W1, issue 0815.

Phase 392 opened with a table of `nm` output pasted into a markdown file: 27% of
a safety-island image was message buffers, and the largest consumers were pools
the inventory could not price. Every later wave of that campaign is defined as a
saving against those numbers ("W3 requires W1 so the saving is measured rather
than asserted"), so the numbers have to come from a tool that anyone can re-run
against any image, not from a paste that ages.

This is that tool. It reads an ELF's symbol table, attributes every byte of RAM
to an OWNER — a storage role (`[executor storage]`, `[component storage]`), else
the Rust crate, `C++ <namespace>` or the path-less C bucket, decided from the
MANGLING (issue 1147) — and, where a pool declares its arithmetic or the
inventory names the knobs that size it, to a named pool; and it reports the
total against the RAM sections so nothing hides in the gap. The C/C++ executor
storage is priced against the build's own sizes header (`N x
NROS_CPP_EXECUTOR_STORAGE_SIZE`).

Why measured rather than declared
---------------------------------
`scripts/gen-pool-inventory.py` prices a pool from a `// nros-pool:` comment
evaluated at the knobs' defaults. That works for a pool of BYTES and stops at a
pool of STRUCTS, for three independent reasons, all of them the same reason:

  * `SERVICE_BUFFERS` is `ZPICO_MAX_SESSIONS * ZPICO_MAX_QUERYABLES *
    sizeof(ServiceBuffer)`, and `ZPICO_MAX_QUERYABLES` has a COMPUTED default —
    there is no integer to put in the comment.
  * `MESSAGE_INFO_TABLE`'s element gains three fields under `alloc` +
    `safety-e2e`, so a constant would be right for one build and wrong for the
    rest. Issue 0739 declined to annotate it for exactly that reason, and was
    right to.
  * `__nros_comp_buf_N` is emitted by codegen as `sizeof(component class)`.

The size is known to the COMPILER, not to a comment. So read it from the
compiler's output. A hand-written figure in a comment is also the drift class
this tree already has gates against (`check-ffi-struct-mirrors`): it is correct
until someone appends a field.

The two instruments compose. `--check` joins the declared arithmetic to the
measured symbol and asserts they agree on an image built at knob defaults, which
turns the inventory's numbers from a claim into a checked fact.

What this tool cannot see
-------------------------
It reports STORAGE, never REFERENCES. Only SIZED symbols are read, and
an undefined reference into libc — the `U malloc` that proves an image reaches
the heap — is exactly a sizeless symbol. So a green report here says nothing
about whether the image allocates. That question has its own tool:
`scripts/check-no-alloc-image.py` (issue 0816), which reads the symbol table
for every symbol, sized or not, for that reason.

The unattributed gap in the report is the other half of the same honesty: symbols
never sum to the section size, so the difference is printed rather than left for
someone to discover when their budget comes up short.

Usage
-----
    scripts/nros-mem-report.py <elf> [<elf>...]      # human report
    scripts/nros-mem-report.py <elf> --json          # machine readable
    scripts/nros-mem-report.py <elf> --check         # declared == measured
    scripts/nros-mem-report.py <elf> --baseline b.json   # deltas vs a baseline

`--check` assumes the image was built at knob DEFAULTS; point it at a fixture,
not at a board build that tunes them. A pool priced through a knob the image
may DERIVE from its declarations is reported, not failed, when it differs.

`--baseline` joins symbols by a build-independent KEY (issue 1180: `.llvm.<hash>`
and `.N` suffixes and legacy crate hashes stripped), and every symbol that does
not join is listed as new or gone — a row with no annotation is never what an
unmatched symbol looks like. A baseline from before `ram_symbols` existed is
marked incomplete rather than read as proof that a symbol is new.
"""

import argparse
import contextlib
import glob
import importlib.util
import io
import json
import os
import re
import shutil
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# nm type letters. Lowercase is local, uppercase global; the CLASS is the same.
RAM_TYPES = set("bBdDgGsS")  # .bss + .data + small-data variants
ROM_TYPES = set("rR")  # .rodata
TEXT_TYPES = set("tTwWiI")  # .text, weak, indirect

NM_LINE = re.compile(r"^([0-9]+)\s+([0-9]+)\s+(\S)\s+(.*)$")

# The leading `crate::` of a demangled Rust symbol. Handles the plain form
# (`nros_rmw_zenoh::shim::...`) and the qualified one (`<T as Trait>::m`), where
# the first identifier inside the brackets is the one that owns the bytes.
#
# Applied to RUST symbols only (issue 1147). It used to run on every demangled
# name, and `nm -C` demangles Itanium C++ too, so the C++ executor storage —
# `rclcpp::Node::GlobalStorageHolder<0>::storage`, then spelled `nros::Node::…`
# — matched the Rust crate `nros` and was filed under it. A name's LANGUAGE is
# in its MANGLING, which `lang_of` reads before anything is demangled.
CRATE = re.compile(r"\b([a-z][a-z0-9_]*)::")

# ---------------------------------------------------------------------------
# Symbol identity across builds — issue 1180.
#
# A `--baseline` joins two images by symbol, and the spelling of one static is
# not stable across two builds of it:
#
#   * LLVM internalises a symbol under LTO / ThinLTO and appends `.llvm.<hash>`;
#     the hash changes with any input, so `EXECUTOR_BACKING` never met itself
#     and its row printed exactly as an unchanged row prints.
#   * GCC and LLVM number function-local and file-local statics (`events.0`,
#     `RESULT_STASH_LEN.0`) and clone functions (`.constprop.0`, `.isra.0`,
#     `.part.0`, `.lto_priv.0`, `.cold`).
#   * LLVM's v0 demangler prints the suffix as ` (.llvm.123)`; `llvm-cxxfilt`
#     as ` (.0)`.
#   * A LEGACY-mangled Rust symbol carries a `::h<16 hex>` crate hash, and a v0
#     one carries a crate disambiguator — both change with the crate's
#     `-C metadata`. The v0 one never reaches a demangled name; the legacy one
#     does, so it is stripped here.
#
# So the join key is the demangled name with every one of those removed, and
# the full name is kept for DISPLAY. Two symbols that collide on one key (two
# TUs each with a `static events`) are SUMMED and COUNTED, never one silently
# chosen: a key's count changing between images is reported beside its delta.
_DISPLAY_SUFFIX = re.compile(r"\s+\((?:\.[A-Za-z_][A-Za-z0-9_]*)*(?:\.[0-9]+)+\)$")
_RAW_SUFFIX = re.compile(
    r"(?:\.llvm\.[0-9]+|\.(?:constprop|isra|part|lto_priv|cold|clone|localalias)(?:\.[0-9]+)?"
    r"|\.[0-9]+)$"
)
_LEGACY_RUST_HASH = re.compile(r"::h[0-9a-f]{16}$")


def symbol_key(name):
    """The build-independent identity of a symbol name (see above)."""
    key = name.strip()
    while True:
        prev = key
        key = _DISPLAY_SUFFIX.sub("", key)
        key = _RAW_SUFFIX.sub("", key)
        key = _LEGACY_RUST_HASH.sub("", key)
        if key == prev:
            return key


# A legacy Rust symbol is an Itanium-shaped `_ZN…E` whose last component is the
# 17-character crate hash. v0 is unambiguous (`_R`).
_LEGACY_RUST_MANGLED = re.compile(r"^_ZN.*17h[0-9a-f]{16}E$")


def lang_of(raw):
    """`rust`, `cpp` or `c`, from the MANGLED name — issue 1147.

    The demangled text cannot answer this: `nros::Node::storage` is valid as a
    Rust path and as a C++ qualified name, and this tree has a Rust crate AND a
    C++ namespace by that name. The mangling scheme can.
    """
    base = _RAW_SUFFIX.sub("", raw)
    while base != raw:
        raw, base = base, _RAW_SUFFIX.sub("", base)
    if base.startswith("_R"):
        return "rust"
    if _LEGACY_RUST_MANGLED.match(base):
        return "rust"
    if base.startswith("_Z"):
        return "cpp"
    return "c"


# ---------------------------------------------------------------------------
# Storage ROLES — issue 1147.
#
# Some RAM has an owner a reader would name before any crate: the executor's
# storage, which RFC-0002 § 4.4b places with the CALLER — the C entry's
# `__nros_executor_storage`, the C/C++ tier table, the C++ boot storage, the
# Rust boot and tier backings — and the codegen'd component storage. Filed
# under whatever crate or language emitted it, the one pool every embedded
# image is dominated by was spread over `(C / asm / no path)`, a Rust crate
# that does not own it (`nros`, issue 1147), and the user's entry crate.
#
# This is an AUTHORED table, which is the drift class the RMW parity map paid
# for. So it is bound in both directions (`check_storage_roles`): every row
# names the source that DEFINES the symbol and a literal that must still be
# there, and every executor/component storage definition in the tracked tree
# must be matched by some row. A rename moves one side and fails the self-test.
#
# (role, key regex, defining source, literal that defines it)
STORAGE_ROLES = [
    (
        "executor storage",
        r"__nros_executor_storage",
        "packages/cli/nros-cli-core/src/codegen/entry/packs/entry/c/boot_wrapper.jinja",
        "static uint64_t __nros_executor_storage[",
    ),
    (
        "executor storage",
        r"__nros_tier_executor_storage",
        "packages/cli/nros-cli-core/src/codegen/entry/packs/entry/c/boot_wrapper.jinja",
        "static uint64_t __nros_tier_executor_storage[",
    ),
    (
        "executor storage",
        r"__nros_tier_executor_storage",
        "packages/cli/nros-cli-core/src/codegen/entry/packs/entry/cpp/boot_wrapper.jinja",
        "static uint64_t __nros_tier_executor_storage[",
    ),
    (
        "executor storage",
        r"(?:[A-Za-z_][A-Za-z0-9_]*::)*Node::GlobalStorageHolder<\d+>::storage",
        "packages/api/nros-cpp/include/nros/node.hpp",
        "Node::GlobalStorageHolder<N>::storage[NROS_CPP_EXECUTOR_STORAGE_SIZE]",
    ),
    (
        "executor storage",
        r"nros_node::executor::backing::EXECUTOR_BACKING",
        "packages/core/nros-node/build.rs",
        "static mut EXECUTOR_BACKING:",
    ),
    (
        "executor storage",
        r"(?:[A-Za-z_][A-Za-z0-9_]*::)*__NROS_TIER_EXECUTOR_BACKING",
        "packages/core/nros-macros/src/main_macro.rs",
        "static __NROS_TIER_EXECUTOR_BACKING:",
    ),
    (
        "component storage",
        r"__nros_comp_buf_\d+",
        "packages/cli/nros-cli-core/src/codegen/entry/packs/entry/cpp/entry.cpp.jinja",
        "static unsigned char __nros_comp_buf_{{ s.index }}[",
    ),
    (
        "component storage",
        r"(?:[A-Za-z_][A-Za-z0-9_]*::)*_+NROS_COMPONENT_[A-Za-z0-9_]+_SLOT_STORE",
        "packages/core/nros-macros/src/lib.rs",
        '"__NROS_COMPONENT_{}_SLOT_STORE"',
    ),
]

# What a DEFINITION of executor or component storage looks like in source, for
# the reverse half of `check_storage_roles`: every file that defines one must be
# a file the table cites, so a new storage static in a new place cannot land
# under some crate's name again without the table learning of it.
_STORAGE_DEF = re.compile(
    r"\bstatic\b.*(?:executor_storage|EXECUTOR_BACKING\b|_SLOT_STORE|__nros_comp_buf_)"
    r"|GlobalStorageHolder<N>::storage\["
    r'|format_ident!\(\s*"[^"]*_SLOT_STORE"'
)
_ROLE_RES = [(role, re.compile(rf"^{rx}$")) for role, rx, _src, _lit in STORAGE_ROLES]


def role_of(key):
    """The storage role a symbol key plays, or None."""
    for role, rx in _ROLE_RES:
        if rx.match(key):
            return role
    return None


_CPP_SPECIAL = re.compile(
    r"^(?:vtable|VTT|typeinfo|typeinfo name|guard variable|construction vtable|"
    r"reference temporary #?\d*|TLS wrapper function|TLS init function) for "
)


def owner_of(key, lang):
    """Who a reader would say owns these bytes.

    A storage ROLE first (`[executor storage]`); then the Rust crate for a Rust
    symbol; `C++ <namespace>` for a C++ one — so a C++ namespace that shares a
    Rust crate's name can no longer be filed under that crate; and the
    path-less bucket for everything else.
    """
    role = role_of(key)
    if role:
        return f"[{role}]"
    if lang == "rust":
        m = CRATE.search(key)
        if m:
            return m.group(1)
        return "(Rust, no path)"
    if lang == "cpp":
        # `vtable for rclcpp::Node`, `guard variable for …` — the owner is the
        # class the special symbol belongs to.
        key = _CPP_SPECIAL.sub("", key)
        ns = key.split("::", 1)[0] if "::" in key else None
        if ns and re.match(r"^[A-Za-z_][A-Za-z0-9_]*$", ns):
            return f"C++ {ns}"
        return "C++ (global namespace)"
    return "(C / asm / no path)"


def load_inventory():
    """Import gen-pool-inventory for its knob/pool scanner (hyphenated name)."""
    path = os.path.join(ROOT, "scripts", "gen-pool-inventory.py")
    spec = importlib.util.spec_from_file_location("gen_pool_inventory", path)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def rustlib_bin(name):
    """`name` inside the active toolchain's llvm-tools, or None.

    rustup ships llvm-nm/llvm-size under
    `<sysroot>/lib/rustlib/<host>/bin/` and does NOT put them on PATH, so a
    contributor who can cross-build for Cortex-M already has the one tool that
    can read the result and does not know it. Look there before telling anyone
    to install a system LLVM.
    """
    try:
        sysroot = subprocess.run(
            ["rustc", "--print", "sysroot"], capture_output=True, text=True, check=True
        ).stdout.strip()
    except (OSError, subprocess.CalledProcessError):
        return None
    rustlib = os.path.join(sysroot, "lib", "rustlib")
    if not os.path.isdir(rustlib):
        return None
    for host in sorted(os.listdir(rustlib)):
        cand = os.path.join(rustlib, host, "bin", name)
        if os.path.isfile(cand) and os.access(cand, os.X_OK):
            return cand
    return None


def find_tool(name):
    return shutil.which(name) or rustlib_bin(name)


def pick_nm(elf):
    """An `nm` that can read THIS file.

    GNU nm is built for one target family and refuses a foreign ELF with "File
    format not recognized" — which is most of what we want to measure, since the
    images that care about RAM are the cross-built ones. llvm-nm reads them all,
    so prefer it and fall back to the toolchain-prefixed and plain names.
    """
    candidates = ["llvm-nm", "arm-none-eabi-nm", "nm"]
    for name in candidates:
        tool = find_tool(name)
        if not tool:
            continue
        probe = subprocess.run(
            [tool, "--print-size", elf], capture_output=True, text=True
        )
        if probe.returncode == 0:
            return tool
    raise SystemExit(
        f"no usable nm for {elf} — tried {', '.join(candidates)}. "
        "For a cross-built image, add rustup's llvm-tools "
        "(`rustup component add llvm-tools`) — it ships an llvm-nm this script "
        "finds without needing it on PATH."
    )


def demangle_all(raws, tool, elf):
    """Demangled names, one per raw name, in order.

    `llvm-cxxfilt` first: it demangles Itanium C++, legacy Rust AND Rust v0, and
    it keeps going past a `.llvm.<hash>` / `.0` suffix (printing it as
    ` (.0)`), where `llvm-nm -C` 14 leaves such a v0 symbol MANGLED — and a
    mangled `_RNv…ARENA_ADVISORY_DONE.0` is a name no reader recognises and no
    key can strip back to its build-independent form. Without it, `nm -C` over
    the same symbol table, which lists symbols in the same order under `-p`, so
    the two outputs zip line for line.
    """
    cxxfilt = find_tool("llvm-cxxfilt")
    if cxxfilt and raws:
        out = subprocess.run(
            [cxxfilt], input="\n".join(raws) + "\n", capture_output=True, text=True
        )
        lines = out.stdout.splitlines()
        if out.returncode == 0 and len(lines) == len(raws):
            return lines
    out = subprocess.run(
        [tool, "-C", "-p", "--print-size", "--radix=d", elf], capture_output=True, text=True
    )
    names = []
    for line in out.stdout.splitlines():
        m = NM_LINE.match(line)
        if m:
            names.append(m.group(4).strip())
    if len(names) != len(raws):
        # Refuse rather than guess: a mis-zip pairs one symbol's size with
        # another's name, which is a wrong report that looks right.
        raise SystemExit(
            f"{tool}: demangled ({len(names)}) and raw ({len(raws)}) symbol lists "
            f"of {elf} differ in length; install llvm-cxxfilt"
        )
    return names


def make_symbol(size, typ, raw, name):
    return {
        "size": size,
        "type": typ,
        "raw": raw,
        "name": name,
        "key": symbol_key(name),
        "lang": lang_of(raw),
    }


def read_symbols(elf):
    """[{size, type, raw, name, key, lang}] for every sized symbol in the image.

    Read MANGLED first (issue 1147): the mangling says which language owns a
    symbol, and a demangled C++ name can be a perfectly valid Rust path.
    """
    tool = pick_nm(elf)
    out = subprocess.run(
        [tool, "-p", "--print-size", "--radix=d", elf],
        capture_output=True,
        text=True,
    )
    if out.returncode != 0:
        raise SystemExit(f"{tool} failed on {elf}: {out.stderr.strip()}")
    rows = []
    for line in out.stdout.splitlines():
        m = NM_LINE.match(line)
        if not m:
            continue
        _addr, size, typ, raw = m.groups()
        rows.append((int(size), typ, raw.strip()))
    names = demangle_all([r for _, _, r in rows], tool, elf)
    return [make_symbol(size, typ, raw, name) for (size, typ, raw), name in zip(rows, names)]


# `[Nr] Name Type Address Off Size ES Flg Lk Inf Al` — the flag column may be
# empty, so a row is anchored on its three trailing integers.
_SECTION_LINE = re.compile(
    r"^\s*\[\s*\d+\]\s+(\S+)\s+(\S+)\s+[0-9a-fA-F]+\s+[0-9a-fA-F]+\s+([0-9a-fA-F]+)"
    r"\s+[0-9a-fA-F]+\s+([A-Za-z]*)\s+\d+\s+\d+\s+\d+\s*$"
)


def parse_section_headers(text):
    """{section: (bytes, flags)} from `readelf -S -W` output."""
    sections = {}
    for line in text.splitlines():
        m = _SECTION_LINE.match(line)
        if m:
            name, _typ, size, flags = m.groups()
            sections[name] = (int(size, 16), flags)
    return sections


def ram_section_total(sections):
    """Bytes in the sections that occupy RAM: ALLOCATED and WRITABLE, by FLAG.

    Not by NAME. This used to sum `.bss*`/`.data*`/`.sbss*`/`.sdata*`, and a
    Zephyr image keeps much of its RAM elsewhere — a `.noinit."<file>".N`
    section per source file (the kernel heap, every `K_THREAD_STACK_DEFINE`),
    `*_area` iterable sections, `.got` on native_sim. Measured on the
    derived-tiers C++ native_sim image: 355,824 bytes "by section" against
    626,641 attributed to symbols, an unattributed gap of -76.1 % — a NEGATIVE
    figure, which a reader rightly takes as a broken tool and then distrusts
    every other line of. `W` + `A` is what the loader and the linker script
    mean by RAM, whatever a section is called.
    """
    return sum(b for b, f in sections.values() if "A" in f and "W" in f)


def read_sections(elf):
    """{section: (bytes, flags)}, the authoritative totals.

    Symbols never sum to the section size — alignment padding, linker-script
    reservations and symbol-less data all live in the gap. Reporting the gap is
    the point: a campaign that only counts what it can name will keep finding
    the image bigger than its own table.
    """
    tool = find_tool("llvm-readelf") or find_tool("readelf")
    if not tool:
        return {}
    out = subprocess.run([tool, "-S", "-W", elf], capture_output=True, text=True)
    if out.returncode != 0:
        return {}
    return parse_section_headers(out.stdout)


def crate_ident_for(rel, inv):
    """The Rust crate identifier that owns a source file, or None.

    A pool is joined to its symbol by crate AND leaf name, never by leaf alone.
    `SLOTS` is declared in `nros-rmw-cffi`, and `nros_log::early::SLOTS` is a
    different 1,440-byte pool that merely shares the last path segment — joining
    on the leaf reported a 6,752-byte "drift" in an image that contains neither
    the crate nor the pool.
    """
    crate_dir = inv.crate_of(rel)
    manifest = os.path.join(ROOT, crate_dir, "Cargo.toml")
    try:
        with open(manifest, encoding="utf8") as fh:
            for line in fh:
                m = re.match(r'\s*name\s*=\s*"([^"]+)"', line)
                if m:
                    return m.group(1).replace("-", "_")
    except OSError:
        return None
    return None


def newest_source_mtime(crate_dirs):
    """mtime of the newest tracked source under the given crate dirs, or None.

    Scoped to the crates that DECLARE the pools being checked, not to all of
    packages/. A number is only as good as the artifact it came from, and this
    tool is pointed at a path typed by hand -- which the harness's staleness
    probe does not guard, because that probe covers fixtures the harness
    RESOLVES. Issue 0827's first draft was measured on a three-week-old binary
    in `examples/**/target-*/`, the pre-phase-340 layout that
    `build-test-fixtures` no longer writes.

    The scope matters as much as the check. A first version compared against
    every tracked source under packages/, which made an unrelated edit -- two
    C test TUs in `nros-c` -- report a perfectly fresh zenoh fixture as stale.
    A staleness rule that fires on files the artifact does not depend on gets
    switched off, and then it guards nothing.
    """
    if not crate_dirs:
        return None
    patterns = []
    for d in sorted(crate_dirs):
        patterns += [f"{d}/*.rs", f"{d}/*.c", f"{d}/*.cpp", f"{d}/*.h", f"{d}/*.hpp"]
    try:
        files = subprocess.run(
            ["git", "ls-files"] + patterns,
            cwd=ROOT, capture_output=True, text=True, check=True,
        ).stdout.split()
    except (OSError, subprocess.CalledProcessError):
        return None
    newest = None
    for rel in files:
        try:
            m = os.path.getmtime(os.path.join(ROOT, rel))
        except OSError:
            continue
        if newest is None or m > newest:
            newest = m
    return newest


def crate_of(name):
    """The crate a demangled RUST name belongs to (its first `ident::`)."""
    m = CRATE.search(name)
    if m:
        return m.group(1)
    return "(C / asm / no path)"


# ---------------------------------------------------------------------------
# Per-build sizes headers — the executor storage, PRICED (issue 1147).
#
# The C/C++ executor storage is `NROS_CPP_EXECUTOR_STORAGE_SIZE` bytes per
# executor, a figure the BUILD states in its own generated header (the
# phase-0088 sizes-header mirror). So for those symbols this tool can do more
# than measure: it can say HOW MANY executors' worth the image reserved, and
# refuse when the measured size is not a whole number of them — which is how a
# stale header, or a reservation sized by a different build, would show.
SIZES_HEADERS = ("nros_cpp_config_generated.h", "nros_config_generated.h")
_DEFINE_INT = re.compile(r"^\s*#\s*define\s+([A-Z][A-Z0-9_]*)\s+\(?\s*([0-9]+)[uUlL]*\s*\)?\s*$", re.M)


def parse_sizes_header(text):
    return {name: int(v) for name, v in _DEFINE_INT.findall(text)}


def find_sizes_headers(elf):
    """The per-build sizes headers of the build that produced `elf`.

    Searched in the mirror directories a build writes (`nros-{c,cpp}-generated/
    nros/`) at or below the ELF's three nearest ancestors. Cargo `OUT_DIR`
    copies under `…/build/<crate>-<hash>/out/` are skipped: a build directory
    accumulates one per feature set, so finding one there says nothing about
    which this image linked.
    """
    d = os.path.dirname(os.path.abspath(elf))
    for _ in range(3):
        found = []
        for depth in ("", "*", os.path.join("*", "*")):
            for h in SIZES_HEADERS:
                kind = "cpp" if "cpp" in h else "c"
                pat = os.path.join(d, depth, f"nros-{kind}-generated", "nros", h)
                for p in sorted(glob.glob(pat)):
                    if "/out/" not in p and p not in found:
                        found.append(p)
        # The NEAREST ancestor that holds any wins, and the walk stops there:
        # one level further up is the directory holding this build's SIBLINGS
        # (a west workspace's `build-*`), whose headers describe other images.
        if found:
            return found
        d = os.path.dirname(d)
    return []


def read_sizes(paths, elf_mtime):
    """(defines, used_paths, problem) — one agreed value per macro, or a refusal.

    A header written AFTER the image was linked describes a later build, and two
    headers that disagree on a macro name two builds; either way no figure from
    them may price this image, and the reason is reported instead.
    """
    merged, used = {}, []
    for p in paths:
        try:
            with open(p, encoding="utf8") as fh:
                defs = parse_sizes_header(fh.read())
            mt = os.path.getmtime(p)
        except OSError:
            continue
        if elf_mtime is not None and mt > elf_mtime + 1:
            return {}, used, f"{p} is newer than the image, so it describes a later build"
        for k, v in defs.items():
            if k in merged and merged[k] != v:
                return {}, used, f"sizes headers disagree on {k} ({merged[k]} vs {v})"
            merged[k] = v
        used.append(p)
    return merged, used, None


# How each C/C++ executor-storage symbol relates to the per-executor size:
# the entry's `uint64_t [(SIZE + 7) / 8]` rows round up to a word, the C++
# boot storage is `uint8_t [SIZE]` exactly.
_EXEC_SLOT_ROUNDING = [
    (re.compile(r"^__nros_(?:tier_)?executor_storage$"), 8),
    (re.compile(r"^(?:[A-Za-z_][A-Za-z0-9_]*::)*Node::GlobalStorageHolder<\d+>::storage$"), 1),
]


def price_executor_storage(key, measured, sizes):
    """'N x S' for a C/C++ executor storage symbol, or why it cannot be said."""
    per = sizes.get("NROS_CPP_EXECUTOR_STORAGE_SIZE")
    for rx, word in _EXEC_SLOT_ROUNDING:
        if rx.match(key):
            if per is None:
                return None, "no per-build sizes header states NROS_CPP_EXECUTOR_STORAGE_SIZE"
            slot = (per + word - 1) // word * word
            if slot and measured % slot == 0:
                return measured // slot, (
                    f"{measured // slot} x {slot:,} (NROS_CPP_EXECUTOR_STORAGE_SIZE = {per:,})"
                )
            return None, (
                f"MISMATCH: {measured:,} is not a whole number of {slot:,}-byte executors "
                f"(NROS_CPP_EXECUTOR_STORAGE_SIZE = {per:,}) — the header and the image "
                "come from different builds"
            )
    return None, "measured; no per-build header states this backing's per-executor size"


def aggregate_ram(syms):
    """{key: {bytes, count, names, owner, lang}} over the RAM symbols."""
    agg = {}
    for s in syms:
        if s["type"] not in RAM_TYPES:
            continue
        a = agg.get(s["key"])
        if a is None:
            a = agg[s["key"]] = {
                "bytes": 0,
                "count": 0,
                "names": [],
                "owner": owner_of(s["key"], s["lang"]),
                "lang": s["lang"],
            }
        a["bytes"] += s["size"]
        a["count"] += 1
        a["names"].append(s["name"])
    return agg


def analyse_symbols(elf, syms, sections, pools_by_name, measured_pools=None, sizes=None):
    """Everything `analyse` reports, over an already-read symbol table.

    Split from the ELF reading so the self-test can drive it with synthetic
    symbols — the attribution and the baseline join are what is under test,
    not `nm`.
    """
    measured_pools = measured_pools or {}
    sizes_defs, sizes_used, sizes_problem = sizes or ({}, [], None)
    agg = aggregate_ram(syms)
    rom = [s for s in syms if s["type"] in ROM_TYPES]
    text = [s for s in syms if s["type"] in TEXT_TYPES]

    by_owner = {}
    for a in agg.values():
        by_owner[a["owner"]] = by_owner.get(a["owner"], 0) + a["bytes"]

    # Join measured symbols to declared pools by crate AND last path segment:
    # a pool is `nros_rmw_zenoh::shim::service::SERVICE_BUFFERS` in the image
    # and `SERVICE_BUFFERS` in the annotation.
    matched = []
    matched_crate_dirs = set()
    for key, a in agg.items():
        leaf = key.rsplit("::", 1)[-1]
        if leaf not in pools_by_name:
            continue
        expr, declared, err, rel, line, crate, crate_dir, derived = pools_by_name[leaf]
        # Same leaf in a different crate is a different pool.
        if crate is not None and not key.startswith(crate + "::"):
            continue
        if crate_dir:
            matched_crate_dirs.add(crate_dir)
        matched.append(
            {
                "pool": leaf,
                "symbol": key,
                "measured": a["bytes"],
                "declared": declared,
                "formula": expr,
                "unpriced_because": err,
                "declared_at": f"{rel}:{line}",
                "derived": derived,
            }
        )

    # issue 0815 — the statics a knob sizes whose element only the compiler can
    # size. The image states them exactly; the inventory names their knobs.
    knob_pools = []
    for sym, (kind, knob_list) in sorted(measured_pools.items()):
        if sym in agg:
            knob_pools.append(
                {"symbol": sym, "kind": kind, "measured": agg[sym]["bytes"], "knobs": knob_list}
            )

    storage = []
    for key, a in sorted(agg.items(), key=lambda kv: -kv[1]["bytes"]):
        role = role_of(key)
        if not role:
            continue
        executors, priced = (None, None)
        if role == "executor storage":
            if sizes_problem:
                priced = f"not priced: {sizes_problem}"
            else:
                executors, priced = price_executor_storage(key, a["bytes"], sizes_defs)
        storage.append(
            {
                "role": role,
                "symbol": key,
                "bytes": a["bytes"],
                "count": a["count"],
                "executors": executors,
                "priced": priced,
            }
        )

    section_ram = ram_section_total(sections)
    try:
        elf_mtime = os.path.getmtime(elf)
    except OSError:
        elf_mtime = None
    newest_src = newest_source_mtime(matched_crate_dirs)
    stale = elf_mtime is not None and newest_src is not None and newest_src > elf_mtime
    ram_rows = sorted(agg.items(), key=lambda kv: (-kv[1]["bytes"], kv[0]))
    return {
        "elf": os.path.relpath(elf, ROOT) if elf.startswith(ROOT) else elf,
        "elf_mtime": elf_mtime,
        "newest_source_mtime": newest_src,
        "stale": stale,
        "ram_symbol_total": sum(a["bytes"] for a in agg.values()),
        "rodata_symbol_total": sum(s["size"] for s in rom),
        "text_symbol_total": sum(s["size"] for s in text),
        "section_ram_total": section_ram,
        "sections": {k: v[0] for k, v in sections.items()},
        "by_owner": by_owner,
        # The full RAM table, keyed build-independently (issue 1180) — what a
        # later `--baseline` joins against. `top_ram` alone could not say
        # whether a symbol missing from it was GONE or merely ranked 41st.
        "ram_symbols": [
            {"key": k, "bytes": a["bytes"], "count": a["count"], "owner": a["owner"]}
            for k, a in ram_rows
        ],
        "top_ram": [
            {"bytes": a["bytes"], "symbol": a["names"][0], "key": k, "owner": a["owner"]}
            for k, a in ram_rows[:40]
        ],
        "pools": matched,
        "knob_pools": knob_pools,
        "storage": storage,
        "sizes_headers": sizes_used,
        "sizes_problem": sizes_problem,
    }


def analyse(elf, pools_by_name, measured_pools=None, sizes_header=None):
    syms = read_symbols(elf)
    sections = read_sections(elf)
    try:
        elf_mtime = os.path.getmtime(elf)
    except OSError:
        elf_mtime = None
    headers = [sizes_header] if sizes_header else find_sizes_headers(elf)
    sizes = read_sizes(headers, None if sizes_header else elf_mtime)
    return analyse_symbols(elf, syms, sections, pools_by_name, measured_pools, sizes)


def fmt(n):
    return f"{n:,}"


# ---------------------------------------------------------------------------
# `--baseline` — issue 1180.
def _table(res):
    """({key: (bytes, count)}, complete?) for a report or a baseline JSON.

    A baseline written before `ram_symbols` existed carries only `top_ram`, so
    a symbol absent from it may simply have ranked lower: that table is marked
    INCOMPLETE and nothing is called new or gone on its evidence. Keys are
    recomputed from the stored names, so an old baseline's `(.llvm.<hash>)`
    spelling still joins.
    """
    if "ram_symbols" in res:
        return {r["key"]: (r["bytes"], r.get("count", 1)) for r in res["ram_symbols"]}, True
    out = {}
    for r in res.get("top_ram", []):
        k = symbol_key(r["symbol"])
        b, c = out.get(k, (0, 0))
        out[k] = (b + r["bytes"], c + 1)
    return out, False


def compare(res, baseline):
    """The before/after join, with every symbol that did NOT join accounted for.

    A `--baseline` that cannot match a symbol used to print the row exactly as
    an unchanged row prints, so a wave could read a clean before/after out of a
    probe that compared nothing (issue 1180). Here every current key is matched
    (delta, possibly 0), new, or unknown because the baseline is incomplete —
    and every baseline key that matched nothing is listed as gone.
    """
    cur, _ = _table(res)
    base, complete = _table(baseline)
    matched = {k: cur[k][0] - base[k][0] for k in cur if k in base}
    new = {k: cur[k][0] for k in cur if k not in base} if complete else {}
    unknown = [k for k in cur if k not in base] if not complete else []
    gone = {k: base[k][0] for k in base if k not in cur}
    recount = {k: (base[k][1], cur[k][1]) for k in matched if base[k][1] != cur[k][1]}
    owners = {}
    for o in set(res.get("by_owner", {})) | set(baseline.get("by_owner", {})):
        d = res.get("by_owner", {}).get(o, 0) - baseline.get("by_owner", {}).get(o, 0)
        if d:
            owners[o] = d
    return {
        "complete": complete,
        "matched": matched,
        "new": new,
        "unknown": unknown,
        "gone": gone,
        "recount": recount,
        "owners": owners,
        "section_ram": res.get("section_ram_total", 0) - baseline.get("section_ram_total", 0),
        "ram_symbols": res.get("ram_symbol_total", 0) - baseline.get("ram_symbol_total", 0),
        "has_owners": "by_owner" in baseline,
    }


def delta_note(key, cmp):
    """The annotation for one row — never empty when a baseline was given."""
    if key in cmp["matched"]:
        d = cmp["matched"][key]
        return f"  ({d:+,})" if d else "  (=)"
    if key in cmp["new"]:
        return "  (new)"
    return "  (no baseline row — baseline lists only its top symbols)"


def report(res, top, baseline=None):
    lines = []
    add = lines.append
    add(f"# static memory — {res['elf']}")
    add("")
    if res.get("stale"):
        add("!! STALE IMAGE — a tracked source under packages/ is NEWER than this")
        add("   artifact, so every number below describes code that is no longer")
        add("   in the tree. Rebuild before quoting any of it — with the recipe")
        add("   that PRODUCES this artifact, which differs by lane:")
        add("     native fixtures      just build-test-fixtures lane=native")
        add("                          (measure under build/cargo-fixtures/, NOT")
        add("                           examples/**/target-*/ — the pre-phase-340")
        add("                           layout, which nothing rewrites)")
        add("     cross link-check     just rust-rtos-link-check")
        add("")
    cmp = compare(res, baseline) if baseline else None
    ram_sym, ram_sec = res["ram_symbol_total"], res["section_ram_total"]
    add(f"RAM (writable allocated sections): {fmt(ram_sec)} bytes")
    add(f"RAM attributed to symbols:         {fmt(ram_sym)} bytes")
    if ram_sec:
        gap = ram_sec - ram_sym
        add(
            f"unattributed (padding, linker reservations, symbol-less data): "
            f"{fmt(gap)} bytes ({100.0 * gap / ram_sec:.1f}%)"
        )
    add(f"rodata in symbols:                 {fmt(res['rodata_symbol_total'])} bytes")
    add(f"text in symbols:                   {fmt(res['text_symbol_total'])} bytes")
    if cmp:
        add(
            f"vs baseline: section RAM {cmp['section_ram']:+,}, "
            f"symbol RAM {cmp['ram_symbols']:+,}"
        )
    add("")

    add(f"## top {top} RAM symbols")
    add("")
    for row in res["top_ram"][:top]:
        share = 100.0 * row["bytes"] / ram_sec if ram_sec else 0.0
        delta = delta_note(row["key"], cmp) if cmp else ""
        add(f"  {fmt(row['bytes']):>12}  {share:5.1f}%  {row['symbol']}{delta}")
    add("")

    add("## RAM by owner")
    add("")
    add("  Storage roles first (`[executor storage]`, `[component storage]`), then")
    add("  the Rust crate, `C++ <namespace>`, or the path-less C bucket.")
    add("")
    for owner, size in sorted(res["by_owner"].items(), key=lambda kv: -kv[1])[:24]:
        share = 100.0 * size / ram_sec if ram_sec else 0.0
        d = ""
        if cmp and cmp["has_owners"]:
            od = cmp["owners"].get(owner, 0)
            d = f"  ({od:+,})" if od else "  (=)"
        add(f"  {fmt(size):>12}  {share:5.1f}%  {owner}{d}")
    add("")

    add("## executor and component storage")
    add("")
    if not res["storage"]:
        add("  (no executor or component storage symbol in this image)")
    for s in res["storage"]:
        many = f" (x{s['count']} symbols)" if s["count"] > 1 else ""
        add(f"  {fmt(s['bytes']):>12}  {s['role']:<17}  {s['symbol']}{many}")
        if s["priced"]:
            add(f"  {'':>12}  {'':<17}  = {s['priced']}")
    if res.get("sizes_headers"):
        for h in res["sizes_headers"]:
            add(f"  sizes header: {h}")
    add("")

    add("## declared pools, measured")
    add("")
    if not res["pools"]:
        add("  (no annotated pool is linked into this image)")
    for p in res["pools"]:
        if p["declared"] is None:
            add(
                f"  {fmt(p['measured']):>12}  {p['pool']}  "
                f"— measured only; not priceable statically ({p['unpriced_because']})"
            )
        elif p["declared"] == p["measured"]:
            add(f"  {fmt(p['measured']):>12}  {p['pool']}  — agrees with `{p['formula']}`")
        elif p.get("derived"):
            add(
                f"  {fmt(p['measured']):>12}  {p['pool']}  — builtin "
                f"{fmt(p['declared'])}; this image DERIVED its own (`{p['formula']}`)"
            )
        else:
            add(
                f"  {fmt(p['measured']):>12}  {p['pool']}  — DECLARED "
                f"{fmt(p['declared'])} at defaults, formula `{p['formula']}`"
            )
    add("")

    add("## knob-sized pools, measured (scripts/pool-inventory-knobs.txt)")
    add("")
    if not res.get("knob_pools"):
        add("  (none of the inventory's knob-sized statics is linked into this image)")
    for p in res.get("knob_pools", []):
        add(f"  {fmt(p['measured']):>12}  {p['symbol']}")
        add(f"  {'':>12}  {p['kind']} sized by {', '.join(p['knobs'])}")

    if cmp:
        add("")
        add("## baseline join")
        add("")
        add(
            f"  matched {len(cmp['matched'])} symbol(s), "
            f"{sum(1 for d in cmp['matched'].values() if d)} changed; "
            f"{len(cmp['new'])} new, {len(cmp['gone'])} gone"
        )
        if not cmp["complete"]:
            add(
                f"  baseline is INCOMPLETE (top symbols only, no `ram_symbols`): "
                f"{len(cmp['unknown'])} current symbol(s) have no row to compare —"
            )
            add("  regenerate it with this version's --json to compare them")
        for title, rows in (("gone", cmp["gone"]), ("new", cmp["new"])):
            if rows:
                add(f"  {title}:")
                for k, b in sorted(rows.items(), key=lambda kv: -kv[1])[:15]:
                    add(f"    {fmt(b):>12}  {k}")
                if len(rows) > 15:
                    add(f"    … and {len(rows) - 15} more")
        for k, (bc, cc) in sorted(cmp["recount"].items()):
            add(f"  {k}: {bc} symbol(s) in the baseline, {cc} now — summed on both sides")
    return "\n".join(lines)


def check(res):
    """Declared arithmetic must equal the measured symbol on a default build."""
    if res.get("stale"):
        print(
            f"check-mem-report: {res['elf']} is STALE — a tracked source is newer\n"
            "than the artifact, so comparing declared arithmetic against it proves\n"
            "nothing about the current tree. Rebuild and re-run."
        )
        return 1
    priced = [p for p in res["pools"] if p["declared"] is not None]
    if not res["pools"]:
        # A check with nothing to check reads as coverage and is not. Every
        # image this is pointed at links a backend, and every backend has at
        # least one annotated pool — so zero MATCHES means the join broke (a
        # renamed symbol, a stripped binary, a demangler that did not run),
        # not that the image is lean.
        print(
            f"check-mem-report: NO annotated pool is linked into {res['elf']} — "
            "the check would be vacuous.\n"
            "Either the image links no RMW backend, or the symbol-to-annotation\n"
            "join broke (stripped binary, renamed pool, demangling off)."
        )
        return 1
    if not priced:
        # MATCHED but not PRICEABLE, which is a different state and not an
        # error. The join worked -- these pools are in the image and were
        # identified -- but their formulas multiply a knob whose default is
        # computed rather than literal, so there is no static number to
        # compare against.
        #
        # Conflating the two cost `queue.yml` every run it ever made. The
        # nuttx talker links exactly `SMALL_PAYLOADS` and `LARGE_PAYLOADS`,
        # both priced through `ZPICO_SUBSCRIBER_RING_DEPTH`, whose default is
        # computed (issue 0829's SYSTEM_DEFAULT sentinel) -- so `priced` was
        # empty and the image was reported as linking no backend, which it
        # plainly does. The freertos talker passes only because it also links
        # `nros_rmw_cffi`'s `SLOTS`, whose formula is literal.
        #
        # Reported, not silent: an unpriceable pool is a gap in what this tool
        # can assert, and a lane that prints nothing about it invites the same
        # misreading from the other direction.
        print(
            f"check-mem-report: {len(res['pools'])} annotated pool(s) linked into "
            f"{res['elf']}, none PRICEABLE.\n"
            "The join worked -- these are the pools, measured -- but every formula\n"
            "multiplies a knob with a computed default, so there is no static\n"
            "number to check the arithmetic against:\n"
        )
        for p in res["pools"]:
            print(f"  {fmt(p['measured']):>12}  {p['pool']}  ({p['declared_at']})")
            print(f"                formula   {p['formula']}")
            print(f"                unpriced  {p['unpriced_because']}")
        print(
            "\nNot a failure: the check asserts arithmetic it cannot compute here.\n"
            "Give one of these knobs a literal default to make the pool checkable."
        )
        return 0
    bad = [p for p in priced if p["declared"] != p["measured"] and not p.get("derived")]
    # issue 0815 — a pool priced through a knob the build may DERIVE from the
    # image's declarations (`env_usize_rung(name, declared, builtin)`) is priced
    # at its builtin, which is what an image that declares nothing pays. An
    # image that declares its entities is entitled to a different figure, so a
    # mismatch there is reported and is not drift. Before the scan learned that
    # reader spelling these pools were not priced at all, so this tolerates
    # nothing the gate used to catch.
    rederived = [p for p in priced if p["declared"] != p["measured"] and p.get("derived")]
    for p in rederived:
        print(
            f"check-mem-report: {p['pool']} measures {fmt(p['measured'])} against a "
            f"builtin of {fmt(p['declared'])} — its knobs are derived per image, so "
            "this is the image's own sizing, not drift"
        )
    if not bad:
        print(
            f"check-mem-report: {len(priced) - len(rederived)} declared pool(s) agree with "
            f"{res['elf']}"
        )
        return 0
    print(f"check-mem-report: {len(bad)} declared pool(s) disagree with the image\n")
    for p in bad:
        print(f"  {p['pool']}  ({p['declared_at']})")
        print(f"    formula   {p['formula']}")
        print(f"    declared  {fmt(p['declared'])} bytes at knob defaults")
        print(f"    measured  {fmt(p['measured'])} bytes in {res['elf']}")
        print("")
    print(
        "Either the formula drifted from the type it prices (a field was\n"
        "appended, or an element size changed), or this image was NOT built at\n"
        "knob defaults. `--check` assumes defaults; point it at a fixture."
    )
    return 1


def selftest():
    """The gate has to be able to FAIL, or a green means nothing.

    Same reasoning as the other generated-page checks in this tree: an
    always-passing check is worse than no check, because it reads as coverage.
    """
    agreeing = {
        "elf": "synthetic",
        "pools": [
            {
                "pool": "P",
                "declared": 1024,
                "measured": 1024,
                "formula": "K * 8",
                "declared_at": "x.rs:1",
            }
        ],
    }
    drifted = {
        "elf": "synthetic",
        "pools": [
            {
                "pool": "P",
                "declared": 1024,
                "measured": 2048,
                "formula": "K * 8",
                "declared_at": "x.rs:1",
            }
        ],
    }
    unpriceable = {
        "elf": "synthetic",
        "pools": [
            {
                "pool": "P",
                "declared": None,
                "measured": 2048,
                "formula": "K * SZ",
                "declared_at": "x.rs:1",
                "unpriced_because": "knob `K` has a computed default",
            }
        ],
    }
    empty = {"elf": "synthetic", "pools": []}
    stale = {
        "elf": "synthetic",
        "stale": True,
        "pools": [
            {
                "pool": "P",
                "declared": 1024,
                "measured": 1024,
                "formula": "K * 8",
                "declared_at": "x.rs:1",
            }
        ],
    }

    def quiet(case):
        # The synthetic failures print their full operator report. Swallow it:
        # a gate whose green output contains four fake failure reports is a
        # gate whose red nobody will spot.
        buf = io.StringIO()
        with contextlib.redirect_stdout(buf):
            return check(case)

    # An unpriceable pool BESIDE a drifted one must still fail on the drift:
    # the tolerance is for "nothing to compare", never for "did not compare".
    mixed = {
        "elf": "synthetic",
        "pools": [unpriceable["pools"][0], drifted["pools"][0]],
    }

    assert quiet(agreeing) == 0, "agreeing pool must pass"
    assert quiet(drifted) == 1, "drifted pool must FAIL"
    assert quiet(unpriceable) == 0, (
        "a pool that MATCHED but cannot be priced is not a failure — the join "
        "worked; there is simply no static number to compare. Conflating this "
        "with the empty case is what kept queue.yml red on every run"
    )
    assert quiet(mixed) == 1, "an unpriceable pool must not mask a drifted one"
    assert quiet(empty) == 1, "a vacuous check must FAIL, not read as coverage"
    assert quiet(stale) == 1, "an agreeing pool on a STALE image must still FAIL"
    rederived = {
        "elf": "synthetic",
        "pools": [dict(drifted["pools"][0], derived=True)],
    }
    assert quiet(rederived) == 0, (
        "a pool whose knobs the image DERIVED (the declared road) is entitled to a "
        "figure other than the builtin; that is not drift"
    )
    assert quiet({"elf": "synthetic", "pools": [rederived["pools"][0], drifted["pools"][0]]}) == 1, (
        "a derived pool must not mask a drifted one"
    )
    selftest_keys()
    selftest_attribution()
    selftest_baseline()
    selftest_sections()
    check_storage_roles()
    print("selftest: ok — the check passes on agreement and fails on drift")
    return 0


def selftest_keys():
    """issue 1180 — one static, two builds, one key; distinct statics stay apart."""
    same = [
        ("nros_node::executor::backing::EXECUTOR_BACKING (.llvm.12488621184790092911)",
         "nros_node::executor::backing::EXECUTOR_BACKING (.llvm.13033674138034542000)"),
        ("_RNvNtNtCs1_9nros_node8executor7backing16EXECUTOR_BACKING.llvm.1",
         "_RNvNtNtCs1_9nros_node8executor7backing16EXECUTOR_BACKING.llvm.2"),
        ("events.0", "events.3"),
        ("nros_cpp::action::RESULT_STASH_LEN (.0)", "nros_cpp::action::RESULT_STASH_LEN"),
        ("encode.constprop.0.isra.0", "encode"),
        ("core::fmt::write::h0123456789abcdef", "core::fmt::write::hfedcba9876543210"),
    ]
    for a, b in same:
        assert symbol_key(a) == symbol_key(b), f"{a!r} and {b!r} must join: {symbol_key(a)!r}"
    for a, b in [("SLOTS", "SLOTS_2"), ("buf_0", "buf_1"), ("x::h12", "x::h13")]:
        assert symbol_key(a) != symbol_key(b), f"{a!r} and {b!r} are different statics"


def selftest_attribution():
    """issue 1147 — the LANGUAGE comes from the mangling, the OWNER from the role."""
    cases = [
        # (raw, demangled, expected owner)
        ("_ZN6rclcpp4Node19GlobalStorageHolderILi0EE7storageE",
         "rclcpp::Node::GlobalStorageHolder<0>::storage", "[executor storage]"),
        # The exact mis-filing 1147 reported: a C++ `nros::` name and a Rust
        # crate named `nros`. Same text, different owners.
        ("_ZN4nros4Node19GlobalStorageHolderILi0EE7storageE",
         "nros::Node::GlobalStorageHolder<0>::storage", "[executor storage]"),
        ("_ZN4nros6detail5stashE", "nros::detail::stash", "C++ nros"),
        ("_RNvNtCs1_4nros3env9ENV_CACHE", "nros::env::ENV_CACHE", "nros"),
        ("__nros_tier_executor_storage", "__nros_tier_executor_storage", "[executor storage]"),
        ("__nros_executor_storage", "__nros_executor_storage", "[executor storage]"),
        ("_RNvNtNtCs1_9nros_node8executor7backing16EXECUTOR_BACKING.llvm.7",
         "nros_node::executor::backing::EXECUTOR_BACKING (.llvm.7)", "[executor storage]"),
        ("_RNvNvCs1_12native_entry16___nros_entry_run28___NROS_TIER_EXECUTOR_BACKING",
         "native_entry::__nros_entry_run::__NROS_TIER_EXECUTOR_BACKING", "[executor storage]"),
        ("__nros_comp_buf_3", "__nros_comp_buf_3", "[component storage]"),
        ("_RNvCs1_8ctrl_pkg36___NROS_COMPONENT_ctrl_pkg_SLOT_STORE",
         "ctrl_pkg::___NROS_COMPONENT_ctrl_pkg_SLOT_STORE", "[component storage]"),
        ("_ZN4core3fmt5write17h0123456789abcdefE", "core::fmt::write::h0123456789abcdef", "core"),
        ("_ZTVN6rclcpp4NodeE", "vtable for rclcpp::Node", "C++ rclcpp"),
        ("_ZL12__nros_tiers", "__nros_tiers", "C++ (global namespace)"),
        ("g_sessions", "g_sessions", "(C / asm / no path)"),
    ]
    for raw, name, want in cases:
        got = owner_of(symbol_key(name), lang_of(raw))
        assert got == want, f"{name!r} ({raw}) attributed to {got!r}, want {want!r}"


def _res(rows, owners=None, complete=True):
    res = {
        "section_ram_total": sum(b for _, b in rows),
        "ram_symbol_total": sum(b for _, b in rows),
        "by_owner": owners or {},
        "top_ram": [{"symbol": n, "bytes": b} for n, b in rows],
    }
    if complete:
        agg = {}
        for n, b in rows:
            k = symbol_key(n)
            pb, pc = agg.get(k, (0, 0))
            agg[k] = (pb + b, pc + 1)
        res["ram_symbols"] = [{"key": k, "bytes": b, "count": c} for k, (b, c) in agg.items()]
    return res


def selftest_baseline():
    """issue 1180's positive control: a `--baseline` that cannot fail is the bug."""
    eb = "nros_node::executor::backing::EXECUTOR_BACKING"
    before = _res([(f"{eb} (.llvm.12488621184790092911)", 86216), ("GONE_POOL", 512)])
    after = _res([(f"{eb} (.llvm.13033674138034542000)", 98504), ("NEW_POOL", 64)])
    cmp = compare(after, before)
    assert cmp["matched"] == {eb: 12288}, (
        f"the measured +12,288 B must survive a changed `.llvm.` hash: {cmp['matched']}"
    )
    assert delta_note(eb, cmp) == "  (+12,288)"
    assert cmp["gone"] == {"GONE_POOL": 512} and cmp["new"] == {"NEW_POOL": 64}, (
        "a symbol on one side only must be REPORTED (issue 1125 lost a 131,072-byte "
        "pool this way), never dropped"
    )
    same = compare(_res([(f"{eb} (.llvm.2)", 86216)]), _res([(f"{eb} (.llvm.1)", 86216)]))
    assert same["matched"] == {eb: 0} and delta_note(eb, same) == "  (=)", (
        "equal bytes must print as MATCHED-AND-EQUAL, distinguishable from no match"
    )
    # An old baseline carries only `top_ram`: nothing may be called new from it.
    old = compare(after, _res([(f"{eb} (.llvm.1)", 86216)], complete=False))
    assert old["matched"] == {eb: 12288} and old["new"] == {} and old["unknown"] == ["NEW_POOL"]
    assert "no baseline row" in delta_note("NEW_POOL", old)
    # Two statics that collide on a key are summed AND counted.
    two = compare(_res([("events.0", 10), ("events.1", 20)]), _res([("events.0", 10)]))
    assert two["matched"] == {"events": 20} and two["recount"] == {"events": (1, 2)}


def selftest_sections():
    """RAM sections by FLAG — a Zephyr `.noinit` section is RAM whatever its name."""
    text = (
        "  [ 1] .text             PROGBITS        00000000 001000 000100 00  AX  0   0  4\n"
        "  [ 2] .rodata           PROGBITS        00000100 001100 000040 00   A  0   0  4\n"
        "  [ 3] .data             PROGBITS        20000000 002000 000010 00  WA  0   0  4\n"
        "  [ 4] .bss              NOBITS          20000010 002010 000020 00  WA  0   0  8\n"
        '  [ 5] .noinit."x.c".0   NOBITS          20000030 002010 000400 00  WA  0   0  8\n'
        "  [ 6] .comment          PROGBITS        00000000 002010 000059 01  MS  0   0  1\n"
        "  [ 7] .symtab           SYMTAB          00000000 002070 000100 10      8   9  4\n"
    )
    secs = parse_section_headers(text)
    assert set(secs) == {".text", ".rodata", ".data", ".bss", '.noinit."x.c".0', ".comment",
                         ".symtab"}, secs
    assert ram_section_total(secs) == 0x10 + 0x20 + 0x400, (
        "RAM is every ALLOCATED + WRITABLE section; a name filter missed `.noinit` and "
        "reported a NEGATIVE unattributed gap on Zephyr"
    )


def check_storage_roles():
    """`STORAGE_ROLES` against the tree, BOTH directions (the parity-map lesson).

    Forward: every row's defining literal is still in the file it cites. Reverse:
    every tracked line that DEFINES executor or component storage is in a file
    some row cites — so a new storage static in a new place fails here instead
    of being filed under a crate.
    """
    cited = set()
    for role, _rx, src, literal in STORAGE_ROLES:
        cited.add(src)
        try:
            with open(os.path.join(ROOT, src), encoding="utf8") as fh:
                text = fh.read()
        except OSError:
            raise AssertionError(f"STORAGE_ROLES cites {src}, which does not exist")
        assert literal in text, (
            f"STORAGE_ROLES ({role}): {src} no longer contains {literal!r} — the "
            "storage was renamed or moved, so mem-report would file it under a crate "
            "again (issue 1147)"
        )
    # A plain alternation for git (its ERE has no `(?:`), the real pattern in
    # Python: a regex git cannot parse makes `git grep` print nothing, and an
    # empty scan would pass this check vacuously.
    out = subprocess.run(
        ["git", "grep", "-n", "-E",
         "executor_storage|EXECUTOR_BACKING|_SLOT_STORE|__nros_comp_buf_|GlobalStorageHolder",
         "--", "*.rs", "*.c", "*.h", "*.hpp", "*.cpp", "*.jinja",
         ":!*.golden", ":!third-party", ":!scripts/nros-mem-report.py"],
        cwd=ROOT, capture_output=True, text=True,
    )
    assert out.returncode == 0, f"git grep failed: {out.stderr.strip()}"
    defining = set()
    for line in out.stdout.splitlines():
        path, _n, body = line.split(":", 2)
        stripped = body.strip()
        if stripped.startswith(("//", "*", "/*", "#", "\"")) or "assert" in body or "contains(" in body:
            continue
        if not _STORAGE_DEF.search(body):
            continue
        defining.add(path)
        assert path in cited, (
            f"{path} defines executor/component storage ({stripped[:80]!r}) but no "
            "STORAGE_ROLES row cites it — add a row, or mem-report files these bytes "
            "under whichever crate emitted them (issue 1147)"
        )
    # And the other way: a cited file in which the scan finds NO definition is
    # either a stale row or a scan that stopped seeing — both make this check
    # vacuous for that row.
    assert cited <= defining, (
        f"STORAGE_ROLES cites {sorted(cited - defining)}, where the definition scan "
        "finds no storage definition — the row is stale or `_STORAGE_DEF` lost it"
    )


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("elf", nargs="*", help="built image(s) to measure")
    ap.add_argument("--top", type=int, default=25, help="how many RAM symbols to list")
    ap.add_argument("--json", action="store_true", help="machine-readable output")
    ap.add_argument(
        "--check",
        action="store_true",
        help="assert declared pool arithmetic equals the measured symbol",
    )
    ap.add_argument("--baseline", help="a --json file to show deltas against")
    ap.add_argument(
        "--sizes-header",
        help="the build's nros_cpp_config_generated.h, when it cannot be found "
        "beside the image; prices the C/C++ executor storage",
    )
    ap.add_argument(
        "--selftest", action="store_true", help="prove the check can fail"
    )
    args = ap.parse_args()

    if args.selftest:
        return selftest()
    # Always, not only behind the flag (phase-472 W9): a negative control
    # nobody runs decays into a comment. Quiet on success.
    with contextlib.redirect_stdout(io.StringIO()) as _selftest_out:
        _selftest_rc = selftest()
    if _selftest_rc:
        sys.stdout.write(_selftest_out.getvalue())
        return _selftest_rc
    if not args.elf:
        ap.error("give at least one ELF, or --selftest")

    inv = load_inventory()
    knobs, pools = inv.scan()
    derived = inv.scan_derived()
    pools_by_name = {}
    for name, expr, rel, line in pools:
        b, err = inv.pool_bytes(expr, knobs)
        pools_by_name[name] = (
            expr, b, err, rel, line, crate_ident_for(rel, inv), inv.crate_of(rel),
            any(t.strip() in derived for t in expr.split("*")),
        )
    # issue 0815 — the statics a knob sizes that no formula can price, joined
    # by their full symbol key, so the image states their exact bytes.
    costs, _problems = inv.load_knob_costs()
    measured_pools = {}
    for knob, (kind, symbols, _reason, _line) in costs.items():
        if kind in ("pool", "heap"):
            for sym in symbols:
                measured_pools.setdefault(sym, (kind, []))[1].append(knob)

    baseline = None
    if args.baseline:
        with open(args.baseline, encoding="utf8") as fh:
            baseline = json.load(fh)
        if isinstance(baseline, list):
            baseline = baseline[0]

    results = [
        analyse(os.path.abspath(e), pools_by_name, measured_pools, args.sizes_header)
        for e in args.elf
    ]

    if args.json:
        print(json.dumps(results if len(results) > 1 else results[0], indent=2))
        return 0

    rc = 0
    for res in results:
        if args.check:
            rc |= check(res)
        else:
            print(report(res, args.top, baseline))
            print("")
    return rc


if __name__ == "__main__":
    sys.exit(main())
