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


def scan(files=None):
    """(knobs, pools) — knobs: name -> (default, file, line); pools: list."""
    knobs, pools = {}, []
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
        "     Regenerate: python3 scripts/gen-pool-inventory.py",
        "     Gated by:   just check pool-inventory -->",
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
    )
    tmp = os.path.join(ROOT, "tmp")
    os.makedirs(tmp, exist_ok=True)
    p = os.path.join(tmp, "_pool_probe.rs")
    with open(p, "w") as fh:
        fh.write(probe)
    k, pl = scan([os.path.relpath(p, ROOT)])
    os.unlink(p)
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
                "  Regenerate + commit:  python3 scripts/gen-pool-inventory.py\n\n"
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
