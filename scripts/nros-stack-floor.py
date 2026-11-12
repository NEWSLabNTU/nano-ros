#!/usr/bin/env python3
"""Worst-case stack depth of a thread role, from the compiler's own frames — issue 2400.

A thread that runs nano-ros needs a minimum stack, and three places used to
state that minimum three ways: computed from a value size
(`NROS_EXECUTOR_MAIN_STACK_MIN`), hand-set from one peak
(`NROS_RUST_MAIN_STACK_FLOOR`), and a measured FreeRTOS high-water mark. None
followed the frames the thread actually runs, so a gate could accept a thread
that overflows (issue 2398 measured the computed floor at a quarter of the open
path's need on fmu-v6x).

This tool takes the frames and the call graph from the build output and sums
the deepest path from a role's ROOTS:

  * FRAMES: each function's frame, from an object's `.stack_sizes` section
    (rustc `-Z emit-stack-sizes`, clang `-fstack-size-section`), or from GCC
    `-fstack-usage` `.su` files for C built by GCC (zenoh-pico and friends).
  * DIRECT EDGES: every relocation inside a function's code that names a
    function symbol. That includes address-taken references made from code,
    which over-counts — the safe direction.
  * INDIRECT CALLS: a call through a register (`blx rN`, `jalr`, `blr`,
    `call *`) cannot be followed statically. Each one is COUNTED, never skipped.
    `--indirect=fold` charges each such site the deepest path among the
    functions whose address is taken from DATA (vtables, callback tables) in
    the inputs — sound when the backend's archive is an input, because that is
    where its vtable lives. `--indirect=allowance:<bytes>` charges a stated
    number instead. With neither, the result is refused while indirect sites
    are on the path, because a direct-only sum UNDER-counts (issue 2398: ~70
    vtable calls on the open path).
  * RECURSION: a cycle on a root's path has no static bound. It is refused,
    naming the cycle, unless `--recursion-allowance <bytes>` states one.

A function reached with no frame from any input is refused by name: a missing
frame would silently count as zero.

Usage:
  nros-stack-floor.py --role OPEN=nros_cpp_init --role SPIN=nros_cpp_spin_once \\
      [--su DIR]... [--indirect fold|allowance:N] [--margin PCT] \\
      [--emit-defines] [--json] INPUT.a|INPUT.o ...
  nros-stack-floor.py --self-test

Tools: `llvm-readobj` and `llvm-objdump` from the pinned toolchain's
`llvm-tools` component (found via `rustc --print sysroot`), or `--llvm-bin`.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import subprocess
import sys
from dataclasses import dataclass, field
from pathlib import Path

# ---------------------------------------------------------------------------
# The graph and the walk (pure; the self-test drives these directly)
# ---------------------------------------------------------------------------


@dataclass
class Graph:
    frames: dict[str, int] = field(default_factory=dict)
    # caller -> callees (direct)
    edges: dict[str, set[str]] = field(default_factory=dict)
    # function -> number of indirect call sites in its code
    indirect_sites: dict[str, int] = field(default_factory=dict)
    # functions whose address is taken from a data section
    address_taken: set[str] = field(default_factory=set)
    # every symbol that is a FUNCTION in some input (defined or frame-known)
    functions: set[str] = field(default_factory=set)
    # names that share one address in one section (GCC identical-code
    # folding, `#[export_name]` aliases): one body, one frame
    alias_groups: list[set[str]] = field(default_factory=list)
    # g++ writes a C++ TU's `.su` line as a SIGNATURE (`void app_main()`,
    # `int ns::f(int)`), not a symbol: name(args) -> bytes, matched to the
    # symbol table by `resolve_cxx_su_frames`.
    su_signatures: dict[str, int] = field(default_factory=dict)


class WalkRefused(Exception):
    pass


@dataclass
class Path_:
    depth: int
    chain: list[str]
    indirect: int  # indirect sites on the path that were charged


def worst_path(g: Graph, root: str, *, indirect_charge: int | None,
               recursion_allowance: int | None,
               externals: set[str] | None = None,
               cuts: tuple[str, ...] = ()) -> Path_:
    """Deepest path from `root`. `indirect_charge` is the bytes charged at a
    function with indirect sites (None = refuse when one is reached).
    `externals` are callees with no definition in any input that the caller
    has accepted as leaves of known cost 0 (e.g. libc memset intrinsics)."""
    externals = externals or set()
    memo: dict[str, Path_] = {}
    on_stack: list[str] = []
    missing: set[str] = set()
    cycles: list[list[str]] = []
    refused_indirect: list[str] = []

    def visit(fn: str) -> Path_:
        if fn in memo:
            return memo[fn]
        if fn in on_stack:
            cyc = on_stack[on_stack.index(fn):] + [fn]
            cycles.append(cyc)
            return Path_(recursion_allowance or 0, [fn + " (recursion)"], 0)
        frame = g.frames.get(fn)
        if frame is None:
            if fn in externals:
                return Path_(0, [fn + " (external, 0)"], 0)
            missing.add(fn)
            return Path_(0, [fn + " (NO FRAME)"], 0)
        if any(c in fn for c in cuts):
            p = Path_(frame, [fn + " (cut)"], 0)
            memo[fn] = p
            return p
        on_stack.append(fn)
        best = Path_(0, [], 0)
        for callee in sorted(g.edges.get(fn, ())):
            if callee == fn and callee not in on_stack[:-1]:
                # direct self-recursion
                cycles.append([fn, fn])
                sub = Path_(recursion_allowance or 0, [fn + " (recursion)"], 0)
            else:
                sub = visit(callee)
            if sub.depth > best.depth:
                best = sub
        n_ind = g.indirect_sites.get(fn, 0)
        if n_ind:
            if indirect_charge is None:
                refused_indirect.append(fn)
            elif indirect_charge > best.depth:
                best = Path_(indirect_charge, [f"<indirect x{n_ind}>"], n_ind)
        on_stack.pop()
        p = Path_(frame + best.depth, [fn] + best.chain, best.indirect + (n_ind if indirect_charge else 0))
        memo[fn] = p
        return p

    if root not in g.frames and root not in g.functions:
        raise WalkRefused(f"root `{root}` is not a function in any input")
    p = visit(root)
    problems = []
    if missing:
        problems.append("functions reached with no frame in any input (a missing "
                        "frame would count as 0): " + ", ".join(sorted(missing)[:20])
                        + (" ..." if len(missing) > 20 else ""))
    if cycles and recursion_allowance is None:
        shown = "; ".join(" -> ".join(c) for c in cycles[:5])
        problems.append(f"recursion has no static bound ({len(cycles)} cycle(s): {shown}); "
                        "state one with --recursion-allowance <bytes>")
    if refused_indirect:
        problems.append(f"{len(refused_indirect)} function(s) on the path call through a "
                        "register, which a direct-only sum under-counts: "
                        + ", ".join(sorted(set(refused_indirect))[:10])
                        + "; pass --indirect fold (with the backend archive as an "
                        "input) or --indirect allowance:<bytes>")
    if problems:
        raise WalkRefused(f"root `{root}`:\n  - " + "\n  - ".join(problems))
    return p


def fold_charge(g: Graph, *, recursion_allowance: int | None,
                externals: set[str], cuts: tuple[str, ...] = (),
                scope: tuple[str, ...] = (), one_level: bool = False) -> int:
    """Bytes charged at an indirect site under --indirect fold: the deepest
    path among address-taken functions, themselves walked with indirect sites
    charged ZERO-then-iterated to a fixed point (a vtable function that itself
    calls through a vtable)."""
    charge = 0
    for _ in range(1 if one_level else 8):
        best = 0
        for fn in g.address_taken:
            if fn not in g.frames:
                continue
            if scope and not any(sc in fn for sc in scope):
                continue
            # A refused target is a refusal of the fold, never a skip: a skip
            # charges the site as if that target were not there.
            p = worst_path(g, fn, indirect_charge=charge,
                           recursion_allowance=recursion_allowance,
                           externals=externals, cuts=cuts)
            best = max(best, p.depth)
        if best == charge or one_level:
            return best
        charge = best
    raise WalkRefused("--indirect fold did not converge in 8 rounds: an "
                      "address-taken function reaches itself through a register; "
                      "use --indirect allowance:<bytes>")


# ---------------------------------------------------------------------------
# Reading inputs
# ---------------------------------------------------------------------------

INDIRECT_RE = re.compile(
    r"\t(blx\s+r\d+|blx\s+(ip|lr|sp)|bl?x?\s+\{|blr\s+x\d+|jalr\s|"
    r"c\.jalr\s|callq?\s+\*|call\s+\*)")
FUNC_LABEL_RE = re.compile(r"^[0-9a-f]+ <(.+)>:$")
# `file:line:col:name`, where a C++ name is a signature holding `::` and spaces
SU_RE = re.compile(r"^(?P<loc>[^\t]*?:\d+:\d+):(?P<fn>[^\t]+)\t(?P<size>\d+)\t(?P<kind>\S+)")


# The checkout this script lives in. rustup picks the toolchain by the CWD's
# `rust-toolchain.toml`, so asking `rustc` from anywhere else answers with the
# user's default toolchain — and llc from a different LLVM refuses the pinned
# compiler's bitcode ("Not an int attribute", measured: 1.98.1 vs 1.99.0).
CHECKOUT = Path(__file__).resolve().parent.parent
_TOOLS: dict[str, str] = {}


class ToolMissing(SystemExit):
    """An analysis tool is absent. A SystemExit so a bare walk still ends
    with its message; `main` degrades it to a warning for an unmeasured
    image under --check-stack (ruling (A): such an image never refuses)."""


def llvm_tool(name: str, llvm_bin: str | None) -> str:
    if llvm_bin:
        return str(Path(llvm_bin) / name)
    if name in _TOOLS:
        return _TOOLS[name]
    try:
        sysroot = subprocess.run(["rustc", "--print", "sysroot"], check=True, cwd=CHECKOUT,
                                 capture_output=True, text=True).stdout.strip()
        for p in Path(sysroot, "lib", "rustlib").glob(f"*/bin/{name}"):
            _TOOLS[name] = str(p)
            return str(p)
    except (OSError, subprocess.CalledProcessError):
        pass
    found = shutil.which(name)
    if found:
        return found
    raise ToolMissing(f"nros-stack-floor: no `{name}`: install the toolchain's llvm-tools "
                      "component or pass --llvm-bin")


def cxx_demangler(stated: str | None, llvm_bin: str | None) -> str:
    """An Itanium demangler. The rustup llvm-tools component ships no
    `llvm-cxxfilt` (measured, 1.99.0), so a host without a system LLVM has
    none; binutils' `c++filt`, or the cross toolchain's `<prefix>c++filt`
    the build names with --cxxfilt, demangles the same ABI."""
    if stated:
        return stated
    try:
        return llvm_tool("llvm-cxxfilt", llvm_bin)
    except ToolMissing:
        pass
    found = shutil.which("c++filt")
    if found:
        return found
    raise ToolMissing("nros-stack-floor: no C++ demangler: none of --cxxfilt, `llvm-cxxfilt` "
                      "or `c++filt` is present")


def sysroot_rlibs(target: str) -> list[str]:
    """The pinned toolchain's core / alloc / compiler_builtins rlibs for
    `target`: their embedded bitcode is what build-std would have compiled."""
    sysroot = subprocess.run(["rustc", "--print", "sysroot"], check=True, cwd=CHECKOUT,
                             capture_output=True, text=True).stdout.strip()
    lib = Path(sysroot, "lib", "rustlib", target, "lib")
    found = [str(p) for stem in ("core", "alloc", "compiler_builtins")
             for p in sorted(lib.glob(f"lib{stem}-*.rlib"))]
    if not found:
        sys.exit(f"nros-stack-floor: --sysroot: no rlibs under {lib} "
                 f"(is the `{target}` target installed for the pinned toolchain?)")
    return found


def bitcode_dirs_expanded(dirs: list[str]) -> list[str]:
    """A --bitcode-dir may be a glob (`**` recurses; a cmake build cannot know
    the keyed cargo directory names at configure time). A glob may match
    nothing (an image with no message FFI crate), but if every one does, that
    is an error: a walk with no Rust bitcode would miss every Rust frame."""
    import glob as _glob
    out: list[str] = []
    globs: list[str] = []
    for d in dirs:
        if any(c in d for c in "*?["):
            globs.append(d)
            out += sorted(p for p in _glob.glob(d, recursive=True) if Path(p).is_dir())  # walk-ok: untracked cargo bitcode dirs under the build dir the caller names
        else:
            out.append(d)
    if globs and not out:
        sys.exit(f"nros-stack-floor: no --bitcode-dir matches ({', '.join(globs)})")
    return out


def readobj_json(tool: str, args: list[str], paths: list[str]) -> list[dict]:
    out = subprocess.run([tool, "--elf-output-style=JSON", *args, *paths],
                         check=True, capture_output=True, text=True).stdout
    # an archive yields one JSON document per member, concatenated
    docs, dec, i = [], json.JSONDecoder(), 0
    while i < len(out):
        while i < len(out) and out[i].isspace():
            i += 1
        if i >= len(out):
            break
        obj, i = dec.raw_decode(out, i)
        docs.extend(obj if isinstance(obj, list) else [obj])
    return docs


# ---------------------------------------------------------------------------
# Stable frames: bitcode -> llc (no unstable flag, no build-std)
# ---------------------------------------------------------------------------
#
# Stable rustc cannot emit `.stack_sizes` (only `-Z emit-stack-sizes` sets the
# TargetMachine option). It CAN keep each codegen unit's optimised bitcode
# (`-C save-temps`), and the toolchain's own `llc` (rustup `llvm-tools`, the
# same LLVM) re-runs codegen on it with `-stack-size-section`. Measured on
# nros-cpp thumbv7em: OPEN/SPIN identical to `-Z emit-stack-sizes`, 1,508 of
# 1,512 frames identical (issue 2400). The prebuilt sysroot rlibs carry their
# bitcode in `.llvmbc`, so core/alloc/compiler_builtins need no build-std.
#
# These flags are FIXED, not caller options: `-function-sections` is
# load-bearing (without it calls inside one `.text` resolve with no relocation
# and the walk sees no edges — OPEN read 56 B), and the rest match rustc's
# defaults for these targets.
LLC_FLAGS = ("-O3", "-filetype=obj", "-stack-size-section", "-relocation-model=static",
             "-function-sections", "-data-sections")
LOAD_CHUNK = 16  # paths per llvm-readobj / llvm-objdump process: 16 kept the nucleo image at 384 MB peak, 256 took 977 MB
_THIN_STAGE = re.compile(r"\.thin-lto-[a-z-]+\.bc$")


def pick_codegen_bitcode(paths: list[Path]) -> list[Path]:
    """From a `-C save-temps` deps dir, the module each codegen unit was
    compiled from: `<cgu>.thin-lto-after-pm.bc` when crate-local ThinLTO ran,
    else `<cgu>.rcgu.bc`. Never `no-opt`, never another ThinLTO stage."""
    # Fat LTO: every crate was merged into the final crate's module, whose
    # post-LTO, post-opt module is `<cgu>.rcgu.bc` beside `<cgu>.rcgu.lto.input.bc`.
    # That module IS the image's Rust code; the per-crate modules beside it
    # never reached codegen and would add frames and edges the image lacks.
    fat = [p for p in paths if p.name.endswith(".rcgu.lto.input.bc")]
    if fat:
        return sorted(p.with_name(p.name[: -len(".lto.input.bc")] + ".bc") for p in fat)
    by_cgu: dict[str, Path] = {}
    for p in sorted(paths):
        n = p.name
        if n.endswith(".no-opt.bc"):
            continue
        if n.endswith(".thin-lto-after-pm.bc"):
            by_cgu[n[: -len(".thin-lto-after-pm.bc")]] = p
        elif _THIN_STAGE.search(n):
            continue
        elif n.endswith(".rcgu.bc"):
            by_cgu.setdefault(n[: -len(".bc")], p)
    return [by_cgu[k] for k in sorted(by_cgu)]


_LLC_VERSION: dict[str, bytes] = {}


def llc_object(bc: Path, llc: str, cache: Path) -> Path:
    import hashlib
    if llc not in _LLC_VERSION:
        _LLC_VERSION[llc] = subprocess.run([llc, "--version"], capture_output=True,
                                           text=True).stdout.encode()
    h = hashlib.sha256()
    h.update(_LLC_VERSION[llc])
    h.update("\0".join(LLC_FLAGS).encode())
    h.update(bc.read_bytes())
    out = cache / f"{h.hexdigest()}.o"
    if not out.exists():
        cache.mkdir(parents=True, exist_ok=True)
        tmp = out.with_suffix(".o.tmp")
        subprocess.run([llc, *LLC_FLAGS, "-o", str(tmp), str(bc)], check=True)
        tmp.replace(out)
    return out


def rlib_bitcode(rlib: Path, llvm_bin: str | None, cache: Path) -> list[Path]:
    """Each member's embedded `.llvmbc` (the prebuilt sysroot rlibs carry it).
    Extracted once per rlib content: a sysroot rlib is fixed per toolchain."""
    import hashlib
    ar = llvm_tool("llvm-ar", llvm_bin)
    objcopy = llvm_tool("llvm-objcopy", llvm_bin)
    d = cache / "rlib" / hashlib.sha256(rlib.read_bytes()).hexdigest()
    done = d / ".complete"
    if done.exists():
        return sorted(d.glob("*.bc"))
    d.mkdir(parents=True, exist_ok=True)
    subprocess.run([ar, "x", str(rlib.resolve())], cwd=d, check=True)
    out = []
    for m in sorted(d.glob("*.o")):
        bc = m.with_suffix(".bc")
        r = subprocess.run([objcopy, "--dump-section", f".llvmbc={bc}", str(m), os.devnull],
                           capture_output=True)
        if r.returncode == 0 and bc.exists() and bc.stat().st_size:
            out.append(bc)
        m.unlink()
    done.touch()
    return out


def link_map_inputs(map_path: Path, link_dirs: list[Path]) -> tuple[list[str], list[str]]:
    """The objects and archives a GNU ld link actually loaded, read from its
    `-Wl,-Map` file's `LOAD <path>` lines, so the walk sees exactly the link
    whatever the cmake generator wrote. Relative paths are the linker's cwd's
    (the first of `link_dirs` holding the file: Makefiles link in the target's
    directory, Ninja in the top one). Inputs outside every link dir (the
    toolchain's libc, libgcc, crt objects) are returned separately and not
    walked: they carry no frames, so a call into them is an external leaf, as
    with no map at all."""
    kept: list[str] = []
    dropped: list[str] = []
    seen: set[str] = set()
    roots = [d.resolve() for d in link_dirs]
    for line in map_path.read_text(errors="replace").splitlines():
        if not line.startswith("LOAD "):
            continue
        raw = line[5:].strip()
        if not raw.endswith((".o", ".obj", ".a")):
            continue
        if raw.startswith("/"):
            path = Path(raw).resolve()
        else:
            cands = [(d / raw).resolve() for d in roots]
            path = next((c for c in cands if c.exists()), cands[0])
        key = str(path)
        if key in seen:
            continue
        seen.add(key)
        (kept if any(path.is_relative_to(r) for r in roots) else dropped).append(key)
    return kept, dropped


def expand_inputs(inputs: list[str], bitcode_dirs: list[str], llvm_bin: str | None,
                  cache: Path) -> list[str]:
    """Objects and archives pass through; `.bc`, `.rlib` and every
    `--bitcode-dir` become llc objects (cached by bitcode + flags + llc)."""
    llc = llvm_tool("llc", llvm_bin)
    out: list[str] = []
    bcs: list[Path] = []
    for d in bitcode_dirs:
        bcs += pick_codegen_bitcode(list(Path(d).glob("*.bc")))  # flat dir, not a walk
    for i in inputs:
        p = Path(i)
        if p.suffix == ".bc":
            bcs.append(p)
        elif p.suffix == ".rlib":
            bcs += rlib_bitcode(p, llvm_bin, cache)
        else:
            out.append(i)
    out += [str(llc_object(bc, llc, cache)) for bc in bcs]
    return out


def load_object(g: Graph, paths: str | list[str], readobj: str, objdump: str) -> None:
    """Read objects/archives. Several paths go to ONE llvm-readobj and ONE
    llvm-objdump each: per-object processes were 5 s of a 5.3 s warm run
    (issue 2400), and every build pays that once measuring is per build."""
    if isinstance(paths, str):
        paths = [paths]
    for doc in readobj_json(readobj, ["--stack-sizes"], paths):
        for e in doc.get("StackSizes", []):
            ent = e["Entry"]
            for fn in ent["Functions"]:
                g.frames[fn] = max(g.frames.get(fn, 0), ent["Size"])
                g.functions.add(fn)

    docs = readobj_json(readobj, ["-S", "--symbols", "-r"], paths)
    for doc in docs:
        secs = {s["Section"]["Index"]: s["Section"] for s in doc.get("Sections", [])}
        # function symbols per section: (start, end, name)
        by_sec: dict[int, list[tuple[int, int, str]]] = {}
        sym_is_func: dict[str, bool] = {}
        for s in doc.get("Symbols", []):
            sym = s["Symbol"]
            name = sym["Name"]["Name"] if isinstance(sym["Name"], dict) else sym["Name"]
            typ = sym["Type"]["Name"] if isinstance(sym["Type"], dict) else sym["Type"]
            sec = sym.get("Section", {})
            idx = sec.get("Value") if isinstance(sec, dict) else None
            if typ == "Function":
                sym_is_func[name] = True
                g.functions.add(name)
                if isinstance(idx, int) and idx > 0:
                    start = sym["Value"] & ~1  # Thumb bit
                    by_sec.setdefault(idx, []).append((start, start + max(sym["Size"], 1), name))
            elif typ == "Section" and isinstance(idx, int):
                pass
        for fns in by_sec.values():
            at: dict[int, set[str]] = {}
            for start, _end, name in fns:
                at.setdefault(start, set()).add(name)
            g.alias_groups.extend(grp for grp in at.values() if len(grp) > 1)
        sec_name_funcs: dict[str, list[str]] = {}
        for idx, fns in by_sec.items():
            sec_name_funcs[secs[idx]["Name"]["Name"] if isinstance(secs[idx]["Name"], dict)
                           else secs[idx]["Name"]] = [f[2] for f in fns]

        for rel in doc.get("Relocations", []):
            rsec = secs.get(rel["SectionIndex"])
            if not rsec:
                continue
            target_idx = rsec.get("Info")
            tsec = secs.get(target_idx)
            if not tsec:
                continue
            tname = tsec["Name"]["Name"] if isinstance(tsec["Name"], dict) else tsec["Name"]
            flags = {f["Name"] for f in tsec.get("Flags", {}).get("Flags", [])}
            is_code = "SHF_EXECINSTR" in flags
            if tname.startswith((".ARM.exidx", ".eh_frame", ".debug", ".stack_sizes")):
                continue
            for r in rel.get("Relocs", []):
                rr = r["Relocation"]
                tgt = rr["Symbol"]["Name"]
                if tgt in sec_name_funcs and len(sec_name_funcs[tgt]) == 1:
                    tgt = sec_name_funcs[tgt][0]
                if not sym_is_func.get(tgt) and tgt not in g.frames:
                    # undefined here: a call to another member/archive keeps its
                    # name; data refs to non-functions are dropped below
                    if tgt.startswith(".") or not tgt:
                        continue
                if is_code:
                    caller = None
                    for start, end, name in by_sec.get(target_idx, ()):
                        if start <= rr["Offset"] < end:
                            caller = name
                            break
                    if caller and caller != tgt or caller == tgt:
                        if caller:
                            g.edges.setdefault(caller, set()).add(tgt)
                else:
                    g.address_taken.add(tgt)

    dis = subprocess.run([objdump, "-d", "--no-show-raw-insn", *paths],
                         check=True, capture_output=True, text=True).stdout
    cur = None
    for line in dis.splitlines():
        m = FUNC_LABEL_RE.match(line)
        if m:
            cur = m.group(1)
            continue
        if cur and INDIRECT_RE.search(line):
            g.indirect_sites[cur] = g.indirect_sites.get(cur, 0) + 1

    # Undefined references that are data-only were added to address_taken /
    # edges by name; anything never defined as a function is pruned at walk
    # time as "NO FRAME" unless named external.


# GCC `-fcallgraph-info=su`: one node per function, titled by its ASSEMBLER
# name (mangled for C++, `<file>:` prefixed when local) and labelled with its
# frame. Exact where a `.su` line is only a signature.
CI_NODE_RE = re.compile(r'node: \{ title: "(?P<title>[^"]+)" label: "[^"]*?\\n(?P<size>\d+) bytes')


def load_ci(g: Graph, f: Path) -> None:
    for m in CI_NODE_RE.finditer(f.read_text(errors="replace")):
        fn = m.group("title").rsplit(":", 1)[-1]
        g.frames[fn] = max(g.frames.get(fn, 0), int(m.group("size")))
        g.functions.add(fn)


def load_su(g: Graph, root: Path) -> None:
    if root.is_file():
        files = [root]
    else:
        files = sorted(root.rglob("*.su")) + sorted(root.rglob("*.ci"))  # walk-ok: .su/.ci files are untracked GCC build output in a build dir the caller names
    for f in files:
        if f.suffix == ".ci":
            load_ci(g, f)
            continue
        for line in f.read_text(errors="replace").splitlines():
            m = SU_RE.match(line)
            if not m:
                continue
            fn = m.group("fn")
            if "(" in fn:
                sig = cxx_su_signature(fn)
                g.su_signatures[sig] = max(g.su_signatures.get(sig, 0), int(m.group("size")))
                continue
            g.frames[fn] = max(g.frames.get(fn, 0), int(m.group("size")))
            g.functions.add(fn)


def cxx_su_signature(text: str) -> str:
    """`void ns::f(int) const` -> `ns::f(int) const`: drop the return type,
    which is everything before the last top-level space ahead of the first
    top-level `(`."""
    depth, cut = 0, None
    for i, ch in enumerate(text):
        if ch in "<[":
            depth += 1
        elif ch in ">]":
            depth -= 1
        elif ch == "(" and depth == 0:
            cut = i
            break
    if cut is None:
        return text
    head = text[:cut]
    depth, space = 0, -1
    for i, ch in enumerate(head):
        if ch == "<":
            depth += 1
        elif ch == ">":
            depth -= 1
        elif ch == " " and depth == 0:
            space = i
    return text[space + 1:]


def resolve_cxx_su_frames(g: Graph, demangle) -> None:
    """Give a frame to each frameless symbol whose C++ `.su` signature
    matches: an `extern "C"` function is its own name plus `()`-args, a
    mangled one is matched by its demangled text."""
    if not g.su_signatures:
        return
    bare = {}
    for sig, n in g.su_signatures.items():
        name = sig.split("(", 1)[0]
        if "::" not in name and "<" not in name:
            bare[name] = max(bare.get(name, 0), n)
    missing = [fn for fn in g.functions if fn not in g.frames]
    mangled = [fn for fn in missing if fn.startswith("_Z")]
    for fn in missing:
        if not fn.startswith(("_Z", "_R")) and fn in bare:
            g.frames[fn] = bare[fn]
    if mangled:
        for fn, text in zip(mangled, demangle(mangled)):
            if text in g.su_signatures:
                g.frames[fn] = g.su_signatures[text]


GCC_CLONE_NUM = re.compile(r"\.\d+$")


def alias_gcc_clones(g: Graph) -> None:
    """GCC names a clone `foo.isra.0` in the symbol table and `foo.isra` in
    its `.su` file, and writes ONE `.su` line for a body that identical-code
    folding gave several names. Give each such name its body's frame."""
    for fn in list(g.functions):
        if fn not in g.frames:
            base = GCC_CLONE_NUM.sub("", fn)
            if base != fn and base in g.frames:
                g.frames[fn] = g.frames[base]
    for grp in g.alias_groups:
        known = [g.frames[n] for n in grp if n in g.frames]
        if known:
            for n in grp:
                g.frames.setdefault(n, max(known))


def prune_data_refs(g: Graph) -> None:
    """A relocation from code to a symbol that is not a function anywhere
    (a static, a string) is data, not a call."""
    for caller, callees in g.edges.items():
        g.edges[caller] = {c for c in callees if c in g.functions or c in g.frames}
    g.address_taken = {f for f in g.address_taken if f in g.functions}


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------


def parse_indirect(s: str | None) -> tuple[str, int | None]:
    if s is None:
        return ("refuse", None)
    if s in ("fold", "fold1"):
        return (s, None)
    m = re.fullmatch(r"allowance:(\d+)", s)
    if m:
        return ("allowance", int(m.group(1)))
    raise SystemExit(f"--indirect: `{s}` is not `fold`, `fold1` or `allowance:<bytes>`")


# ---------------------------------------------------------------------------
# The allowance table (config/stack-floors.toml), schema 2
# ---------------------------------------------------------------------------
#
# Floors are NOT in the table: they are measured every build (issue 2400,
# ruled 2026-11-10). The table holds what a build cannot measure:
#
#   floor = ceil8( (static_now + allowance) * (100 + margin_pct) / 100 )
#
# - `[[allowance]]`, per (image, target, role) — ruled (A), 2026-11-10: an
#   allowance belongs to the IMAGE SHAPE that was measured, not to its target,
#   because the static walk's indirect charge is wrong in both directions
#   (3-8 KB low on Rust images, 12.6 KB HIGH on the C talker), so one image's
#   measurement says nothing about another's. It is the twin's run-time peak
#   minus the static walk measured at the same time, and may be NEGATIVE: a
#   measured image's floor is then ~ its peak + margin, while the static part
#   still moves with every direct frame on every build. Each names issue
#   2402, which would retire it.
# - `[[policy]]` rows: the walk's cuts, externals and stated frames, each
#   filtered by optional `rmw` / `target` and each with a reason.
# - `[indirect.<rmw>]`: how indirect sites are charged in the static walk.
#
# An image with no row is UNMEASURED: it gets the static floor, a loud
# warning naming issue 2402, and `measured = false` in the result. A consumer
# (W4's CHECK) never refuses an unmeasured image — its static floor can be off
# by more than 12 KB either way — and refuses a measured one only.

ROLES = ("OPEN", "SPIN")
ISSUE_ID = re.compile(r"^\d{4}$")
POLICY_KINDS = ("cut", "external", "frame")


def floor_formula(static_bytes: int, allowance: int, margin_pct: int) -> int:
    raw = -(-max(0, static_bytes + allowance) * (100 + margin_pct) // 100)
    return -(-raw // 8) * 8


def load_toml(path: Path) -> dict:
    try:
        import tomllib  # Python 3.11+
    except ModuleNotFoundError:  # pragma: no cover - 3.10 hosts
        import tomli as tomllib
    with open(path, "rb") as f:
        return tomllib.load(f)


def _matches(row: dict, rmw: str | None, target: str | None) -> bool:
    return (row.get("rmw") in (None, rmw)) and (row.get("target") in (None, target))


def find_allowance(doc: dict, image: str | None, target: str, role: str) -> dict | None:
    if not image:
        return None
    hits = [a for a in doc.get("allowance", [])
            if (a.get("image"), a.get("target"), a.get("role")) == (image, target, role)]
    return hits[0] if hits else None


def validate_table(doc: dict, issue_ids: set[str]) -> list[str]:
    """Offline checks: needs no build output, so it can run on the fast line."""
    problems: list[str] = []
    if doc.get("schema") != 2:
        problems.append("`schema = 2` is missing or not 2")
    if not isinstance(doc.get("margin_pct"), int):
        problems.append("`margin_pct` (an integer percent) is missing")
    for i, row in enumerate(doc.get("policy", [])):
        where = f"[[policy]] #{i + 1}"
        kind = row.get("kind")
        if kind not in POLICY_KINDS:
            problems.append(f"{where}: kind must be one of {POLICY_KINDS}")
        key = "match" if kind == "cut" or (kind == "external" and "match" in row) else "name"
        if not row.get(key):
            problems.append(f"{where}: a {kind} needs `{key}`")
        if kind == "frame" and not isinstance(row.get("bytes"), int):
            problems.append(f"{where}: a frame needs integer `bytes`")
        if not str(row.get("reason", "")).strip():
            problems.append(f"{where}: no reason ({row.get(key)})")
    seen: set[tuple] = set()
    for i, a in enumerate(doc.get("allowance", [])):
        k = (a.get("image"), a.get("target"), a.get("role"))
        where = f"[[allowance]] #{i + 1} {k}"
        if None in k or not a.get("rmw"):
            problems.append(f"{where}: needs `image`, `rmw`, `target` and `role`")
        if k in seen:
            problems.append(f"{where}: duplicate")
        seen.add(k)
        if a.get("role") not in ROLES:
            problems.append(f"{where}: role must be one of {ROLES}")
        if not str(a.get("reason", "")).strip():
            problems.append(f"{where}: no reason")
        iid = str(a.get("issue", ""))
        if not ISSUE_ID.match(iid):
            problems.append(f"{where}: must name the issue that retires it "
                            f"(`issue = \"NNNN\"`), got `{iid}`")
        elif issue_ids and iid not in issue_ids:
            problems.append(f"{where}: names issue {iid}, which has no file")
        m = a.get("measured") or {}
        for f in ("peak", "static", "method", "issue"):
            if f not in m:
                problems.append(f"{where}: measured.{f} missing")
        if isinstance(m.get("peak"), int) and isinstance(m.get("static"), int):
            if a.get("bytes") != m["peak"] - m["static"]:
                problems.append(f"{where}: bytes = {a.get('bytes')}, but measured.peak - "
                                f"measured.static = {m['peak'] - m['static']}")
    return problems


def apply_policy(args: argparse.Namespace, doc: dict) -> None:
    """Fill the walk's options from the table rows that match --rmw/--target
    (explicit flags still add)."""
    for row in doc.get("policy", []):
        if not _matches(row, args.rmw, args.target):
            continue
        if row["kind"] == "cut":
            args.cut = (args.cut or []) + [row["match"]]
        elif row["kind"] == "external":
            if "match" in row:
                args.external_match = (getattr(args, "external_match", None) or []) + [row["match"]]
            else:
                args.external = (args.external or []) + [row["name"]]
        else:
            args.frame = (args.frame or []) + [f"{row['name']}={row['bytes']}"]
    ind = doc.get("indirect", {})
    ind = ind.get(args.rmw or "") or ind.get("default", {})
    args.indirect = args.indirect or ind.get("mode")
    args.fold_scope = (args.fold_scope or []) + list(ind.get("scope", []))
    if args.recursion_allowance is None:
        args.recursion_allowance = doc.get("recursion_allowance")
    if args.margin is None:
        args.margin = doc.get("margin_pct", 0)


def knob_text(knob: str, floor: int) -> str:
    """`--stack-knob` names what raises the stack, with `{bytes}` / `{kb}`
    standing for the floor in that knob's unit (`{kb}` rounds up)."""
    if "{" not in knob:
        return f"{knob} >= {floor}"
    return knob.format(bytes=floor, kb=-(-floor // 1024))


def check_stacks(results: dict, stacks: dict[str, int], knob: str) -> tuple[int, list[str]]:
    """A build's CHECK (issue 2400): a MEASURED role whose floor exceeds its
    stated stack fails, naming the knob that raises it; an UNMEASURED one only
    warns (ruling (A)). A stack is never lowered here."""
    rc, lines = 0, []
    for role, stack in stacks.items():
        r = results.get(role)
        if r is None:
            continue
        if r["floor"] <= stack:
            lines.append(f"nros-stack-floor: {role} stack {stack} B holds the floor "
                         f"{r['floor']} B ({'measured' if r['measured'] else 'unmeasured'})")
        elif r["measured"]:
            rc = 1
            lines.append(f"nros-stack-floor: ERROR: the {role} stack is {stack} B, below the "
                         f"measured floor {r['floor']} B. Raise it: "
                         f"{knob_text(knob, r['floor'])} (issue 2400)")
        else:
            lines.append(f"nros-stack-floor: WARNING: the {role} stack is {stack} B, below the "
                         f"UNMEASURED floor {r['floor']} B. Not enforced (no [[allowance]] row); "
                         f"if the image overflows, {knob_text(knob, r['floor'])} is the first "
                         "thing to try")
    return rc, lines


def parse_check_stacks(args: argparse.Namespace) -> dict[str, int]:
    stacks: dict[str, int] = {}
    for spec in args.check_stack or []:
        role, _, n = spec.partition("=")
        if not n.isdigit():
            raise SystemExit(f"--check-stack `{spec}`: write ROLE=<bytes>")
        stacks[role.upper()] = int(n)
    return stacks


def refused(args: argparse.Namespace, doc, refusals: list[str]) -> int:
    """Every walk refusal (the --indirect fold's and a role's) ends here, so
    ruling (A) holds for both: under --check-stack an UNMEASURED image is never
    refused, the walk included; a measured one fails."""
    print("nros-stack-floor: REFUSED\n" + "\n".join(refusals), file=sys.stderr)
    stacks = parse_check_stacks(args)
    measured_roles = [r for r in stacks
                      if doc is not None and find_allowance(doc, args.image, args.target, r)]
    if stacks and not measured_roles:
        print("nros-stack-floor: WARNING: the walk was refused for an UNMEASURED image; "
              "not enforced (issue 2400)", file=sys.stderr)
        return 0
    return 1


def run(args: argparse.Namespace) -> int:
    doc = None
    if args.table:
        doc = load_toml(Path(args.table))
        apply_policy(args, doc)
    if args.margin is None:
        args.margin = 0
    readobj = llvm_tool("llvm-readobj", args.llvm_bin)
    objdump = llvm_tool("llvm-objdump", args.llvm_bin)
    g = Graph()
    cache = Path(args.llc_cache or Path.home() / ".cache" / "nros-stack-floor")
    if args.link_map:
        mp = Path(args.link_map)
        kept, dropped = link_map_inputs(mp, [Path(d) for d in (args.link_dir or [mp.parent])])
        if not kept:
            raise SystemExit(f"nros-stack-floor: {mp} names no input under the link directory: "
                             "the map is not a GNU ld map, or --link-dir is wrong")
        args.inputs = kept + list(args.inputs)
        if dropped:
            print(f"nros-stack-floor: {len(dropped)} toolchain input(s) outside the link "
                  "directory not walked (calls into them are external leaves)", file=sys.stderr)
    if args.sysroot:
        if not args.target:
            raise SystemExit("nros-stack-floor: --sysroot needs --target")
        args.inputs = list(args.inputs) + sysroot_rlibs(args.target)
    args.inputs = expand_inputs(args.inputs, bitcode_dirs_expanded(args.bitcode_dir or []),
                                args.llvm_bin, cache)
    for i in range(0, len(args.inputs), LOAD_CHUNK):
        load_object(g, args.inputs[i:i + LOAD_CHUNK], readobj, objdump)
    for su in args.su or []:
        load_su(g, Path(su))
    for spec in args.frame or []:
        name, _, n = spec.partition("=")
        if not n.isdigit():
            raise SystemExit(f"--frame `{spec}`: write NAME=BYTES")
        if name in g.frames:
            raise SystemExit(f"--frame `{name}`: an input already states its frame "
                             f"({g.frames[name]} B); a stated frame never overrides a measured one")
        g.frames[name] = int(n)
    resolve_cxx_su_frames(g, lambda names: subprocess.run(
        [cxx_demangler(args.cxxfilt, args.llvm_bin)], input="\n".join(names), check=True,
        capture_output=True, text=True).stdout.splitlines())
    alias_gcc_clones(g)
    prune_data_refs(g)
    externals = set(args.external or [])
    for sub in getattr(args, "external_match", None) or []:
        # a substring: a leaf whose mangled name carries a per-build hash
        externals |= {fn for fn in g.functions if sub in fn and fn not in g.frames}
    cuts = tuple(args.cut or [])

    mode, allowance = parse_indirect(args.indirect)
    if mode in ("fold", "fold1"):
        try:
            charge = fold_charge(g, recursion_allowance=args.recursion_allowance,
                                 externals=externals, cuts=cuts,
                                 scope=tuple(args.fold_scope or []),
                                 one_level=(mode == "fold1"))
        except WalkRefused as e:
            return refused(args, doc, [str(e)])
    elif mode == "allowance":
        charge = allowance
    else:
        charge = None

    results = {}
    refusals = []
    for spec in args.role:
        role, _, roots = spec.partition("=")
        if not roots:
            raise SystemExit(f"--role `{spec}`: write ROLE=root[,root...]")
        best: Path_ | None = None
        best_root = ""
        for root in roots.split(","):
            try:
                p = worst_path(g, root, indirect_charge=charge,
                               recursion_allowance=args.recursion_allowance,
                               externals=externals, cuts=cuts)
            except WalkRefused as e:
                refusals.append(f"{role}: {e}")
                continue
            if best is None or p.depth > best.depth:
                best, best_root = p, root
        if best is not None:
            allowance, measured_by = 0, None
            if doc is not None:
                a = find_allowance(doc, args.image, args.target, role.upper())
                if a is not None:
                    allowance, measured_by = int(a["bytes"]), a
                else:
                    print(f"nros-stack-floor: WARNING: image `{args.image}` ({args.target}, "
                          f"{role.upper()}) is UNMEASURED. Its {role.upper()} floor is the static "
                          f"walk ({best.depth} B) + margin, which can be off by more than 12 KB "
                          "either way, because calls through the RMW vtable are charged, not "
                          "followed (issue 2402). It is reported, never enforced. Measure a twin "
                          f"peak and add an [[allowance]] row to {args.table}.",
                          file=sys.stderr)
            results[role.upper()] = {
                "floor": floor_formula(best.depth, allowance, args.margin),
                "depth": best.depth, "allowance": allowance,
                "allowance_issue": measured_by.get("issue") if measured_by else None,
                "measured": measured_by is not None,
                "root": best_root,
                "margin_pct": args.margin, "indirect_charged": best.indirect,
                "indirect_mode": mode, "indirect_charge": charge,
                "path": best.chain,
            }
    stacks = parse_check_stacks(args)
    if refusals:
        return refused(args, doc, refusals)
    if stacks:
        rc, lines = check_stacks(results, stacks, args.stack_knob or "the stack size")
        for line in lines:
            print(line, file=sys.stderr)
        if rc:
            return rc
    if args.json:
        print(json.dumps(results, indent=2))
    elif args.emit_defines:
        for role, r in results.items():
            print(f"#define NROS_STACK_FLOOR_{role} {r['floor']}")
            # 0 = unmeasured: a consumer reports the floor and never refuses on it
            print(f"#define NROS_STACK_FLOOR_{role}_MEASURED {int(r['measured'])}")
    else:
        for role, r in results.items():
            print(f"{role}: floor {r['floor']} B (path {r['depth']} B + allowance "
                  f"{r['allowance']} B + {r['margin_pct']}%)"
                  f" from `{r['root']}`, indirect={r['indirect_mode']}"
                  f"({r['indirect_charge']}) charged x{r['indirect_charged']}")
            for fn in r["path"]:
                print(f"    {g.frames.get(fn, ''):>7}  {fn}")
    return 0


# ---------------------------------------------------------------------------
# Self-test (pure graph; no toolchain needed)
# ---------------------------------------------------------------------------


def self_test() -> int:
    fails = []

    ran = []

    def check(name, cond):
        ran.append(name)
        if not cond:
            fails.append(name)

    g = Graph(frames={"root": 100, "a": 50, "b": 400, "c": 10},
              edges={"root": {"a", "b"}, "a": {"c"}},
              functions={"root", "a", "b", "c"})
    p = worst_path(g, "root", indirect_charge=None, recursion_allowance=None)
    check("deepest of two branches", p.depth == 500 and p.chain == ["root", "b"])

    # a callee with no frame is refused, never counted as 0
    g2 = Graph(frames={"root": 100}, edges={"root": {"ghost"}}, functions={"root", "ghost"})
    try:
        worst_path(g2, "root", indirect_charge=None, recursion_allowance=None)
        check("missing frame refused", False)
    except WalkRefused as e:
        check("missing frame names the function", "ghost" in str(e))
    # ... unless declared external
    p = worst_path(g2, "root", indirect_charge=None, recursion_allowance=None,
                   externals={"ghost"})
    check("external leaf counts 0", p.depth == 100)

    # recursion refused without an allowance, bounded with one
    g3 = Graph(frames={"r": 10, "x": 20}, edges={"r": {"x"}, "x": {"r"}}, functions={"r", "x"})
    try:
        worst_path(g3, "r", indirect_charge=None, recursion_allowance=None)
        check("recursion refused", False)
    except WalkRefused as e:
        check("recursion names the cycle", "r -> x -> r" in str(e))
    p = worst_path(g3, "r", indirect_charge=None, recursion_allowance=1000)
    check("recursion allowance added", p.depth == 1030)

    # an indirect site is refused, charged by allowance, or folded
    g4 = Graph(frames={"open": 100, "vt_big": 900, "vt_small": 5, "helper": 50},
               edges={"vt_big": {"helper"}},
               indirect_sites={"open": 3},
               address_taken={"vt_big", "vt_small"},
               functions={"open", "vt_big", "vt_small", "helper"})
    try:
        worst_path(g4, "open", indirect_charge=None, recursion_allowance=None)
        check("indirect refused", False)
    except WalkRefused as e:
        check("indirect refusal names the function", "open" in str(e))
    p = worst_path(g4, "open", indirect_charge=200, recursion_allowance=None)
    check("indirect allowance", p.depth == 300)
    charge = fold_charge(g4, recursion_allowance=None, externals=set())
    check("fold = deepest address-taken path", charge == 950)
    p = worst_path(g4, "open", indirect_charge=charge, recursion_allowance=None)
    check("folded depth", p.depth == 1050)
    g7 = Graph(frames={"open": 10, "vt": 5}, edges={"vt": {"ghost"}}, indirect_sites={"open": 1},
               address_taken={"vt"}, functions={"open", "vt", "ghost"})
    try:
        fold_charge(g7, recursion_allowance=None, externals=set())
        check("fold refuses a refused target", False)
    except WalkRefused as e:
        check("fold names the target's missing frame", "ghost" in str(e))
    check("fold scope narrows targets",
          fold_charge(g4, recursion_allowance=None, externals=set(), scope=("small",)) == 5)

    # fold converges when a vtable function itself calls indirectly
    g5 = Graph(frames={"vt": 100}, indirect_sites={"vt": 1}, address_taken={"vt"},
               functions={"vt"})
    try:
        fold_charge(g5, recursion_allowance=None, externals=set())
        check("non-converging fold refused", False)
    except WalkRefused:
        pass
    check("fold1 charges one level", fold_charge(g5, recursion_allowance=None,
                                                 externals=set(), one_level=True) == 100)

    # GCC callgraph-info
    import tempfile as _tf
    with _tf.TemporaryDirectory() as d:
        Path(d, "t.ci").write_text(
            'graph: { title: "t.cpp"\n'
            'node: { title: "t.cpp:_ZZ3appENKUliE_clEi" label: "app(int)::<lambda(int)>\\nt.cpp:2:35\\n40 bytes (static)" }\n'
            'node: { title: "app" label: "int app(int)\\nt.cpp:2:16\\n16 bytes (static)" }\n'
            'node: { title: "memcpy" label: "memcpy\\n<built-in>" shape : ellipse }\n}\n')
        g8 = Graph()
        load_su(g8, Path(d))
        check(".ci frames by assembler name, local prefix dropped, externals skipped",
              g8.frames == {"_ZZ3appENKUliE_clEi": 40, "app": 16})
    # C++ .su signatures
    m = SU_RE.match("/a/b.cpp:74:18:rclcpp::Result fx::S::configure(rclcpp::Node&)\t80\tstatic")
    check("c++ .su line keeps its qualified signature",
          m is not None and m.group("fn") == "rclcpp::Result fx::S::configure(rclcpp::Node&)")
    check("c++ .su: return type dropped",
          cxx_su_signature("void app_main()") == "app_main()"
          and cxx_su_signature("std::pair<int, int> ns::f(int) const") == "ns::f(int) const"
          and cxx_su_signature("int nros_app_main(int, char**)") == "nros_app_main(int, char**)")
    g7 = Graph()
    g7.functions |= {"app_main", "_ZN2ns1fEi", "_Z4gonev"}
    g7.su_signatures = {"app_main()": 32, "ns::f(int)": 24}
    demangled = {"_ZN2ns1fEi": "ns::f(int)", "_Z4gonev": "gone()"}
    resolve_cxx_su_frames(g7, lambda ns: [demangled[n] for n in ns])
    check("c++ .su: extern C by name, mangled by demangled text",
          g7.frames == {"app_main": 32, "_ZN2ns1fEi": 24})
    # the build's CHECK
    res = {"OPEN": {"floor": 40000, "measured": True}}
    rc, lines = check_stacks(res, {"OPEN": 32768}, "K")
    check("check: a measured floor above the stack fails, naming the knob",
          rc == 1 and "K >= 40000" in lines[0])
    res = {"OPEN": {"floor": 40000, "measured": False}}
    rc, lines = check_stacks(res, {"OPEN": 32768}, "K")
    check("check: an unmeasured floor above the stack only warns",
          rc == 0 and "WARNING" in lines[0])
    rc, lines = check_stacks({"OPEN": {"floor": 100, "measured": True}}, {"OPEN": 65536}, "K")
    check("check: a stack at or above the floor passes", rc == 0 and "holds" in lines[0])
    check("knob template in the knob's unit",
          knob_text("NROS_FREERTOS_APP_STACK_KB={kb}", 15216) == "NROS_FREERTOS_APP_STACK_KB=15")
    # link map
    import tempfile
    with tempfile.TemporaryDirectory() as d:
        Path(d, "a.obj").write_text(""); Path(d, "lib").mkdir(); Path(d, "lib", "b.a").write_text("")
        Path(d, "m.map").write_text(
            "Archive member included to satisfy reference by file (symbol)\n\n"
            "LOAD a.obj\nLOAD lib/b.a\nLOAD /opt/gcc/lib/libc_nano.a\nLOAD lib/b.a\n"
            "LOAD linker stubs\n")
        kept, dropped = link_map_inputs(Path(d, "m.map"), [Path(d, "nope"), Path(d)])
        check("link map: build-dir inputs kept once, in order",
              [Path(k).name for k in kept] == ["a.obj", "b.a"])
        check("link map: toolchain inputs dropped", dropped == [str(Path("/opt/gcc/lib/libc_nano.a").resolve())])
    # .su parsing
    import tempfile
    with tempfile.TemporaryDirectory() as d:
        Path(d, "x.su").write_text("src/x.c:12:5:_z_open\t48\tstatic\n"
                                   "src/x.c:30:6:helper\t16\tdynamic,bounded\n")
        g6 = Graph()
        load_su(g6, Path(d))
        check(".su frames", g6.frames == {"_z_open": 48, "helper": 16})
        g6.frames["notify.isra"] = 12
        g6.functions.add("notify.isra.0")
        alias_gcc_clones(g6)
        check("GCC clone takes its .su frame", g6.frames.get("notify.isra.0") == 12)
        g6.alias_groups.append({"_z_keyexpr_intmap_make", "_z_declared_keyexpr_intmap_make"})
        g6.frames["_z_keyexpr_intmap_make"] = 8
        alias_gcc_clones(g6)
        check("ICF alias takes the body's frame",
              g6.frames.get("_z_declared_keyexpr_intmap_make") == 8)

    # a cut function counts its own frame, not its callees
    gc = Graph(frames={"root": 10, "panic": 30, "deep": 999}, edges={"root": {"panic"}, "panic": {"deep", "panic"}},
               functions={"root", "panic", "deep"})
    p = worst_path(gc, "root", indirect_charge=None, recursion_allowance=None, cuts=("panic",))
    check("cut is a leaf", p.depth == 40)

    # the floor formula: (static + allowance) * margin, align 8 — an addend, not a max
    check("floor = static + allowance + margin", floor_formula(26232, 2952, 10) == 32104)
    check("no allowance: static + margin", floor_formula(3592, 0, 0) == 3592)
    check("a negative allowance lowers the floor", floor_formula(24312, -12616, 10) == 12872)
    check("the floor never goes below zero", floor_formula(100, -500, 10) == 0)
    good = {"schema": 2, "margin_pct": 10,
            "policy": [{"kind": "cut", "match": "panicking", "reason": "diverges"},
                       {"kind": "frame", "name": "z_malloc", "bytes": 0, "target": "t",
                        "reason": "tail call"},
                       {"kind": "external", "match": "3mem6memcpy", "reason": "leaf"}],
            "allowance": [{"image": "img", "rmw": "zenoh", "target": "t", "role": "OPEN",
                           "bytes": 2952, "issue": "2402", "reason": "RMW vtable",
                           "measured": {"peak": 29184, "static": 26232, "method": "twin",
                                        "issue": "2360"}},
                          {"image": "c", "rmw": "zenoh", "target": "t", "role": "OPEN",
                           "bytes": -12616, "issue": "2402", "reason": "over-charged",
                           "measured": {"peak": 11696, "static": 24312, "method": "twin",
                                        "issue": "2400"}}]}
    check("a valid table passes (a negative allowance included)",
          validate_table(good, {"2360", "2402", "2400"}) == [])
    bad = json.loads(json.dumps(good))
    bad["allowance"][0]["bytes"] = 3000
    check("an allowance off peak - static is refused",
          any("measured.peak - measured.static = 2952" in p for p in validate_table(bad, set())))
    bad = json.loads(json.dumps(good))
    bad["policy"][0]["reason"] = ""
    check("a policy row with no reason is refused",
          any("no reason" in p for p in validate_table(bad, set())))
    bad = json.loads(json.dumps(good))
    bad["allowance"][0]["issue"] = "99"
    check("an allowance must name an issue",
          any("must name the issue" in p for p in validate_table(bad, set())))
    bad = json.loads(json.dumps(good))
    bad["allowance"].append(dict(bad["allowance"][0]))
    check("a duplicate allowance is refused", any("duplicate" in p for p in validate_table(bad, set())))
    bad = json.loads(json.dumps(good))
    del bad["allowance"][0]["image"]
    check("an allowance needs its image", any("needs `image`" in p for p in validate_table(bad, set())))
    check("allowance lookup is by (image, target, role)",
          find_allowance(good, "img", "t", "OPEN") is not None
          and find_allowance(good, "img", "t", "SPIN") is None
          and find_allowance(good, "other", "t", "OPEN") is None
          and find_allowance(good, None, "t", "OPEN") is None)
    ns = argparse.Namespace(rmw="zenoh", target="other", cut=None, external=None, frame=None,
                            indirect=None, fold_scope=None, recursion_allowance=None, margin=None)
    apply_policy(ns, good)
    check("a target-filtered policy row applies only to its target",
          ns.cut == ["panicking"] and not ns.frame and ns.margin == 10)
    check("an external by substring is carried as a match",
          getattr(ns, "external_match", None) == ["3mem6memcpy"])

    picked = [p.name for p in pick_codegen_bitcode([Path(n) for n in (
        "a.cgu.0.rcgu.bc", "a.cgu.0.rcgu.no-opt.bc", "b.cgu.0.rcgu.bc", "b.cgu.0.rcgu.no-opt.bc",
        "b.cgu.0.rcgu.thin-lto-input.bc", "b.cgu.0.rcgu.thin-lto-after-pm.bc")])]
    check("codegen bitcode: post-opt, or ThinLTO after-pm when present",
          picked == ["a.cgu.0.rcgu.bc", "b.cgu.0.rcgu.thin-lto-after-pm.bc"])
    picked = [p.name for p in pick_codegen_bitcode([Path(n) for n in (
        "dep.x.cgu.0.rcgu.bc", "app.y.cgu.0.rcgu.bc", "app.y.cgu.0.rcgu.no-opt.bc",
        "app.y.cgu.0.rcgu.lto.input.bc", "app.y.cgu.0.rcgu.lto.after-restriction.bc")])]
    check("fat LTO: only the merged post-LTO module", picked == ["app.y.cgu.0.rcgu.bc"])

    # The rustup llvm-tools ship no llvm-cxxfilt: c++filt stands in, and with
    # neither the demangler is a ToolMissing, never a bare exit.
    g_ = globals()
    saved = (g_["llvm_tool"], shutil.which)
    try:
        def no_llvm(name, llvm_bin):
            raise ToolMissing(f"no `{name}`")
        g_["llvm_tool"] = no_llvm
        shutil.which = lambda n: "/bin/c++filt" if n == "c++filt" else None
        check("demangler: c++filt when no llvm-cxxfilt", cxx_demangler(None, None) == "/bin/c++filt")
        check("demangler: --cxxfilt wins", cxx_demangler("/x/arm-none-eabi-c++filt", None)
              == "/x/arm-none-eabi-c++filt")
        shutil.which = lambda n: None
        try:
            cxx_demangler(None, None)
            check("demangler: none present is ToolMissing", False)
        except ToolMissing:
            check("demangler: none present is ToolMissing", True)
    finally:
        g_["llvm_tool"], shutil.which = saved

    # ruling (A): a CHECK whose walk cannot run degrades to a warning for an
    # unmeasured image, and still fails for a measured one.
    import tempfile, io, contextlib
    with tempfile.TemporaryDirectory() as td:
        table = Path(td, "t.toml")
        table.write_text('[[allowance]]\nimage = "m@b"\ntarget = "t"\nrole = "OPEN"\n'
                         'bytes = 0\nissue = "0000"\n')
        saved_run, saved_argv = g_["run"], sys.argv
        try:
            def tool_gone(args):
                raise ToolMissing("nros-stack-floor: no `llvm-cxxfilt`")
            g_["run"] = tool_gone

            def gate(image):
                sys.argv = ["x", "--table", str(table), "--rmw", "zenoh", "--target", "t",
                            "--image", image, "--role", "OPEN=r", "--check-stack",
                            "OPEN=4096", "--link-map", "m.map"]
                with contextlib.redirect_stderr(io.StringIO()):
                    try:
                        return main()
                    except SystemExit:
                        return "exit"
            measured_row = find_allowance(load_toml(table), "m@b", "t", "OPEN") is not None
            check("missing tool: unmeasured image warns, rc 0", gate("u@b") == 0)
            check("missing tool: the test table's row is found", measured_row)
            check("missing tool: measured image still fails", gate("m@b") == "exit")
        finally:
            g_["run"], sys.argv = saved_run, saved_argv

        # The --indirect fold's own refusal degrades the same way. It once
        # returned 1 before the unmeasured guard ran, which failed four
        # FreeRTOS C++ mps2-an385 images that have no allowance row (root
        # `service_server_is_available_trampoline`, frameless zpico_graph_*).
        stubbed = ("fold_charge", "llvm_tool", "expand_inputs")
        saved_fold, saved_argv = {k: g_[k] for k in stubbed}, sys.argv
        folded = []
        try:
            # no toolchain and no objects: the run reaches the fold with an
            # empty graph, which is all this case is about
            g_["llvm_tool"] = lambda name, _bin=None: name
            g_["expand_inputs"] = lambda inputs, *a: []
            def fold_refuses(*a, **k):
                folded.append(1)
                raise WalkRefused("root `service_server_is_available_trampoline`:\n  - "
                                  "functions reached with no frame in any input: "
                                  "zpico_graph_entry_at")
            g_["fold_charge"] = fold_refuses

            def fold_gate(image, check_stack=True):
                sys.argv = ["x", "--table", str(table), "--rmw", "zenoh", "--target", "t",
                            "--image", image, "--role", "OPEN=r", "--indirect", "fold",
                            "app.o"]
                if check_stack:
                    sys.argv += ["--check-stack", "OPEN=4096"]
                err = io.StringIO()
                with contextlib.redirect_stderr(err):
                    try:
                        rc = main()
                    except SystemExit as e:
                        rc = "exit" if e.code not in (None, 0) else 0
                return rc, err.getvalue()
            rc, err = fold_gate("u@b")
            check("refused fold: the fold ran", folded != [])
            check("refused fold: unmeasured image under CHECK warns, rc 0",
                  rc == 0 and "UNMEASURED" in err and "REFUSED" in err)
            rc, err = fold_gate("m@b")
            check("refused fold: measured image under CHECK fails", rc not in (0, None))
            rc, err = fold_gate("u@b", check_stack=False)
            check("refused fold: no CHECK still fails", rc not in (0, None))
        finally:
            g_.update(saved_fold)
            sys.argv = saved_argv

    for f in fails:
        print(f"FAIL: {f}", file=sys.stderr)
    print(f"nros-stack-floor self-test: {len(ran)} checks, {len(fails)} failed")
    return 1 if fails else 0


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("inputs", nargs="*",
                    help="archives (.a) / objects (.o) carrying .stack_sizes, or bitcode "
                         "(.bc) / rlibs with .llvmbc, which llc compiles with fixed flags")
    ap.add_argument("--bitcode-dir", action="append",
                    help="a `-C save-temps` deps dir; each codegen unit's optimised module")
    ap.add_argument("--llc-cache", help="llc output cache, keyed by llc + flags + bitcode "
                    "(default ~/.cache/nros-stack-floor)")
    ap.add_argument("--role", action="append", default=[],
                    help="ROLE=root[,root...]; the floor is the deepest root")
    ap.add_argument("--su", action="append", help="a .su file or a dir searched for *.su")
    ap.add_argument("--link-map", help="a GNU ld `-Wl,-Map` file: walk the inputs its LOAD "
                    "lines name (added to any positional inputs)")
    ap.add_argument("--link-dir", action="append",
                    help="the linker's working directory, which relative LOAD paths are under; "
                    "repeat for candidates (default: the map's directory)")
    ap.add_argument("--check-stack", action="append",
                    help="ROLE=<bytes>: the stated stack to CHECK (a measured image whose floor "
                    "exceeds it fails; an unmeasured one warns)")
    ap.add_argument("--stack-knob", help="what the user sets to raise the stack, for the "
                    "message; `{bytes}` / `{kb}` stand for the floor in the knob's unit")
    ap.add_argument("--sysroot", action="store_true",
                    help="also walk the target's core / alloc / compiler_builtins rlibs "
                    "from the pinned toolchain's sysroot (needs --target)")
    ap.add_argument("--indirect", help="`fold` or `allowance:<bytes>`; default refuses")
    ap.add_argument("--recursion-allowance", type=int)
    ap.add_argument("--external", action="append",
                    help="a callee with no definition in the inputs accepted as a 0-byte leaf")
    ap.add_argument("--frame", action="append",
                    help="NAME=BYTES: a frame no input states (hand assembly, a C wrapper "
                         "built without -fstack-usage), read off its prologue by the caller. "
                         "Refused if an input states one")
    ap.add_argument("--fold-scope", action="append",
                    help="a substring; under --indirect fold only address-taken functions "
                         "matching one are charged at an indirect site (a static walk cannot "
                         "tie a dyn call to its trait's vtable, and exported entry points are "
                         "address-taken too). Default: every address-taken function")
    ap.add_argument("--cut", action="append",
                    help="a substring; a matching function is a LEAF (its frame counts, its "
                         "callees are not followed). For diverging paths such as the panic "
                         "machinery, stated by the caller, never implied")
    ap.add_argument("--margin", type=int, default=None,
                    help="percent added to the path (default: the table's margin_pct, else 0)")
    ap.add_argument("--llvm-bin", help="directory holding llvm-readobj / llvm-objdump")
    ap.add_argument("--cxxfilt", help="an Itanium demangler (default: llvm-cxxfilt, "
                    "then c++filt)")
    ap.add_argument("--json", action="store_true")
    ap.add_argument("--emit-defines", action="store_true")
    ap.add_argument("--table", help="config/stack-floors.toml: the walk policy for --rmw / "
                    "--target, the margin, and the measured allowance added to each role")
    ap.add_argument("--rmw", help="the image's RMW backend (selects policy and allowance)")
    ap.add_argument("--target", help="the image's target triple (selects policy and allowance)")
    ap.add_argument("--image", help="the image's shape id (selects its measured allowance; "
                    "an image with no row is unmeasured, reported and never enforced)")
    ap.add_argument("--validate-table", metavar="TABLE",
                    help="offline: every reason, allowance issue and floor formula holds")
    ap.add_argument("--self-test", action="store_true")
    args = ap.parse_args()
    if args.self_test:
        return self_test()
    if args.validate_table:
        # The negative control runs on the normal path: a broken formula or
        # validator must not be able to pass the table.
        if self_test() != 0:
            return 1
        root = Path(args.validate_table).resolve().parent
        while root != root.parent and not (root / "docs" / "issues").is_dir():
            root = root.parent
        ids = {p.name[:4] for p in (root / "docs" / "issues").glob("[0-9][0-9][0-9][0-9]-*.md")}
        ids |= {p.name[:4] for p in (root / "docs" / "issues" / "archived").glob("[0-9][0-9][0-9][0-9]-*.md")}
        problems = validate_table(load_toml(Path(args.validate_table)), ids)
        for p in problems:
            print(f"  - {p}", file=sys.stderr)
        print(f"{args.validate_table}: {len(problems)} problem(s)")
        return 1 if problems else 0
    if args.table and not (args.rmw and args.target):
        ap.error("--table needs --rmw and --target (they select the policy and allowance)")
    if not (args.inputs or args.link_map) or not args.role:
        ap.error("need INPUT(s) and at least one --role")
    if not args.check_stack:
        return run(args)
    try:
        return run(args)
    except SystemExit as e:
        # ruling (A): a CHECK of an UNMEASURED image never refuses, whether
        # the walk refused or could not run at all (a missing tool, an input
        # the build did not produce). A measured image keeps the failure.
        if not isinstance(e.code, str) or checked_image_measured(args):
            raise
        print(e.code, file=sys.stderr)
        print("nros-stack-floor: WARNING: the floor of an UNMEASURED image could not be "
              "computed; not enforced (issue 2400)", file=sys.stderr)
        return 0


def checked_image_measured(args: argparse.Namespace) -> bool:
    """Does any --check-stack role of this image have an allowance row?"""
    if not args.table:
        return False
    doc = load_toml(Path(args.table))
    return any(find_allowance(doc, args.image, args.target, spec.partition("=")[0].upper())
               for spec in args.check_stack)


if __name__ == "__main__":
    sys.exit(main())
