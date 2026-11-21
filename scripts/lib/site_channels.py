"""Which functions an indirect call site can hold, by the channel its callee
reaches it through (phase-557 W7f, issue 2550 step 2).

The walk binds an indirect call by the callee's TYPE (W7, W7d). A coarse type
(`void(ptr)`) matches hundreds of functions, which keeps the call graph's
strongly connected component large, and the exact search (issue 2550 step 3)
needs it small. This module asks a narrower question, per site: through which
CHANNEL does the callee arrive, and can every way into that channel be read?

- **Field (T, F).** The callee is loaded from field F of struct type T. The
  site can hold only what is stored into (T, F): a `store ptr @fn`, or a
  constant initializer of a T whose field F is `@fn`.
- **Parameter (f, P).** The callee is the function's own parameter P. The site
  can hold what each direct caller of f passes at position P.

A channel is TRUSTED only when every entry into it is one of the forms the
rules read: a function, another trusted channel (a parameter, a field load).
Any other entry (a value of unknown origin, an opaque or whole-struct write,
the field's address escaping, the function's address escaping) makes it
UNTRUSTED, and its sites keep the walk's type binding. So the result is never
looser than the walk's today.

W7f.0 is the classifier and its share. W7f.1-W7f.3 widen what it reads:
`nested=True` reads constant initializers nested inside another constant
(W7f.1), and the return channel and cc-rs C IR follow.

The IR is the text the walk itself reads: Rust bitcode disassembled
(optimised SSA, byte-offset GEPs, so only the parameter channel applies to
Rust), and clang `-O0` IR of the C, upgraded to opaque pointers by the walk's
`llvm-dis` (struct-typed GEPs survive, parameters go through allocas).
"""

from __future__ import annotations

import re
import statistics
from dataclasses import dataclass, field

_DEFINE = re.compile(r'^define\b[^@]*@("[^"]+"|[\w.$-]+)\((.*)\)[^{]*\{\s*$')
_TYPEDEF = re.compile(r'^(%[\w.$"-]+)\s*=\s*type\s+(.*)$')
_GLOBAL = re.compile(r'^@("[^"]+"|[\w.$-]+)\s*=\s*(?:[\w()]+\s+)*?(?:global|constant)\s+(.*)$')
_ASSIGN = re.compile(r'^\s*(%[\w.$-]+)\s*=\s*(.*)$')
_FNREF = re.compile(r'@("[^"]+"|[\w.$-]+)')
_VAL = re.compile(r'%[\w.$-]+')
_STRUCT_GEP = re.compile(
    r'^getelementptr\s+(?:inbounds\s+)?(%[\w.$"-]+),\s*ptr\s+(%[\w.$-]+|@[\w.$"-]+)'
    r'((?:,\s*i\d+\s+-?\d+)+)\s*$')
_LOAD_PTR = re.compile(r'^load\s+(?:volatile\s+)?ptr,\s*ptr\s+(%[\w.$-]+|@[\w.$"-]+)')
_STORE = re.compile(r'^\s*store\s+(?:volatile\s+)?(.+?)\s+(%[\w.$-]+|@[\w.$"-]+|[\w.-]+),\s*ptr\s+'
                    r'(%[\w.$-]+|@[\w.$"-]+)')
_ALLOCA = re.compile(r'^alloca\s+([^,]+)')
_CALL = re.compile(r'\b(?:tail\s+|musttail\s+|notail\s+)?(?:call|invoke)\b(.*)$')


def _name(s: str) -> str:
    return s.strip().strip('"').lstrip("@").strip('"')


def _split_top(s: str) -> list[str]:
    out, depth, cur = [], 0, []
    for ch in s:
        if ch in "([{<":
            depth += 1
        elif ch in ")]}>":
            depth -= 1
        if ch == "," and depth == 0:
            out.append("".join(cur).strip())
            cur = []
        else:
            cur.append(ch)
    if "".join(cur).strip():
        out.append("".join(cur).strip())
    return out


def _call_parts(rest: str) -> tuple[str, list[str]] | None:
    """`(callee, [arg values])` of a call's text after `call`."""
    m = re.search(r'(%[\w.$-]+|@(?:"[^"]+"|[\w.$-]+))\((.*)\)', rest)
    if not m:
        return None
    args = []
    for a in _split_top(m.group(2)):
        toks = a.split()
        if toks:
            args.append(toks[-1])
    return m.group(1), args


@dataclass
class Func:
    name: str
    params: list[str]
    lines: list[str]
    defs: dict[str, str] = field(default_factory=dict)
    entry_end: int = 0  # index of the first line after the entry block


@dataclass
class Module:
    lang: str  # "c" or "rust"
    funcs: dict[str, Func]
    named: dict[str, str]
    globals_: list[tuple[str, str]]  # (name, "type initializer")
    text_lines: list[str]
    externs: dict[str, str] = field(default_factory=dict)  # name -> type


def parse_module(text: str, lang: str) -> Module:
    funcs: dict[str, Func] = {}
    named: dict[str, str] = {}
    globals_: list[tuple[str, str]] = []
    externs: dict[str, str] = {}
    lines = text.splitlines()
    cur: Func | None = None
    for line in lines:
        if cur is None:
            m = _TYPEDEF.match(line)
            if m:
                named[m.group(1)] = m.group(2).strip()
                continue
            g = _GLOBAL.match(line)
            if g:
                if re.search(r'=\s*(?:[\w()]+\s+)*?external\b', line):
                    externs[_name(g.group(1))] = g.group(2).split(",")[0].strip()
                else:
                    globals_.append((_name(g.group(1)), g.group(2)))
                continue
            d = _DEFINE.match(line)
            if d:
                params = [a.split()[-1] for a in _split_top(d.group(2))
                          if a and a != "..." and a.split()[-1].startswith("%")]
                cur = Func(_name(d.group(1)), params, [])
            continue
        if line.startswith("}"):
            funcs[cur.name] = cur
            cur = None
            continue
        cur.lines.append(line)
        a = _ASSIGN.match(line)
        if a:
            cur.defs[a.group(1)] = a.group(2).strip()
    for f in funcs.values():
        f.entry_end = len(f.lines)
        for i, line in enumerate(f.lines):
            if i and re.match(r'^[\w.$-]+:', line):
                f.entry_end = i
                break
    return Module(lang, funcs, named, globals_, lines, externs)


# ------------------------------------------------------------------ channels

Key = tuple  # ("field", T, path) | ("param", f, index)


@dataclass
class Channel:
    fns: set[str] = field(default_factory=set)
    links: set = field(default_factory=set)
    untrusted: list[str] = field(default_factory=list)


@dataclass
class Site:
    lang: str
    caller: str
    key: Key | None  # None: a channel the rules do not read
    why: str  # how it was classified
    vtable: bool = False


class Analysis:
    def __init__(self, modules: list[Module], level: int = 0):
        self.mods = modules
        # W7f.1: nested constant initializers, and a zero memset (it writes
        # only nulls, which add no target) is not an opaque write.
        self.nested = level >= 1
        self.zero_memset_ok = level >= 1
        # W7f.2: a coerced store that is provably a whole copy of a T (a
        # by-value T argument, or a returned T) is a T-from-T copy.
        self.coerced_copy_ok = level >= 2
        self._coerced: list[tuple[Module, Func, str, str, str]] = []
        self.funcs_by_name = {n: (m, fn) for m in modules for n, fn in m.funcs.items()}
        self.ch: dict[Key, Channel] = {}
        self.sites: list[Site] = []
        self.escaped: set[str] = set()
        self.direct_callers: dict[str, list[tuple[Func, list[str], Module]]] = {}
        self._memdst: set[tuple[str, int]] = set()
        self._index()
        for m in modules:
            for f in m.funcs.values():
                self._scan(m, f)
            if m.lang == "c":
                self._initializers(m)
        self.boundary = level >= 1
        if self.boundary:
            self._unseen_writers()
        self._params()

    def _level1(self) -> bool:
        return getattr(self, "nested", False)

    def chan(self, k: Key) -> Channel:
        return self.ch.setdefault(k, Channel())

    # -- what a value is ---------------------------------------------------

    @staticmethod
    def strip(f: Func, v: str) -> str:
        """`v` through identity casts: the walk's `llvm-dis` upgrades clang
        14's typed bitcode to opaque pointers and leaves each old pointer
        cast as `bitcast ptr %x to ptr`, which names the same address."""
        seen = 0
        while seen < 16:
            d = f.defs.get(v, "")
            m = re.match(r'^bitcast\s+ptr\s+(%[\w.$-]+)\s+to\s+ptr\s*$', d)
            if not m:
                return v
            v = m.group(1)
            seen += 1
        return v

    def _alloca_param(self, f: Func, slot: str) -> str | None:
        """The parameter an -O0 alloca holds, if the alloca follows the rule:
        one store, of a parameter, in the entry block; every other use a
        plain load; its address never escapes."""
        if not f.defs.get(slot, "").startswith("alloca"):
            return None
        stored: list[tuple[int, str]] = []
        for i, line in enumerate(f.lines):
            if slot not in _VAL.findall(line):
                continue
            if line.strip().startswith(f"{slot} ="):
                continue
            s = _STORE.match(line)
            if s and s.group(3) == slot and s.group(2) != slot:
                stored.append((i, s.group(2)))
                continue
            a = _ASSIGN.match(line)
            if a and _LOAD_PTR.match(a.group(2)) and _LOAD_PTR.match(a.group(2)).group(1) == slot:
                continue
            if a and re.match(r'^load\s', a.group(2)) and a.group(2).split(",")[-1].strip() \
                    .endswith(slot):
                continue
            return None  # passed, stored, cast, GEP'd: escapes
        if len(stored) != 1:
            return None
        i, v = stored[0]
        return v if (i < f.entry_end and v in f.params) else None

    def value_key(self, m: Module, f: Func, v: str) -> tuple[Key | None, str]:
        """The channel a pointer VALUE comes from, or None with a reason."""
        if v.startswith("@"):
            return ("fn", _name(v)), "a function"
        v = self.strip(f, v)
        if v in f.params:
            return ("param", f.name, f.params.index(v)), "a parameter"
        d = f.defs.get(v)
        if d is None:
            return None, "a value with no definition"
        lm = _LOAD_PTR.match(d)
        if lm:
            addr = lm.group(1)
            if m.lang == "c":
                p = self._alloca_param(f, addr)
                if p is not None:
                    return ("param", f.name, f.params.index(p)), "a parameter through its alloca"
            fk = self.field_of(m, f, addr)
            if fk is not None:
                return fk, "a field load"
            return None, "a load from memory the rules do not place"
        if d.startswith(("phi ", "select ")):
            return None, "a phi or select"
        if d.startswith(("inttoptr", "bitcast", "addrspacecast")):
            return None, "a cast"
        return None, "a value the rules do not read"

    def field_of(self, m: Module, f: Func, addr: str) -> Key | None:
        d = f.defs.get(self.strip(f, addr))
        if d is None:
            return None
        g = _STRUCT_GEP.match(d)
        if not g or not g.group(1).startswith("%struct."):
            return None
        idx = [int(x.split()[-1]) for x in g.group(3).split(",") if x.strip()]
        if len(idx) < 2 or idx[0] != 0:
            return None
        return ("field", g.group(1), tuple(idx[1:]))

    # -- scanning ----------------------------------------------------------

    def _index(self) -> None:
        for m in self.mods:
            for f in m.funcs.values():
                for line in f.lines:
                    c = _CALL.search(line)
                    parts = _call_parts(c.group(1)) if c else None
                    callee = parts[0] if parts else None
                    if parts and callee.startswith("@"):
                        self.direct_callers.setdefault(_name(callee), []).append((f, parts[1], m))
                    for r in _FNREF.findall(line):
                        if callee is None or _name(r) != _name(callee):
                            self.escaped.add(_name(r))
            for _, init in m.globals_:
                for r in _FNREF.findall(init):
                    self.escaped.add(_name(r))

    def _scan(self, m: Module, f: Func) -> None:
        for line in f.lines:
            s = _STORE.match(line)
            if s:
                self._store(m, f, s.group(1).strip(), s.group(2), s.group(3))
            a = _ASSIGN.match(line)
            body = a.group(2) if a else line.strip()
            c = _CALL.search(body)
            if c and " asm " in body and m.lang == "c" and self._level1():
                # inline asm with a pointer operand into a T can write T.
                ops = re.search(r'asm\s+(?:sideeffect\s+)?(?:alignstack\s+)?"[^"]*",\s*"[^"]*"\((.*)\)',
                                body)
                for a in _split_top(ops.group(1)) if ops else []:
                    v = a.split()[-1] if a.split() else ""
                    t = self._struct_at(m, f, v) if v.startswith(("%", "@")) else None
                    fk = self.field_of(m, f, v) if v.startswith("%") else None
                    for tt in {t, fk[1] if fk else None} - {None}:
                        self._opaque.setdefault(tt, []).append(f"an inline asm operand ({f.name})")
            if c and " asm " not in body:
                parts = _call_parts(c.group(1))
                if parts and parts[0].startswith("%"):
                    self._site(m, f, parts[0])
                if parts and m.lang == "c":
                    # a field's address passed to a call: whoever receives it
                    # can write the field.
                    for v in parts[1]:
                        fk = self.field_of(m, f, v)
                        if fk is not None:
                            self.chan(fk).untrusted.append(f"its address is passed ({f.name})")
            if m.lang == "c" and "@llvm.mem" in body:
                self._memop(m, f, body)

    def _site(self, m: Module, f: Func, callee: str) -> None:
        d = f.defs.get(callee, "")
        if m.lang == "rust":
            lm = _LOAD_PTR.match(d)
            if lm:
                # a load at a known offset of a pointer: the walk binds it by
                # vtable slot (W7), so it is not this item's to bind.
                self.sites.append(Site("rust", f.name, None, "vtable slot", vtable=True))
                return
        k, why = self.value_key(m, f, callee)
        if k is not None and k[0] == "fn":
            k = None
            why = "a constant function (not indirect)"
        self.sites.append(Site(m.lang, f.name, k, why))

    def _store(self, m: Module, f: Func, ty: str, val: str, addr: str) -> None:
        if m.lang != "c":
            return
        fk = self.field_of(m, f, addr)
        if fk is not None:
            if ty == "ptr":
                if val == "null":
                    return
                k, why = self.value_key(m, f, val)
                if k is None:
                    self.chan(fk).untrusted.append(f"a store of {why} ({f.name})")
                elif k[0] == "fn":
                    self.chan(fk).fns.add(k[1])
                else:
                    self.chan(fk).links.add(k)
            return
        # A whole-struct or ABI-coerced store onto a T (`store [2 x i32] %v`):
        # an opaque write to every pointer field of T.
        if ty != "ptr":
            t = self._struct_at(m, f, addr)
            if t is not None and self.coerced_copy_ok and re.match(r'^\[\d+ x i\d+\]$|^i\d+$', ty):
                # decided once every function is known (see solve)
                self._coerced.append((m, f, ty, val, t))
                t = None
            if t is not None:
                for k in [k for k in list(self.ch) if k[0] == "field" and k[1] == t]:
                    self.ch[k].untrusted.append(f"an opaque {ty} store onto {t} ({f.name})")
                self._opaque.setdefault(t, []).append(f"an opaque {ty} store ({f.name})")
        # The address of a field stored somewhere: it escapes.
        vk = self.field_of(m, f, val) if val.startswith("%") else None
        if vk is not None:
            self.chan(vk).untrusted.append(f"its address is stored ({f.name})")

    _opaque: dict[str, list[str]] = {}

    def _struct_at(self, m: Module, f: Func, addr: str) -> str | None:
        """The struct type a pointer points at the START of, when the IR says:
        an alloca of it, a global of it, or a GEP to a field of that type."""
        addr = self.strip(f, addr)
        if addr.startswith("@"):
            g = dict(m.globals_).get(_name(addr), "")
            t = g.split(None, 1)[0] if g else ""
            inner = re.match(r'^\[\d+ x (%struct\.[\w.$-]+)\]$', t)
            t = inner.group(1) if inner else t
            return t if t.startswith("%struct.") else None
        d = f.defs.get(addr, "")
        a = _ALLOCA.match(d)
        if a:
            t = a.group(1).strip()
            inner = re.match(r'^\[\d+ x (%struct\.[\w.$-]+)\]$', t)
            t = inner.group(1) if inner else t
            if t.startswith("%struct."):
                return t
        g = _STRUCT_GEP.match(d)
        if g and g.group(1).startswith("%struct."):
            idx = [int(x.split()[-1]) for x in g.group(3).split(",") if x.strip()]
            if len(idx) == 1 or all(i == 0 for i in idx):
                return g.group(1)
        # An element of an array of T, at a constant or variable index.
        e = re.match(r'^getelementptr\s+(?:inbounds\s+)?\[\d+ x (%struct\.[\w.$-]+)\],\s*ptr\s+'
                     r'\S+,\s*i\d+\s+0,\s*i\d+\s+\S+\s*$', d)
        if e:
            return e.group(1)
        return None

    def _field_type(self, m: Module, t: str, path: tuple) -> str | None:
        """The type of field `path` of struct `t`, from the module's named
        types."""
        for i in path:
            body = m.named.get(t, "")
            if not body.startswith("{"):
                inner = re.match(r'^\[\d+ x (.+)\]$', t)
                if inner:
                    t = inner.group(1).strip()
                    continue
                return None
            fields = _split_top(body[1:-1])
            if i >= len(fields):
                return None
            t = fields[i].strip()
        return t

    def _types_at(self, m: Module, f: Func, addr: str) -> list[str]:
        """Every struct type that STARTS at `addr`, outermost first: the
        pointee the IR names, then its first field, its first field's
        first field, and so on. One address is all of them at once."""
        t = self._struct_at(m, f, addr)
        if t is None:
            d = f.defs.get(self.strip(f, addr), "")
            g = _STRUCT_GEP.match(d)
            if g and g.group(1).startswith("%struct."):
                idx = [int(x.split()[-1]) for x in g.group(3).split(",") if x.strip()]
                ft = self._field_type(m, g.group(1), tuple(idx[1:])) if len(idx) > 1 else None
                t = ft if ft and ft.startswith("%struct.") else None
        out: list[str] = []
        while t and t.startswith("%struct.") and t not in out:
            out.append(t)
            ft = self._field_type(m, t, (0,))
            t = ft
        return out

    def _memop(self, m: Module, f: Func, body: str) -> None:
        parts = _call_parts(body)
        if not parts or not parts[1]:
            return
        dst = parts[1][0]
        chain = self._types_at(m, f, dst) if self.coerced_copy_ok else []
        t = chain[0] if chain else self._struct_at(m, f, dst)
        if t is None:
            # An untyped destination. When it is the function's own parameter
            # (directly or through its -O0 alloca), what the callers pass
            # there is written opaquely (a registration through memcpy).
            if self._level1():
                p = self.strip(f, dst)
                pd = f.defs.get(p, "")
                lm = _LOAD_PTR.match(pd)
                if lm:
                    q = self._alloca_param(f, lm.group(1))
                    p = q if q is not None else p
                if p in f.params:
                    self._memdst.add((f.name, f.params.index(p)))
            return
        if "memcpy" in parts[0] or "memmove" in parts[0]:
            src = parts[1][1] if len(parts[1]) > 1 else ""
            if self._struct_at(m, f, src) == t:
                return  # a T copied from a T copies trusted fields
            # W7f.2: the destination is every struct that starts there, so a
            # copy from any one of them is a T-from-T copy of that one.
            if chain and set(self._types_at(m, f, src)) & set(chain):
                return
        elif "memset" in parts[0] and self.zero_memset_ok and len(parts[1]) > 1 \
                and parts[1][1] == "0":
            return  # zero fill: every pointer field becomes null, which calls nothing
        self._opaque.setdefault(t, []).append(f"{_name(parts[0])} onto it ({f.name})")

    def _initializers(self, m: Module) -> None:
        for _, init in m.globals_:
            toks = init.split(None, 1)
            if not toks:
                continue
            t = toks[0]
            if t.startswith("%struct.") and len(toks) > 1:
                self._const(m, t, toks[1].split(", align")[0].split(", section")[0], (),
                            top=True)
            elif self.nested and t.startswith("[") and len(toks) > 1:
                self._const(m, t, toks[1].split(", align")[0].split(", section")[0], (),
                            top=True)

    def _const(self, m: Module, t: str, value: str, path: tuple, top: bool) -> None:
        value = value.strip()
        body_t = m.named.get(t, t).strip()
        if value[:1] == "{" and body_t[:1] == "{":
            et = _split_top(body_t[1:-1])
            ev = _split_top(value[1:-1])
        elif value[:2] == "<{" and body_t[:2] == "<{":
            et = _split_top(body_t[2:-2])
            ev = _split_top(value[2:-2])
        elif value[:1] == "[" and body_t[:1] == "[":
            for el in _split_top(value[1:-1]):
                parts = el.split(None, 1)
                if len(parts) == 2:
                    self._const(m, parts[0], parts[1], path, top=False)
            return
        else:
            return
        for i, (ft, fv) in enumerate(zip(et, ev)):
            fv_t, _, fv_v = fv.partition(" ")
            if fv_t == "ptr":
                r = _FNREF.match(fv_v.strip())
                if r and t.startswith("%struct.") and (top or self.nested):
                    self.chan(("field", t, path + (i,) if not t.startswith("%struct.")
                               else (i,))).fns.add(_name(r.group(1)))
            elif self.nested and fv_t.startswith(("%struct.", "[", "{")):
                self._const(m, fv_t, fv_v, (), top=False)

    def _params(self) -> None:
        for k in list(self.ch) + [s.key for s in self.sites if s.key]:
            if k[0] != "param":
                continue
            f_name, p = k[1], k[2]
            ch = self.chan(k)
            if f_name in self.escaped:
                ch.untrusted.append(f"{f_name}'s address is taken, so it has indirect callers")
            for caller, args, m in self.direct_callers.get(f_name, []):
                if p >= len(args):
                    ch.untrusted.append(f"a call from {caller.name} passes no argument {p}")
                    continue
                v = args[p]
                if v == "null":
                    continue
                vk, why = self.value_key(m, caller, v)
                if vk is None:
                    ch.untrusted.append(f"{caller.name} passes {why}")
                elif vk[0] == "fn":
                    ch.fns.add(vk[1])
                else:
                    ch.links.add(vk)
                    if vk[0] == "param":
                        self._params_one(vk)

    def _params_one(self, k: Key) -> None:
        if k in self._seen_params:
            return
        self._seen_params.add(k)
        ch = self.chan(k)
        f_name, p = k[1], k[2]
        if f_name in self.escaped:
            ch.untrusted.append(f"{f_name}'s address is taken, so it has indirect callers")
        for caller, args, m in self.direct_callers.get(f_name, []):
            if p >= len(args):
                ch.untrusted.append(f"a call from {caller.name} passes no argument {p}")
                continue
            v = args[p]
            if v == "null":
                continue
            vk, why = self.value_key(m, caller, v)
            if vk is None:
                ch.untrusted.append(f"{caller.name} passes {why}")
            elif vk[0] == "fn":
                ch.fns.add(vk[1])
            else:
                ch.links.add(vk)
                if vk[0] == "param":
                    self._params_one(vk)

    _seen_params: set = set()

    # -- solving -----------------------------------------------------------

    # -- the return / by-value channel (W7f.2) ----------------------------

    def _coerced_load_of(self, m: Module, f: Func, v: str, t: str, depth: int = 0) -> str | None:
        """None when `v` (an ABI-coerced value) is provably the contents of a
        T, else why not: a coerced load of a T slot, a by-value parameter
        every direct caller fills that way, or the result of a call whose
        callee returns that way."""
        if depth > 8:
            return "a copy chain too deep to follow"
        v = self.strip(f, v)
        d = f.defs.get(v, "")
        lm = re.match(r'^load\s+(?:volatile\s+)?(\[\d+ x i\d+\]|i\d+),\s*ptr\s+(%[\w.$-]+|@[\w.$"-]+)', d)
        if lm:
            src = self._struct_at(m, f, lm.group(2))
            return None if src == t else f"a coerced load of {src or 'memory of no known type'}"
        if v in f.params:
            if f.name in self.escaped:
                return f"{f.name}'s address is taken"
            p = f.params.index(v)
            callers = self.direct_callers.get(f.name, [])
            if not callers:
                return f"{f.name} has no direct caller in the IR"
            for caller, args, cm in callers:
                if p >= len(args):
                    return f"a call from {caller.name} passes no argument {p}"
                why = self._coerced_load_of(cm, caller, args[p], t, depth + 1)
                if why:
                    return why
            return None
        c = re.match(r'^(?:tail\s+)?call\s+(?:\S+\s+)*?(?:\[\d+ x i\d+\]|i\d+)\s+@("[^"]+"|[\w.$-]+)\(', d)
        if c:
            callee = self.funcs_by_name.get(_name(c.group(1)))
            if callee is None:
                return f"a return from {_name(c.group(1))}, which has no IR"
            cm, cf = callee
            rets = [r.group(1) for line in cf.lines
                    for r in [re.match(r'^\s*ret\s+\S+(?:\s+x\s+\S+\])?\s+(%[\w.$-]+)', line)] if r]
            if not rets:
                return f"{cf.name} returns no value the rules read"
            for rv in rets:
                why = self._coerced_load_of(cm, cf, rv, t, depth + 1)
                if why:
                    return why
            return None
        return "a coerced value of unknown origin"

    def _decide_coerced(self) -> None:
        for m, f, ty, val, t in self._coerced:
            why = self._coerced_load_of(m, f, val, t)
            if why:
                self._opaque.setdefault(t, []).append(f"an opaque {ty} store: {why} ({f.name})")

    # -- writers the IR cannot see (W7f.1) ---------------------------------

    def _unseen_writers(self) -> None:
        """A struct type of an external global that NO module defines is
        written by code the walk has no IR for (an assembly object, a
        linker-provided table): untrusted."""
        defined = {n for m in self.mods for n, _ in m.globals_}
        for m in self.mods:
            if m.lang != "c":
                continue
            for n, t in m.externs.items():
                if n in defined:
                    continue
                inner = re.match(r'^\[\d+ x (%struct\.[\w.$-]+)\]$', t)
                t = inner.group(1) if inner else t
                if t.startswith("%struct."):
                    self._opaque.setdefault(t, []).append(f"@{n}, which no IR defines")

    def _cross_boundary(self) -> set[str]:
        """Struct types a pointer reaching across Rust<->C can point into,
        transitively, from the C side: Rust may write their fields, and its
        byte-offset GEPs cannot be attributed to (T, F)."""
        cmods = [m for m in self.mods if m.lang == "c"]
        rmods = [m for m in self.mods if m.lang == "rust"]
        cdef = {n: (m, f) for m in cmods for n, f in m.funcs.items()}
        rdef = {n for m in rmods for n in m.funcs}
        rust_refs: set[str] = set()
        for m in rmods:
            for line in m.text_lines:
                rust_refs.update(_name(r) for r in _FNREF.findall(line))
        work: list[tuple[str, str]] = []  # (C function, exposed value)
        seen: set[tuple[str, str]] = set()
        types: set[str] = set()
        self.ffi_fns: set[str] = set()
        for n, (m, f) in cdef.items():
            if n in rust_refs:  # Rust calls it or takes its address
                self.ffi_fns.add(n)
                work += [(n, p) for p in f.params]
            for line in f.lines:
                c = _CALL.search(line)
                parts = _call_parts(c.group(1)) if c else None
                if parts and parts[0].startswith("@") and _name(parts[0]) in rdef:
                    self.ffi_fns.add(_name(parts[0]))
                    work += [(n, v) for v in parts[1] if v.startswith("%")]
                    a = _ASSIGN.match(line)
                    if a:
                        work.append((n, a.group(1)))
                r = re.match(r'^\s*ret\s+ptr\s+(%[\w.$-]+)', line)
                if r and n in rust_refs:
                    work.append((n, r.group(1)))
        for m in cmods:  # a global both sides name
            for g, init in m.globals_:
                if g in rust_refs:
                    t = init.split(None, 1)[0]
                    inner = re.match(r'^\[\d+ x (%struct\.[\w.$-]+)\]$', t)
                    t = inner.group(1) if inner else t
                    if t.startswith("%struct."):
                        types |= self.contained(t)
        while work:
            fn, v = work.pop()
            if (fn, v) in seen or fn not in cdef:
                continue
            seen.add((fn, v))
            m, f = cdef[fn]
            t = self._struct_at(m, f, v)
            if t:
                types |= self.contained(t)
            for line in f.lines:
                if v not in _VAL.findall(line):
                    continue
                a = _ASSIGN.match(line)
                body = a.group(2) if a else line.strip()
                g = _STRUCT_GEP.match(body)
                if g and g.group(2) == v and g.group(1).startswith("%struct."):
                    types |= self.contained(g.group(1))
                st = _STORE.match(line)
                if st and st.group(2) == v and f.defs.get(st.group(3), "").startswith("alloca"):
                    work.append((fn, st.group(3)))  # loads of this slot are exposed too
                    continue
                c = _CALL.search(body)
                parts = _call_parts(c.group(1)) if c else None
                if parts and v in parts[1] and parts[0].startswith("@"):
                    callee = _name(parts[0])
                    if callee in cdef:
                        cp = cdef[callee][1].params
                        i = parts[1].index(v)
                        if i < len(cp):
                            work.append((callee, cp[i]))
                    continue
                if a and (body.startswith(("getelementptr", "load", "bitcast", "phi", "select"))):
                    work.append((fn, a.group(1)))
        return types

    def contained(self, t: str, seen: set[str] | None = None) -> set[str]:
        """`t` and every struct type nested in it, through arrays: an opaque
        write onto a T writes every struct it contains."""
        seen = set() if seen is None else seen
        if t in seen:
            return seen
        seen.add(t)
        body = next((m.named[t] for m in self.mods if t in m.named), "")
        for inner in re.findall(r'%struct\.[\w.$-]+', body):
            self.contained(inner, seen)
        return seen

    def solve(self) -> dict[Key, tuple[bool, set[str], list[str]]]:
        """Each channel's (trusted, functions, why untrusted), to a fixed
        point: a channel linked to an untrusted one is untrusted."""
        self._decide_coerced()
        if getattr(self, "boundary", False):
            for t in self._cross_boundary():
                self._opaque.setdefault(t, []).append("reachable across Rust<->C")
            for fn_name, p in self._memdst:
                for caller, args, m in self.direct_callers.get(fn_name, []):
                    if p < len(args):
                        t = self._struct_at(m, caller, args[p])
                        if t:
                            self._opaque.setdefault(t, []).append(
                                f"written by {fn_name}'s untyped mem op ({caller.name})")
            for k, ch in self.ch.items():
                if k[0] == "param" and k[1] in self.ffi_fns:
                    ch.untrusted.append(f"{k[1]} is called across Rust<->C")
        for t, why in self._opaque.items():
            hit = self.contained(t)
            for k in self.ch:
                if k[0] == "field" and k[1] in hit:
                    self.ch[k].untrusted += [w if k[1] == t else f"{w}, which contains it ({t})"
                                             for w in why]
        fns = {k: set(c.fns) for k, c in self.ch.items()}
        bad = {k: list(c.untrusted) for k, c in self.ch.items()}
        changed = True
        while changed:
            changed = False
            for k, c in self.ch.items():
                for lk in c.links:
                    src = self.ch.get(lk)
                    if src is None:
                        if not bad[k] or "a channel with no entries" not in bad[k][-1]:
                            pass
                        continue
                    if bad.get(lk) and not bad[k]:
                        bad[k] = [f"linked to an untrusted channel ({lk[1]})"]
                        changed = True
                    new = fns.get(lk, set()) - fns[k]
                    if new:
                        fns[k] |= new
                        changed = True
        return {k: (not bad[k], fns[k], bad[k]) for k in self.ch}


def classify(rust_texts: list[str], c_texts: list[str], level: int = 0) -> dict:
    """The W7f trusted-share report for one image, under the rules of stage
    `level` (0: W7f.0, 1: W7f.1, 2: W7f.2)."""
    Analysis._opaque = {}
    Analysis._seen_params = set()
    mods = [parse_module(t, "rust") for t in rust_texts] + \
           [parse_module(t, "c") for t in c_texts]
    an = Analysis(mods, level=level)
    sol = an.solve()
    out: dict = {"c": {"sites": 0, "trusted": 0}, "rust": {"sites": 0, "trusted": 0,
                                                            "vtable": 0},
                 "misses": {}, "sizes": []}
    for s in an.sites:
        lang = out[s.lang]
        if s.vtable:
            lang["vtable"] += 1
            continue
        lang["sites"] += 1
        if s.key is None:
            why = f"untrusted: {s.why}"
        else:
            # A channel nothing enters has no untrusting entry either: trusted,
            # with no store found (Checkpoint 1's category).
            trusted, fns, bad = sol.get(s.key, (True, set(), []))
            if trusted and fns:
                lang["trusted"] += 1
                lang["resolved"] = lang.get("resolved", 0) + 1
                out["sizes"].append(len(fns))
                continue
            if trusted:
                # trusted with no store found: it calls nothing, so it is
                # resolved too (closed world: the untrusting rules above
                # stand for every writer the IR cannot see).
                lang["resolved"] = lang.get("resolved", 0) + 1
            what = s.key[1] if s.key[0] == "field" else f"param of {s.key[1]}"
            why = (f"trusted, no store found: {what}" if trusted
                   else f"untrusted: {what}: {bad[0] if bad else '?'}")
        key = why.split(" (")[0]
        out["misses"][key] = out["misses"].get(key, 0) + 1
    sz = out["sizes"]
    out["median"] = statistics.median(sz) if sz else 0
    out["max"] = max(sz) if sz else 0
    del out["sizes"]
    return out


def resolved_sets(rust_texts: list[str], c_texts: list[str], level: int = 2
                  ) -> dict[str, list[set[str] | None]]:
    """Per caller, each indirect site's targets when the site is RESOLVED
    (its channel's set, possibly empty), or None when it keeps the walk's
    binding (untrusted, a channel the rules do not read, or a vtable slot,
    which W7 binds). For Checkpoint 2's measurement only."""
    Analysis._opaque = {}
    Analysis._seen_params = set()
    mods = [parse_module(t, "rust") for t in rust_texts] + \
           [parse_module(t, "c") for t in c_texts]
    an = Analysis(mods, level=level)
    sol = an.solve()
    out: dict[str, list[set[str] | None]] = {}
    for st in an.sites:
        val: set[str] | None = None
        if not st.vtable and st.key is not None:
            trusted, fns, _ = sol.get(st.key, (True, set(), []))
            if trusted:
                val = set(fns)
        out.setdefault(st.caller, []).append(val)
    return out


def site_reasons(rust_texts: list[str], c_texts: list[str], level: int = 2
                 ) -> dict[str, list[str]]:
    """Per caller, why each indirect site is or is not resolved (issue 2658:
    which unresolved sites make the walk's deepest path)."""
    Analysis._opaque = {}
    Analysis._seen_params = set()
    mods = [parse_module(t, "rust") for t in rust_texts] + \
           [parse_module(t, "c") for t in c_texts]
    an = Analysis(mods, level=level)
    sol = an.solve()
    out: dict[str, list[str]] = {}
    for st in an.sites:
        if st.vtable:
            why = "vtable slot (W7)"
        elif st.key is None:
            why = f"unresolved: {st.why}"
        else:
            trusted, fns, bad = sol.get(st.key, (True, set(), []))
            what = st.key[1] if st.key[0] == "field" else f"param of {st.key[1]}"
            why = (f"resolved: {what}" if trusted
                   else f"unresolved: {what}: {(bad or ['?'])[0].split(' (')[0]}")
        out.setdefault(st.caller, []).append(why)
    return out


def self_test() -> list[str]:
    bad: list[str] = []

    def check(name: str, ok: bool) -> None:
        if not ok:
            bad.append(name)

    c = """%struct.T = type { ptr, i32 }
@tab = constant %struct.T { ptr @h1, i32 0 }
define void @set(ptr %t) {
  %f = getelementptr inbounds %struct.T, ptr %t, i32 0, i32 0
  store ptr @h2, ptr %f
  ret void
}
define void @use(ptr %t) {
  %f = getelementptr inbounds %struct.T, ptr %t, i32 0, i32 0
  %fn = load ptr, ptr %f
  call void %fn(ptr %t)
  ret void
}
define void @h1(ptr %x) {
  ret void
}
define void @h2(ptr %x) {
  ret void
}
"""
    r = classify([], [c])
    check("a field holds its constant initializer and its store",
          (r["c"]["sites"], r["c"]["trusted"]) == (1, 1) and r["max"] == 2)
    bad_store = c.replace("store ptr @h2, ptr %f", "%q = inttoptr i32 4 to ptr\n"
                                                    "  store ptr %q, ptr %f")
    r = classify([], [bad_store])
    check("a store of a cast untrusts the field", r["c"]["trusted"] == 0)
    opaque = c.replace("store ptr @h2, ptr %f", "%a = alloca %struct.T\n"
                                                 "  store [2 x i32] zeroinitializer, ptr %a\n"
                                                 "  store ptr @h2, ptr %f")
    r = classify([], [opaque])
    check("an ABI-coerced store onto a T untrusts its fields", r["c"]["trusted"] == 0)
    coerced = c.replace("store ptr @h2, ptr %f", "%a = alloca %struct.T\n"
                                                  "  %b = bitcast ptr %a to ptr\n"
                                                  "  store [2 x i32] zeroinitializer, ptr %b\n"
                                                  "  store ptr @h2, ptr %f")
    r = classify([], [coerced])
    check("an ABI-coerced store through an identity bitcast still untrusts T",
          r["c"]["trusted"] == 0)
    inner = ("%struct.S = type { i32, [1 x %struct.T] }\n" + c.replace(
        "store ptr @h2, ptr %f", "%s = alloca %struct.S\n"
        "  call void @llvm.memset.p0.i32(ptr %s, i8 0, i32 12, i1 false)\n"
        "  store ptr @h2, ptr %f"))
    r = classify([], [inner])
    check("a memset onto a struct that CONTAINS a T untrusts T", r["c"]["trusted"] == 0)
    escape = c.replace("store ptr @h2, ptr %f", "store ptr @h2, ptr %f\n"
                                                 "  call void @ext(ptr %f)")
    r = classify([], [escape])
    check("a field's address passed to a call untrusts it", r["c"]["trusted"] == 0)
    mc = c.replace("store ptr @h2, ptr %f", "%a = alloca %struct.T\n"
                                             "  call void @llvm.memcpy.p0.p0.i32(ptr %a, "
                                             "ptr %x, i32 8, i1 false)\n"
                                             "  store ptr @h2, ptr %f")
    r = classify([], [mc])
    check("a memcpy onto a T from elsewhere untrusts it", r["c"]["trusted"] == 0)

    p = """define void @apply(ptr %v, ptr %cb) {
  %cb.addr = alloca ptr
  store ptr %cb, ptr %cb.addr
  %c = load ptr, ptr %cb.addr
  call void %c(ptr %v)
  ret void
}
define void @a(ptr %v) {
  call void @apply(ptr %v, ptr @e1)
  ret void
}
define void @e1(ptr %x) {
  ret void
}
"""
    r = classify([], [p])
    check("an -O0 parameter alloca is the parameter channel",
          (r["c"]["sites"], r["c"]["trusted"]) == (1, 1))
    two = p.replace("  %c = load", "  store ptr null, ptr %cb.addr\n  %c = load")
    r = classify([], [two])
    check("an alloca stored twice is untrusted", r["c"]["trusted"] == 0)
    esc = p + "@ptr = global ptr @apply\n"
    r = classify([], [esc])
    check("a function whose address escapes has untrusted parameters",
          r["c"]["trusted"] == 0)
    rust = """define void @r(ptr %f, ptr %x) {
  call void %f(ptr %x)
  ret void
}
define void @q(ptr %x) {
  call void @r(ptr @k, ptr %x)
  ret void
}
define void @k(ptr %x) {
  ret void
}
define void @v(ptr %o) {
  %vt = load ptr, ptr %o
  call void %vt(ptr %o)
  ret void
}
"""
    r = classify([rust], [])
    check("a Rust SSA parameter is the parameter channel; a vtable load is left to W7",
          (r["rust"]["sites"], r["rust"]["trusted"], r["rust"]["vtable"]) == (1, 1, 1))
    ffi_r = "define void @rs(ptr %p) {\n  call void @use(ptr %p)\n  ret void\n}\n"
    check("a struct reached from a C function Rust calls is untrusted at level 1",
          classify([ffi_r], [c], level=1)["c"]["trusted"] == 0
          and classify([ffi_r], [c], level=0)["c"]["trusted"] == 1)
    ext = "@ext_tab = external global %struct.T\n" + c
    check("a struct type of an external global no IR defines is untrusted at level 1",
          classify([], [ext], level=1)["c"]["trusted"] == 0)
    asm = c.replace("store ptr @h2, ptr %f", "store ptr @h2, ptr %f\n"
                    "  call void asm sideeffect \"\", \"r\"(ptr %t)")
    check("an inline asm pointer operand into a T untrusts it at level 1",
          classify([], [asm.replace("%t)", "%f)")], level=1)["c"]["trusted"] == 0)
    reg = c + ("define void @reg(ptr %d, ptr %s) {\n"
               "  call void @llvm.memcpy.p0.p0.i32(ptr %d, ptr %s, i32 8, i1 false)\n"
               "  ret void\n}\n"
               "define void @caller(ptr %x) {\n  %a = alloca %struct.T\n"
               "  call void @reg(ptr %a, ptr %x)\n  ret void\n}\n")
    check("a T passed to a function that memcpys into its parameter is untrusted at level 1",
          classify([], [reg], level=1)["c"]["trusted"] == 0)
    byval = ("%struct.T = type { ptr, i32 }\n"
             "define void @take([2 x i32] %a) {\n  %s = alloca %struct.T\n"
             "  store [2 x i32] %a, ptr %s\n"
             "  %f = getelementptr inbounds %struct.T, ptr %s, i32 0, i32 0\n"
             "  %fn = load ptr, ptr %f\n  call void %fn(ptr %s)\n  ret void\n}\n"
             "define void @give() {\n  %s = alloca %struct.T\n"
             "  %f = getelementptr inbounds %struct.T, ptr %s, i32 0, i32 0\n"
             "  store ptr @h1, ptr %f\n  %v = load [2 x i32], ptr %s\n"
             "  call void @take([2 x i32] %v)\n  ret void\n}\n"
             "define void @h1(ptr %x) {\n  ret void\n}\n")
    check("a by-value T argument is an opaque store at level 1",
          classify([], [byval], level=1)["c"]["trusted"] == 0)
    check("a by-value T argument whose callers pass a T is a T copy at level 2 (W7f.2)",
          classify([], [byval], level=2)["c"]["trusted"] == 1)
    forged = byval.replace("%v = load [2 x i32], ptr %s", "%v = load [2 x i32], ptr @raw")
    check("a by-value T argument filled from untyped memory stays opaque at level 2",
          classify([], ["@raw = global [2 x i32] zeroinitializer\n" + forged],
                   level=2)["c"]["trusted"] == 0)
    ret = ("%struct.T = type { ptr, i32 }\n"
           "define [2 x i32] @mk() {\n  %s = alloca %struct.T\n"
           "  %f = getelementptr inbounds %struct.T, ptr %s, i32 0, i32 0\n"
           "  store ptr @h1, ptr %f\n  %v = load [2 x i32], ptr %s\n  ret [2 x i32] %v\n}\n"
           "define void @use() {\n  %s = alloca %struct.T\n"
           "  %r = call [2 x i32] @mk()\n  store [2 x i32] %r, ptr %s\n"
           "  %f = getelementptr inbounds %struct.T, ptr %s, i32 0, i32 0\n"
           "  %fn = load ptr, ptr %f\n  call void %fn(ptr %s)\n  ret void\n}\n"
           "define void @h1(ptr %x) {\n  ret void\n}\n")
    check("a returned T is a T copy at level 2 (W7f.2)",
          classify([], [ret], level=2)["c"]["trusted"] == 1
          and classify([], [ret], level=1)["c"]["trusted"] == 0)
    firstfield = ("%struct.S = type { ptr, i32 }\n%struct.E = type { %struct.S, i32 }\n"
                  "define void @cp(ptr %e) {\n  %src = alloca %struct.S\n"
                  "  %sf = getelementptr inbounds %struct.S, ptr %src, i32 0, i32 0\n"
                  "  store ptr @h1, ptr %sf\n"
                  "  %d = getelementptr inbounds %struct.E, ptr %e, i32 0, i32 0\n"
                  "  call void @llvm.memcpy.p0.p0.i32(ptr %d, ptr %src, i32 8, i1 false)\n"
                  "  %f = getelementptr inbounds %struct.S, ptr %d, i32 0, i32 0\n"
                  "  %fn = load ptr, ptr %f\n  call void %fn(ptr %e)\n  ret void\n}\n"
                  "define void @h1(ptr %x) {\n  ret void\n}\n")
    check("a copy of an S into an E's first field (also an S) is a T copy at level 2",
          classify([], [firstfield], level=2)["c"]["trusted"] == 1
          and classify([], [firstfield], level=1)["c"]["trusted"] == 0)
    nested_c = """%struct.A = type { ptr }
%struct.D = type { i32, %struct.A }
@d = constant %struct.D { i32 1, %struct.A { ptr @h1 } }
define void @use(ptr %t) {
  %f = getelementptr inbounds %struct.A, ptr %t, i32 0, i32 0
  %fn = load ptr, ptr %f
  call void %fn()
  ret void
}
define void @h1() {
  ret void
}
"""
    check("a nested initializer is not read at level 0 (W7f.0)",
          classify([], [nested_c])["c"]["trusted"] == 0)
    check("a nested initializer is read at level 1 (W7f.1)",
          classify([], [nested_c], level=1)["c"]["trusted"] == 1)
    zero = c.replace("store ptr @h2, ptr %f", "%a = alloca %struct.T\n"
                                               "  call void @llvm.memset.p0.i32(ptr %a, i8 0, "
                                               "i32 8, i1 false)\n"
                                               "  store ptr @h2, ptr %f")
    check("a zero memset onto a T untrusts it at level 0",
          classify([], [zero])["c"]["trusted"] == 0)
    check("a zero memset onto a T is a null write at level 1 (W7f.1)",
          classify([], [zero], level=1)["c"]["trusted"] == 1)
    nonzero = zero.replace("i8 0, i32 8", "i8 %v, i32 8")
    check("a memset of a nonzero value still untrusts a T at level 1",
          classify([], [nonzero], level=1)["c"]["trusted"] == 0)
    return bad
