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
                scope: tuple[str, ...] = ()) -> int:
    """Bytes charged at an indirect site under --indirect fold: the deepest
    path among address-taken functions, themselves walked with indirect sites
    charged ZERO-then-iterated to a fixed point (a vtable function that itself
    calls through a vtable)."""
    charge = 0
    for _ in range(8):
        best = 0
        for fn in g.address_taken:
            if fn not in g.frames:
                continue
            if scope and not any(sc in fn for sc in scope):
                continue
            try:
                p = worst_path(g, fn, indirect_charge=charge,
                               recursion_allowance=recursion_allowance,
                               externals=externals, cuts=cuts)
            except WalkRefused:
                continue
            best = max(best, p.depth)
        if best == charge:
            return charge
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


def llvm_tool(name: str, llvm_bin: str | None) -> str:
    if llvm_bin:
        return str(Path(llvm_bin) / name)
    try:
        sysroot = subprocess.run(["rustc", "--print", "sysroot"], check=True,
                                 capture_output=True, text=True).stdout.strip()
        for p in Path(sysroot, "lib", "rustlib").glob(f"*/bin/{name}"):
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
    if s == "fold":
        return ("fold", None)
    m = re.fullmatch(r"allowance:(\d+)", s)
    if m:
        return ("allowance", int(m.group(1)))
    raise SystemExit(f"--indirect: `{s}` is neither `fold` nor `allowance:<bytes>`")


def run(args: argparse.Namespace) -> int:
    readobj = llvm_tool("llvm-readobj", args.llvm_bin)
    objdump = llvm_tool("llvm-objdump", args.llvm_bin)
    g = Graph()
    for inp in args.inputs:
        load_object(g, inp, readobj, objdump)
    for su in args.su or []:
        load_su(g, Path(su))
    prune_data_refs(g)
    externals = set(args.external or [])
    cuts = tuple(args.cut or [])

    mode, allowance = parse_indirect(args.indirect)
    if mode == "fold":
        try:
            charge = fold_charge(g, recursion_allowance=args.recursion_allowance,
                                 externals=externals, cuts=cuts,
                                 scope=tuple(args.fold_scope or []))
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

    # .su parsing
    import tempfile
    with tempfile.TemporaryDirectory() as d:
        Path(d, "x.su").write_text("src/x.c:12:5:_z_open\t48\tstatic\n"
                                   "src/x.c:30:6:helper\t16\tdynamic,bounded\n")
        g6 = Graph()
        load_su(g6, Path(d))
        check(".su frames", g6.frames == {"_z_open": 48, "helper": 16})

    # a cut function counts its own frame, not its callees
    gc = Graph(frames={"root": 10, "panic": 30, "deep": 999}, edges={"root": {"panic"}, "panic": {"deep", "panic"}},
               functions={"root", "panic", "deep"})
    p = worst_path(gc, "root", indirect_charge=None, recursion_allowance=None, cuts=("panic",))
    check("cut is a leaf", p.depth == 40)

    for f in fails:
        print(f"FAIL: {f}", file=sys.stderr)
    print(f"nros-stack-floor self-test: {14 - len(fails)} checks, {len(fails)} failed")
    return 1 if fails else 0


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("inputs", nargs="*", help="archives (.a) or objects (.o)")
    ap.add_argument("--role", action="append", default=[],
                    help="ROLE=root[,root...]; the floor is the deepest root")
    ap.add_argument("--su", action="append", help="a .su file or a dir searched for *.su")
    ap.add_argument("--indirect", help="`fold` or `allowance:<bytes>`; default refuses")
    ap.add_argument("--recursion-allowance", type=int)
    ap.add_argument("--external", action="append",
                    help="a callee with no definition in the inputs accepted as a 0-byte leaf")
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
    ap.add_argument("--self-test", action="store_true")
    args = ap.parse_args()
    if args.self_test:
        return self_test()
    if not args.inputs or not args.role:
        ap.error("need INPUT(s) and at least one --role")
    return run(args)


if __name__ == "__main__":
    sys.exit(main())
