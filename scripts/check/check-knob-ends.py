#!/usr/bin/env python3
"""phase-412 — a configuration knob has TWO ends, and a knob missing either is a
lie that reads as a working knob.

  producer   something SETS it: Kconfig forwarded by cmake, a ladder rung, the
             entity inventory / a CLI emitter, a fixture row or a leaf `[env]`,
             or a row in the user-facing env reference.
  consumer   something READS it: a build script, a runtime `env::var`, a C
             `#if`/macro use, a cmake `$ENV{}`/`${}`, a shell `$X`.

Two rules, one per missing end.

RULE 1 — nothing may be DOCUMENTED or SET that nothing reads. The env references
(`book/`, `docs/reference/`, `docs/guides/embedded-tuning.md`, `.env.example`,
the API crates' config pages) and the build inputs (`examples/fixtures.toml`
`env = {}` rows, tracked `.cargo/config.toml` `[env]` keys) are CLAIMS; each
claimed `NROS_*`/`ZPICO_*` name must be read by a real read IDIOM somewhere.
Issue 1181 is the case this was written for: `ZPICO_SUBSCRIBER_BUFFER_SIZE`
was renamed at its reader in phase-403 and stayed in four documents, `.env.example`
and a fixture row — the `large_msg` listener was built with a knob that did
nothing, while its test's doc comment said it relied on it.

A MENTION is not a read. A test that names the variable in a string, a comment
that explains it, a gate that greps for it — none of them consume it, and the
first version of this method counted them, which is why 1181's name looked live.

RULE 2 — nothing may be READ as a knob that nothing can set. Scope is
deliberately the two places a missing producer is a defect rather than a
convention:

  * a build-script env name the config census classifies as a KNOB (`sizing`
    or `derived`; `infra` names are paths/flags an operator sets by hand), and
  * a Zephyr-module C `#ifndef NAME / #define NAME <default>` — on Zephyr the
    producer of a C define is Kconfig, and a header-level `-D` hook no app
    passes is exactly how `NROS_ZEPHYR_MAX_TIERS` and
    `NROS_ZEPHYR_TIER_STACK_SIZE` came to be named by diagnostics nobody could
    act on.

Header `-D` hooks elsewhere (`NROS_COMPONENT_MAX_TIMERS`, the uORB pools, ...)
are OUT of scope on purpose: their producer is the consumer's own compile line,
by documented design, and several are ruled individually by
`check-c-array-pool-floors` (issue 1131).

Buildless: `git ls-files` and regexes. Runs its selftest on every invocation.

Run:  python3 scripts/check/check-knob-ends.py [--list]
"""

from __future__ import annotations

import importlib.util
import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

NAME = r"(?:NROS|ZPICO)_[A-Z0-9_]*[A-Z0-9]"

# Where a claim is made. Only the CURRENT user-facing references: design RFCs,
# phase docs and issues describe history and are not a promise that a knob works.
CLAIM_DOCS = (
    "book/src/reference/environment-variables.md",
    "docs/reference/environment-variables.md",
    "docs/guides/embedded-tuning.md",
    ".env.example",
)
CLAIM_DOC_GLOBS = (
    re.compile(r"^packages/api/nros-c/docs/[^/]+\.md$"),
    re.compile(r"^packages/api/nros-cpp/docs/[^/]+\.md$"),
    re.compile(r"^packages/api/nros/src/guide/[^/]+\.rs$"),
)

# A claimed name that is legitimately read by nothing. Each needs a reason, and
# the list may only shrink: an entry nothing claims any more is a failure.
EXEMPT_CONSUMERLESS: dict[str, str] = {
    "NROS_MAX_CONCURRENT_GOALS":
        "its env-reference row says 'compile-time constant, not env-var "
        "configurable' — the row documents a LIMIT (nros-c `constants.rs`), "
        "and claims no knob",
}

# A knob legitimately set by nothing in the tree. Same rules.
EXEMPT_PRODUCERLESS: dict[str, str] = {}

# Zephyr-only C: a `#ifndef` default here is a knob whose producer must be
# Kconfig (zephyr/CMakeLists.txt or zephyr/cmake/*.cmake).
ZEPHYR_C = re.compile(
    r"^(zephyr/|packages/platform/nros-platform-zephyr/|packages/boards/nros-board-zephyr/)"
    r".*\.(c|h)$"
)
ZEPHYR_FORWARDERS = re.compile(r"^zephyr/(CMakeLists\.txt|cmake/[^/]+\.cmake)$")

# The XRCE backend's configuration, whose `knob` and `define` rows ARE the read
# sites for its env knobs — neither lane spells a name. See
# `xrce_manifest_readers`.
XRCE_CONFIG_MANIFEST = "packages/rmw/xrce/xrce-config.txt"

TEXT_EXT = (
    ".rs", ".c", ".h", ".cpp", ".hpp", ".cc", ".in", ".cmake", ".txt", ".sh",
    ".just", ".py", ".toml", ".md", ".conf", ".yml", ".yaml", ".example",
)
SKIP = re.compile(
    r"(^|/)third-party/|/generated/|^docs/issues/|^docs/roadmap/|"
    r"^scripts/check|/check-[^/]*$|^\.config/"
)


def is_claim_doc(path: str) -> bool:
    return path in CLAIM_DOCS or any(g.match(path) for g in CLAIM_DOC_GLOBS)


def is_doc(path: str) -> bool:
    return path.endswith(".md") or path.startswith(("docs/", "book/")) or is_claim_doc(path)


# ---------------------------------------------------------------------------
# Claims
# ---------------------------------------------------------------------------
def claims_in(path: str, text: str) -> set[str]:
    """Names a user-facing document or a build input says can be SET."""
    out: set[str] = set()
    if is_claim_doc(path):
        # A table row whose FIRST cell is the name, or an assignment line.
        out |= set(re.findall(r"(?m)^\s*(?://!\s*)?\|\s*`(" + NAME + r")`\s*\|", text))
        out |= set(re.findall(r"(?<![A-Za-z0-9_$])(" + NAME + r")=[^\s=]", text))
    elif path.endswith("fixtures.toml"):
        for body in re.findall(r"\benv\s*=\s*\{([^}]*)\}", text):
            out |= set(re.findall(r"\b(" + NAME + r")\s*=", body))
    elif path.endswith(".cargo/config.toml"):
        m = re.search(r"(?ms)^\[env\]\s*$(.*?)(?=^\[|\Z)", text)
        if m:
            out |= set(re.findall(r"(?m)^\s*(" + NAME + r")\s*=", m.group(1)))
    return out


# ---------------------------------------------------------------------------
# Readers — IDIOMS, never mentions
# ---------------------------------------------------------------------------
def _strip_comments(path: str, text: str) -> str:
    if path.endswith((".c", ".h", ".cpp", ".hpp", ".cc", ".in", ".rs")):
        text = re.sub(r"/\*.*?\*/", "", text, flags=re.S)
        text = re.sub(r"(?m)//.*$", "", text)
    else:
        text = re.sub(r"(?m)^\s*#(?!\s*(?:if|ifdef|ifndef|elif|define|undef)).*$", "", text)
    return text


def xrce_manifest_readers(text: str) -> set[str]:
    """The env names `packages/rmw/xrce/xrce-config.txt` binds — phase-454 W6.b.

    A READ TABLE, exactly like the `(env, "CONFIG_*")` pair tables the Rust arm
    below already credits: both XRCE lanes walk these rows and resolve each
    row's `<env>` column through the knob ladder, so a name here is consumed by
    two build lanes even though neither spells it.

    Before this arm the XRCE knobs were credited by ACCIDENT — the reader the
    gate found for `NROS_XRCE_BUFFER_SIZE` was its own name inside an error
    STRING in `session.c`, which is precisely the mention-is-not-a-read shape
    this file's header says the first version of the method got wrong. Editing
    that diagnostic's wording was enough to make a live knob read as dead.

    The grammar is not re-implemented here: `check-xrce-config-manifest.py` is
    the gate over that file and already owns the parser, and a second copy is
    how two readers come to disagree about a record the gate accepts.
    """
    mod = xrce_manifest_readers.__dict__.get("_mod")
    if mod is None:
        spec = importlib.util.spec_from_file_location(
            "_knob_ends_xrce_manifest",
            os.path.join(ROOT, "scripts", "check-xrce-config-manifest.py"),
        )
        mod = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(mod)
        xrce_manifest_readers._mod = mod
    _values, knobs, _flags, defines = mod.parse_manifest(text, XRCE_CONFIG_MANIFEST)
    return {env for _t, _token, env, _d, _m in knobs} | {env for _m, env, _min in defines}


def readers_in(path: str, text: str, read_callees: set[str]) -> set[str]:
    """Names read from the environment, or consumed as a C macro, in one file."""
    if is_doc(path):
        return set()
    if path == XRCE_CONFIG_MANIFEST:
        return xrce_manifest_readers(text)
    t = _strip_comments(path, text)
    out: set[str] = set()
    if path.endswith(".rs"):
        # Every name literal in a read call's argument list, not just the
        # first: `env_usize_compat("NROS_X", "ZPICO_X", d)` reads a legacy
        # alias as its SECOND argument.
        for m in re.finditer(r"([A-Za-z_][A-Za-z0-9_:]*!?)\s*\(", t):
            callee = m.group(1).rsplit("::", 1)[-1]
            if callee.rstrip("!") not in read_callees and callee not in ("env!", "option_env!"):
                continue
            args = t[m.end():m.end() + 400].split(";", 1)[0]
            out |= set(re.findall(r"^\s*&?\"(" + NAME + r")\"", args))
            out |= set(re.findall(r",\s*&?\"(" + NAME + r")\"", args))
        # A name held in a `&str` const and read through it
        # (`const MULTICAST_TRANSPORT_ENV: &str = "ZPICO_..."; env::var(MULTICAST_TRANSPORT_ENV)`).
        for const, n in re.findall(r"const\s+([A-Z_][A-Z0-9_]*)\s*:\s*&(?:'static\s+)?str\s*=\s*\"(" + NAME + r")\"", t):
            if re.search(r"\b(?:var|var_os)\s*\(\s*(?:[a-z_:]*::)?" + const + r"\b", t):
                out.add(n)
        # A (env, CONFIG_*) pair table is a READ table: `kconfig_fallback`
        # looks the env name up in it (nros-zpico-build's KCONFIG_KNOBS).
        out |= set(re.findall(r"\(\s*\"(" + NAME + r")\"\s*,\s*\"CONFIG_", t))
    elif path.endswith((".c", ".h", ".cpp", ".hpp", ".cc", ".in")):
        for line in t.splitlines():
            if re.match(r"\s*#\s*define\s", line):
                continue
            out |= set(re.findall(r"\b(" + NAME + r")\b", line))
    elif path.endswith((".cmake", "CMakeLists.txt")):
        out |= set(re.findall(r"\$ENV\{(" + NAME + r")\}", t))
        out |= set(re.findall(r"\$\{(" + NAME + r")\}", t))
        out |= set(re.findall(r"DEFINED\s+(?:ENV\{)?(" + NAME + r")\b", t))
        out |= set(re.findall(r"\bif\s*\(\s*(?:NOT\s+)?(" + NAME + r")\b", t))
    elif path.endswith((".sh", ".just", ".py")) or path == "justfile":
        out |= set(re.findall(r"\$\{?(" + NAME + r")\b", t))
        out |= set(re.findall(r"(?:environ(?:\.get)?\s*[\(\[]|getenv\s*\()\s*[\"'](" + NAME + r")[\"']", t))
    return out


# ---------------------------------------------------------------------------
# Producers
# ---------------------------------------------------------------------------
def producers_in(path: str, text: str) -> set[str]:
    """Names a file SETS, forwards or emits. Claims count too (rule 2 accepts a
    documented operator input as the producer of an env knob)."""
    out = set(claims_in(path, text))
    if is_doc(path):
        return out
    if os.path.basename(path).startswith("Kconfig"):
        # A Kconfig symbol is the producer of the same-named env knob; whether
        # its value actually CROSSES to the Rust lane is issue 0460's question,
        # held by `check-kconfig-knob-forwarding`, not re-asked here.
        return set(re.findall(r"(?m)^\s*(?:menu)?config\s+(NROS_[A-Z0-9_]*[A-Z0-9])\s*$", text))
    t = _strip_comments(path, text)
    if path.endswith((".cmake", "CMakeLists.txt")):
        # forwarded from Kconfig, set into the env, or emitted as NAME=value
        out |= set(re.findall(r"\b(" + NAME + r")=\$\{", t))
        out |= set(re.findall(r"set\(ENV\{(" + NAME + r")\}", t))
        out |= set(re.findall(r"_nros_resolve_(?:derivable_)?knob\(\s*(" + NAME + r")\b", t))
        out |= set(re.findall(r"\"(" + NAME + r")=", t))
    elif path.endswith(".rs"):
        out |= set(re.findall(r"(?:set_var|\.env|insert)\(\s*\"(" + NAME + r")\"", t))
        out |= set(re.findall(r"\(\s*\"(" + NAME + r")\"\s*,\s*\"CONFIG_", t))
        if path.startswith("packages/cli/"):
            # the CLI's emitters name the keys they write as literals
            out |= set(re.findall(r"\"(" + NAME + r")\"", t))
    elif path.endswith((".sh", ".just")) or path == "justfile":
        out |= set(re.findall(r"(?:^|\s|export\s+)(" + NAME + r")=", t, re.M))
    return out


def zephyr_c_defaults(path: str, text: str) -> set[str]:
    if not ZEPHYR_C.match(path):
        return set()
    return set(re.findall(
        r"(?m)^\s*#\s*ifndef\s+(" + NAME + r")\s*\n\s*#\s*define\s+\1[ \t]+\S", text))


def zephyr_forwarded(path: str, text: str) -> set[str]:
    if not ZEPHYR_FORWARDERS.match(path):
        return set()
    return set(re.findall(r"\b(" + NAME + r")=\$\{CONFIG_", _strip_comments(path, text)))


# ---------------------------------------------------------------------------
def evaluate(texts: dict[str, str], read_callees: set[str], extra_readers: set[str],
             ladder: set[str], build_knobs: set[str]):
    claims: dict[str, set[str]] = {}
    readers = set(extra_readers) | set(ladder)
    producers = set(ladder)
    zdefaults: dict[str, str] = {}
    zfwd: set[str] = set()
    for path, text in texts.items():
        for n in claims_in(path, text):
            claims.setdefault(n, set()).add(path)
        readers |= readers_in(path, text, read_callees)
        producers |= producers_in(path, text)
        for n in zephyr_c_defaults(path, text):
            zdefaults[n] = path
        zfwd |= zephyr_forwarded(path, text)

    consumerless = {n: sorted(fs) for n, fs in claims.items()
                    if n not in readers and n not in EXEMPT_CONSUMERLESS}
    producerless = {n: "build-script knob (census sizing/derived)" for n in build_knobs
                    if n not in producers and n not in EXEMPT_PRODUCERLESS}
    for n, f in zdefaults.items():
        if n not in zfwd and n not in EXEMPT_PRODUCERLESS:
            producerless[n] = f"Zephyr C default in {f}, no Kconfig forward in zephyr/"
    stale = sorted(
        [n for n in EXEMPT_CONSUMERLESS if n not in claims]
        + [n for n in EXEMPT_PRODUCERLESS if n not in build_knobs and n not in zdefaults])
    return consumerless, producerless, stale


def self_test() -> None:
    """Both rules must FIRE on a broken input and stay quiet on a fixed one. A
    probe that can only report "nothing" reads exactly like a clean tree."""
    callees = {"env_usize", "var"}
    broken = {
        # rule 1: a documented name nothing reads, and a fixture row setting it
        "book/src/reference/environment-variables.md":
            "| `ZPICO_DEAD_SIZE` | x | `1` |\n| `NROS_LIVE_SIZE` | y | `2` |\n",
        "examples/fixtures.toml": 'env = { ZPICO_DEAD_SIZE = "8192" }\n',
        # a MENTION in a test string and a comment must not count as a read
        "packages/testing/t.rs": 'let s = "ZPICO_DEAD_SIZE=8192"; // ZPICO_DEAD_SIZE\n',
        "packages/x/build.rs": 'let n = env_usize("NROS_LIVE_SIZE", 2);\n'
                               'let k = env_usize("NROS_ORPHAN_KNOB", 4);\n',
        # rule 2: a Zephyr C default with no Kconfig forward
        "zephyr/shim.c": "#ifndef NROS_ZEPHYR_POOL\n#define NROS_ZEPHYR_POOL 4\n#endif\n"
                         "int a[NROS_ZEPHYR_POOL];\n",
        "zephyr/CMakeLists.txt": "zephyr_library_sources(shim.c)\n",
    }
    c, p, _ = evaluate(broken, callees, set(), set(), {"NROS_ORPHAN_KNOB"})
    assert set(c) == {"ZPICO_DEAD_SIZE"}, f"selftest: rule 1 found {sorted(c)}"
    assert set(p) == {"NROS_ORPHAN_KNOB", "NROS_ZEPHYR_POOL"}, f"selftest: rule 2 found {sorted(p)}"

    fixed = dict(broken)
    fixed["packages/y/build.rs"] = 'let d = env_usize("ZPICO_DEAD_SIZE", 1);\n'
    fixed["zephyr/CMakeLists.txt"] += (
        "zephyr_compile_definitions(NROS_ZEPHYR_POOL=${CONFIG_NROS_ZEPHYR_POOL})\n")
    fixed["just/x.just"] = "NROS_ORPHAN_KNOB=8 cargo build\n"
    c, p, _ = evaluate(fixed, callees, set(), set(), {"NROS_ORPHAN_KNOB"})
    assert not c, f"selftest: rule 1 false positive {sorted(c)}"
    assert not p, f"selftest: rule 2 false positive {sorted(p)}"

    # the two indirect read shapes: a legacy alias as a later argument, and a
    # name held in a `&str` const
    src = ('let n = env_usize_compat(\n    "NROS_NEW",\n    "ZPICO_OLD",\n    1,\n);\n'
           'pub const E: &str = "ZPICO_VIA_CONST";\nfn f() { std::env::var(E); }\n')
    got = readers_in("packages/z/build.rs", src, {"env_usize_compat"})
    assert got == {"NROS_NEW", "ZPICO_OLD", "ZPICO_VIA_CONST"}, f"selftest: indirect reads {sorted(got)}"
    # a Kconfig symbol produces its env knob
    assert producers_in("zephyr/Kconfig", "config NROS_FOO\n    int \"x\"\n") == {"NROS_FOO"}

    # a `.cargo/config.toml` [env] key is a claim; a [patch] key is not
    cfg = '[env]\nNROS_A = "1"\n\n[patch.crates-io]\nNROS_B = { path = "x" }\n'
    assert claims_in("ex/.cargo/config.toml", cfg) == {"NROS_A"}, "selftest: [env] scoping"

    # the XRCE manifest is a READ TABLE (phase-454 W6.b). Both record types
    # bind an env name; `value` and `flag` rows bind none, and a name in a
    # COMMENT is a mention rather than a read, exactly as everywhere else here.
    man = (
        "# NROS_XRCE_MENTIONED_ONLY in prose\n"
        "value  ucdr CONFIG_MACHINE_ENDIANNESS 1\n"
        "knob   uxr  UCLIENT_UDP_TRANSPORT_MTU NROS_XRCE_TRANSPORT_MTU 4096 128\n"
        "flag   uxr  UCLIENT_PROFILE_UDP posix_ip\n"
        "define XRCE_BUFFER_SIZE NROS_XRCE_BUFFER_SIZE 64\n"
    )
    got = readers_in(XRCE_CONFIG_MANIFEST, man, callees)
    assert got == {"NROS_XRCE_TRANSPORT_MTU", "NROS_XRCE_BUFFER_SIZE"}, (
        f"selftest: xrce manifest reads {sorted(got)}"
    )


def load_census():
    spec = importlib.util.spec_from_file_location(
        "config_knob_census", os.path.join(ROOT, "scripts/check/config-knob-census.py"))
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def tracked_texts() -> dict[str, str]:
    files = subprocess.check_output(["git", "-C", ROOT, "ls-files"], text=True).split()
    out = {}
    for f in files:
        if SKIP.search(f):
            continue
        base = os.path.basename(f)
        if not (f.endswith(TEXT_EXT) or base in ("justfile", "CMakeLists.txt")
                or base.startswith("Kconfig")):
            continue
        try:
            with open(os.path.join(ROOT, f), encoding="utf-8", errors="replace") as fh:
                out[f] = fh.read()
        except OSError:
            pass
    return out


def main() -> int:
    self_test()
    census = load_census()
    census_readers = census.build_script_env("NROS") | census.build_script_env("ZPICO")
    build_knobs = {n for n in census_readers
                   if census.KNOB_CLASS.get(n, ("",))[0] in ("sizing", "derived")}
    consumerless, producerless, stale = evaluate(
        tracked_texts(), set(census.READ_CALLEES), census_readers,
        census.ladder_env_keys(), build_knobs)

    if "--list" in sys.argv:
        print(f"{len(build_knobs)} build-script knobs checked for a producer")

    errs = 0
    if consumerless:
        errs += 1
        print("check-knob-ends: FAILED — documented or set, read by NOTHING:", file=sys.stderr)
        for n, fs in sorted(consumerless.items()):
            print(f"    {n:36}  claimed in {', '.join(fs[:3])}", file=sys.stderr)
        print("  Rename it to the name the reader uses, delete the claim, or give it a\n"
              "  reader. A user who sets it gets the default and no diagnostic (1181).",
              file=sys.stderr)
    if producerless:
        errs += 1
        print("check-knob-ends: FAILED — read as a knob, set by NOTHING:", file=sys.stderr)
        for n, why in sorted(producerless.items()):
            print(f"    {n:36}  {why}", file=sys.stderr)
        print("  Wire a producer (Kconfig forward, ladder rung, env reference row) or\n"
              "  delete the knob. A knob nothing sets is indistinguishable from one\n"
              "  that is honoured.", file=sys.stderr)
    if stale:
        errs += 1
        print("check-knob-ends: FAILED — exemptions for names no longer in scope "
              f"(delete them): {', '.join(stale)}", file=sys.stderr)
    if errs:
        return 1
    print("check-knob-ends OK — every claimed knob has a reader and every "
          "build-script / Zephyr-C knob has a producer.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
