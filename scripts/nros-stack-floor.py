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
SU_RE = re.compile(r"^(?P<loc>[^\t]*):(?P<fn>[^:\t]+)\t(?P<size>\d+)\t(?P<kind>\S+)")


# The checkout this script lives in. rustup picks the toolchain by the CWD's
# `rust-toolchain.toml`, so asking `rustc` from anywhere else answers with the
# user's default toolchain — and llc from a different LLVM refuses the pinned
# compiler's bitcode ("Not an int attribute", measured: 1.98.1 vs 1.99.0).
CHECKOUT = Path(__file__).resolve().parent.parent
_TOOLS: dict[str, str] = {}


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
    sys.exit(f"nros-stack-floor: no `{name}`: install the toolchain's llvm-tools "
             "component or pass --llvm-bin")


def readobj_json(tool: str, args: list[str], path: str) -> list[dict]:
    out = subprocess.run([tool, "--elf-output-style=JSON", *args, path],
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


def load_object(g: Graph, path: str, readobj: str, objdump: str) -> None:
    for doc in readobj_json(readobj, ["--stack-sizes"], path):
        for e in doc.get("StackSizes", []):
            ent = e["Entry"]
            for fn in ent["Functions"]:
                g.frames[fn] = max(g.frames.get(fn, 0), ent["Size"])
                g.functions.add(fn)

    docs = readobj_json(readobj, ["-S", "--symbols", "-r"], path)
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

    dis = subprocess.run([objdump, "-d", "--no-show-raw-insn", path],
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


def load_su(g: Graph, root: Path) -> None:
    files = [root] if root.is_file() else sorted(root.rglob("*.su"))  # walk-ok: .su files are untracked GCC build output in a build dir the caller names
    for f in files:
        for line in f.read_text(errors="replace").splitlines():
            m = SU_RE.match(line)
            if not m:
                continue
            fn = m.group("fn")
            g.frames[fn] = max(g.frames.get(fn, 0), int(m.group("size")))
            g.functions.add(fn)


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
# The per-target floor table (config/stack-floors.toml)
# ---------------------------------------------------------------------------
#
# One `[[floor]]` per (image shape, target, role). It carries the walk POLICY
# (which roots, which cuts / externals / stated frames, how indirect calls are
# charged) and the two measurements the floor is derived from:
#
#   floor = ceil8( max(static_bytes + sum(allowances), measured.bytes)
#                  * (100 + margin_pct) / 100 )
#
# `static_bytes` is what this tool printed for the entry's walk; `measured` is
# a run-time peak on a twin (absent for a role with no twin, e.g. PX4's
# executor-pass thread, which then leans on its allowances). Every cut,
# external, stated frame and allowance carries a reason, and every allowance
# names the tracked issue that would retire it (issue 2402 for the RMW vtable).

ROLES = ("OPEN", "SPIN")
ISSUE_ID = re.compile(r"^\d{4}$")


def floor_formula(static_bytes: int, allowances: int, measured: int | None,
                  margin_pct: int) -> int:
    base = max(static_bytes + allowances, measured or 0)
    raw = -(-base * (100 + margin_pct) // 100)
    return -(-raw // 8) * 8


def load_toml(path: Path) -> dict:
    try:
        import tomllib  # Python 3.11+
    except ModuleNotFoundError:  # pragma: no cover - 3.10 hosts
        import tomli as tomllib
    with open(path, "rb") as f:
        return tomllib.load(f)


def validate_table(doc: dict, issue_ids: set[str]) -> list[str]:
    """Offline checks: needs no build output, so it can run on the fast line."""
    problems: list[str] = []
    if doc.get("schema") != 1:
        problems.append("`schema = 1` is missing or not 1")
    seen: set[str] = set()
    for i, e in enumerate(doc.get("floor", [])):
        where = f"[[floor]] #{i + 1} ({e.get('id', '<no id>')})"
        for k in ("id", "target", "role", "roots", "static_bytes", "margin_pct", "floor"):
            if k not in e:
                problems.append(f"{where}: missing `{k}`")
        if e.get("id") in seen:
            problems.append(f"{where}: duplicate id")
        seen.add(e.get("id"))
        if e.get("role") not in ROLES:
            problems.append(f"{where}: role must be one of {ROLES}")
        walk = e.get("walk", {})
        for kind in ("cut", "external", "frame"):
            for item in walk.get(kind, []):
                if not str(item.get("reason", "")).strip():
                    problems.append(f"{where}: a walk.{kind} has no reason: {item}")
        allowances = 0
        for a in walk.get("allowance", []):
            allowances += int(a.get("bytes", 0))
            if not str(a.get("reason", "")).strip():
                problems.append(f"{where}: an allowance has no reason")
            iid = str(a.get("issue", ""))
            if not ISSUE_ID.match(iid):
                problems.append(f"{where}: an allowance must name the issue that retires it "
                                f"(`issue = \"NNNN\"`), got `{iid}`")
            elif issue_ids and iid not in issue_ids:
                problems.append(f"{where}: allowance names issue {iid}, which has no file")
        measured = e.get("measured")
        m_bytes = None
        if measured is not None:
            m_bytes = measured.get("bytes")
            for k in ("bytes", "method", "issue"):
                if k not in measured:
                    problems.append(f"{where}: measured.{k} missing")
        elif not walk.get("allowance"):
            problems.append(f"{where}: no `measured` peak and no allowance: a role with no twin "
                            "must state what covers its indirect calls")
        if all(k in e for k in ("static_bytes", "margin_pct", "floor")):
            want = floor_formula(int(e["static_bytes"]), allowances, m_bytes, int(e["margin_pct"]))
            if int(e["floor"]) != want:
                problems.append(f"{where}: floor = {e['floor']}, but the formula gives {want}")
    return problems


def apply_entry(args: argparse.Namespace, entry: dict) -> None:
    """Fill the walk's options from a table entry (explicit flags still add)."""
    walk = entry.get("walk", {})
    args.role = [f"{entry['role']}={','.join(entry['roots'])}"]
    args.indirect = args.indirect or walk.get("indirect")
    args.fold_scope = (args.fold_scope or []) + list(walk.get("fold_scope", []))
    if args.recursion_allowance is None:
        args.recursion_allowance = walk.get("recursion_allowance")
    args.cut = (args.cut or []) + [c["match"] for c in walk.get("cut", [])]
    args.external = (args.external or []) + [x["name"] for x in walk.get("external", [])]
    args.frame = (args.frame or []) + [f"{f['name']}={f['bytes']}" for f in walk.get("frame", [])]


def run(args: argparse.Namespace) -> int:
    entry = None
    if args.table:
        doc = load_toml(Path(args.table))
        matches = [e for e in doc.get("floor", []) if e.get("id") == args.entry]
        if len(matches) != 1:
            raise SystemExit(f"--entry `{args.entry}`: {len(matches)} entries in {args.table}")
        entry = matches[0]
        apply_entry(args, entry)
    readobj = llvm_tool("llvm-readobj", args.llvm_bin)
    objdump = llvm_tool("llvm-objdump", args.llvm_bin)
    g = Graph()
    cache = Path(args.llc_cache or Path.home() / ".cache" / "nros-stack-floor")
    args.inputs = expand_inputs(args.inputs, args.bitcode_dir or [], args.llvm_bin, cache)
    for inp in args.inputs:
        load_object(g, inp, readobj, objdump)
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
    alias_gcc_clones(g)
    prune_data_refs(g)
    externals = set(args.external or [])
    cuts = tuple(args.cut or [])

    mode, allowance = parse_indirect(args.indirect)
    if mode in ("fold", "fold1"):
        try:
            charge = fold_charge(g, recursion_allowance=args.recursion_allowance,
                                 externals=externals, cuts=cuts,
                                 scope=tuple(args.fold_scope or []),
                                 one_level=(mode == "fold1"))
        except WalkRefused as e:
            print(f"nros-stack-floor: REFUSED\n{e}", file=sys.stderr)
            return 1
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
            floor = best.depth + (best.depth * args.margin + 99) // 100
            results[role.upper()] = {
                "floor": floor, "depth": best.depth, "root": best_root,
                "margin_pct": args.margin, "indirect_charged": best.indirect,
                "indirect_mode": mode, "indirect_charge": charge,
                "path": best.chain,
            }
    if refusals:
        print("nros-stack-floor: REFUSED\n" + "\n".join(refusals), file=sys.stderr)
        return 1
    if entry is not None:
        got = results[entry["role"].upper()]["depth"]
        want = int(entry["static_bytes"])
        if got != want:
            print(f"nros-stack-floor: STALE `{entry['id']}`: the walk gives {got} B, the "
                  f"table says static_bytes = {want}. Re-measure: write {got} and re-derive "
                  "`floor` (--validate-table prints the formula's answer).", file=sys.stderr)
            return 1
        print(f"{entry['id']}: static_bytes {got} B matches the table")
        return 0
    if args.json:
        print(json.dumps(results, indent=2))
    elif args.emit_defines:
        for role, r in results.items():
            print(f"#define NROS_STACK_FLOOR_{role} {r['floor']}")
    else:
        for role, r in results.items():
            print(f"{role}: floor {r['floor']} B (path {r['depth']} B + {r['margin_pct']}%)"
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

    def check(name, cond):
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

    # the floor formula: max of static (+allowances) and measured, margin, align 8
    check("floor from measured", floor_formula(26232, 0, 29184, 10) == 32104)
    check("floor from static + allowance", floor_formula(3592, 1000, None, 0) == 4592)
    good = {"schema": 1, "floor": [{
        "id": "x/OPEN", "target": "t", "role": "OPEN", "roots": ["r"],
        "static_bytes": 26232, "margin_pct": 10, "floor": 32104,
        "measured": {"bytes": 29184, "method": "twin", "issue": "2360"},
        "walk": {"cut": [{"match": "panicking", "reason": "diverges"}]}}]}
    check("a valid table passes", validate_table(good, {"2360"}) == [])
    bad = json.loads(json.dumps(good))
    bad["floor"][0]["floor"] = 32768
    check("a floor off the formula is refused",
          any("formula gives 32104" in p for p in validate_table(bad, set())))
    bad = json.loads(json.dumps(good))
    bad["floor"][0]["walk"]["cut"][0]["reason"] = ""
    check("a cut with no reason is refused", any("no reason" in p for p in validate_table(bad, set())))
    bad = json.loads(json.dumps(good))
    del bad["floor"][0]["measured"]
    bad["floor"][0]["floor"] = floor_formula(26232, 0, None, 10)
    check("no twin and no allowance is refused",
          any("no `measured` peak" in p for p in validate_table(bad, set())))
    bad["floor"][0]["walk"]["allowance"] = [{"bytes": 100, "reason": "vtable", "issue": "99"}]
    bad["floor"][0]["floor"] = floor_formula(26232, 100, None, 10)
    check("an allowance must name an issue",
          any("must name the issue" in p for p in validate_table(bad, set())))

    picked = [p.name for p in pick_codegen_bitcode([Path(n) for n in (
        "a.cgu.0.rcgu.bc", "a.cgu.0.rcgu.no-opt.bc", "b.cgu.0.rcgu.bc", "b.cgu.0.rcgu.no-opt.bc",
        "b.cgu.0.rcgu.thin-lto-input.bc", "b.cgu.0.rcgu.thin-lto-after-pm.bc")])]
    check("codegen bitcode: post-opt, or ThinLTO after-pm when present",
          picked == ["a.cgu.0.rcgu.bc", "b.cgu.0.rcgu.thin-lto-after-pm.bc"])
    picked = [p.name for p in pick_codegen_bitcode([Path(n) for n in (
        "dep.x.cgu.0.rcgu.bc", "app.y.cgu.0.rcgu.bc", "app.y.cgu.0.rcgu.no-opt.bc",
        "app.y.cgu.0.rcgu.lto.input.bc", "app.y.cgu.0.rcgu.lto.after-restriction.bc")])]
    check("fat LTO: only the merged post-LTO module", picked == ["app.y.cgu.0.rcgu.bc"])

    for f in fails:
        print(f"FAIL: {f}", file=sys.stderr)
    print(f"nros-stack-floor self-test: {28 - len(fails)} checks, {len(fails)} failed")
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
    ap.add_argument("--margin", type=int, default=0, help="percent added to the path")
    ap.add_argument("--llvm-bin", help="directory holding llvm-readobj / llvm-objdump")
    ap.add_argument("--json", action="store_true")
    ap.add_argument("--emit-defines", action="store_true")
    ap.add_argument("--table", help="config/stack-floors.toml; with --entry, the walk's "
                    "policy comes from that entry and the result is checked against it")
    ap.add_argument("--entry", help="the [[floor]] id to walk")
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
    if args.table:
        if not args.entry or not args.inputs:
            ap.error("--table needs --entry and the image's INPUT(s)")
        return run(args)
    if not args.inputs or not args.role:
        ap.error("need INPUT(s) and at least one --role")
    return run(args)


if __name__ == "__main__":
    sys.exit(main())
