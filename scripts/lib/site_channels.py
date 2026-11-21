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


def parse_module(text: str, lang: str) -> Module:
    funcs: dict[str, Func] = {}
    named: dict[str, str] = {}
    globals_: list[tuple[str, str]] = []
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
    return Module(lang, funcs, named, globals_, lines)


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
    def __init__(self, modules: list[Module], nested: bool = False):
        self.mods = modules
        self.nested = nested
        self.ch: dict[Key, Channel] = {}
        self.sites: list[Site] = []
        self.escaped: set[str] = set()
        self.direct_callers: dict[str, list[tuple[Func, list[str]]]] = {}
        self._index()
        for m in modules:
            for f in m.funcs.values():
                self._scan(m, f)
            if m.lang == "c":
                self._initializers(m)
        self._params()

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

    def _memop(self, m: Module, f: Func, body: str) -> None:
        parts = _call_parts(body)
        if not parts or not parts[1]:
            return
        dst = parts[1][0]
        t = self._struct_at(m, f, dst)
        if t is None:
            return
        if "memcpy" in parts[0] or "memmove" in parts[0]:
            src = parts[1][1] if len(parts[1]) > 1 else ""
            if self._struct_at(m, f, src) == t:
                return  # a T copied from a T copies trusted fields
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


def classify(rust_texts: list[str], c_texts: list[str], nested: bool = False) -> dict:
    """The W7f trusted-share report for one image."""
    Analysis._opaque = {}
    Analysis._seen_params = set()
    mods = [parse_module(t, "rust") for t in rust_texts] + \
           [parse_module(t, "c") for t in c_texts]
    an = Analysis(mods, nested=nested)
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
                out["sizes"].append(len(fns))
                continue
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
    check("a field holds its constant initializer and its store", r["c"] == {"sites": 1,
                                                                             "trusted": 1}
          and r["max"] == 2)
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
    check("an -O0 parameter alloca is the parameter channel", r["c"] == {"sites": 1,
                                                                         "trusted": 1})
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
          r["rust"] == {"sites": 1, "trusted": 1, "vtable": 1})
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
    check("a nested initializer is not read without nested=True (W7f.0)",
          classify([], [nested_c])["c"]["trusted"] == 0)
    check("a nested initializer is read with nested=True (W7f.1)",
          classify([], [nested_c], nested=True)["c"]["trusted"] == 1)
    return bad
