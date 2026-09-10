#!/usr/bin/env python3
"""Harvest + compare HAND-WRITTEN Rust declarations of a generated C ABI.

Issue 1208. RFC-0054 makes the C headers the SSoT for each ABI surface and
`scripts/gen-abi-bindings.sh` generates the Rust declarations into a committed
`generated.rs`. Two of those surfaces are link-time-bound FREE SYMBOLS rather
than a vtable — the platform surface and the board-entry surface — so a crate
that cannot depend on the cffi crate reaches the seam by writing its own
`unsafe extern "C"` block.

That is using the seam, not bypassing it: `nros-platform-cffi/src/lib.rs` says
the platform layer is deliberately not a vtable, and `nros-core/src/clock.rs`
records that it sits BELOW `nros-platform-cffi` and so cannot depend on it.
What a hand block also does is create a SECOND mirror of a generated file, and
until this module existed no gate read those.

Consequence of the gap, spelled out because the two prior instances were
expensive: `scripts/check-retired-platform-clock-symbols.py` polices retired
NAMES (#547 `undefined reference to 'nros_platform_clock_ms'`, #548 five
undefined refs that took the tier-2 fixture build down). A SIGNATURE change to
a LIVE symbol regenerates `generated.rs`, passes `check-abi-bindings` and the
name half of `check-{platform,board}-abi-mirror`, and leaves every hand mirror
on a stale prototype. An argument gained or lost is a link error; a `-> u64`
that becomes `-> u32` is not — it is silent garbage in the caller's register,
which is why `--self-test` puts the return-type case first.

This is the function-side sibling of `check-ffi-struct-mirrors` (issue 0160),
which answers the same question for `repr(C)` structs.

The RMW surface is deliberately NOT here: that seam IS a runtime vtable
(`NrosRmwVtable`), so a backend reaches it through a struct rather than by
declaring free symbols, and `check-rmw-abi-shape` / `check-rmw-api-parity`
already answer the per-slot question. The `nros_rmw_*` free symbols that do
exist are backend REGISTRATION entry points, which live in no ABI header.

Usage — `abi_hand_decls.py <surface> [--list|--self-test]`, where <surface> is
a key of SURFACES. Invoked by `scripts/check-platform-abi-mirror.sh` and
`scripts/check-board-abi-mirror.sh`; not a gate of its own.
"""

from __future__ import annotations

import re
import sys
from dataclasses import dataclass, field
from pathlib import Path

# ---------------------------------------------------------------------------
# Scope

REPO = Path(__file__).resolve().parents[2]

# Where hand-written mirrors may live. The generated file itself and the
# `nros_*_export*!` macros are the two things that are NOT mirrors (one is the
# generator's output, the other EMITS definitions on the port side and is
# checked by the name half of the surface's own gate).
SWEEP_ROOTS = ("packages", "examples")


@dataclass(frozen=True)
class Surface:
    """One generated ABI surface and the hand mirrors that may shadow it."""

    name: str
    prefix: str
    generated: Path
    header_dir: Path
    # Files that emit definitions rather than mirroring declarations.
    skip: frozenset = frozenset()
    # Hand-declared symbols the bindgen surface does NOT carry, each with the
    # reason. A new entry needs a reason; an entry whose symbol has since
    # joined the header is reported as stale, so this cannot quietly become a
    # bypass list.
    out_of_surface: dict = field(default_factory=dict)


SURFACES = {
    "platform": Surface(
        name="platform",
        prefix="nros_platform_",
        generated=Path("packages/platform/nros-platform-cffi/src/generated.rs"),
        header_dir=Path("packages/platform/nros-platform-api/include/nros"),
        skip=frozenset({Path("packages/platform/nros-platform-cffi/src/lib.rs")}),
        out_of_surface={
            "nros_platform_zephyr_wait_network": (
                "platform_zephyr.h — Zephyr-conditional surface, deliberately"
                " excluded from the bindgen wrapper in gen-abi-bindings.sh"
            ),
            "nros_platform_freertos_seed_rng": (
                "FreeRTOS port extension (nros-board-freertos), not part of the"
                " portable ABI"
            ),
            "nros_platform_stub_counter": (
                "test-only counter in nros-platform-cffi's C stub port"
            ),
            "nros_platform_stub_reset_counters": (
                "test-only counter in nros-platform-cffi's C stub port"
            ),
        },
    ),
    "board": Surface(
        name="board",
        prefix="nros_board_",
        generated=Path("packages/boards/nros-board-cffi/src/generated.rs"),
        header_dir=Path("packages/boards/nros-board-cffi/include/nros"),
        skip=frozenset({Path("packages/boards/nros-board-cffi/src/lib.rs")}),
        out_of_surface={
            "nros_board_log": (
                "nros-board-threadx-linux's own console shim, board-local"
            ),
            "nros_board_log_write_stderr": (
                "nros-board-threadx-linux's own console shim, board-local"
            ),
            "nros_board_freertos_run_init_array": (
                "FreeRTOS board ctor runner, board-local (no ABI header)"
            ),
        },
    ),
}

# ---------------------------------------------------------------------------
# Type normalisation
#
# Two spellings of one ABI type must compare equal, and nothing else may. The
# whole value of this module is in that second clause, so each equivalence
# below names the reason it is one.
#
#  * path prefixes — bindgen emits `core::ffi::c_void`, hand mirrors write
#    `c_void` after a `use core::ffi::c_void`. Same type.
#  * character pointee — `*const c_char` vs `*const u8`. `c_char` is `i8` on
#    x86_64 and `u8` on aarch64/arm, so no fixed spelling is portable and both
#    are the same pointer at the ABI. Normalised ONLY behind a pointer: a bare
#    `-> i8` and a bare `-> u8` stay different, because a return type is where
#    a sign change is a real change.
#  * `Option<fn(..)>` vs `fn(..)` — bindgen renders every C function pointer
#    through `Option` to model NULL. Rust guarantees the null-pointer
#    optimisation for `Option<fn>`, so the two are one word at the ABI. Same
#    rendering the RMW vtable slots carry.
#  * typedefs — `nros_platform_timer_callback_t` resolves to the fn pointer it
#    aliases, read from `generated.rs` rather than hardcoded.
#  * argument NAMES — a mirror may name its parameters anything.

_PTR = re.compile(r"^\*(?:const|mut)\b")
_CHAR_POINTEE = {"c_char", "i8", "u8"}
_FN_PTR = re.compile(r'^(?:unsafe\s+)?extern\s*"C"\s*fn\s*\(')
# `name: ty`, but not the `::` of a path.
_ARG_NAME = re.compile(r"^(?:mut\s+)?[A-Za-z_][A-Za-z0-9_]*\s*:(?!:)")

# `pub type X = Y;` from generated.rs, filled in by load_canonical().
TYPEDEFS: dict[str, str] = {}


def _matching(src: str, open_idx: int, opener: str, closer: str) -> int:
    """Index of the delimiter closing the one at `open_idx`.

    The `>` of a `->` is not a closing angle bracket. Without that case
    `Option<extern "C" fn(..) -> *mut c_void>` closes at the arrow, and the
    callback's return type silently reads as `()` — which is a false DRIFT
    report against a mirror that is correct.
    """
    depth = 0
    i = open_idx
    while i < len(src):
        if src[i] == "-" and src[i : i + 2] == "->":
            i += 2
            continue
        if src[i] == opener:
            depth += 1
        elif src[i] == closer:
            depth -= 1
            if depth == 0:
                return i
        i += 1
    return len(src)


# `core::ffi::c_void` / `::std::os::raw::c_char` / `core::option::Option` and
# their bare spellings are one type each. Anchored so `nros_core::Thing` — a
# crate whose name ENDS in `core` — is left alone.
_STD_PATH = re.compile(
    r"(?<![A-Za-z0-9_])(?:::)?(?:core|std|alloc)::(?:ffi::|option::|os::raw::)?"
)


def _pre_clean(ty: str) -> str:
    # rustfmt leaves a trailing comma inside a wrapped `Option<..,>` / arg list;
    # it is punctuation, not part of the type.
    t = " ".join(ty.split()).rstrip(",").strip()
    t = _STD_PATH.sub("", t)
    t = re.sub(r"\s*\*\s*", "*", t)
    t = re.sub(r"\*(const|mut)\s*", r"*\1 ", t)
    return t.strip()


def split_args(arglist: str) -> list[str]:
    """Split a Rust argument list on top-level commas.

    `->` is NOT a closing angle bracket: a nested fn-pointer return type would
    otherwise drop the nesting depth below zero and split in the wrong place.
    """
    out: list[str] = []
    depth = 0
    cur = ""
    i = 0
    while i < len(arglist):
        ch = arglist[i]
        if ch == "-" and arglist[i : i + 2] == "->":
            cur += "->"
            i += 2
            continue
        if ch in "(<[":
            depth += 1
        elif ch in ")>]":
            depth -= 1
        if ch == "," and depth == 0:
            out.append(cur)
            cur = ""
        else:
            cur += ch
        i += 1
    if cur.strip():
        out.append(cur)
    return [a for a in (x.strip() for x in out) if a]


def _strip_arg_name(arg: str) -> str:
    m = _ARG_NAME.match(arg.strip())
    return arg.strip()[m.end() :].strip() if m else arg.strip()


def normalize_type(ty: str, _depth: int = 0) -> str:
    t = _pre_clean(ty)
    if not t:
        return "()"
    if _depth > 8:
        return t

    # typedef alias -> the type it names (generated.rs is the dictionary)
    if t in TYPEDEFS:
        return normalize_type(TYPEDEFS[t], _depth + 1)

    # Option<fn(..)> is the same word as fn(..)
    if t.startswith("Option<"):
        close = _matching(t, t.index("<"), "<", ">")
        inner = t[t.index("<") + 1 : close].strip()
        if _FN_PTR.match(_pre_clean(inner)):
            return normalize_type(inner, _depth + 1)
        return f"Option<{normalize_type(inner, _depth + 1)}>"

    # function pointer: rebuild from normalised parts, names discarded
    if _FN_PTR.match(t):
        popen = t.index("(")
        pclose = _matching(t, popen, "(", ")")
        args = ", ".join(
            normalize_type(_strip_arg_name(a), _depth + 1)
            for a in split_args(t[popen + 1 : pclose])
        )
        tail = t[pclose + 1 :].strip()
        ret = normalize_type(tail.split("->", 1)[1], _depth + 1) if "->" in tail else "()"
        return f'extern "C" fn({args}) -> {ret}'

    if _PTR.match(t):
        kind, _, pointee = t.partition(" ")
        pointee = normalize_type(pointee, _depth + 1)
        if pointee in _CHAR_POINTEE:
            pointee = "<byte>"
        return f"{kind} {pointee}"

    return t


def arg_types(arglist: str) -> list[str]:
    return [normalize_type(_strip_arg_name(a)) for a in split_args(arglist)]


@dataclass(frozen=True)
class Sig:
    args: tuple[str, ...]
    ret: str

    def render(self) -> str:
        return f"({', '.join(self.args)}) -> {self.ret}"


@dataclass(frozen=True)
class Decl:
    symbol: str
    sig: Sig
    path: Path
    line: int


# ---------------------------------------------------------------------------
# Parsing

_COMMENT = re.compile(r"//.*?$", re.MULTILINE)
_ATTR = re.compile(r"^\s*#(!?)\[[^\n]*\]\s*$", re.MULTILINE)


def _strip_noise(src: str) -> str:
    src = _COMMENT.sub("", src)
    return src


_EXTERN_BLOCK = re.compile(r'(?:\bunsafe\s+)?extern\s+"C"\s*\{')


def _fn_re(prefix: str) -> re.Pattern:
    return re.compile(
        r"\b(?:pub(?:\([^)]*\))?\s+)?fn\s+(" + re.escape(prefix) + r"[A-Za-z0-9_]+)\s*\("
    )


def parse_decls(path: Path, text: str, prefix: str) -> list[Decl]:
    """Every `fn <prefix>*` DECLARATION inside an `extern "C" { }` block."""
    src = _strip_noise(text)
    fn_re = _fn_re(prefix)
    decls: list[Decl] = []
    for blk in _EXTERN_BLOCK.finditer(src):
        open_idx = src.index("{", blk.start())
        close_idx = _matching(src, open_idx, "{", "}")
        body = src[open_idx + 1 : close_idx]
        base = open_idx + 1
        for fn in fn_re.finditer(body):
            popen = body.index("(", fn.end() - 1)
            pclose = _matching(body, popen, "(", ")")
            args = body[popen + 1 : pclose]
            tail = body[pclose + 1 :]
            semi = tail.find(";")
            brace = tail.find("{")
            if brace != -1 and (semi == -1 or brace < semi):
                # a definition, not a declaration (macro bodies)
                continue
            ret_src = tail[:semi] if semi != -1 else ""
            ret = ret_src.split("->", 1)[1] if "->" in ret_src else "()"
            line = src.count("\n", 0, base + fn.start()) + 1
            decls.append(
                Decl(
                    symbol=fn.group(1),
                    sig=Sig(tuple(arg_types(args)), normalize_type(ret)),
                    path=path,
                    line=line,
                )
            )
    return decls


def noreturn_symbols(header_dir: Path, prefix: str) -> set[str]:
    """Symbols the header marks `NROS_PLATFORM_NORETURN`.

    bindgen runs without `--enable-function-attribute-detection` on the
    platform surface, so `generated.rs` renders these as `-> ()`. A hand mirror
    writing `-> !` is MORE faithful to the header, not drift — but only for a
    symbol the header actually marks, which is why this is read rather than
    hardcoded.
    """
    out: set[str] = set()
    pat = re.compile(
        r"(?:NROS_PLATFORM_NORETURN|_Noreturn|\[\[noreturn\]\]|__attribute__\s*\(\(\s*noreturn"
        r"\s*\)\))\b[^;{]*?\b(" + re.escape(prefix) + r"[A-Za-z0-9_]+)\s*\(",
        re.S,
    )
    for hdr in sorted(header_dir.glob("*.h")):
        text = hdr.read_text()
        # The #define lines that introduce the macro are not declarations.
        text = re.sub(r"^\s*#\s*define\b.*$", "", text, flags=re.M)
        for m in pat.finditer(text):
            out.add(m.group(1))
    return out


_TYPEDEF = re.compile(r"^pub type ([A-Za-z_][A-Za-z0-9_]*)\s*=\s*(.*?);", re.S | re.M)


def load_canonical(repo: Path, surface: Surface) -> dict[str, Sig]:
    """Canonical signatures, plus the typedef dictionary a mirror may expand.

    TYPEDEFS is populated BEFORE parsing so `nros_platform_timer_callback_t`
    and the fn pointer a hand mirror spells out compare as one type.
    """
    text = (repo / surface.generated).read_text()
    TYPEDEFS.clear()
    for name, body in _TYPEDEF.findall(_strip_noise(text)):
        TYPEDEFS[name] = body
    return {
        d.symbol: d.sig
        for d in parse_decls(surface.generated, text, surface.prefix)
    }


def iter_rust_files(repo: Path, surface: Surface):
    for root in SWEEP_ROOTS:
        base = repo / root
        if not base.is_dir():
            continue
        for path in base.rglob("*.rs"):
            rel = path.relative_to(repo)
            parts = set(rel.parts)
            if "target" in parts or any(p.startswith("target-") for p in rel.parts):
                continue
            if "third-party" in parts or "generated" in parts:
                continue
            if rel == surface.generated or rel in surface.skip:
                continue
            yield rel, path


# Every `fn nros_platform_*` in the tree, parsed or not. The gate's own
# coverage guard: a declaration this finds and `parse_decls` does not is a
# mirror nothing is checking, which is the issue-0196 shape this gate exists
# to close — so it is reported rather than skipped.
def _fn_any_re(prefix: str) -> re.Pattern:
    return re.compile(r"\bfn\s+(" + re.escape(prefix) + r"[A-Za-z0-9_]+)\s*\(")


def _is_definition(body: str, pclose: int) -> bool:
    tail = body[pclose + 1 :]
    m = re.match(r"\s*(->[^;{]*)?", tail)
    rest = tail[m.end() :].lstrip() if m else tail.lstrip()
    return rest.startswith("{")


def unaccounted(
    path: Path, text: str, decls: list[Decl], prefix: str
) -> list[tuple[str, int]]:
    """`fn <prefix>*` occurrences that are neither parsed nor definitions."""
    src = _strip_noise(text)
    covered = {(d.symbol, d.line) for d in decls}
    out: list[tuple[str, int]] = []
    for m in _fn_any_re(prefix).finditer(src):
        line = src.count("\n", 0, m.start()) + 1
        if (m.group(1), line) in covered:
            continue
        popen = src.index("(", m.end() - 1)
        pclose = _matching(src, popen, "(", ")")
        if _is_definition(src, pclose):
            continue
        out.append((m.group(1), line))
    return out


def collect(repo: Path, surface: Surface) -> tuple[list[Decl], list[str]]:
    found: list[Decl] = []
    gaps: list[str] = []
    for rel, path in iter_rust_files(repo, surface):
        try:
            text = path.read_text()
        except (OSError, UnicodeDecodeError):
            continue
        if surface.prefix not in text:
            continue
        decls = parse_decls(rel, text, surface.prefix)
        found.extend(decls)
        for sym, line in unaccounted(rel, text, decls, surface.prefix):
            gaps.append(
                f"{rel}:{line}: {sym} is declared here but the harvester could"
                f" not parse it, so its signature is UNCHECKED.\n"
                f"    Fix scripts/lib/abi_hand_decls.py rather than leaving"
                f" the declaration outside the gate's reach."
            )
    return found, gaps


# ---------------------------------------------------------------------------
# Negative control
#
# "N declarations match" is also what a comparator that can never fail would
# print. These cases are run by `--self-test` on every gate invocation: each
# pair must compare EQUAL or UNEQUAL as stated, and the return-type case is
# first because that is the drift direction with no link error behind it.

_SELF_TEST = [
    # (name, generated-side, hand-side, must_match)
    (
        "return type narrowed (silent: no link error, garbage in the register)",
        "pub fn nros_platform_clock_ns() -> u64;",
        "fn nros_platform_clock_ns() -> u32;",
        False,
    ),
    (
        "argument type widened",
        "pub fn nros_platform_sleep_us(us: usize);",
        "fn nros_platform_sleep_us(us: u64);",
        False,
    ),
    (
        "argument dropped",
        "pub fn nros_platform_wake_wait_ms(w: *mut core::ffi::c_void, t: u32) -> i8;",
        "fn nros_platform_wake_wait_ms(w: *mut c_void) -> i8;",
        False,
    ),
    (
        "argument added",
        "pub fn nros_platform_wake_signal(w: *mut core::ffi::c_void) -> i8;",
        "fn nros_platform_wake_signal(w: *mut c_void, flags: u32) -> i8;",
        False,
    ),
    (
        "pointer mutability flipped",
        "pub fn nros_platform_wake_init(w: *mut core::ffi::c_void) -> i8;",
        "fn nros_platform_wake_init(w: *const c_void) -> i8;",
        False,
    ),
    (
        "path spelling only — same type",
        "pub fn nros_platform_wake_drop(w: *mut core::ffi::c_void) -> i8;",
        "fn nros_platform_wake_drop(handle: *mut c_void) -> i8;",
        True,
    ),
    (
        "Option<fn> vs bare fn — null-pointer-optimised, one word",
        'pub fn nros_platform_t(cb: ::core::option::Option<unsafe extern "C" '
        'fn(u: *mut core::ffi::c_void)>) -> i8;',
        'fn nros_platform_t(cb: unsafe extern "C" fn(*mut c_void)) -> i8;',
        True,
    ),
    (
        "char pointee spelling — c_char is u8 on arm, i8 on x86_64",
        "pub fn nros_platform_p(m: *const core::ffi::c_char, n: usize);",
        "fn nros_platform_p(m: *const u8, n: usize);",
        True,
    ),
]


def self_test() -> int:
    """Prove the comparator can distinguish; a gate that cannot fail is not one.

    Surface-independent: these are synthetic sources, and what is under test is
    the normaliser and the comparison, not the tree.
    """
    bad: list[str] = []
    for name, gen_src, hand_src, must_match in _SELF_TEST:
        g = parse_decls(
            Path("<self-test:generated>"),
            f'unsafe extern "C" {{ {gen_src} }}',
            "nros_platform_",
        )
        h = parse_decls(
            Path("<self-test:hand>"),
            f'unsafe extern "C" {{ {hand_src} }}',
            "nros_platform_",
        )
        if len(g) != 1 or len(h) != 1:
            bad.append(
                f"{name}: parser produced {len(g)}/{len(h)} declarations, want 1/1"
            )
            continue
        equal = g[0].sig == h[0].sig
        if equal != must_match:
            verdict = "compared EQUAL" if equal else "compared UNEQUAL"
            want = "equal" if must_match else "unequal"
            bad.append(
                f"{name}: {verdict}, want {want}\n"
                f"      generated: {g[0].sig.render()}\n"
                f"      hand:      {h[0].sig.render()}"
            )
    if bad:
        print("ABI signature comparator self-test FAILED:", file=sys.stderr)
        for b in bad:
            print(f"  {b}", file=sys.stderr)
        return 2
    print(f"comparator self-test: {len(_SELF_TEST)} cases behave as declared")
    return 0


def main(argv: list[str]) -> int:
    repo = REPO
    args = [a for a in argv if not a.startswith("--")]
    flags = {a for a in argv if a.startswith("--")}
    unknown_flags = flags - {"--list", "--self-test"}
    if unknown_flags or len(args) > 1:
        print(f"usage: {Path(__file__).name} <{'|'.join(SURFACES)}> [--list|--self-test]",
              file=sys.stderr)
        return 2
    key = args[0] if args else "platform"
    surface = SURFACES.get(key)
    if surface is None:
        print(
            f"error: unknown ABI surface {key!r}; known: {', '.join(sorted(SURFACES))}",
            file=sys.stderr,
        )
        return 2

    canonical = load_canonical(repo, surface)
    if not canonical:
        print(
            f"error: no {surface.prefix}* declarations parsed from"
            f" {surface.generated} — the harvester is broken, not the tree",
            file=sys.stderr,
        )
        return 2

    if "--self-test" in flags:
        return self_test()

    noreturn = noreturn_symbols(repo / surface.header_dir, surface.prefix)

    decls, gaps = collect(repo, surface)
    if not decls:
        print(
            f"error: swept {' + '.join(SWEEP_ROOTS)} and found no hand-written"
            f" {surface.prefix}* declaration — the harvester matched nothing,"
            " which is not a green result",
            file=sys.stderr,
        )
        return 2

    if "--list" in flags:
        for d in sorted(decls, key=lambda x: (str(x.path), x.line)):
            state = (
                "checked"
                if d.symbol in canonical
                else ("exempt" if d.symbol in surface.out_of_surface else "UNKNOWN")
            )
            print(f"{d.path}:{d.line}\t{state}\t{d.symbol}{d.sig.render()}")
        return 0

    problems: list[str] = list(gaps)
    checked = 0
    exempted = 0

    for d in sorted(decls, key=lambda x: (str(x.path), x.line)):
        want = canonical.get(d.symbol)
        if want is None:
            if d.symbol in surface.out_of_surface:
                exempted += 1
                continue
            problems.append(
                f"{d.path}:{d.line}: {d.symbol} is declared by hand but is not"
                f" in {surface.generated}.\n"
                f"    Either the header retired it (the #547 / #548 shape:"
                f" compiles, fails at LINK), or it is a port extension —\n"
                f"    in which case add it to SURFACES[{surface.name!r}]"
                f".out_of_surface in scripts/lib/abi_hand_decls.py with the"
                f" reason."
            )
            continue

        checked += 1
        if d.sig == want:
            continue
        # A hand mirror may write `-> !` where bindgen wrote `-> ()`, but only
        # for a symbol the HEADER marks noreturn — read, not assumed. bindgen
        # runs without --enable-function-attribute-detection on this surface,
        # so `!` there is the more faithful rendering, not drift.
        if (
            d.symbol in noreturn
            and d.sig.ret == "!"
            and want.ret == "()"
            and d.sig.args == want.args
        ):
            continue
        problems.append(
            f"{d.path}:{d.line}: {d.symbol} signature drifted from the"
            f" generated mirror.\n"
            f"    header/generated: {d.symbol}{want.render()}\n"
            f"    hand-written:     {d.symbol}{d.sig.render()}\n"
            f"    The header is the SSoT (RFC-0054). Fix the hand declaration,"
            f" or edit the header and rerun scripts/gen-abi-bindings.sh."
        )

    for sym, reason in sorted(surface.out_of_surface.items()):
        if sym in canonical:
            problems.append(
                f"scripts/lib/abi_hand_decls.py: {sym} is exempted as"
                f" out-of-surface ({reason}) but {surface.generated} now"
                f" declares it. Drop the exemption so the signature is checked."
            )

    if problems:
        print(
            f"hand-written {surface.name} ABI declarations disagree with the"
            f" generated mirror:",
            file=sys.stderr,
        )
        for p in problems:
            print(f"  {p}", file=sys.stderr)
        return 1

    files = len({d.path for d in decls})
    print(
        f"hand-written {surface.name} ABI mirrors clean: {checked} declarations"
        f" across {files} files match {surface.generated}"
        f" ({exempted} exempted as out-of-surface)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
