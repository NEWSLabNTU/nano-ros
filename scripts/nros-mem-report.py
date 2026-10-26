#!/usr/bin/env python3
"""Static-memory report for a built nano-ros image — phase 392 W1, issue 0815.

Phase 392 opened with a table of `nm` output pasted into a markdown file: 27% of
a safety-island image was message buffers, and the largest consumers were pools
the inventory could not price. Every later wave of that campaign is defined as a
saving against those numbers ("W3 requires W1 so the saving is measured rather
than asserted"), so the numbers have to come from a tool that anyone can re-run
against any image, not from a paste that ages.

This is that tool. It reads an ELF's symbol table, attributes every byte of RAM
to an OWNER — a nano-ros storage role (`[executor storage]`, `[component storage]`,
`[level task memory]`, `[level scheduler]`, `[monitor storage]`, `[platform task
stacks]`), else the Rust crate, `C++ <namespace>` or the path-less C bucket,
decided from the MANGLING (issue 1147) — and, where a pool declares its arithmetic or the
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
sys.path.insert(0, os.path.join(ROOT, "scripts", "lib"))
import git_grep  # noqa: E402  issue 1816 — a failed search is not "no match"

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
        "packages/api/nros-cpp/include/rclcpp/node.hpp",
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
        "static unsigned char __nros_comp_buf_",
    ),
    (
        "component storage",
        r"(?:[A-Za-z_][A-Za-z0-9_]*::)*_+NROS_COMPONENT_[A-Za-z0-9_]+_SLOT_STORE",
        "packages/core/nros-macros/src/lib.rs",
        '"__NROS_COMPONENT_{}_SLOT_STORE"',
    ),
    # Issue 2040 -- a level image's statics (phase-509). The core's one
    # executor is carved from `__nros_levels_backing` (P9b); before P9b each
    # level carried its own `__nros_tier_backing_<i>`.
    (
        "executor storage",
        r"__nros_levels_backing",
        "packages/cli/nros-cli-core/src/codegen/entry/packs/entry/c/boot_wrapper.jinja",
        "static uint64_t __nros_levels_backing[",
    ),
    (
        "executor storage",
        r"__nros_levels_backing",
        "packages/cli/nros-cli-core/src/codegen/entry/packs/entry/cpp/boot_wrapper.jinja",
        "static uint64_t __nros_levels_backing[",
    ),
    (
        "executor storage",
        r"__nros_tier_backing_\d+",
        "packages/cli/nros-cli-core/src/codegen/entry/packs/entry/c/boot_wrapper.jinja",
        "static uint64_t __nros_tier_backing_",
    ),
    (
        "executor storage",
        r"__nros_tier_backing_\d+",
        "packages/cli/nros-cli-core/src/codegen/entry/packs/entry/cpp/boot_wrapper.jinja",
        "static uint64_t __nros_tier_backing_",
    ),
    (
        "level scheduler",
        r"__nros_scheduler_\d+",
        "packages/cli/nros-cli-core/src/codegen/entry/packs/entry/c/boot_wrapper.jinja",
        "static uint64_t __nros_scheduler_0[",
    ),
    (
        "level scheduler",
        r"__nros_scheduler_\d+",
        "packages/cli/nros-cli-core/src/codegen/entry/packs/entry/cpp/boot_wrapper.jinja",
        "static uint64_t __nros_scheduler_0[",
    ),
    # A spawned level's worker: its stack and its thread / task control block,
    # from `NROS_TIER_TASK_MEMORY_DEFINE` in the per-RTOS header.
    (
        "level task memory",
        r"__nros_tier_(?:stack|thread)_\d+",
        "packages/api/nros-c/include/rclc/detail/tier_task_memory_zephyr.h",
        "static K_THREAD_STACK_DEFINE(__nros_tier_stack_##i,",
    ),
    (
        "level task memory",
        r"__nros_tier_(?:stack|tcb)_\d+",
        "packages/api/nros-c/include/rclc/detail/tier_task_memory_freertos.h",
        "static StackType_t __nros_tier_stack_##i[",
    ),
    (
        "level task memory",
        r"__nros_tier_stack_\d+",
        "packages/api/nros-c/include/rclc/detail/tier_task_memory_nuttx.h",
        "static uint8_t __nros_tier_stack_##i[",
    ),
    # The generated entry's contract-monitor tables (rows, ages, hazards, modes).
    (
        "monitor storage",
        r"__nros_(?:mon|age|hz|mode)_storage|__nros_mon_rows",
        "packages/cli/nros-cli-core/src/codegen/entry/packs/entry/c/entry.c.jinja",
        "static uint64_t __nros_mon_storage[",
    ),
    (
        "monitor storage",
        r"__nros_(?:mon|age|hz|mode)_storage|__nros_mon_rows",
        "packages/cli/nros-cli-core/src/codegen/entry/packs/entry/cpp/entry.cpp.jinja",
        "alignas(8) static unsigned char __nros_mon_storage[",
    ),
    # The Zephyr platform's task-stack pool (zenoh-pico's read, lease and
    # tx-flush tasks): CONFIG_NROS_ZEPHYR_TASK_SLOTS stacks of
    # CONFIG_NROS_ZEPHYR_TASK_STACK_SIZE, priced from the image's .config.
    (
        "platform task stacks",
        r"nros_thread_stacks",
        "zephyr/nros_platform_zephyr_shims.c",
        "K_THREAD_STACK_ARRAY_DEFINE(nros_thread_stacks,",
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
    # issue 2040 -- the level image's statics and the platform task stacks.
    r"|\bstatic\b.*\b__nros_(?:levels_backing|tier_backing_|scheduler_|tier_stack_|tier_thread_"
    r"|tier_tcb_|mon_storage|age_storage|hz_storage|mode_storage|mon_rows)"
    r"|STACK_ARRAY_DEFINE(?:_IN)?\(\s*nros_thread_stacks\b"
)
# The same set, as a POSIX ERE `git grep` can parse (it has no `(?:`).
_STORAGE_GREP = (
    "executor_storage|EXECUTOR_BACKING|_SLOT_STORE|__nros_comp_buf_|GlobalStorageHolder"
    "|__nros_levels_backing|__nros_tier_backing_|__nros_scheduler_|__nros_tier_stack_"
    "|__nros_tier_thread_|__nros_tier_tcb_|__nros_mon_storage|__nros_age_storage"
    "|__nros_hz_storage|__nros_mode_storage|__nros_mon_rows|nros_thread_stacks"
)
# Grouped before anchoring: a row's regex may be an alternation.
_ROLE_RES = [(role, re.compile(rf"^(?:{rx})$")) for role, rx, _src, _lit in STORAGE_ROLES]


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
                # `nros/` was the mirror's layout until the headers moved under
                # `rclcpp/detail/` / `rclc/detail/` (phase-501); both are read.
                subdirs = ("nros", os.path.join("rclcpp" if kind == "cpp" else "rclc", "detail"))
                for sub in subdirs:
                    pat = os.path.join(d, depth, f"nros-{kind}-generated", sub, h)
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


# Issue 2040 -- the level statics the generated entry sizes from ONE per-build
# macro each, as `uint64_t [MACRO / 8u]`: a whole number of words, rounded down.
_SIZED_BY_MACRO = [
    (re.compile(r"^__nros_levels_backing$"), "NROS_CPP_LEVELS_BACKING_SIZE"),
    (re.compile(r"^__nros_tier_backing_(\d+)$"), "NROS_CPP_TIER_BACKING_SIZE_{}"),
    (re.compile(r"^__nros_scheduler_(\d+)$"), "NROS_CPP_SCHEDULER_BACKING_SIZE_{}"),
]


def price_by_macro(key, measured, sizes):
    """What a macro-sized level static was priced at, or None if `key` is not one."""
    for rx, pattern in _SIZED_BY_MACRO:
        m = rx.match(key)
        if not m:
            continue
        macro = pattern.format(*m.groups())
        if macro not in sizes:
            return f"measured; no per-build sizes header states {macro}"
        want = sizes[macro] // 8 * 8
        if measured == want:
            return f"{macro} = {sizes[macro]:,}"
        return (
            f"MISMATCH: {measured:,} bytes, but {macro} = {sizes[macro]:,} reserves "
            f"{want:,} -- the header and the image come from different builds"
        )
    return None


# Issue 2040 -- the Zephyr platform's task-stack pool. The shim defines
# `nros_thread_stacks` as NROS_ZEPHYR_MAX_THREADS (CONFIG_NROS_ZEPHYR_TASK_SLOTS)
# stacks of NROS_ZEPHYR_STACK_SIZE (CONFIG_NROS_ZEPHYR_TASK_STACK_SIZE, 0 =
# CONFIG_MAIN_STACK_SIZE); `K_THREAD_STACK_ARRAY_DEFINE` pads each to the
# architecture's stack alignment and guard, so a slot is never LESS than asked.
TASK_STACK_KCONFIG = (
    "CONFIG_NROS_ZEPHYR_TASK_SLOTS",
    "CONFIG_NROS_ZEPHYR_TASK_STACK_SIZE",
    "CONFIG_MAIN_STACK_SIZE",
)


def read_task_kconfig(elf):
    """The image's task-stack Kconfig integers, from the `.config` beside it, or None."""
    path = os.path.join(os.path.dirname(os.path.abspath(elf)), ".config")
    try:
        with open(path, encoding="utf8") as fh:
            found = dict((k, int(v)) for k, v in _KCONFIG_INT.findall(fh.read()))
    except OSError:
        return None
    return {k: found[k] for k in TASK_STACK_KCONFIG if k in found}


def price_task_stacks(measured, kconfig):
    """'N x S' for `nros_thread_stacks` against the image's Kconfig, or why not."""
    slots_k, size_k, main_k = TASK_STACK_KCONFIG
    if kconfig is None:
        return f"measured; no .config beside the image states {slots_k}"
    slots = kconfig.get(slots_k)
    if not slots:
        return f"measured; the image's .config does not state {slots_k}"
    size, src = kconfig.get(size_k, 0), size_k
    if not size:
        size, src = kconfig.get(main_k), main_k
    if measured % slots:
        return f"MISMATCH: {measured:,} is not a whole number of {slots_k} = {slots} stacks"
    stride = measured // slots
    if size is not None and stride < size:
        return (
            f"MISMATCH: {slots} x {stride:,} is less than the {src} = {size:,} each "
            "slot is asked for -- the .config and the image come from different builds"
        )
    asked = f", {src} = {size:,} plus guard and alignment" if size is not None else ""
    return f"{slots} x {stride:,} ({slots_k} = {slots}{asked})"


def price_executor_storage(key, measured, sizes):
    """'N x S' for a C/C++ executor storage symbol, or why it cannot be said."""
    by_macro = price_by_macro(key, measured, sizes)
    if by_macro is not None:
        return None, by_macro
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


# phase-479 W5 (RFC-0102 D5) — the runtime-logger arenas `nros_log::pool` carves
# from `NROS_LOG_DYNAMIC_LOGGERS`. The name arena is a static of its own that is
# EXACTLY this many bytes per slot (`NAME_ARENA_BYTES_PER_LOGGER` in pool.rs),
# so its measured size states the image's capacity — a COUNT, which no other
# static says. The logger arena beside it (`POOL`) is a struct whose size moves
# with `Logger`'s fields, so it is reported in bytes only.
LOG_NAME_ARENA = "nros_log::pool::NAMES"
LOG_LOGGER_ARENA = "nros_log::pool::POOL"
LOG_NAME_BYTES_PER_LOGGER = 24


def runtime_logger_arena(agg):
    """{capacity, names_bytes, pool_bytes} for the runtime-logger arenas, or None.

    `None` when neither static is linked — an image that never calls
    `get_or_create_logger` has no arena, which is a fact, not a gap. A name
    arena that is not a whole number of slots is REFUSED rather than rounded:
    it would mean the per-slot constant moved without this reader.
    """
    names = agg.get(LOG_NAME_ARENA, {}).get("bytes")
    pool = agg.get(LOG_LOGGER_ARENA, {}).get("bytes")
    if names is None and pool is None:
        return None
    out = {"names_bytes": names, "pool_bytes": pool, "capacity": None, "problem": None}
    if names is None:
        # A zero-capacity build has a zero-size name arena, which `nm` does not
        # list as a sized symbol; the logger arena still carries its counters.
        out["problem"] = (
            f"no {LOG_NAME_ARENA} symbol: NROS_LOG_DYNAMIC_LOGGERS=0 (a zero-size "
            "arena), or the linker kept the logger arena alone"
        )
    elif names % LOG_NAME_BYTES_PER_LOGGER:
        out["problem"] = (
            f"MISMATCH: {names:,} bytes is not a whole number of "
            f"{LOG_NAME_BYTES_PER_LOGGER}-byte slots — pool.rs's per-slot constant moved"
        )
    else:
        out["capacity"] = names // LOG_NAME_BYTES_PER_LOGGER
    return out


def analyse_symbols(elf, syms, sections, pools_by_name, measured_pools=None, sizes=None,
                    task_kconfig=None):
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

    runtime_loggers = runtime_logger_arena(agg)

    storage = []
    for key, a in sorted(agg.items(), key=lambda kv: -kv[1]["bytes"]):
        role = role_of(key)
        if not role:
            continue
        executors, priced = (None, None)
        if role in ("executor storage", "level scheduler"):
            if sizes_problem:
                priced = f"not priced: {sizes_problem}"
            elif role == "executor storage":
                executors, priced = price_executor_storage(key, a["bytes"], sizes_defs)
            else:
                priced = price_by_macro(key, a["bytes"], sizes_defs)
        elif role == "platform task stacks":
            priced = price_task_stacks(a["bytes"], task_kconfig)
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
        "runtime_loggers": runtime_loggers,
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
    return analyse_symbols(elf, syms, sections, pools_by_name, measured_pools, sizes,
                           read_task_kconfig(elf))


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
        add("     native fixtures      just build-test-fixtures lane=linux")
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
    add("  nano-ros storage roles first (`[executor storage]`, `[component storage]`,")
    add("  ...), then the Rust crate, `C++ <namespace>`, or the path-less C bucket.")
    add("")
    for owner, size in sorted(res["by_owner"].items(), key=lambda kv: -kv[1])[:24]:
        share = 100.0 * size / ram_sec if ram_sec else 0.0
        d = ""
        if cmp and cmp["has_owners"]:
            od = cmp["owners"].get(owner, 0)
            d = f"  ({od:+,})" if od else "  (=)"
        add(f"  {fmt(size):>12}  {share:5.1f}%  {owner}{d}")
    add("")

    add("## nano-ros storage roles")
    add("")
    if not res["storage"]:
        add("  (no nano-ros storage-role symbol in this image)")
    for s in res["storage"]:
        many = f" (x{s['count']} symbols)" if s["count"] > 1 else ""
        add(f"  {fmt(s['bytes']):>12}  {s['role']:<20}  {s['symbol']}{many}")
        if s["priced"]:
            add(f"  {'':>12}  {'':<20}  = {s['priced']}")
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

    add("")
    add("## runtime loggers (NROS_LOG_DYNAMIC_LOGGERS)")
    add("")
    rl = res.get("runtime_loggers")
    if not rl:
        add("  (no runtime-logger arena is linked: the image never calls get_or_create_logger)")
    else:
        if rl["capacity"] is not None:
            add(f"  capacity       {rl['capacity']} loggers   (from {LOG_NAME_ARENA}, "
                f"{LOG_NAME_BYTES_PER_LOGGER} bytes per slot)")
        else:
            add(f"  capacity       not stated: {rl['problem']}")
        if rl["names_bytes"] is not None:
            add(f"  name arena     {fmt(rl['names_bytes'])} bytes")
        if rl["pool_bytes"] is not None:
            add(f"  logger arena   {fmt(rl['pool_bytes'])} bytes")
        add("  slots IN USE are a run-time figure: read `runtime loggers` in the boot")
        add("  report (scripts/read-boot-report.py) and size the knob from it.")

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
    selftest_level_image()
    selftest_runtime_loggers()
    selftest_baseline()
    selftest_sections()
    selftest_levels()
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


def selftest_runtime_loggers():
    """phase-479 W5 — the capacity is read back out of the name arena's size."""
    assert runtime_logger_arena({}) is None, "no arena linked must say so, not 0"
    got = runtime_logger_arena({LOG_NAME_ARENA: {"bytes": 32 * 24}, LOG_LOGGER_ARENA: {"bytes": 999}})
    assert got["capacity"] == 32 and got["problem"] is None, got
    bad = runtime_logger_arena({LOG_NAME_ARENA: {"bytes": 32 * 24 + 1}})
    assert bad["capacity"] is None and "MISMATCH" in bad["problem"], bad
    zero = runtime_logger_arena({LOG_LOGGER_ARENA: {"bytes": 16}})
    assert zero["capacity"] is None and "NROS_LOG_DYNAMIC_LOGGERS=0" in zero["problem"], zero


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
         "linux_entry::__nros_entry_run::__NROS_TIER_EXECUTOR_BACKING", "[executor storage]"),
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


def selftest_level_image():
    """issue 2040 -- a level image's statics report under nano-ros roles, priced.

    The symbols and sizes are the simple-autoware-safety-island board image's
    (Zephyr, S32K344, three levels on one executor): a C++ entry, so its file
    statics are `_ZL`-mangled and used to land in `C++ (global namespace)`,
    and the shim's task-stack pool in `(C / asm / no path)`.
    """
    rows = [
        # (raw, demangled, bytes, owner)
        ("_ZL21__nros_levels_backing", "__nros_levels_backing", 40640, "[executor storage]"),
        ("_ZL28__nros_tier_executor_storage", "__nros_tier_executor_storage", 2792,
         "[executor storage]"),
        ("_ZL20__nros_tier_backing_1", "__nros_tier_backing_1", 4096, "[executor storage]"),
        ("_ZL18__nros_tier_stack_1", "__nros_tier_stack_1", 16384, "[level task memory]"),
        ("_ZL18__nros_tier_stack_2", "__nros_tier_stack_2", 16384, "[level task memory]"),
        ("_ZL19__nros_tier_thread_1", "__nros_tier_thread_1", 272, "[level task memory]"),
        ("_ZL19__nros_tier_thread_2", "__nros_tier_thread_2", 272, "[level task memory]"),
        ("__nros_tier_tcb_1", "__nros_tier_tcb_1", 100, "[level task memory]"),
        ("_ZL18__nros_scheduler_0", "__nros_scheduler_0", 232, "[level scheduler]"),
        ("_ZL18__nros_mon_storage", "__nros_mon_storage", 400, "[monitor storage]"),
        ("_ZL19__nros_mode_storage", "__nros_mode_storage", 288, "[monitor storage]"),
        ("_ZL17__nros_hz_storage", "__nros_hz_storage", 120, "[monitor storage]"),
        ("_ZL18__nros_age_storage", "__nros_age_storage", 80, "[monitor storage]"),
        ("_ZL15__nros_mon_rows", "__nros_mon_rows", 48, "[monitor storage]"),
        ("nros_thread_stacks", "nros_thread_stacks", 4 * 5120, "[platform task stacks]"),
        ("__nros_comp_buf_0", "__nros_comp_buf_0", 3368, "[component storage]"),
        # Near misses stay with their language: the anchors hold.
        ("_ZL12__nros_tiers", "__nros_tiers", 96, "C++ (global namespace)"),
        ("my_nros_thread_stacks", "my_nros_thread_stacks", 64, "(C / asm / no path)"),
        ("__nros_mon_rows_x", "__nros_mon_rows_x", 8, "(C / asm / no path)"),
    ]
    for raw, name, _b, want in rows:
        got = owner_of(symbol_key(name), lang_of(raw))
        assert got == want, f"{name!r} ({raw}) attributed to {got!r}, want {want!r}"

    syms = [make_symbol(b, "b", raw, name) for raw, name, b, _o in rows]
    sizes = {"NROS_CPP_LEVELS_BACKING_SIZE": 40644, "NROS_CPP_SCHEDULER_BACKING_SIZE_0": 232,
             "NROS_CPP_EXECUTOR_STORAGE_SIZE": 2788}
    kconfig = {"CONFIG_NROS_ZEPHYR_TASK_SLOTS": 4, "CONFIG_NROS_ZEPHYR_TASK_STACK_SIZE": 0,
               "CONFIG_MAIN_STACK_SIZE": 5120}
    res = analyse_symbols("synthetic", syms, {}, {}, sizes=(sizes, ["h"], None),
                          task_kconfig=kconfig)
    by = res["by_owner"]
    assert by["[level task memory]"] == 2 * 16384 + 2 * 272 + 100, by
    assert by["[executor storage]"] == 40640 + 2792 + 4096, by
    assert by["[platform task stacks]"] == 20480 and by["[level scheduler]"] == 232, by
    assert "C++ (global namespace)" in by and by["C++ (global namespace)"] == 96, (
        f"only the non-role static may stay in the C++ bucket: {by}"
    )
    priced = {s["symbol"]: s["priced"] for s in res["storage"]}
    assert priced["__nros_levels_backing"] == "NROS_CPP_LEVELS_BACKING_SIZE = 40,644", priced
    assert priced["__nros_scheduler_0"] == "NROS_CPP_SCHEDULER_BACKING_SIZE_0 = 232", priced
    assert "no per-build sizes header states NROS_CPP_TIER_BACKING_SIZE_1" in (
        priced["__nros_tier_backing_1"]), priced
    assert priced["nros_thread_stacks"].startswith(
        "4 x 5,120 (CONFIG_NROS_ZEPHYR_TASK_SLOTS = 4, CONFIG_MAIN_STACK_SIZE = 5,120"), priced
    assert priced["__nros_tier_stack_1"] is None, "a level stack is priced by --levels"

    # The pricing must be able to FAIL.
    assert price_by_macro("__nros_levels_backing", 40640,
                          {"NROS_CPP_LEVELS_BACKING_SIZE": 13112}).startswith("MISMATCH")
    assert price_by_macro("__nros_tiers", 96, sizes) is None
    assert price_task_stacks(20481, kconfig).startswith("MISMATCH"), "not whole slots"
    assert price_task_stacks(4 * 4096, kconfig).startswith("MISMATCH"), "slot below the ask"
    assert price_task_stacks(4 * 8192, dict(kconfig, CONFIG_NROS_ZEPHYR_TASK_STACK_SIZE=8192)
                             ).startswith("4 x 8,192 (CONFIG_NROS_ZEPHYR_TASK_SLOTS = 4, "
                                          "CONFIG_NROS_ZEPHYR_TASK_STACK_SIZE = 8,192")
    assert price_task_stacks(20480, None).startswith("measured; no .config")


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
    out = git_grep.run(
        ["-n", "-E", _STORAGE_GREP,
         "--", "*.rs", "*.c", "*.h", "*.hpp", "*.cpp", "*.jinja",
         ":!*.golden", ":!third-party", ":!scripts/nros-mem-report.py"],
        ROOT,
    )
    assert out, "git grep matched no executor storage at all — the scan would pass vacuously"
    defining = set()
    for line in out.splitlines():
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


# ---------------------------------------------------------------------------
# phase-509 P9 (RFC-0108 D4 rule 7) -- the per-level cost report.
#
# After the target compile: per memory region, and per level its stack and its
# worker record; per core its scheduler and the backing. Text and JSON. It is a
# CHECK, never an input: the derivation reads no byte count (D4 rule 4), and
# the image's own guard (a linker assert) stays the authority. What `--check`
# asserts is that the image holds what was generated: each level's stack is the
# derived one, each backing and the scheduler the generated size.
#
# The entry's statics, named by the generated entry (boot_wrapper.jinja):
#   __nros_tier_stack_<i>, __nros_tier_thread_<i>   a spawned level's worker
#   __nros_tier_executor_storage                     n contexts, one per level
#   __nros_tier_backing_<i>                          a level's own backing
#   __nros_scheduler_0                               the core's scheduler
# `i` is the row of the entry's table (the runner's order); the LEVEL is the
# row's urgency order among the rows, the rule `NROS_ENTRY_LEVEL_OF_<node>` is
# generated by.
# ---------------------------------------------------------------------------

PRIORITY_RECORD = "nros_priority_plan.json"
# zephyr/Kconfig defaults, used when the image's .config is not beside it.
# The worker frame is the C level runner's measured setup peak (issue 2065,
# the S32K344 island); the selftest reads both back from zephyr/Kconfig.
LEVEL_KCONFIG_DEFAULTS = {
    "CONFIG_NROS_ZEPHYR_TIER_STACK_SIZE": 16384,
    "CONFIG_NROS_ZEPHYR_LEVEL_WORKER_FRAME": 13312,
}
_KCONFIG_INT = re.compile(r"^(CONFIG_[A-Z0-9_]+)=([0-9]+)\s*$", re.M)
_LEVEL_SYM = re.compile(
    r"^__nros_(tier_stack|tier_thread|tier_backing|scheduler)_([0-9]+)$"
    r"|^__nros_tier_executor_storage$|^__nros_levels_backing$"
)


def scheduler_backing_size(n_levels, ptr):
    """`NROS_CPP_SCHEDULER_BACKING_SIZE(n)` of <rclc/detail/main.h>, for a
    target whose pointers are `ptr` bytes: an 8-byte head, the session and the
    core's executor, then one seat per level -- phase-509 P9b: the level's
    worker record and waiter, at most 8 pointers + 40 bytes -- rounded up to 8."""
    return (8 + 2 * ptr + n_levels * (8 * ptr + 40) + 7) // 8 * 8


def scheduler_seat_bytes(ptr):
    """One level's share of the scheduler: its seat (phase-509 P9b), the
    bound the C macro reserves."""
    return 8 * ptr + 40


# Issue 2019 -- the FreeRTOS and NuttX headers' knobs, which an entry may
# override with a `-D`; the image does not record them, so the report states
# the header defaults it used. The worker frames are the C level runner's
# measured setup peaks (issue 2019), which the selftest reads back from the
# headers.
LEVEL_HEADER_DEFAULTS = {
    "freertos": ("NROS_FREERTOS_LEVEL_STACK_DEFAULT", 262144,
                 "NROS_FREERTOS_LEVEL_WORKER_FRAME", 10240),
    "nuttx": ("NROS_NUTTX_LEVEL_STACK_DEFAULT", 16384,
              "NROS_NUTTX_LEVEL_WORKER_FRAME", 11264),
}


def level_knobs(family, kconfig):
    """(level default, worker frame, source) of `family`'s
    <rclc/detail/tier_task_memory_<family>.h>: Zephyr's from the image's
    `.config`, FreeRTOS's and NuttX's the header defaults."""
    if family in LEVEL_HEADER_DEFAULTS:
        dname, dval, fname, fval = LEVEL_HEADER_DEFAULTS[family]
        return dval, fval, f"<rclc/detail/tier_task_memory_{family}.h> defaults ({dname}, {fname})"
    return (
        kconfig["CONFIG_NROS_ZEPHYR_TIER_STACK_SIZE"],
        kconfig["CONFIG_NROS_ZEPHYR_LEVEL_WORKER_FRAME"],
        "CONFIG_NROS_ZEPHYR_TIER_STACK_SIZE, CONFIG_NROS_ZEPHYR_LEVEL_WORKER_FRAME",
    )


def derived_level_stack(member_bytes, level_default, frame):
    """`NROS_LEVEL_STACK_BYTES(member)` of <rclc/detail/tier_task_memory_*.h>:
    the largest member's stated stack (issue 2019, `[[component]]
    stack_bytes`), or the per-callback default (the level default minus the
    worker frame) when none states one, plus the worker frame."""
    return (member_bytes if member_bytes > 0 else level_default - frame) + frame


def level_order(priorities, smaller_is_urgent):
    """Each row's level: its urgency order among the rows, equal priorities in
    row order (`nros_cpp_tier_level`)."""
    key = (lambda i: (priorities[i], i)) if smaller_is_urgent else (lambda i: (-priorities[i], i))
    order = sorted(range(len(priorities)), key=key)
    levels = [0] * len(priorities)
    for lvl, i in enumerate(order):
        levels[i] = lvl
    return levels


def find_near(elf, name, depth=4):
    """The nearest file called `name` in one of the ELF's ancestors (the
    generated entry writes it at the build's top, beside `zephyr/`)."""
    d = os.path.dirname(os.path.abspath(elf))
    for _ in range(depth):
        p = os.path.join(d, name)
        if os.path.isfile(p):
            return p
        d = os.path.dirname(d)
    return None


def read_kconfig(elf):
    """(values, source): the image's `.config` integers the level stacks use."""
    path = os.path.join(os.path.dirname(os.path.abspath(elf)), ".config")
    values = dict(LEVEL_KCONFIG_DEFAULTS)
    if not os.path.isfile(path):
        return values, "zephyr/Kconfig defaults (no .config beside the image)"
    with open(path, encoding="utf8") as fh:
        found = dict((k, int(v)) for k, v in _KCONFIG_INT.findall(fh.read()))
    for k in values:
        if k in found:
            values[k] = found[k]
    return values, path


def read_symbol_table(elf):
    """{name: (addr, size, section)} for the defined, sized symbols."""
    tool = find_tool("llvm-readelf") or find_tool("readelf")
    if not tool:
        raise SystemExit("--levels needs readelf")
    secs = subprocess.run([tool, "-S", "-W", elf], capture_output=True, text=True).stdout
    index = {}
    for line in secs.splitlines():
        m = re.match(r"^\s*\[\s*(\d+)\]\s+(\S+)\s", line)
        if m:
            index[m.group(1)] = m.group(2)
    syms = subprocess.run([tool, "-s", "-W", elf], capture_output=True, text=True).stdout
    out = {}
    for line in syms.splitlines():
        f = line.split()
        if len(f) < 8 or not f[0].endswith(":"):
            continue
        _num, value, size, _typ, _bind, _vis, ndx, name = f[:8]
        if not ndx.isdigit():
            continue
        # A C++ entry's file-scope static is `_ZL<len><name>`.
        m = re.match(r"^_ZL(\d+)(.*)$", name)
        if m and int(m.group(1)) == len(m.group(2)):
            name = m.group(2)
        try:
            out[name] = (int(value, 16), int(size, 0), index.get(ndx, ndx))
        except ValueError:
            continue
    return out


def elf_pointer_bytes(elf):
    with open(elf, "rb") as fh:
        ident = fh.read(5)
    return 8 if ident[4:5] == b"\x02" else 4


def read_memory_regions(elf):
    """[(name, origin, length)] from the link map beside the image (Zephyr
    writes `zephyr.map`), or [] when it states none."""
    path = os.path.splitext(os.path.abspath(elf))[0] + ".map"
    if not os.path.isfile(path):
        return []
    regions, inside = [], False
    with open(path, encoding="utf8", errors="replace") as fh:
        for line in fh:
            if line.startswith("Memory Configuration"):
                inside = True
                continue
            if inside and line.startswith("Linker script and memory map"):
                break
            m = re.match(r"^(\S+)\s+0x([0-9a-fA-F]+)\s+0x([0-9a-fA-F]+)", line) if inside else None
            if m and m.group(1) != "*default*":
                regions.append((m.group(1), int(m.group(2), 16), int(m.group(3), 16)))
    return regions


def region_of(addr, section, regions):
    """The link map's memory region holding `addr`, else the output section,
    without the per-file suffix Zephyr gives `.noinit` (`.noinit."<file>".N`)."""
    for name, origin, length in regions:
        if origin <= addr < origin + length:
            return name
    return section.split('."', 1)[0]


def level_costs(elf, sizes_header=None):
    """The per-level cost of `elf`, as a dict, with every check's verdict."""
    record_path = find_near(elf, PRIORITY_RECORD)
    if record_path is None:
        return {"elf": elf, "problem": f"no {PRIORITY_RECORD} near the image"}
    with open(record_path, encoding="utf8") as fh:
        record = json.load(fh)
    tiers = record.get("tiers") or []
    res = {"elf": elf, "record": record_path, "levels": [], "cores": [], "checks": []}
    if not tiers:
        res["note"] = "one level: the image spawns no worker, so it has no level statics"
        return res
    smaller = record.get("plan", {}).get("direction", "") in ("smaller-is-urgent", "smaller")
    family = (record.get("tier_key") or "").split("/")[0]
    # The runner boots tiers[0] on Zephyr, the last row on FreeRTOS and NuttX.
    boot = len(tiers) - 1 if family in ("freertos", "nuttx") else 0
    kconfig, kconfig_src = read_kconfig(elf)
    res["kconfig"] = {"source": kconfig_src, **kconfig}
    level_default, frame, knob_src = level_knobs(family, kconfig)
    res["level_knobs"] = {"level_default": level_default, "worker_frame": frame,
                          "source": knob_src}
    sizes, _used, problem = read_sizes(
        [sizes_header] if sizes_header else find_sizes_headers(elf), None
    )
    symbols = read_symbol_table(elf)
    regions = read_memory_regions(elf)
    ptr = elf_pointer_bytes(elf)
    n = len(tiers)
    levels = level_order([t["priority"] for t in tiers], smaller)

    def check(what, measured, expected):
        res["checks"].append(
            {"what": what, "measured": measured, "expected": expected, "ok": measured == expected}
        )

    storage = symbols.get("__nros_tier_executor_storage")
    # phase-509 P9b (RFC-0108 "As built in P9b") -- a level image opens ONE
    # executor per core, carved from the levels backing; a level is a worker
    # record (its scheduler seat) and a stack. Before P9b each level was an
    # executor: a context and a backing per row.
    levels_backing = symbols.get("__nros_levels_backing")
    one_executor = levels_backing is not None
    res["layout"] = "one executor per core (P9b)" if one_executor else "an executor per level (P9)"
    per_ctx = None
    if storage is not None:
        per_ctx = storage[1] // (1 if one_executor else n)
        if "NROS_CPP_EXECUTOR_STORAGE_SIZE" in sizes:
            want = (sizes["NROS_CPP_EXECUTOR_STORAGE_SIZE"] + 7) // 8 * 8
            if one_executor:
                check("core executor context (NROS_CPP_EXECUTOR_STORAGE_SIZE)", storage[1], want)
            else:
                check("executor contexts (n x NROS_CPP_EXECUTOR_STORAGE_SIZE)", storage[1], n * want)
    if one_executor and "NROS_CPP_LEVELS_BACKING_SIZE" in sizes:
        check("core executor backing (NROS_CPP_LEVELS_BACKING_SIZE)", levels_backing[1],
              sizes["NROS_CPP_LEVELS_BACKING_SIZE"])
    for i, t in enumerate(tiers):
        members = [m[0] if isinstance(m, list) else m for m in t.get("members", [])]
        row = {
            "row": i,
            "level": levels[i],
            "name": t.get("name"),
            "priority": t.get("priority"),
            "members": members,
            "spawned": i != boot,
        }
        stated = t.get("stack_bytes") or 0
        member = t.get("member_stack_bytes") or 0
        if row["spawned"]:
            derived = stated if stated else derived_level_stack(member, level_default, frame)
            row["stack_derived"] = derived
            row["member_stack"] = member
            row["stack_rule"] = (
                "authored stack_bytes" if stated
                else "largest member stack + worker frame" if member
                else "per-callback default + worker frame"
            )
            row["members_defaulted"] = [] if stated or member else members
            st = symbols.get(f"__nros_tier_stack_{i}")
            th = symbols.get(f"__nros_tier_thread_{i}")
            if st:
                row["stack"] = st[1]
                row["stack_region"] = region_of(st[0], st[2], regions)
                # K_THREAD_STACK_DEFINE adds the architecture's guard and rounds
                # to its alignment, never less than asked.
                res["checks"].append(
                    {
                        "what": f"level {levels[i]} stack (`{t.get('name')}`) holds the derived "
                        f"{derived} bytes",
                        "measured": st[1],
                        "expected": derived,
                        "ok": st[1] >= derived,
                    }
                )
            else:
                res["checks"].append(
                    {"what": f"level {levels[i]} stack symbol", "measured": None,
                     "expected": derived, "ok": False}
                )
            if th:
                row["thread"] = th[1]
                row["thread_region"] = region_of(th[0], th[2], regions)
        else:
            row["stack_rule"] = "the caller's thread (main), not the entry's"
        bk = symbols.get(f"__nros_tier_backing_{i}")
        if bk:
            row["backing"] = bk[1]
            row["backing_region"] = region_of(bk[0], bk[2], regions)
            key = f"NROS_CPP_TIER_BACKING_SIZE_{i}"
            if key in sizes:
                check(f"level {levels[i]} backing ({key})", bk[1], sizes[key])
        if one_executor:
            # The level's seat in the scheduler: its worker record.
            row["seat"] = scheduler_seat_bytes(ptr)
        elif per_ctx is not None:
            row["context"] = per_ctx
            row["context_region"] = region_of(storage[0], storage[2], regions)
        row["worker_record"] = (
            row.get("context", 0) + row.get("backing", 0) + row.get("thread", 0)
            + row.get("seat", 0)
        )
        res["levels"].append(row)
    sched = symbols.get("__nros_scheduler_0")
    core = {"core": 0, "levels": n}
    if sched:
        core["scheduler"] = sched[1]
        core["scheduler_region"] = region_of(sched[0], sched[2], regions)
        check("core 0 scheduler (NROS_CPP_SCHEDULER_BACKING_SIZE)", sched[1],
              scheduler_backing_size(n, ptr))
    else:
        res["checks"].append(
            {"what": "core 0 scheduler symbol", "measured": None,
             "expected": scheduler_backing_size(n, ptr), "ok": False}
        )
    core["backing"] = sum(r.get("backing", 0) for r in res["levels"])
    if one_executor:
        core["executor_context"] = per_ctx or 0
        core["executor_backing"] = levels_backing[1]
        core["executor_region"] = region_of(levels_backing[0], levels_backing[2], regions)
    res["cores"].append(core)
    if problem:
        res["sizes_problem"] = problem
    by_region = {}
    for r in res["levels"]:
        for k in ("stack", "backing", "context", "thread"):
            if k in r:
                reg = r.get(f"{k}_region", "?")
                by_region[reg] = by_region.get(reg, 0) + r[k]
    if sched:
        by_region[core["scheduler_region"]] = by_region.get(core["scheduler_region"], 0) + sched[1]
    if one_executor:
        reg = core["executor_region"]
        by_region[reg] = by_region.get(reg, 0) + core["executor_backing"]
        if storage is not None:
            sreg = region_of(storage[0], storage[2], regions)
            by_region[sreg] = by_region.get(sreg, 0) + storage[1]
    res["regions"] = by_region
    return res


def level_report(res):
    out = [f"level cost report: {res['elf']}"]
    if "problem" in res:
        return out[0] + f"\n  {res['problem']}"
    if "note" in res:
        return out[0] + f"\n  {res['note']}"
    out.append(f"  record: {res['record']}")
    out.append(f"  kconfig: {res['kconfig']['source']}")
    lk = res.get("level_knobs")
    if lk:
        out.append(
            f"  level stack: default {fmt(lk['level_default'])} B, worker frame "
            f"{fmt(lk['worker_frame'])} B ({lk['source']})"
        )
    out.append(f"  layout: {res.get('layout', '?')}")
    out.append(
        f"  {'lvl':>3} {'row':>3} {'name':<24} {'stack':>8} {'derived':>8} {'context':>8} "
        f"{'backing':>8} {'thread':>7} {'seat':>6} {'record':>8}  members"
    )
    for r in sorted(res["levels"], key=lambda r: r["level"]):
        out.append(
            f"  {r['level']:>3} {r['row']:>3} {r['name'] or '?':<24} "
            f"{fmt(r['stack']) if 'stack' in r else 'main':>8} "
            f"{fmt(r['stack_derived']) if 'stack_derived' in r else '-':>8} "
            f"{fmt(r.get('context', 0)):>8} {fmt(r.get('backing', 0)):>8} "
            f"{fmt(r.get('thread', 0)):>7} {fmt(r.get('seat', 0)):>6} "
            f"{fmt(r['worker_record']):>8}  "
            + ", ".join(r["members"])
        )
        if r.get("member_stack"):
            out.append(
                f"        largest stated member stack: {fmt(r['member_stack'])} B "
                "([[component]] stack_bytes)"
            )
        if r.get("members_defaulted"):
            out.append(
                "        per-callback default (no stated stack): "
                + ", ".join(r["members_defaulted"])
            )
    for c in res["cores"]:
        if "executor_backing" in c:
            out.append(
                f"  core {c['core']}: executor context {fmt(c['executor_context'])} B + "
                f"backing {fmt(c['executor_backing'])} B (one executor), scheduler "
                f"{fmt(c.get('scheduler', 0))} B, {c['levels']} levels"
            )
        else:
            out.append(
                f"  core {c['core']}: scheduler {fmt(c.get('scheduler', 0))} B, "
                f"level backings {fmt(c['backing'])} B, {c['levels']} levels"
            )
    out.append("  by region:")
    for reg, b in sorted(res["regions"].items()):
        out.append(f"    {reg:<32} {fmt(b):>10}")
    if res.get("sizes_problem"):
        out.append(f"  sizes header: {res['sizes_problem']}")
    return "\n".join(out)


def level_check(res):
    """0 when the image holds what was generated, 1 otherwise (or vacuous)."""
    if "problem" in res:
        print(f"levels --check: {res['elf']}: {res['problem']}")
        return 1
    if "note" in res:
        print(f"levels --check: {res['elf']}: {res['note']}")
        return 0
    bad = [c for c in res["checks"] if not c["ok"]]
    for c in res["checks"]:
        mark = "ok" if c["ok"] else "MISMATCH"
        print(f"  [{mark}] {c['what']}: measured {c['measured']}, generated {c['expected']}")
    if not res["checks"]:
        print("levels --check: nothing was checked, which proves nothing")
        return 1
    return 1 if bad else 0


def selftest_levels():
    assert scheduler_backing_size(3, 8) == 336 and scheduler_backing_size(3, 4) == 232
    assert scheduler_backing_size(1, 4) == 88
    k = dict(LEVEL_KCONFIG_DEFAULTS)
    d, f, _ = level_knobs("zephyr", k)
    assert derived_level_stack(0, d, f) == 16384, "members that state nothing keep the knob"
    assert derived_level_stack(6000, d, f) == 19312, "a stated member plus the worker frame"
    d, f, _ = level_knobs("freertos", k)
    assert derived_level_stack(0, d, f) == 262144 and derived_level_stack(8192, d, f) == 18432
    d, f, _ = level_knobs("nuttx", k)
    assert derived_level_stack(0, d, f) == 16384 and derived_level_stack(2048, d, f) == 13312
    root = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")
    # Issue 2065 -- the Zephyr defaults above are zephyr/Kconfig's.
    with open(os.path.join(root, "zephyr", "Kconfig")) as fh:
        kconfig_text = fh.read()
    for sym, val in LEVEL_KCONFIG_DEFAULTS.items():
        m = re.search(rf"^config {sym[len('CONFIG_'):]}\n(?:[ \t]+.*\n)*?[ \t]+default (\d+)$",
                      kconfig_text, re.M)
        assert m and int(m.group(1)) == val, f"{sym} drifted from zephyr/Kconfig"
    detail = os.path.join(root, "packages", "api", "nros-c", "include", "rclc", "detail")
    for family, (dname, dval, fname, fval) in LEVEL_HEADER_DEFAULTS.items():
        with open(os.path.join(detail, f"tier_task_memory_{family}.h")) as fh:
            text = fh.read()
        for name, val in ((dname, dval), (fname, fval)):
            m = re.search(rf"^#define {name} (\d+)u?$", text, re.M)
            assert m and int(m.group(1)) == val, f"{family}: {name} drifted from its header"
    assert level_order([10, 5, 8], True) == [2, 0, 1]
    assert level_order([10, 5, 5], True) == [2, 0, 1], "equal priorities in row order"
    assert level_order([10, 5, 8], False) == [0, 2, 1]
    with contextlib.redirect_stdout(io.StringIO()):
        assert level_check({"elf": "x", "checks": []}) == 1, "a vacuous check must fail"
        assert level_check({"elf": "x", "checks": [{"what": "w", "measured": 1,
                                                     "expected": 2, "ok": False}]}) == 1


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
    ap.add_argument(
        "--levels",
        action="store_true",
        help="phase-509 P9: the per-level cost report (per region, per level its "
        "stack and worker record, per core its scheduler and backing); with "
        "--check, assert the image holds the generated sizes and derived stacks",
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
    if args.levels:
        results = [level_costs(os.path.abspath(e), args.sizes_header) for e in args.elf]
        if args.json:
            print(json.dumps(results if len(results) > 1 else results[0], indent=2))
            return 0
        rc = 0
        for res in results:
            print(level_report(res))
            if args.check:
                rc |= level_check(res)
        return rc

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
