"""Stack frames for a toolchain's prebuilt builtins, measured from its own archive.

Phase-557 W9b. `libgcc.a` (and the like) carry no `.su`, and the hand-assembly
members (`__aeabi_dadd`, `__aeabi_l2d`, `__aeabi_cdcmple`, ...) carry no CFI
either, so a linked image's `.debug_frame` has no frame for them and the walk
refuses. This module measures each global function of such an archive ONCE,
from the archive's own objects, and the walk reads the result back keyed by
the archive's CODE KEY: a hash of every member's executable sections and the
relocations on them, by member and section name. That is what a frame is a
fact about, and it is the same on every host:

- the toolchain store is built per HOST arch, so one target library is
  different bytes on an x86_64 and an aarch64 host (archive headers,
  `.comment`, debug paths) with identical code; a whole-file hash split one
  library into two keys and failed validation on the other host. Measured:
  arm-none-eabi 13.2's v7-m libgcc hashes c6a3bf92... (x86_64) and
  8001f36d... (aarch64), with one code key and identical frames;
- a different GCC, multilib (cpu, float ABI) or build is different code, so a
  different key, and a frame is never borrowed from a near toolchain;
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

#: The table's schema; bump with a reader change. 2: `code_sha256` (the code
#: key) replaces `sha256` (the whole file's bytes, which differ by host).
SCHEMA = 2

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


# newlib's members (`libc_a-memchr.o`, `libm_a-s_frexp.o`) and libgcc's
# (`_bswapsi2.o`): the toolchain's C runtime, copied into a board crate's
# archive (issue 2514). Named, so an ordinary C file that lost its own frame
# source still refuses.
_TOOLCHAIN_C_RUNTIME = re.compile(
    r"^(?:lib[cmg]_a-[\w.-]+"            # newlib: libc_a-memchr, libm_a-s_frexp
    r"|_[a-z0-9_]+"                       # libgcc: _bswapsi2, _udivdi3
    r"|(?:add|sub|mul|div|neg|eq|ne|ge|gt|le|lt|unord|fix(?:uns)?|float(?:un|uns)?|extend"
    r"|trunc|powi|cmp)(?:[sdtxhb]f|[sdt]i)[a-z0-9]*"  # libgcc soft-fp: trunctfdf2, addtf3
    r")\.o$")


def is_toolchain_c_runtime(member: str) -> bool:
    return bool(_TOOLCHAIN_C_RUNTIME.match(Path(member).name))


def runtime_member_checks() -> list[tuple[str, bool]]:
    """The toolchain-runtime member names (issue 2514), held to examples."""
    yes = ["libc_a-memchr.o", "libm_a-s_frexp.o", "_bswapsi2.o", "trunctfdf2.o", "addtf3.o",
           "fixunsdfsi.o", "floatsidf.o", "extendsfdf2.o"]
    no = ["pdf.o", "main.o", "zpico.o", "nros_platform.o", "elf.o", "libfoo.o"]
    return [(f"toolchain runtime member `{m}`", is_toolchain_c_runtime(m)) for m in yes] + \
        [(f"not a toolchain runtime member `{m}`", not is_toolchain_c_runtime(m)) for m in no]


def is_linked_runtime(member: str) -> bool:
    """What the linked-objects road measures: compiler_builtins, and the
    toolchain's C runtime."""
    return is_compiler_builtins(member) or is_toolchain_c_runtime(member)


_AR_SKIP = (b"/", b"/SYM64/", b"__.SYMDEF", b"__.SYMDEF SORTED")


def _ar_members(data: bytes) -> list[tuple[str, bytes]]:
    """The members of a System V / GNU `ar` archive, in order."""
    if data[:8] != b"!<arch>\n":
        raise ValueError("not an ar archive")
    off, longnames, out = 8, b"", []
    while off + 60 <= len(data):
        hdr = data[off:off + 60]
        name, size = hdr[:16].rstrip(b" "), int(hdr[48:58])
        body = data[off + 60:off + 60 + size]
        off += 60 + size + (size & 1)
        if name in _AR_SKIP:
            continue
        if name == b"//":
            longnames = body
            continue
        if name.startswith(b"/") and name[1:].isdigit():
            i = int(name[1:])
            name = longnames[i:longnames.index(b"/\n", i)]
        else:
            name = name.rstrip(b"/")
        out.append((name.decode(errors="replace"), body))
    return out


def _code_sections(obj: bytes) -> list[tuple[str, bytes]] | None:
    """An ELF object's executable sections and its relocation sections, by
    name. `None` for a member that is not ELF."""
    import struct
    if obj[:4] != b"\x7fELF":
        return None
    e = "<" if obj[5] == 1 else ">"
    if obj[4] == 1:
        (shoff,) = struct.unpack_from(e + "I", obj, 0x20)
        shentsize, shnum, shstrndx = struct.unpack_from(e + "HHH", obj, 0x2E)
        fmt = e + "IIIIIIIIII"
    else:
        (shoff,) = struct.unpack_from(e + "Q", obj, 0x28)
        shentsize, shnum, shstrndx = struct.unpack_from(e + "HHH", obj, 0x3A)
        fmt = e + "IIQQQQIIQQ"
    secs = [struct.unpack_from(fmt, obj, shoff + i * shentsize) for i in range(shnum)]
    names = secs[shstrndx][4]
    out = []
    for s in secs:
        sh_name, sh_type, sh_flags, sh_off, sh_size = s[0], s[1], s[2], s[4], s[5]
        # SHF_EXECINSTR (and not NOBITS), or SHT_RELA / SHT_REL: the code, and
        # what it calls. Not .symtab/.strtab/.comment/debug, which carry paths
        # and build details that differ by host while the code does not.
        if (sh_flags & 0x4 and sh_type != 8) or sh_type in (4, 9):
            nm = obj[names + sh_name:obj.index(b"\0", names + sh_name)].decode(errors="replace")
            out.append((nm, obj[sh_off:sh_off + sh_size]))
    return out


def archive_code_key(path: Path) -> str:
    """The archive's CODE KEY: a sha256 over each ELF member's executable and
    relocation sections, ordered by member then section name (a repeated
    member name counts its occurrences), so the same code on any host gives
    the same key."""
    rows: list[tuple[str, str, bytes]] = []
    seen: dict[str, int] = {}
    for name, body in _ar_members(path.read_bytes()):
        secs = _code_sections(body)
        if secs is None:
            continue
        n = seen.get(name, 0)
        seen[name] = n + 1
        rows += [(f"{name}#{n}", sn, b) for sn, b in secs]
    h = hashlib.sha256()
    for mem, sn, b in sorted(rows, key=lambda r: (r[0], r[1])):
        h.update(mem.encode() + b"\0" + sn.encode() + b"\0" + len(b).to_bytes(8, "little") + b)
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


# ------------------------------------------------------------------ RISC-V
#
# Issue 2514: newlib and libgcc objects linked into a RISC-V image carry no
# `.su` and no CFI, like the Thumb hand-assembly W9b reads. The same rules,
# for RISC-V's encoding:
#   - a frame is the sum of every `addi sp, sp, -N` (`c.addi16sp` prints the
#     same); `sub sp, sp, <reg>` moves sp by a run-time amount: unmeasured;
#   - a call is `auipc ra` + R_RISCV_CALL(_PLT) (then `jalr ra`), or `jal`
#     to a symbol; the same pair through any other register (`auipc t1` ...
#     `jr t1`) is a TAIL call, the same frame;
#   - `jalr <reg>` with no call relocation is a call through a register:
#     unmeasured;
#   - `.L` labels are inside a body, never a new one.

_RV_RELOC = re.compile(r"^\s*[0-9a-f]+:\s+R_RISCV_(?P<type>\S+)\s+(?P<sym>\S+)")
_RV_TERMINATORS = ("ret", "j", "jr", "tail", "ebreak", "unimp", "mret", "c.j", "c.jr")


def sp_decrement_rv(mnemonic: str, operands: str) -> int | None:
    mn = mnemonic.lower()
    ops = [o.strip() for o in operands.lower().split(",")]
    if mn in ("addi", "c.addi", "c.addi16sp") and ops and ops[0] == "sp":
        src = ops[1] if len(ops) == 3 else "sp"
        imm = ops[-1]
        try:
            v = int(imm, 0)
        except ValueError:
            return None
        if src == "sp":
            return -v if v < 0 else 0
        return 0  # sp from a frame pointer: an epilogue restore
    if mn in ("sub", "subw") and ops and ops[0] == "sp":
        return None
    if mn in ("add", "addw", "mv", "c.mv", "c.add") and ops and ops[0] == "sp":
        return 0  # sp restored from another register (an epilogue)
    return 0


def parse_member_rv(text: str) -> dict[str, Body]:
    bodies: dict[str, Body] = {}
    order: list[str] = []
    cur: Body | None = None
    call_reg: str | None = None  # the register an auipc paired with a call reloc set
    for line in text.splitlines():
        m = _FUNC_HDR.match(line)
        if m:
            name = m["name"].strip()
            if name.startswith(".L") or name.startswith("$"):
                continue  # a local label inside the current body
            cur = bodies.setdefault(name, Body(name))
            order.append(name)
            call_reg = None
            continue
        if cur is None:
            continue
        m = _RV_RELOC.match(line)
        if m:
            t, sym = m["type"], m["sym"]
            if t in ("CALL", "CALL_PLT") and cur.last and cur.last[0] == "auipc":
                reg = cur.last[1].split(",")[0].strip().lower()
                (cur.calls if reg == "ra" else cur.jumps).add(sym)
                call_reg = reg
            elif t == "JAL" and cur.last:
                mn, ops = cur.last
                if mn == "jal" and (ops.split(",")[0].strip() in ("ra",) or "," not in ops):
                    cur.calls.add(sym)
                elif not sym.startswith(".L"):
                    cur.jumps.add(sym)
            elif t in ("BRANCH", "RVC_BRANCH", "RVC_JUMP") and not sym.startswith(".L"):
                if sym != cur.name:
                    cur.jumps.add(sym)
            continue
        m = _INSN.match(line)
        if not m or m["mn"].startswith("<") or m["mn"] == "...":
            continue
        mn, ops = m["mn"].lower(), (m["ops"] or "").strip()
        if mn.startswith(".") or re.fullmatch(r"[0-9a-f]{2,8}", mn):
            continue
        d = sp_decrement_rv(mn, ops)
        if d is None:
            cur.unmeasured = cur.unmeasured or f"moves sp by a register (`{mn} {ops}`)"
        else:
            cur.frame += d
        if mn in ("jalr", "c.jalr"):
            reg = ops.split(",")[0].strip().lower().split("(")[-1].rstrip(")")
            if call_reg is None or reg != call_reg:
                cur.unmeasured = cur.unmeasured or f"calls through a register (`{mn} {ops}`)"
            call_reg = None
        elif mn in ("jr", "c.jr"):
            reg = ops.split(",")[0].strip().lower()
            if reg != "ra" and (call_reg is None or reg != call_reg):
                cur.unmeasured = cur.unmeasured or f"jumps through a register (`{mn} {ops}`)"
            call_reg = None
        elif mn != "auipc":
            call_reg = None
        if mn != "nop" and mn != "c.nop":
            cur.last = (mn, ops)
    for a, b in zip(order, order[1:]):
        body = bodies[a]
        if body.last is not None and body.last[0] not in _RV_TERMINATORS:
            body.next = b
    for body in bodies.values():
        if body.next:
            body.jumps.add(body.next)
    return bodies


def object_arch(obj: bytes) -> str:
    """`riscv` or `arm`, from an ELF object's e_machine (243, 40)."""
    if obj[:4] != b"\x7fELF":
        return "arm"
    e = "<" if obj[5] == 1 else ">"
    import struct
    (mach,) = struct.unpack_from(e + "H", obj, 18)
    return "riscv" if mach == 243 else "arm"


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
            try:
                arch = object_arch(obj.read_bytes()[:20])
            except OSError:
                arch = "arm"  # a stubbed archive in a self-test
            parse = parse_member_rv if arch == "riscv" else parse_member
            bodies[mem] = parse(r.stdout)
    return _depths(defs, bodies)


def measure_objects(objs: list[Path], objdump: str, nm: str, own: bool = False
                    ) -> dict[str, dict]:
    """`measure_archive` for loose objects: members an LLD-map walk already
    extracted from one archive (issue 2514). Each object is a member, named
    by its file name.

    `own=True` gives each function its OWN frame only. Use it for objects the
    walk itself loads: their calls, branches and indirect sites are already
    edges of the walk's graph, so a total depth would count the chain twice,
    and an indirect call is the walk's to bind, not a reason to refuse. Only
    an sp moved by a register (a run-time amount) still refuses."""
    defs: dict[str, tuple[str, str]] = {}
    bodies: dict[str, dict[str, Body]] = {}
    for obj in objs:
        mem = obj.name
        r = subprocess.run([nm, "--defined-only", "--format=posix", str(obj)],
                           capture_output=True, text=True)
        for line in r.stdout.splitlines():
            m = re.match(r"^(?P<name>\S+)\s+(?P<t>\S)\s+(?P<v>[0-9a-fA-F]+)", line)
            # a walked object's local functions (`t`: `quorem`, an `.isra`
            # clone) are walked too, so own-frame mode states them
            if m and (m["t"] in "TW" or (own and m["t"] == "t")):
                defs.setdefault(m["name"], (mem, f"{mem}@{int(m['v'], 16) & ~1:x}"))
        r = subprocess.run([objdump, "-dr", "--no-show-raw-insn", str(obj)],
                           capture_output=True, text=True)
        try:
            arch = object_arch(obj.read_bytes()[:20])
        except OSError:
            continue
        bodies[mem] = (parse_member_rv if arch == "riscv" else parse_member)(r.stdout)
    if own:
        out: dict[str, dict] = {}
        for name, (mem, _) in defs.items():
            b = bodies.get(mem, {}).get(name)
            if b is None:
                continue
            if b.unmeasured and b.unmeasured.startswith("moves sp"):
                out[name] = {"unmeasured": b.unmeasured}
            else:
                out[name] = {"bytes": b.frame}
        return out
    return _depths(defs, bodies)


def _depths(defs: dict[str, tuple[str, str]], bodies: dict[str, dict[str, Body]]
            ) -> dict[str, dict]:
    """Each defined function's total depth (its own frame plus the deepest
    chain inside the measured set), or why it is unmeasured."""
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
        if row.get("code_sha256") == sha:
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
        sha = row.get("code_sha256", "")
        if not re.fullmatch(r"[0-9a-f]{64}", str(sha)):
            problems.append(f"{where}: `code_sha256` is not a 64-hex code key")
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
            # A store copy built for another host has other bytes and the
            # same code, so it is checked by code key; an archive this host
            # does not have is not checked at all (never a failure).
            if p.is_file() and archive_code_key(p) != sha:
                problems.append(f"{where}: {row['path']}'s code no longer matches its "
                                f"`code_sha256` (the archive changed; re-run `{RECIPE} {p}`)")
    return problems


def render_row(path_label: str, sha: str, toolchain: str, measured: str,
               result: dict[str, dict]) -> str:
    """One `[[archive]]` block, in the one spelling the recipe writes."""
    lines = ["[[archive]]",
             f'path = "{path_label}"',
             f'code_sha256 = "{sha}"',
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
        if row.get("code_sha256") == sha:
            return dict(row.get("unmeasured") or {})
    return {}


def defined_names(archive: Path, nm: str) -> set[str]:
    """The function names an archive defines (for naming it in a refusal)."""
    return set(defined_functions(archive, nm))


HEADER = f"""# Stack frames of toolchain builtins, measured from the toolchain's own archive.
#
# Phase-557 W9b. GENERATED: every row is written by `{RECIPE} <archive>` and
# none is typed by hand (`--validate-table` checks each row's key and, where
# the archive is installed, that its code still has that key). A row is keyed
# by the archive's CODE KEY (its executable and relocation sections, which are
# the same bytes on every host the toolchain is built for): the walk reads the
# frames of exactly the archives its link map names, so a different compiler,
# multilib or build is a different key, and a key with no row is a refusal
# naming the archive and the recipe, never a near key's numbers.
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
    out = [b for b in rest if f'code_sha256 = "{sha}"' not in b]
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
    # The code key: the same code built on two hosts (other ar header bytes,
    # another `.comment`) is one key; other code is another.
    import struct as _st

    def _elf(text: bytes, comment: bytes) -> bytes:
        shstr = b"\0.text\0.comment\0.shstrtab\0"
        body = text + comment + shstr
        shoff = 52 + len(body)
        hdr = b"\x7fELF\x01\x01\x01" + b"\0" * 9 + _st.pack(
            "<HHIIIIIHHHHHH", 1, 40, 1, 0, 0, shoff, 0, 52, 0, 0, 40, 4, 3)
        sh = b"\0" * 40
        sh += _st.pack("<IIIIIIIIII", 1, 1, 0x6, 0, 52, len(text), 0, 0, 2, 0)
        sh += _st.pack("<IIIIIIIIII", 7, 1, 0x30, 0, 52 + len(text), len(comment), 0, 0, 1, 1)
        sh += _st.pack("<IIIIIIIIII", 16, 3, 0, 0, 52 + len(text) + len(comment), len(shstr),
                       0, 0, 1, 0)
        return hdr + body + sh

    def _ar(members: list[tuple[str, bytes]], stamp: str) -> bytes:
        out = b"!<arch>\n"
        for name, body in members:
            out += (f"{name + '/':<16}{stamp:<12}{'0':<6}{'0':<6}{'644':<8}{len(body):<10}`\n"
                    .encode()) + body + (b"\n" if len(body) & 1 else b"")
        return out

    with tempfile.TemporaryDirectory() as d:
        code = b"\x10\xb5\x10\xbd"  # push {r4, lr}; pop {r4, pc}
        a1 = Path(d, "a.a")
        a1.write_bytes(_ar([("x.o", _elf(code, b"GCC: x86_64 host\0"))], "1700000000"))
        a2 = Path(d, "b.a")
        a2.write_bytes(_ar([("x.o", _elf(code, b"GCC: an aarch64 host build\0"))], "1800000000"))
        a3 = Path(d, "c.a")
        a3.write_bytes(_ar([("x.o", _elf(b"\x30\xb5\x30\xbd", b"GCC: x86_64 host\0"))],
                           "1700000000"))
        out.append(("builtins: one code on two hosts is one code key; other code is another",
                    archive_code_key(a1) == archive_code_key(a2) != archive_code_key(a3)))

    good = {"schema": SCHEMA, "archive": [{
        "path": "x/libgcc.a", "code_sha256": "a" * 64, "toolchain": "gcc 12.2.0",
        "method": "m", "measured": "2026-11-21", "frames": {"__aeabi_dadd": 12},
        "unmeasured": {"abort_user": "calls `abort`"}}]}
    out.append(("builtins: the fixed-point family is skipped, the rest of __gnu_ is not",
                bool(FIXED_POINT.match("__gnu_satfractdqusq")) and bool(FIXED_POINT.match("__gnu_addha3"))
                and bool(FIXED_POINT.match("__gnu_ssmulusa3"))
                and not FIXED_POINT.match("__gnu_d2h_ieee")
                and not FIXED_POINT.match("__gnu_Unwind_Save_VFP")
                and not FIXED_POINT.match("__aeabi_dadd")))
    out.append(("builtins: a complete table validates", validate(good) == []))
    bad = {"schema": SCHEMA, "archive": [dict(good["archive"][0], code_sha256="nothex",
                                              frames={"__aeabi_dadd": -1})]}
    probs = validate(bad)
    out.append(("builtins: a bad hash and a negative frame are refused",
                any("code_sha256" in p for p in probs) and any("byte count" in p for p in probs)))
    out.append(("builtins: lookup is by exact code key, never a near key",
                frames_for(good, "a" * 64) == {"__aeabi_dadd": 12}
                and frames_for(good, "b" * 64) is None))
    t = replace_row("", "a" * 64, render_row("x/libgcc.a", "a" * 64, "gcc", "d",
                                             {"f": {"bytes": 4}}))
    t2 = replace_row(t, "a" * 64, render_row("x/libgcc.a", "a" * 64, "gcc", "d",
                                             {"f": {"bytes": 8}}))
    out.append(("builtins: re-measuring an archive replaces its row",
                t2.count("[[archive]]") == 1 and '"f" = 8' in t2))
    return out
