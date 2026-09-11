#!/usr/bin/env python3
"""Enumerate every build-time sizing knob, so a consumer can find them all.

Issue 0739, from issue 0271's own conclusion. A 256 KB-class image was
rightsized with NINE tuning envs and still inherited ~145 KB of defaults across
four separate features:

    ZPICO_MAX_LARGE_SUBSCRIBERS(2) x ZPICO_SUBSCRIBER_RING_DEPTH(4)
        x ZPICO_SUBSCRIBER_LARGE_SIZE(16384)  =  131,072 bytes

Every one of those knobs already existed. Four of the five wins in that audit
were the same shape — **the knob was there and the consumer did not know** — and
0271 recorded the lesson: "the durable fix is not more knobs, it is making the
existing ones enumerable". Not one of those five appears in the book's
environment-variables reference today, which is why this exists.

## What is mechanical, and what is not

Enumerating the KNOBS is mechanical: each is read at a call site whose default
is a literal argument, so name + default + owning crate are all recoverable
without building anything. That alone surfaces all four knobs 0271's consumer
missed, which is the whole point.

BYTES are not mechanical. A pool is a `static mut [[[u8; A]; B]; C]` over
generated consts from several crates; resolving that needs a compiler, and
guessing would put fabricated numbers next to measured ones. So bytes are
OPT-IN: a pool declares its own arithmetic and this computes it at the knobs'
defaults.

    // nros-pool: LARGE_PAYLOADS = ZPICO_MAX_LARGE_SUBSCRIBERS \
    //   * ZPICO_SUBSCRIBER_RING_DEPTH * ZPICO_SUBSCRIBER_LARGE_SIZE

Unannotated knobs still get a row — name, default, crate — they simply carry no
byte figure, and the table says so rather than implying zero.

Run:  python3 scripts/gen-pool-inventory.py [--check] [--self-test]
"""

import importlib.util
import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT = os.path.join(ROOT, "book", "src", "reference", "static-pool-inventory.md")

# The spellings a knob is read by. Each yields (NAME, default-literal).
KNOB_PATTERNS = [
    re.compile(r'\benv_usize\(\s*"([A-Z0-9_]+)"\s*,\s*([0-9_]+)\s*\)'),
    re.compile(r'\benv_usize_compat\(\s*"([A-Z0-9_]+)"\s*,\s*"[A-Z0-9_]+"\s*,\s*([0-9_]+)\s*\)'),
    re.compile(r'\bknob_usize\([^,]+,\s*"([A-Z0-9_]+)"\s*,\s*([0-9_]+)\s*\)'),
    re.compile(
        r'std::env::var\(\s*"([A-Z0-9_]+)"\s*\)[\s\S]{0,120}?unwrap_or_else\(\s*\|_\|\s*"([0-9_]+)"'
    ),
    # A crate-local wrapper over `knob_usize` — `knob("NAME", 8)`, the shape
    # `packages/rmw/cffi/build.rs` took when it moved off a bare `env::var`
    # (issue 0752 follow-up). That move dropped three knobs and one pool's byte
    # figure out of this table: `SLOTS` went from "8,192" to "unknown knob",
    # which is exactly the enumeration failure issue 0271 cost ~145 KB to. A
    # wrapper is the natural thing to write when several knobs share a
    # resolution rule, so match the shape rather than asking each crate not to.
    re.compile(r'\bknob\(\s*"([A-Z0-9_]+)"\s*,\s*([0-9_]+)\s*\)'),
    # `env_usize_min("NAME", default, floor)` — a knob whose reader REFUSES a
    # value below a floor instead of silently rounding it up (issue 0827). The
    # reported figure is the DEFAULT, exactly as for every other spelling: the
    # floor is a validity rule on what a user may ask for, not a size the image
    # pays. Added with the spelling, not after it: renaming the two zpico reads
    # to this wrapper made `ZPICO_SUBSCRIBER_RING_DEPTH` and
    # `ZPICO_MAX_LARGE_SUBSCRIBERS` invisible here, and with them the byte
    # figures for BOTH payload pools -- the `SLOTS` regression this list already
    # records, one wrapper later.
    re.compile(r'\benv_usize_min\(\s*"([A-Z0-9_]+)"\s*,\s*([0-9_]+)\s*,\s*[0-9_]+\s*\)'),
    # `knob("NAME", rung, 32)` — the LADDER shape (phase-400 W6): env, then
    # Kconfig, then the platform/board rung, then the builtin LAST. So the
    # figure this table wants is the third argument, where every other spelling
    # puts it second.
    #
    # Third entry in this list added for the same reason as the two above it,
    # which is the point: a crate that grows a resolution rule wraps its reads,
    # and the wrapper is invisible here until someone notices a knob went
    # missing. The params tenant's five knobs rendered as "computed — see
    # build.rs:25" -- a line number in a generated table, churning on every
    # edit, in place of the default it exists to publish.
    re.compile(
        r'\bknob\(\s*"([A-Z0-9_]+)"\s*,\s*[A-Za-z0-9_.]+\s*,\s*([0-9_]+)\s*\)'
    ),
]

# `// nros-pool: NAME = KNOB * KNOB * 4` — products of knobs and integers only.
POOL_ANNOT = re.compile(
    r"//\s*nros-pool:\s*([A-Za-z0-9_]+)\s*=\s*([A-Z0-9_*\s\\/]+?)\s*$", re.M
)


def tracked_rust():
    out = subprocess.run(
        ["git", "ls-files", "*.rs"], cwd=ROOT, capture_output=True, text=True, check=True
    ).stdout.split()
    return [f for f in out if "/third-party/" not in f]


def crate_of(rel):
    """The crate directory owning a file — the nearest ancestor with Cargo.toml."""
    d = os.path.dirname(os.path.join(ROOT, rel))
    while d.startswith(ROOT) and len(d) > len(ROOT):
        if os.path.isfile(os.path.join(d, "Cargo.toml")):
            return os.path.relpath(d, ROOT)
        d = os.path.dirname(d)
    return os.path.dirname(rel)


# Any env_usize read at all, literal default or not. The delta between this
# and the literal patterns is exactly the set of computed-default knobs —
# the ones issue-0271's failure mode hides (executor arena, zpico batch/frag
# buffers: the LARGEST consumers). They must appear in the table, not be
# silently dropped by a literal-only regex.
KNOB_ANY = re.compile(r'\b(?:env_usize(?:_compat|_min)?|knob)\(\s*"([A-Z0-9_]+)"')

# `const SUBSCRIBER_RING_DEPTH_DEFAULT: usize = 4;` — a file-local builtin that
# a knob read reaches through `.unwrap_or(NAME)`.
CONST_USIZE = re.compile(r"\bconst\s+([A-Z][A-Z0-9_]*)\s*:\s*usize\s*=\s*([0-9_]+)\s*;")

# A knob whose default is a LADDER CHAIN ending in `.unwrap_or(<const>)` —
# phase-454 W6.a:
#
#     env_usize_min("ZPICO_SUBSCRIBER_RING_DEPTH",
#         limits.subscriber_ring_depth.or(declared(..)).unwrap_or(RING_DEPTH_DEFAULT), 1)
#
# Fourth entry in this family, for the third time for the same reason: a crate
# that grows a resolution rule rewrites its READS, and the builtin stops being a
# literal argument. Each previous time the knob silently left this table — and
# with `ZPICO_SUBSCRIBER_RING_DEPTH` that also unprices BOTH payload pools,
# which is the `SLOTS` regression recorded against `env_usize_min` above. The
# builtin here is still a single number, it is just named; resolving the name
# publishes the figure instead of a churning line number.
#
# `[^"]` bounds the chain: it cannot run past a string literal, so a match can
# never pair one knob's name with the NEXT knob read's default.
KNOB_UNWRAP_OR_CONST = re.compile(
    r'\b(?:env_usize(?:_compat|_min|_rung)?|knob)\(\s*"([A-Z0-9_]+)"\s*,'
    r'[^"]{0,400}?\.unwrap_or\(\s*([A-Z][A-Z0-9_]*)\s*\)'
)

# A knob whose FRONT-END NAME lives in a ladder mapping rather than a call.
#
# phase-400 W6 moves a knob's resolution out of its build script and into the
# RFC-0049 ladder. The build script then reads no environment at all — the
# ladder does, through `<tenant>_env_key`, whose body is a match from field name
# to env name:
#
#     "frag_max_size" => "ZPICO_FRAG_MAX_SIZE",
#
# The call-shaped patterns above cannot see that, so migrating a tenant DELETED
# its knobs from this table: five vanished with the `zenoh.wire` tenant, three
# with `params` before that. A knob nobody can enumerate is a knob nobody sets
# (issue 0271), and "it moved to the ladder" is not a reason to stop listing it
# — the ladder is where a user sets it.
#
# Their default is deliberately recorded as COMPUTED: a ladder knob's builtin
# may be per-platform (`nros-zpico-build` picks a batch size from the
# transport), so a single number here would be a claim this file cannot make.
LADDER_ENV_KEY = re.compile(r'=>\s*"([A-Z][A-Z0-9_]+)"\s*,')

# ---------------------------------------------------------------------------
# The XRCE backend's knobs, which have no Rust declaration site at all.
#
# Issue 1078. Five knobs that between them size ~86 % of `xrce_session_state_t`
# (`NROS_XRCE_MAX_SUBSCRIBERS`, `_MAX_SERVICE_SERVERS`, `_MAX_SERVICE_CLIENTS`,
# `_SUBSCRIBER_RING_DEPTH`, `_BUFFER_SIZE`) appeared nowhere in this table,
# before phase-420 W9 and after it. Every pattern above matches a Rust CALL,
# and these are `-D<MACRO>=<n>` arguments both XRCE lanes pass to a C compiler:
# there is no `env_usize(...)` anywhere to find. The enumeration failure issue
# 0271 measured at ~145 KB, in the one backend whose knobs are not Rust.
#
# phase-420 W9 made this cheap: `packages/rmw/xrce/xrce-config.txt` is the ONE
# statement of that configuration, read by both lanes, and its `knob` and
# `define` records carry the env name. So this reads the manifest rather than
# restating five rows here — restatement is the shape W9 spent itself removing.
#
# The DEFAULT comes from neither this file nor the manifest:
#
#   * a `knob` row states its own default (it fills a `@TOKEN@` in an upstream
#     `config.h.in`, which has no default of its own to hold);
#   * a `define` row deliberately has NO default column — xrce-config.txt says
#     so in as many words, because `nros-rmw-xrce/src/internal.h` holds it in
#     an `#ifndef` and "that is the one statement of it". So the `#ifndef`
#     block is what this reads.
#
# Both lanes' env names, both lanes' defaults, zero new copies.
XRCE_MANIFEST = "packages/rmw/xrce/xrce-config.txt"
XRCE_DEFAULTS_H = "packages/rmw/xrce/nros-rmw-xrce/src/internal.h"

# `#ifndef XRCE_FOO` … `#define XRCE_FOO 8` — the guarded default, not a bare
# `#define`. A bare one is a constant nobody can override, so it is not a knob
# and must not be reported as one. The backreference anchors the pair, so an
# intervening comment block (every one of these has one) does not break it.
_IFNDEF_DEFAULT = re.compile(
    r"^#ifndef\s+([A-Za-z_][A-Za-z0-9_]*)\s*$.*?^#define\s+\1\s+([0-9]+)\s*$",
    re.M | re.S,
)
# The same pair whose default is ANOTHER guarded macro rather than a literal —
# phase-454 W6.b. `XRCE_SUBSCRIBER_BUFFER_SIZE` and its two siblings default to
# `XRCE_BUFFER_SIZE`, because one number legitimately serves three families
# until an image says otherwise. Writing 1024 beside each of them would be the
# fourth, fifth and sixth home for a figure this file exists to stop copying;
# following the alias publishes what the compiler actually computes.
_IFNDEF_ALIAS = re.compile(
    r"^#ifndef\s+([A-Za-z_][A-Za-z0-9_]*)\s*$.*?"
    r"^#define\s+\1\s+([A-Za-z_][A-Za-z0-9_]*)\s*$",
    re.M | re.S,
)


def xrce_guarded_defaults(header_text):
    """macro -> its `#ifndef` default, following one macro-valued alias chain.

    An alias whose target has no guarded default of its own resolves to
    NOTHING, deliberately: "say `computed`, never invent one" is the rule the
    literal case already holds to, and an alias is not a reason to guess.
    """
    numeric = {m: int(n) for m, n in _IFNDEF_DEFAULT.findall(header_text)}
    alias = {m: t for m, t in _IFNDEF_ALIAS.findall(header_text) if m not in numeric}
    for macro, target in alias.items():
        seen = {macro}
        # Bounded walk. A cycle is a header that would not compile, but this
        # reads a file rather than trusting one, and an unbounded walk over a
        # cycle is a hang with no message.
        while target in alias and target not in seen:
            seen.add(target)
            target = alias[target]
        if target in numeric:
            numeric[macro] = numeric[target]
    return numeric


def _xrce_parse_manifest(text, where):
    """The manifest's own parser, imported — never a second one.

    `scripts/check-xrce-config-manifest.py` is the gate over this file and
    already implements the grammar its header documents. A reader with its own
    copy is how two parsers come to disagree about a record the gate accepts,
    which is the class this repo keeps paying for; so this borrows that one.
    """
    mod = _xrce_parse_manifest.__dict__.get("_mod")
    if mod is None:
        spec = importlib.util.spec_from_file_location(
            "_xrce_config_manifest", os.path.join(ROOT, "scripts", "check-xrce-config-manifest.py")
        )
        mod = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(mod)
        _xrce_parse_manifest._mod = mod
    return mod.parse_manifest(text, where)


def xrce_knobs(manifest_text, header_text, manifest_rel=XRCE_MANIFEST):
    """XRCE env knobs from the manifest → {name: (default, rel, line, conflict)}.

    A `knob` row's default is its own column; a `define` row's is the `#ifndef`
    in `internal.h`, keyed by the MACRO the row binds — so a row rewired to a
    different macro reports that macro's default, and a row naming a macro with
    no guarded default reports no figure rather than inventing one.
    """
    _values, knob_rows, _flags, define_rows = _xrce_parse_manifest(manifest_text, manifest_rel)
    guarded = xrce_guarded_defaults(header_text)

    line_of = {}
    for n, raw in enumerate(manifest_text.splitlines(), 1):
        f = raw.split("#", 1)[0].split()
        if f and f[0] in ("knob", "define"):
            line_of.setdefault((f[0], f[1] if f[0] == "define" else f[2]), n)

    out = {}
    for _template, token, env, default, _min in knob_rows:
        _xrce_record(out, env, default, manifest_rel, line_of.get(("knob", token), 0))
    for macro, env, _min in define_rows:
        _xrce_record(
            out, env, guarded.get(macro), manifest_rel, line_of.get(("define", macro), 0)
        )
    return out


def _xrce_record(out, env, default, rel, line):
    """One env name, possibly bound by several rows.

    `NROS_XRCE_TRANSPORT_MTU` legitimately drives both the UDP and the TCP MTU,
    so the same name arrives twice; agreeing rows collapse. Rows that DISAGREE
    are the issue-0135 shape one layer up — one env, two values — and get the
    same `conflicting defaults` flag every other reader does.
    """
    if env in out:
        if out[env][0] != default:
            out[env] = (out[env][0], out[env][1], out[env][2], True)
        return
    out[env] = (default, rel, line, False)


def xrce_sources():
    """(manifest text, header text). Both files are tracked; a missing one is a
    broken checkout, not a configuration, so this raises rather than reporting
    an XRCE backend with no knobs."""
    with open(os.path.join(ROOT, XRCE_MANIFEST), encoding="utf8") as fh:
        manifest = fh.read()
    with open(os.path.join(ROOT, XRCE_DEFAULTS_H), encoding="utf8") as fh:
        header = fh.read()
    return manifest, header


def scan_xrce():
    """`xrce_knobs` over the real files."""
    return xrce_knobs(*xrce_sources())


def scan(files=None, xrce=True):
    """(knobs, pools) — knobs: name -> (default, file, line); pools: list."""
    knobs, pools = {}, []
    if xrce:
        # FIRST, deliberately. Two of these names are also reached by the
        # RFC-0049 ladder's `xrce_env_key` match in `platform_config.rs`, which
        # can state no figure (`LADDER_ENV_KEY` yields a computed default), and
        # the Rust scans below only ever `setdefault`. Seeding the manifest
        # first is what makes `NROS_XRCE_CUSTOM_TRANSPORT_MTU` render as 4096
        # and `NROS_XRCE_STREAM_HISTORY` as 16 instead of as a line number in
        # a generated table.
        knobs.update(scan_xrce())
    for rel in files if files is not None else tracked_rust():
        try:
            with open(os.path.join(ROOT, rel), encoding="utf8", errors="replace") as fh:
                text = fh.read()
        except OSError:
            continue
        for pat in KNOB_PATTERNS:
            for m in pat.finditer(text):
                name = m.group(1)
                default = int(m.group(2).replace("_", ""))
                line = text[: m.start()].count("\n") + 1
                # First definition wins, but a later one that DISAGREES is a
                # real finding: two crates defaulting one knob differently is
                # how a consumer sets it and only half the tree moves.
                if name in knobs and knobs[name][0] != default:
                    knobs[name] = (knobs[name][0], knobs[name][1], knobs[name][2], True)
                    continue
                knobs.setdefault(name, (default, rel, line, False))
        # The named builtin, resolved in the file that names it. A const this
        # file cannot see leaves the knob with a computed default, which is the
        # same honest answer as before — never a guess.
        consts = {n: int(v.replace("_", "")) for n, v in CONST_USIZE.findall(text)}
        for m in KNOB_UNWRAP_OR_CONST.finditer(text):
            name, ident = m.group(1), m.group(2)
            if ident not in consts:
                continue
            default = consts[ident]
            line = text[: m.start()].count("\n") + 1
            if name in knobs and knobs[name][0] is not None and knobs[name][0] != default:
                knobs[name] = (knobs[name][0], knobs[name][1], knobs[name][2], True)
                continue
            knobs.setdefault(name, (default, rel, line, False))
        for m in KNOB_ANY.finditer(text):
            name = m.group(1)
            if name not in knobs:
                line = text[: m.start()].count("\n") + 1
                # None default = computed expression; render says so rather
                # than dropping the row.
                knobs.setdefault(name, (None, rel, line, False))
        if "_env_key" in text:
            for m in LADDER_ENV_KEY.finditer(text):
                name = m.group(1)
                if name not in knobs:
                    line = text[: m.start()].count("\n") + 1
                    knobs.setdefault(name, (None, rel, line, False))
        for m in POOL_ANNOT.finditer(text):
            expr = m.group(2).replace("\\", " ").strip()
            pools.append((m.group(1), expr, rel, text[: m.start()].count("\n") + 1))
    return knobs, pools


def pool_bytes(expr, knobs):
    """Evaluate a knob product at defaults. Returns (bytes, None) or (None, why)."""
    terms = [t.strip() for t in expr.split("*") if t.strip()]
    if not terms:
        return None, "empty expression"
    total = 1
    for t in terms:
        if t.isdigit():
            total *= int(t)
        elif t in knobs:
            if knobs[t][0] is None:
                return None, f"knob `{t}` has a computed default"
            total *= knobs[t][0]
        else:
            return None, f"unknown knob `{t}`"
    return total, None


def render(knobs, pools):
    by_pool = {}
    for name, expr, rel, line in pools:
        b, err = pool_bytes(expr, knobs)
        by_pool[name] = (expr, b, err, rel, line)

    lines = [
        "<!-- GENERATED by scripts/gen-pool-inventory.py — do not edit by hand.",
        "     NOT COMMITTED: this file is gitignored and rebuilt on demand.",
        "     Regenerate: python3 scripts/gen-pool-inventory.py",
        "     Built by:   just book, just check pool-inventory, docs.yml -->",
        "",
        "# Static pool inventory",
        "",
        "Every build-time sizing knob nano-ros reads, with the default it uses when",
        "you do not set it. Set them as environment variables at BUILD time.",
        "",
        "This page exists because knowing a knob exists is the hard part. Issue 0271",
        "audited a 256 KB image that already tuned nine of these and still carried",
        "~145 KB of defaults it did not know to change — every knob it needed was",
        "already there. Four separate features had each added a static pool with a",
        "knob, silently.",
        "",
        "## Pools with a computed size",
        "",
        "Bytes are at the DEFAULTS below, computed from the pool's own declared",
        "arithmetic (`// nros-pool:` in the source). Change a knob and the figure",
        "moves with it.",
        "",
        "| pool | bytes at default | formula | declared in |",
        "| --- | ---: | --- | --- |",
    ]
    if by_pool:
        for name in sorted(by_pool):
            expr, b, err, rel, line = by_pool[name]
            shown = f"{b:,}" if b is not None else f"— ({err})"
            lines.append(f"| `{name}` | {shown} | `{expr}` | `{rel}:{line}` |")
    else:
        lines.append("| _(none annotated yet)_ | | | |")

    lines += [
        "",
        "## Every sizing knob",
        "",
        "A knob with no pool row above is still tunable; it simply has not declared",
        "its byte cost yet. Absence of a figure is not a claim that it is free.",
        "",
        "| knob | default | read by |",
        "| --- | ---: | --- |",
    ]
    for name in sorted(knobs):
        default, rel, line, conflict = knobs[name]
        note = " **(conflicting defaults — see below)**" if conflict else ""
        shown = default if default is not None else f"computed — see `{rel}:{line}`"
        lines.append(f"| `{name}` | {shown} | `{crate_of(rel)}`{note} |")

    conflicts = [n for n, v in knobs.items() if v[3]]
    if conflicts:
        lines += [
            "",
            "## Conflicting defaults",
            "",
            "These knobs are read in more than one place with DIFFERENT defaults, so",
            "setting one moves only part of the tree — the issue-0135 split-brain",
            "shape one layer up.",
            "",
        ]
        lines += [f"* `{n}`" for n in sorted(conflicts)]
    lines.append("")
    return "\n".join(lines)


def self_test():
    knobs = {"A": (2, "f.rs", 1, False), "B": (4, "f.rs", 2, False)}
    got, err = pool_bytes("A * B * 16", knobs)
    assert got == 128 and err is None, f"product at defaults wrong: {got} {err}"
    got, err = pool_bytes("A * NOPE", knobs)
    assert got is None and "NOPE" in err, "an unknown knob must not silently vanish"
    # Every reader spelling gets a probe line. A spelling with no case here is
    # a spelling that can be added, break the scan, and still self-test green —
    # which is how `env_usize_min` deleted two knobs and two pool figures.
    probe = (
        'let x = env_usize("NROS_PROBE_SLOTS", 12);\n'
        'let y = env_usize_min("NROS_PROBE_DEPTH", 3, 1);\n'
        '// nros-pool: P = NROS_PROBE_SLOTS * 8\n'
        '// nros-pool: Q = NROS_PROBE_DEPTH * NROS_PROBE_SLOTS\n'
        'let z = knob("NROS_PROBE_LADDER", rungs.thing, 9);\n'
        'let w = knob("NROS_PROBE_PLAIN", 7);\n'
        # phase-454 W6.a's shape: the builtin is a NAMED const at the end of a
        # ladder chain. Without a case here the spelling can be added, delete a
        # knob and both payload-pool figures with it, and still self-test green
        # — which is exactly how `env_usize_min` did it.
        'const PROBE_RING_DEFAULT: usize = 5;\n'
        'let r = env_usize_min("NROS_PROBE_RING",\n'
        '    rungs.ring.or(declared(d)).unwrap_or(PROBE_RING_DEFAULT), 1);\n'
        '// nros-pool: R = NROS_PROBE_RING * NROS_PROBE_SLOTS\n'
        # The RFC-0049 ladder's front-end match, verbatim in shape — the ONLY
        # place `NROS_XRCE_STREAM_HISTORY` is named in Rust, and it can state
        # no figure. It is here to pin the PRECEDENCE below.
        'fn xrce_env_key(k: &str) -> &\'static str { match k {\n'
        '    "stream_history" => "NROS_XRCE_STREAM_HISTORY",\n'
        '    _ => "",\n} }\n'
    )
    tmp = os.path.join(ROOT, "tmp")
    os.makedirs(tmp, exist_ok=True)
    p = os.path.join(tmp, "_pool_probe.rs")
    with open(p, "w") as fh:
        fh.write(probe)
    k, pl = scan([os.path.relpath(p, ROOT)], xrce=False)
    # The same probe WITH the XRCE manifest seeded — the integration, over the
    # real `xrce-config.txt` and `internal.h`, because a scanner that parses
    # perfectly and is never called publishes exactly the table issue 1078
    # filed. And the PRECEDENCE: the ladder line above reaches
    # `NROS_XRCE_STREAM_HISTORY` with no figure, so seeding the manifest second
    # would put a line number back in the default column.
    ki, _ = scan([os.path.relpath(p, ROOT)], xrce=True)
    os.unlink(p)
    # A RELATION, not a list of numbers. Naming the five defaults here would
    # put `internal.h`'s figures in a THIRD file — the generator would then
    # disagree with the tree by construction the moment someone legitimately
    # retunes one, and the failure would tell them to edit the generator. That
    # is the second home this whole change exists to avoid, one level up in the
    # tooling. So state the property instead: every `define` row's published
    # default IS the `#ifndef` guard for the macro that row binds, and every
    # `knob` row's IS its own column. A retune moves both sides together; a
    # broken wire moves only one.
    man_text, hdr_text = xrce_sources()
    _v, knob_rows, _f, define_rows = _xrce_parse_manifest(man_text, XRCE_MANIFEST)
    guarded = xrce_guarded_defaults(hdr_text)
    assert define_rows and knob_rows, (
        f"{XRCE_MANIFEST} parsed to no `define`/`knob` rows — the relation below "
        "would hold vacuously, which is the shape `check-no-vacuous-tests` bans"
    )
    for macro, env, _min in define_rows:
        # xrce-config.txt's own documented invariant: a `define` row has no
        # default column BECAUSE `internal.h` holds it in an `#ifndef`, "and
        # that is the one statement of it". A row whose macro has no guard is
        # that statement missing, not a figure this file may supply.
        assert macro in guarded, (
            f"{XRCE_MANIFEST} binds `-D{macro}`, but {XRCE_DEFAULTS_H} has no "
            f"`#ifndef {macro}` guarding a default. The manifest states no default "
            "for a `define` on purpose; with neither file stating one, the knob "
            "reaches this table with no figure at all."
        )
        assert ki.get(env, (None,))[0] == guarded[macro], (
            f"issue 1078: `{env}` must reach the inventory at {XRCE_DEFAULTS_H}'s "
            f"`#ifndef {macro}` guard ({guarded[macro]}); got {ki.get(env)}. Either "
            f"{XRCE_MANIFEST} is not seeded into `scan`, or it is seeded AFTER the "
            "Rust scans and the ladder's computed default won."
        )
    for _t, _token, env, default, _min in knob_rows:
        if ki.get(env, (None, None, None, False))[3]:
            continue  # conflicting rows carry the table's own flag, not one figure
        assert ki.get(env, (None,))[0] == default, (
            f"issue 1078: `{env}` must reach the inventory at the `knob` row's own "
            f"default ({default}); got {ki.get(env)}"
        )
    assert k.get("NROS_PROBE_SLOTS", (None,))[0] == 12, "knob not scanned"
    assert k.get("NROS_PROBE_DEPTH", (None,))[0] == 3, (
        "env_usize_min knob not scanned — its DEFAULT is the reported figure, "
        "not its floor (issue 0827)"
    )
    assert k.get("NROS_PROBE_LADDER", (None,))[0] == 9, (
        "ladder knob not scanned — `knob(name, rung, builtin)` puts the "
        "builtin THIRD, and reading the second argument yields a rung "
        "expression, not a figure (phase-400 W6)"
    )
    assert k.get("NROS_PROBE_PLAIN", (None,))[0] == 7, (
        "the two-argument `knob(name, default)` spelling must still scan"
    )
    by_name = {name: expr for name, expr, *_ in pl}
    assert "P" in by_name, "pool annotation not scanned"
    assert pool_bytes(by_name["P"], k)[0] == 96, "annotated pool bytes wrong"
    assert pool_bytes(by_name["Q"], k)[0] == 36, (
        "a pool sized by an env_usize_min knob must resolve to bytes"
    )
    assert k.get("NROS_PROBE_RING", (None,))[0] == 5, (
        "a knob whose builtin is a NAMED const at the end of a `.unwrap_or(..)` "
        "ladder chain must still publish its figure (phase-454 W6.a) — a line "
        "number in the default column is the enumeration failure issue 0271 "
        "measured at ~145 KB"
    )
    assert pool_bytes(by_name["R"], k)[0] == 60, (
        "a pool sized by such a knob must resolve to bytes, not to `computed default`"
    )

    # --- issue 1078: the XRCE knobs, which have no Rust declaration site -----
    #
    # Synthetic manifest + header, so this tests the WIRING and not today's
    # contents: a `knob` row's default is its own column, a `define` row's is
    # the `#ifndef` guarding the MACRO IT NAMES, and one env bound twice
    # collapses only when the two agree.
    man = (
        "# comment\n"
        "value  ucdr CONFIG_MACHINE_ENDIANNESS 1\n"
        "knob   uxr  UCLIENT_UDP_TRANSPORT_MTU NROS_XRCE_TRANSPORT_MTU 4096 128\n"
        "knob   uxr  UCLIENT_TCP_TRANSPORT_MTU NROS_XRCE_TRANSPORT_MTU 4096 128\n"
        "flag   uxr  UCLIENT_PROFILE_UDP posix_ip\n"
        "define XRCE_BUFFER_SIZE  NROS_XRCE_BUFFER_SIZE  64\n"
        "define XRCE_MAX_SUBSCRIBERS NROS_XRCE_MAX_SUBSCRIBERS 0\n"
        "define XRCE_UNGUARDED    NROS_XRCE_UNGUARDED    1\n"
        "define XRCE_SUB_ALIAS    NROS_XRCE_SUB_ALIAS    1\n"
        "define XRCE_DANGLING     NROS_XRCE_DANGLING     1\n"
    )
    hdr = (
        "#ifndef XRCE_BUFFER_SIZE\n/* prose */\n#define XRCE_BUFFER_SIZE 1024\n#endif\n"
        "#ifndef XRCE_MAX_SUBSCRIBERS\n#define XRCE_MAX_SUBSCRIBERS 8\n#endif\n"
        "#define XRCE_UNGUARDED 77\n"  # not overridable ⇒ not a knob's default
        # phase-454 W6.b — a guarded default that IS another guarded macro.
        "#ifndef XRCE_SUB_ALIAS\n/* prose */\n#define XRCE_SUB_ALIAS XRCE_BUFFER_SIZE\n#endif\n"
        # …and one whose target has no guarded default. Still `computed`.
        "#ifndef XRCE_DANGLING\n#define XRCE_DANGLING XRCE_UNGUARDED\n#endif\n"
    )
    xk = xrce_knobs(man, hdr, "M")
    assert xk["NROS_XRCE_BUFFER_SIZE"][0] == 1024, (
        "a `define` row's default is internal.h's `#ifndef`, not the manifest's "
        "MINIMUM column — reading column 4 would publish 64 for a 1024-byte buffer"
    )
    assert xk["NROS_XRCE_MAX_SUBSCRIBERS"][0] == 8, "guarded default not paired"
    assert xk["NROS_XRCE_TRANSPORT_MTU"][:2] == (4096, "M"), (
        "one env bound by two agreeing rows collapses to one row: %r"
        % (xk["NROS_XRCE_TRANSPORT_MTU"],)
    )
    assert xk["NROS_XRCE_TRANSPORT_MTU"][2] == 3, "first binding row wins the citation"
    assert xk["NROS_XRCE_UNGUARDED"][0] is None, (
        "a macro with no `#ifndef` guard is not overridable, so this file has no "
        "default to publish — say `computed`, never invent one"
    )
    assert xk["NROS_XRCE_SUB_ALIAS"][0] == 1024, (
        "a guarded default that is another guarded MACRO must resolve to what the "
        "compiler computes; publishing nothing would drop the three phase-454 W6.b "
        "family knobs out of this table, and writing 1024 beside each of them would "
        "be the copy this file exists to prevent"
    )
    assert xk["NROS_XRCE_DANGLING"][0] is None, (
        "…and an alias whose target has no guarded default resolves to NOTHING. An "
        "alias is not a reason to guess"
    )
    assert "NROS_XRCE_STREAM_HISTORY" not in xk, "only rows in the manifest"
    assert "CONFIG_MACHINE_ENDIANNESS" not in xk and "UCLIENT_PROFILE_UDP" not in xk, (
        "`value` and `flag` rows are not env knobs — nobody sets them"
    )

    # THE CROSSED WIRE. Shape stays perfectly valid: same record types, same
    # columns, same macros, same envs. `NROS_XRCE_BUFFER_SIZE` is simply bound
    # to the other macro, and the published default must move with it — a check
    # that only rejects malformed rows would pass this, which is the hole every
    # gate written in this area this week had.
    crossed = man.replace(
        "define XRCE_BUFFER_SIZE  NROS_XRCE_BUFFER_SIZE  64",
        "define XRCE_MAX_SERVICE_SERVERS NROS_XRCE_BUFFER_SIZE 64",
    )
    xc = xrce_knobs(crossed, hdr + "#ifndef XRCE_MAX_SERVICE_SERVERS\n"
                    "#define XRCE_MAX_SERVICE_SERVERS 4\n#endif\n", "M")
    assert xc["NROS_XRCE_BUFFER_SIZE"][0] == 4, (
        "the default must follow the MACRO the row binds; reporting 1024 here "
        "would mean the pairing is by env name and a rewire is invisible"
    )

    # Disagreeing rows on one env are the issue-0135 shape one layer up.
    split = man.replace(
        "knob   uxr  UCLIENT_TCP_TRANSPORT_MTU NROS_XRCE_TRANSPORT_MTU 4096 128",
        "knob   uxr  UCLIENT_TCP_TRANSPORT_MTU NROS_XRCE_TRANSPORT_MTU 512 128",
    )
    assert xrce_knobs(split, hdr, "M")["NROS_XRCE_TRANSPORT_MTU"][3] is True, (
        "one env, two defaults, no flag — setting it would move half the tree"
    )

    sys.stdout.write("gen-pool-inventory self-test: OK\n")


def main():
    if "--self-test" in sys.argv:
        self_test()
        return
    self_test()
    knobs, pools = scan()
    text = render(knobs, pools)
    if "--check" in sys.argv:
        try:
            with open(OUT, encoding="utf8") as fh:
                have = fh.read()
        except OSError:
            have = None
        if have != text:
            sys.stderr.write(
                "error: the static pool inventory is stale.\n\n"
                "  Regenerate:  python3 scripts/gen-pool-inventory.py\n\n"
                "A knob nobody can enumerate is a knob nobody sets — issue 0271\n"
                "cost ~145 KB in one image to exactly that (issue 0739).\n"
            )
            sys.exit(1)
        sys.stdout.write(
            "pool-inventory OK — %d knob(s), %d annotated pool(s).\n" % (len(knobs), len(pools))
        )
        return
    with open(OUT, "w", encoding="utf8") as fh:
        fh.write(text)
    sys.stdout.write("wrote %s — %d knob(s), %d pool(s)\n" % (OUT, len(knobs), len(pools)))


if __name__ == "__main__":
    main()
