"""Stack frames for a toolchain's prebuilt builtins, measured from its own archive.

Phase-557 W9b. `libgcc.a` (and the like) carry no `.su`, and the hand-assembly
members (`__aeabi_dadd`, `__aeabi_l2d`, `__aeabi_cdcmple`, ...) carry no CFI
either, so a linked image's `.debug_frame` has no frame for them and the walk
refuses. This module measures each global function of such an archive ONCE,
from the archive's own objects, and the walk reads the result back keyed by
the archive's CONTENT HASH, the identity of exactly what the link loaded:

- a different GCC, multilib (cpu, float ABI) or build of the same library is a
  different hash, so a frame is never borrowed from a near toolchain;
- a hash the table does not hold is a refusal that names the archive and the
  recipe that measures it (`just zephyr stack-builtins <archive>`).

What a row's number is: the function's TOTAL depth, its own frame plus the
deepest chain of calls it makes inside the same archive. The walk does not
descend into toolchain archives (`link_map_inputs` returns them as dropped
inputs), so a builtin is a leaf to the walk and its row must carry what it
reaches, the way `__aeabi_d2ulz` reaches `__aeabi_dmul`.

How the frame is read: Thumb prologue decoding, the SUM of every stack
decrement in the function's body (`push`, `stmdb sp!`, `str ..., [sp, #-N]!`,
`sub sp, #N`, `vpush`). A hand-written routine may push on one path and not on
another, and the sum is never below the deepest of them, so the number is an
upper bound and never an under-count. A body that moves `sp` by a register
(`sub sp, rN`), calls through a register, or calls outside the archive is not
measured: its row records why, and the walk refuses on it as before.
"""

from __future__ import annotations

import hashlib
import re
import subprocess
import tempfile
from pathlib import Path

#: The table's schema; bump with a reader change.
SCHEMA = 1

#: GCC's fixed-point runtime (ISO TR 18037 `_Fract`/`_Accum`, libgcc's
#: fixed-bit.c): about 1,000 of libgcc's 1,271 functions, and nothing in this
#: tree has a fixed-point type. The recipe does not measure them, and a walk
#: that reaches one is refused with that said.
FIXED_POINT = re.compile(
    r"^__gnu_(?:(?:sat)?fract\w*"
    r"|(?:ss|us)?(?:add|sub|mul|div|udiv|neg|cmp|ashl|ashr|lshr|saturate[12]"
    r"|ashlhelper|divhelper|mulhelper)u?(?:qq|hq|sq|dq|tq|ha|sa|da|ta)\d?)$")

#: The recipe that adds an archive's rows, named in every refusal.
RECIPE = "just zephyr stack-builtins"


def is_compiler_builtins(member: str) -> bool:
    """A Rust `compiler_builtins` codegen unit (`compiler_builtins-<hash>.….o`),
    which `-Zbuild-std` compiles from the pinned toolchain's own source into an
    image's staticlib. Its hand-written `#[naked]` routines (`memcpy`,
    `__aeabi_memcpy`, ...) get no frame from `llc --stack-sizes`."""
    return Path(member).name.startswith("compiler_builtins-")


def archive_sha256(path: Path) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


# ---------------------------------------------------------------- decoding

_REG_LIST = re.compile(r"\{([^}]*)\}")
_IMM = re.compile(r"#(-?(?:0x[0-9a-fA-F]+|\d+))")


def _count_regs(body: str) -> int:
    """`r4, r5, lr` or GNU's `r4-r7, lr` (also `d8-d9`)."""
    n = 0
    for part in body.split(","):
        part = part.strip()
        if not part:
            continue
        m = re.fullmatch(r"([rdsq])(\d+)\s*-\s*[rdsq]?(\d+)", part)
        n += int(m[3]) - int(m[2]) + 1 if m else 1
    return n


def _imm(text: str) -> int | None:
    m = _IMM.search(text)
    return int(m[1], 0) if m else None


def sp_decrement(mnemonic: str, operands: str) -> int | None:
    """Bytes this instruction moves `sp` DOWN by: 0 when it does not, None
    when it moves it by an amount not in the instruction (a register)."""
    mn = mnemonic.lower().split(".")[0]
    ops = operands.lower().replace(" ", "")
    if mn in ("push", "stmdb", "stmfd") and (mn == "push" or ops.startswith("sp!")):
        regs = _REG_LIST.search(operands)
        return 4 * _count_regs(regs[1]) if regs else None
    if mn in ("vpush", "vstmdb") and (mn == "vpush" or ops.startswith("sp!")):
        regs = _REG_LIST.search(operands)
        if not regs:
            return None
        width = 8 if regs[1].strip().lower().startswith("d") else 4
        return width * _count_regs(regs[1])
    if mn.startswith("str") and "[sp,#-" in ops and ops.endswith("]!"):
        return -(_imm(ops.split("[sp,", 1)[1]) or 0)
    if mn in ("sub", "subs", "subw") and ops.startswith("sp,"):
        rest = ops[3:]
        if rest.startswith("sp,"):
            rest = rest[3:]
        if rest.startswith("#"):
            return _imm(rest) or 0
        return None  # sub sp, rN: a run-time amount
    if mn in ("add", "adds", "addw") and ops.startswith("sp,"):
        rest = ops[3:]
        if rest.startswith("sp,"):
            rest = rest[3:]
        v = _imm(rest) if rest.startswith("#") else 0
        return -v if v is not None and v < 0 else 0
    return 0


_TERMINATORS = ("bx", "b", "udf", "bkpt")


def ends_flow(mnemonic: str, operands: str) -> bool:
    """Whether the last instruction of a body leaves it (so control does not
    fall through into the next symbol)."""
    mn = mnemonic.lower().split(".")[0]
    if mn in _TERMINATORS:
        return True
    ops = operands.lower()
    if mn in ("pop", "ldm", "ldmia", "ldmfd") and "pc" in ops:
        return True
    if mn.startswith("ldr") and ops.replace(" ", "").startswith("pc,"):
        return True
    if mn in ("mov", "movs") and ops.replace(" ", "").startswith("pc,"):
        return True
    return False


_FUNC_HDR = re.compile(r"^[0-9a-f]+ <(?P<name>[^>]+)>:$")
_INSN = re.compile(r"^\s*(?P<addr>[0-9a-f]+):\s+(?P<mn>\S+)(?:\s+(?P<ops>.*?))?\s*$")
_RELOC = re.compile(r"^\s*[0-9a-f]+:\s+R_ARM_(?P<type>\S+)\s+(?P<sym>\S+)")
_TARGET = re.compile(r"<(?P<sym>[^>+]+)(?:\+0x[0-9a-f]+)?>")
_CALL_RELOCS = ("THM_CALL", "CALL", "PC24")
_JUMP_RELOCS = ("THM_JUMP24", "THM_JUMP19", "THM_JUMP11", "THM_JUMP8", "JUMP24")


class Body:
    def __init__(self, name: str):
        self.name = name
        self.frame = 0
        self.unmeasured: str | None = None
        self.calls: set[str] = set()   # bl/blx: the callee's frame stacks on this one
        self.jumps: set[str] = set()   # b, a tail branch or falling through: the same frame
        self.last: tuple[str, str] | None = None
        self.next: str | None = None


def parse_member(text: str) -> dict[str, Body]:
    """`llvm-objdump -dr` of one object: each labelled body's own frame, its
    calls and branches out, and the label it falls through into."""
    bodies: dict[str, Body] = {}
    order: list[str] = []
    cur: Body | None = None
    for line in text.splitlines():
        m = _FUNC_HDR.match(line)
        if m:
            name = m["name"]
            if name.startswith("$"):
                continue  # a mapping symbol
            cur = bodies.setdefault(name, Body(name))
            order.append(name)
            continue
        if cur is None:
            continue
        m = _RELOC.match(line)
        if m:
            if m["type"] in _CALL_RELOCS:
                cur.calls.add(m["sym"])
            elif m["type"] in _JUMP_RELOCS:
                cur.jumps.add(m["sym"])
            continue
        m = _INSN.match(line)
        if not m or m["mn"].startswith("<") or m["mn"] == "...":
            continue
        mn, ops = m["mn"], m["ops"] or ""
        if mn.startswith(".") or re.fullmatch(r"[0-9a-f]{2,8}", mn):
            continue  # data (a literal pool word)
        d = sp_decrement(mn, ops)
        if d is None:
            cur.unmeasured = cur.unmeasured or f"moves sp by a register (`{mn} {ops}`)"
        else:
            cur.frame += max(d, 0)
        base = mn.lower().split(".")[0]
        if base in ("blx", "bx") and not _TARGET.search(ops) and ops.strip().lower() != "lr":
            if base == "blx":
                cur.unmeasured = cur.unmeasured or f"calls through a register (`{mn} {ops}`)"
        if base.startswith("b") and base not in ("bic", "bics", "bfi", "bfc", "bkpt"):
            t = _TARGET.search(ops)
            if t and t["sym"] != cur.name and not t["sym"].startswith("$"):
                (cur.calls if base in ("bl", "blx") else cur.jumps).add(t["sym"])
        if mn.lower().split(".")[0] != "nop":
            cur.last = (mn, ops)  # alignment padding is not where a body ends
    for a, b in zip(order, order[1:]):
        body = bodies[a]
        if body.last is not None and not ends_flow(*body.last):
            body.next = b
    for body in bodies.values():
        if body.next:
            body.jumps.add(body.next)
    return bodies


# ------------------------------------------------------------------ archive

def defined_functions(archive: Path, nm: str) -> dict[str, tuple[str, str]]:
    """Global and weak FUNCTION names the archive defines -> (member, alias
    group key). Aliases (one address, several names) share the key."""
    r = subprocess.run([nm, "--defined-only", "--format=posix", "-A", str(archive)],
                       capture_output=True, text=True, check=True)
    out: dict[str, tuple[str, str]] = {}
    for line in r.stdout.splitlines():
        # "<archive>[<member>]: <name> <type> <value> [<size>]"
        m = re.match(r"^.*\[(?P<mem>[^\]]+)\]:\s+(?P<name>\S+)\s+(?P<t>\S)\s+(?P<v>[0-9a-fA-F]+)", line)
        if not m or m["t"] not in "TW":
            continue
        out.setdefault(m["name"], (m["mem"], f"{m['mem']}@{int(m['v'], 16) & ~1:x}"))
    return out


def measure_archive(archive: Path, objdump: str, nm: str, ar: str,
                    only=None) -> dict[str, dict]:
    """Every global function the archive defines -> {"bytes": N} or
    {"unmeasured": reason}. `only(member)` narrows it to some members (a
    call out of them is then a call outside the archive)."""
    defs = defined_functions(archive, nm)
    if only is not None:
        defs = {n: v for n, v in defs.items() if only(v[0])}
    members = sorted({mem for mem, _ in defs.values()})
    bodies: dict[str, dict[str, Body]] = {}
    with tempfile.TemporaryDirectory(prefix="nros-stack-builtins-") as tmp:
        subprocess.run([ar, "x", str(archive.resolve())], cwd=tmp, check=True)
        for mem in members:
            obj = Path(tmp) / mem
            if not obj.is_file():
                continue
            r = subprocess.run([objdump, "-dr", "--no-show-raw-insn", str(obj)],
                               capture_output=True, text=True, check=True)
            bodies[mem] = parse_member(r.stdout)
    # an alias group shares one body: find it under any of its names
    by_group: dict[str, Body] = {}
    for name, (mem, group) in defs.items():
        b = bodies.get(mem, {}).get(name)
        if b is not None:
            by_group.setdefault(group, b)

    def body_of(name: str, mem: str | None) -> Body | None:
        if mem is not None and name in bodies.get(mem, {}):
            return bodies[mem][name]
        if name in defs:
            m2, group = defs[name]
            return bodies.get(m2, {}).get(name) or by_group.get(group)
        return None

    # Jumps (a `b` to another label, or falling through into the next one)
    # keep the frame: hand-written float routines branch into each other's
    # normalisation tails. A body's depth is its own pushes plus the deepest
    # body it jumps to OR calls. Bodies that jump to each other in a cycle are
    # one component whose frame is the SUM of their pushes (an upper bound: a
    # path may not run them all). A call back into its own component, or a
    # cycle of calls, is not measured.
    mem_of: dict[int, str] = {}
    every: dict[int, Body] = {}
    for mem, bs in bodies.items():
        for b in bs.values():
            mem_of[id(b)] = mem
            every[id(b)] = b
    out_jumps: dict[int, list[int]] = {}
    why: dict[int, str] = {}
    for k, b in every.items():
        out_jumps[k] = []
        if b.unmeasured:
            why[k] = b.unmeasured
        for j in sorted(b.jumps):
            jb = body_of(j, mem_of[k])
            if jb is None:
                why.setdefault(k, f"branches to `{j}`, which this archive does not define")
            elif jb is not b:
                out_jumps[k].append(id(jb))
    # Tarjan's SCC over the jump edges, iteratively.
    comp: dict[int, int] = {}
    index: dict[int, int] = {}
    low: dict[int, int] = {}
    on: set[int] = set()
    st: list[int] = []
    counter = 0
    for root in every:
        if root in index:
            continue
        work = [(root, 0)]
        while work:
            v, i = work.pop()
            if i == 0:
                index[v] = low[v] = counter
                counter += 1
                st.append(v)
                on.add(v)
            edges = out_jumps[v]
            if i < len(edges):
                work.append((v, i + 1))
                w = edges[i]
                if w not in index:
                    work.append((w, 0))
                elif w in on:
                    low[v] = min(low[v], index[w])
                continue
            for w in edges:
                if w in on:
                    low[v] = min(low[v], low[w])
            if low[v] == index[v]:
                while True:
                    w = st.pop()
                    on.discard(w)
                    comp[w] = v
                    if w == v:
                        break
    cframe: dict[int, int] = {}
    cnext: dict[int, set[int]] = {}
    ccalls: dict[int, set[tuple[str, int]]] = {}
    cwhy: dict[int, str] = {}
    for k, b in every.items():
        c = comp[k]
        cframe[c] = cframe.get(c, 0) + b.frame
        cnext.setdefault(c, set()).update(comp[w] for w in out_jumps[k] if comp[w] != c)
        ccalls.setdefault(c, set()).update((name, k) for name in b.calls)
        if k in why and c not in cwhy:
            cwhy[c] = why[k]

    memo: dict[int, tuple[int | None, str | None]] = {}

    def depth(c: int, stack: tuple[int, ...]) -> tuple[int | None, str | None]:
        if c in memo:
            return memo[c]
        if c in stack:
            return None, "recursion (a call cycle inside the archive)"
        if c in cwhy:
            return None, cwhy[c]
        nexts: set[int] = set(cnext.get(c, ()))
        res: tuple[int | None, str | None] | None = None
        for name, k in sorted(ccalls.get(c, ())):
            cb = body_of(name, mem_of[k])
            if cb is None:
                res = (None, f"calls `{name}`, which this archive does not define")
                break
            if comp[id(cb)] == c:
                res = (None, f"calls into its own code (`{name}`)")
                break
            nexts.add(comp[id(cb)])
        if res is None:
            deepest = 0
            for n in sorted(nexts):
                d, w = depth(n, stack + (c,))
                if d is None:
                    res = (None, w)
                    break
                deepest = max(deepest, d)
            if res is None:
                res = (cframe.get(c, 0) + deepest, None)
        memo[c] = res
        return res

    out: dict[str, dict] = {}
    for name in sorted(defs):
        if FIXED_POINT.match(name):
            continue
        mem, _ = defs[name]
        b = body_of(name, mem)
        if b is None:
            out[name] = {"unmeasured": f"no code body in {mem}"}
            continue
        d, w = depth(comp[id(b)], ())
        out[name] = {"bytes": d} if d is not None else {"unmeasured": w}
    return out


# -------------------------------------------------------------------- table

def frames_for(table: dict, sha: str) -> dict[str, int] | None:
    """The measured frames of the archive with this content hash, or None when
    the table holds no row for it."""
    for row in table.get("archive", []):
        if row.get("sha256") == sha:
            return {k: v for k, v in row.get("frames", {}).items() if isinstance(v, int)}
    return None


def validate(table: dict, root: Path | None = None) -> list[str]:
    """Offline checks. Each row names its archive, the archive's hash, the
    toolchain that shipped it and how it was measured; a frame is an integer
    or an `unmeasured` reason, never hand-typed (the recipe writes both). When
    the archive is present at `root`/<path> its hash is checked too."""
    problems: list[str] = []
    if table.get("schema") != SCHEMA:
        problems.append(f"`schema = {SCHEMA}` is missing or wrong")
    seen: set[str] = set()
    for i, row in enumerate(table.get("archive", [])):
        where = f"[[archive]] #{i + 1}"
        sha = row.get("sha256", "")
        if not re.fullmatch(r"[0-9a-f]{64}", str(sha)):
            problems.append(f"{where}: `sha256` is not a 64-hex content hash")
        if sha in seen:
            problems.append(f"{where}: a second row for archive {sha[:12]}")
        seen.add(sha)
        for key in ("path", "toolchain", "method", "measured"):
            if not str(row.get(key, "")).strip():
                problems.append(f"{where}: no `{key}`")
        frames = row.get("frames")
        if not isinstance(frames, dict) or not frames:
            problems.append(f"{where}: no `frames`")
            frames = {}
        for name, v in frames.items():
            if isinstance(v, bool) or not isinstance(v, int) or v < 0:
                problems.append(f"{where}: frame `{name}` is not a byte count")
        for name, why in (row.get("unmeasured") or {}).items():
            if not str(why).strip():
                problems.append(f"{where}: `{name}` is unmeasured with no reason")
            if name in frames:
                problems.append(f"{where}: `{name}` is both measured and unmeasured")
        if root is not None and row.get("path"):
            p = root / row["path"]
            if p.is_file() and archive_sha256(p) != sha:
                problems.append(f"{where}: {row['path']} does not hash to its `sha256` (the "
                                f"archive changed; re-run `{RECIPE} {p}`)")
    return problems


def render_row(path_label: str, sha: str, toolchain: str, measured: str,
               result: dict[str, dict]) -> str:
    """One `[[archive]]` block, in the one spelling the recipe writes."""
    lines = ["[[archive]]",
             f'path = "{path_label}"',
             f'sha256 = "{sha}"',
             f'toolchain = "{toolchain}"',
             'method = "thumb prologue sum, plus the deepest in-archive callee chain"',
             f'measured = "{measured}"',
             "",
             "[archive.frames]"]
    for name in sorted(result):
        if "bytes" in result[name]:
            lines.append(f'"{name}" = {result[name]["bytes"]}')
    un = {n: r["unmeasured"] for n, r in result.items() if "unmeasured" in r}
    if un:
        lines += ["", "[archive.unmeasured]"]
        for name in sorted(un):
            why = un[name].replace("\\", "\\\\").replace('"', '\\"')
            lines.append(f'"{name}" = "{why}"')
    return "\n".join(lines) + "\n"


def unmeasured_for(table: dict, sha: str) -> dict[str, str]:
    """Why the recipe could not measure each such function of this archive."""
    for row in table.get("archive", []):
        if row.get("sha256") == sha:
            return dict(row.get("unmeasured") or {})
    return {}


def defined_names(archive: Path, nm: str) -> set[str]:
    """The function names an archive defines (for naming it in a refusal)."""
    return set(defined_functions(archive, nm))


HEADER = f"""# Stack frames of toolchain builtins, measured from the toolchain's own archive.
#
# Phase-557 W9b. GENERATED: every row is written by `{RECIPE} <archive>` and
# none is typed by hand (`--validate-table` checks each row's hash and, where
# the archive is installed, that it still hashes to it). A row is keyed by the
# archive's CONTENT HASH: the walk reads the frames of exactly the archives
# its link map names, so a different compiler, multilib or build is a
# different key, and a key with no row is a refusal naming the archive and the
# recipe, never a near key's numbers.
#
# A frame is the function's total depth: its own pushes plus the deepest chain
# of calls it makes inside the same archive (the walk does not descend into a
# toolchain archive). See scripts/lib/stack_builtins.py for how it is read.

schema = {SCHEMA}
"""


def replace_row(text: str, sha: str, row: str) -> str:
    """`text` with the `[[archive]]` block for `sha` replaced by `row` (or
    `row` appended), in the one layout the recipe writes."""
    if not text.strip():
        return HEADER + "\n" + row
    blocks = re.split(r"(?m)^(?=\[\[archive\]\]\s*$)", text)
    head, rest = blocks[0], blocks[1:]
    out = [b for b in rest if f'sha256 = "{sha}"' not in b]
    out.append(row)
    out.sort(key=lambda b: (re.search(r'(?m)^path = "(.*)"$', b) or [None, ""])[1])
    return head.rstrip("\n") + "\n\n" + "\n".join(o.rstrip("\n") + "\n" for o in out)


def self_checks() -> list[tuple[str, bool]]:
    """Pure checks (no toolchain): the decoder, the depth rules and the table.
    Run by `nros-stack-floor.py --self-test` and so by `--validate-table`."""
    out: list[tuple[str, bool]] = []
    out.append(("builtins: push counts registers, GNU ranges too",
                sp_decrement("push", "{r4, r5, lr}") == 12
                and sp_decrement("push.w", "{r4-r7, lr}") == 20))
    out.append(("builtins: str pre-index, sub sp, vpush",
                sp_decrement("str", "r12, [sp, #-4]!") == 4
                and sp_decrement("sub", "sp, #16") == 16
                and sp_decrement("sub.w", "sp, sp, #0x108") == 264
                and sp_decrement("vpush", "{d8, d9}") == 16))
    out.append(("builtins: sp moved by a register is unmeasured",
                sp_decrement("sub", "sp, r3") is None))
    out.append(("builtins: pop and add sp never count",
                sp_decrement("pop", "{r4, pc}") == 0 and sp_decrement("add", "sp, #8") == 0))
    # A synthetic `llvm-objdump -dr` of one member, with the shapes libgcc has:
    # an alias-named shared body, nop padding before the next label (NOT a
    # fall-through), a real fall-through, a shared tail reached by a branch,
    # a call, and a call back into the caller's own jump component.
    text = """
00000000 <__aeabi_drsub>:
       0:      	eor	r1, r1, #2147483648
       4:      	b	0xc <__aeabi_dadd>
00000008 <__subdf3>:
       8:      	eor	r3, r3, #2147483648
0000000c <__aeabi_dadd>:
       c:      	push	{r4, r5, lr}
      10:      	beq	0x20 <__aeabi_dadd+0x14>
      14:      	pop	{r4, r5, pc}
      18:      	nop
00000020 <__floatsidf>:
      20:      	push	{r4, r5, lr}
      24:      	b	0x14 <__aeabi_dadd+0x8>
00000030 <__aeabi_d2ulz>:
      30:      	push	{r4, r6, r7, lr}
      34:      	bl	0x0 <__aeabi_dmul>
      38:      		00000034:  R_ARM_THM_CALL	__aeabi_dmul
      3c:      	pop	{r4, r6, r7, pc}
00000040 <__aeabi_dmul>:
      40:      	push	{r4, r5, r6, lr}
      44:      	pop	{r4, r5, r6, pc}
00000050 <selfcall>:
      50:      	push	{r4, lr}
      54:      	bl	0x58 <selfcall_tail>
      58:      	b	0x60 <selfcall_tail>
00000060 <selfcall_tail>:
      60:      	b	0x50 <selfcall>
"""
    bodies = parse_member(text)
    out.append(("builtins: nop padding is not a fall-through",
                bodies["__aeabi_dadd"].next is None))
    out.append(("builtins: a body with no exit falls through into the next label",
                bodies["__subdf3"].next == "__aeabi_dadd"))
    out.append(("builtins: bl is a call, b a jump",
                "__aeabi_dmul" in bodies["__aeabi_d2ulz"].calls
                and "__aeabi_dadd" in bodies["__floatsidf"].jumps))
    defs = {n: ("m.o", f"m.o@{n}") for n in bodies}
    import unittest.mock as _mock
    with _mock.patch(__name__ + ".defined_functions", return_value=defs), \
         _mock.patch(__name__ + ".subprocess.run") as run, \
         _mock.patch(__name__ + ".Path.is_file", return_value=True):
        run.return_value.stdout = text
        r = measure_archive(Path("x.a"), "objdump", "nm", "ar")
    out.append(("builtins: the alias-named body measures its own prologue",
                r["__aeabi_dadd"] == {"bytes": 12}))
    out.append(("builtins: falling through into a body adds nothing of its own",
                r["__subdf3"] == {"bytes": 12} and r["__aeabi_drsub"] == {"bytes": 12}))
    out.append(("builtins: a branch into a shared tail stacks its own pushes (upper bound)",
                r["__floatsidf"] == {"bytes": 24}))
    out.append(("builtins: a call adds the callee's depth",
                r["__aeabi_d2ulz"] == {"bytes": 32}))
    out.append(("builtins: a call back into its own jump component is not measured",
                "calls into its own code" in r["selfcall"].get("unmeasured", "")))
    good = {"schema": SCHEMA, "archive": [{
        "path": "x/libgcc.a", "sha256": "a" * 64, "toolchain": "gcc 12.2.0",
        "method": "m", "measured": "2026-11-21", "frames": {"__aeabi_dadd": 12},
        "unmeasured": {"abort_user": "calls `abort`"}}]}
    out.append(("builtins: the fixed-point family is skipped, the rest of __gnu_ is not",
                bool(FIXED_POINT.match("__gnu_satfractdqusq")) and bool(FIXED_POINT.match("__gnu_addha3"))
                and bool(FIXED_POINT.match("__gnu_ssmulusa3"))
                and not FIXED_POINT.match("__gnu_d2h_ieee")
                and not FIXED_POINT.match("__gnu_Unwind_Save_VFP")
                and not FIXED_POINT.match("__aeabi_dadd")))
    out.append(("builtins: a complete table validates", validate(good) == []))
    bad = {"schema": SCHEMA, "archive": [dict(good["archive"][0], sha256="nothex",
                                              frames={"__aeabi_dadd": -1})]}
    probs = validate(bad)
    out.append(("builtins: a bad hash and a negative frame are refused",
                any("sha256" in p for p in probs) and any("byte count" in p for p in probs)))
    out.append(("builtins: lookup is by exact content hash, never a near key",
                frames_for(good, "a" * 64) == {"__aeabi_dadd": 12}
                and frames_for(good, "b" * 64) is None))
    t = replace_row("", "a" * 64, render_row("x/libgcc.a", "a" * 64, "gcc", "d",
                                             {"f": {"bytes": 4}}))
    t2 = replace_row(t, "a" * 64, render_row("x/libgcc.a", "a" * 64, "gcc", "d",
                                             {"f": {"bytes": 8}}))
    out.append(("builtins: re-measuring an archive replaces its row",
                t2.count("[[archive]]") == 1 and '"f" = 8' in t2))
    return out
