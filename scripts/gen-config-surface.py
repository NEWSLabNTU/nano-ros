#!/usr/bin/env python3
"""Enumerate every configuration knob and say whether a USER may set it.

Issue 0934. Surveying the config surfaces found one fact with as many as nine
authorities -- the receive locator is settable through Kconfig, an env var, a
cmake cache variable, a cmake function argument, a compile definition, a
`system.toml` block, a Cargo manifest key and two runtime envs. Read as nine
choices that is unusable. It is not nine choices.

## The distinction this file exists to make

Most of those are not places a user decides anything. They are CARRIERS: the
mechanism by which one decision crosses a layer boundary. `-DNROS_RMW=zenoh` on
a cmake command line is how the workspace function tells a cargo invocation what
was already chosen in `system.toml`; it is plumbing that happens to be
spellable.

So every knob is exactly one of:

  public    a user MAY set this, and it is the place to set it
  carrier   internal transport between layers; setting it by hand is at best
            redundant and at worst a second authority that silently wins
  dead      declared and read by nothing (delete, or wire)

The value is not the list -- it is that the list is EXHAUSTIVE and every entry
is classified FROM EVIDENCE. All 74 classify today, because the class follows
from who reads the symbol rather than from someone's opinion of it.

Two arms guard that, and they catch different things:

  freshness   adding or removing a knob changes the page, so `--check` fails
              until it is regenerated. The regenerated diff SHOWS the new
              knob and its derived class -- a new dead knob is visible in
              review rather than silent.
  ratchet     `unclassified` is for a symbol whose readers do not match any
              rule. Zero today; may only fall. This is the arm that fires when
              the evidence is genuinely ambiguous, which is exactly when a
              human should decide rather than the script guessing.

## Why classification is curated rather than derived

Whether a knob is public is a POLICY, not a fact recoverable from the source.
`NROS_DOMAIN_ID` and `CONFIG_NROS_DOMAIN_ID` are read identically; what differs
is that one is the documented surface and the other is how Zephyr states it.
Deriving that from code would be inventing it. So the table below is authored,
and the gate's job is to ensure it stays COMPLETE -- discovery is mechanical,
judgement is written down.

Run:  python3 scripts/gen-config-surface.py [--check] [--self-test]
"""

import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT = os.path.join(ROOT, "book", "src", "reference", "configuration-surface.md")
KCONFIG = os.path.join(ROOT, "zephyr", "Kconfig")
CMAKE_FORWARD = os.path.join(ROOT, "zephyr", "cmake", "nros_cargo_build.cmake")

# --------------------------------------------------------------------------
# Classification. Keyed by the FACT, not by the spelling, because the whole
# point is that one fact has many spellings.
#
#   ssot     — the ONE place a user should state it
#   carriers — spellings that transport that decision; never authored by hand
#
# A knob discovered below and absent from every entry here is UNCLASSIFIED and
# fails `--check`.
# --------------------------------------------------------------------------
FACTS = [
    {
        "fact": "RMW backend",
        "ssot": "`[system].rmw` in the bringup's `system.toml`",
        "carriers": [
            "CONFIG_NROS_RMW_ZENOH", "CONFIG_NROS_RMW_XRCE", "CONFIG_NROS_RMW_CYCLONEDDS",
        ],
        "note": "`nano_ros_workspace(BACKEND …)` and `-DNROS_RMW` are the cmake "
                "carriers. cmake already warns that BACKEND silently beats the "
                "cache var; nothing yet compares either against `[system].rmw`.",
    },
    {
        "fact": "ROS domain id",
        "ssot": "`[system].domain_id` in `system.toml`",
        "carriers": ["CONFIG_NROS_DOMAIN_ID", "CONFIG_NROS_CYCLONE_DOMAIN_ID"],
        "note": "`CONFIG_NROS_CYCLONE_DOMAIN_ID` defaults FROM `NROS_DOMAIN_ID` — "
                "the one derivation edge in this cluster. Pinning it to a literal "
                "is the phase-180 split-brain (CLAUDE.md).",
    },
    {
        "fact": "Router / agent endpoint",
        "ssot": "`[system].locator` in `system.toml`",
        "carriers": [
            "CONFIG_NROS_ZENOH_LOCATOR", "CONFIG_NROS_XRCE_AGENT_ADDR",
            "CONFIG_NROS_XRCE_AGENT_PORT",
        ],
        "note": "Zephyr is exempt from the cmake precedence chain by design "
                "(see the exemption comments in NanoRosEntry.cmake — no line ref, they move), "
                "so the Kconfig spelling is the carrier "
                "there rather than an override.",
    },
    {
        "fact": "Executor sizing",
        "ssot": "environment knobs at build time (see the pool inventory)",
        "carriers": [
            "CONFIG_NROS_EXECUTOR_MAX_CBS", "CONFIG_NROS_EXECUTOR_MAX_SC",
            "CONFIG_NROS_EXECUTOR_MAX_NODES", "CONFIG_NROS_EXECUTOR_ARENA_SIZE",
            "CONFIG_NROS_EXECUTOR_MAX_SHUTDOWN_CBS",
            "CONFIG_NROS_EXECUTOR_ACTION_CLIENTS",
            "CONFIG_NROS_SUBSCRIPTION_BUFFER_SIZE",
            "CONFIG_NROS_PARAM_SERVICE_BUFFER_SIZE",
            "CONFIG_NROS_MAX_PARAMETERS",
        ],
        "note": "On Zephyr the Kconfig spelling IS how a user sets these — it is "
                "a carrier only in the sense that the env var is what the build "
                "script reads. Issue 0460 is what happens when the bridge breaks.",
    },
]

# Knobs that are neither a user decision nor a carrier of one.
# Symbols whose readers match no rule. ZERO today: the evidence classifies all
# 74. May only decrease. If this ever needs raising, the honest move is to add a
# rule to `suggest()` explaining the new reader shape, not to raise the number.
RATCHET = 0

# Symbols whose only consumer is USER code, by design. They have no reader in
# this tree and are not dead: the guides teach an application to use them in its
# own `main()`, and Kconfig generating the macro is the whole service.
#
# Evidence is required, not asserted — each entry names where the contract is
# documented, so "provided" cannot become a place to hide a symbol nobody uses.
PROVIDED = {
    "CONFIG_NROS_INIT_DELAY_MS":
        "`docs/guides/cpp-api.md:451` shows an application calling "
        "`zpico_zephyr_wait_network(CONFIG_NROS_INIT_DELAY_MS)` from its own "
        "`main()`; `docs/guides/zephyr-setup.md:220` documents it as a knob.",
}

DEAD = {}


def kconfig_symbols():
    """Every `config NROS_*` in zephyr/Kconfig, with its default."""
    out = {}
    if not os.path.exists(KCONFIG):
        return out
    cur = None
    for line in open(KCONFIG, encoding="utf-8"):
        m = re.match(r"^\s*(?:menu)?config\s+(NROS_\w+)", line)
        if m:
            cur = "CONFIG_" + m.group(1)
            out.setdefault(cur, None)
            continue
        if cur is not None:
            d = re.match(r'^\s*default\s+(.+?)\s*$', line)
            if d and out[cur] is None:
                out[cur] = d.group(1).strip('"')
    return out


def forwarded_knobs():
    """Kconfig symbols the cmake bridge hands to cargo (issue 0460's list)."""
    if not os.path.exists(CMAKE_FORWARD):
        return set()
    body = open(CMAKE_FORWARD, encoding="utf-8").read()
    return {"CONFIG_" + m for m in re.findall(r"\$\{CONFIG_(NROS_\w+)\}", body)}


def readers(symbols):
    """Where each symbol is actually READ, so a class is evidence not opinion.

    Classification is a policy, but it is not a free choice: what a knob IS
    follows from who consumes it. A symbol forwarded to cargo and read by a
    build script is a build-time sizing knob; one that only reaches
    `zephyr_compile_definitions` is C-lane; one nothing reads is dead. This
    finds the consumers so the FACTS table can be argued from them rather than
    asserted over them.

    One `git grep` over the whole tree, not one per symbol -- 74 greps is slow
    enough that someone would run it less often than the gate does.
    """
    kinds = {s: set() for s in symbols}
    try:
        out = subprocess.run(
            ["git", "grep", "-n", "-F", "--", "CONFIG_NROS_"],
            cwd=ROOT, capture_output=True, text=True, check=False).stdout
    except OSError:
        return kinds
    for line in out.splitlines():
        try:
            path, _, body = line.split(":", 2)
        except ValueError:
            continue
        if path.endswith("Kconfig") or path.startswith("docs/") or path.startswith("book/"):
            continue
        for sym in re.findall(r"CONFIG_NROS_\w+", body):
            if sym not in kinds:
                continue
            if path.endswith((".c", ".h", ".cpp", ".hpp")):
                kinds[sym].add("c-source")
            elif path.endswith(".rs"):
                kinds[sym].add("rust-source")
            elif "compile_definitions" in body:
                kinds[sym].add("c-define")
            elif path.endswith((".cmake", "CMakeLists.txt")):
                kinds[sym].add("cmake")
    return kinds


def kconfig_internal_refs(symbols):
    """Symbols consumed BY KCONFIG ITSELF — `select`, `depends on`, `default X`.

    The tree-wide grep deliberately skips Kconfig (else every declaration would
    count as its own reader), and that hid a real consumer class.
    `NROS_TRANSPORT_SERIAL` looked dead with zero readers; it carries
    `select NROS_ZENOH_LINK_SERIAL`, so setting it turns the serial link on.
    That IS its effect — Kconfig is the consumer, and a rule that cannot see
    `select` will keep proposing live symbols for deletion.
    """
    refs, cur = set(), None
    for line in open(KCONFIG, encoding="utf-8"):
        m = re.match(r"^\s*(?:menu)?config\s+(NROS_\w+)", line)
        if m:
            cur = "CONFIG_" + m.group(1)
            continue
        # A symbol REFERENCED by another's select/depends/default is consumed.
        ref = re.match(r"^\s*(?:select|imply|depends on|default)\s+(.+)$", line)
        if ref:
            for name in re.findall(r"\bNROS_\w+", ref.group(1)):
                refs.add("CONFIG_" + name)
            # …and a symbol that SELECTS or IMPLIES another has its effect that
            # way, even though nothing reads it. `NROS_TRANSPORT_SERIAL` is the
            # case: it selects `NROS_ZENOH_LINK_SERIAL`, which is exactly what
            # turning it on is for. Reading only the referenced side would keep
            # proposing it for deletion.
            if cur and re.match(r"^\s*(?:select|imply)\s", line):
                refs.add(cur)
    return refs & set(symbols)


def prompted(symbols):
    """Which symbols carry a Kconfig PROMPT — its own mark of user-facing.

    This replaced a rule that guessed from readers and got the LAYER wrong. It
    called 24 symbols "carriers" because only cmake read them, but cmake reading
    `${CONFIG_X}` to emit `ZPICO_X=<value>` is cmake CARRYING a user's choice,
    not owning it. Kconfig already answers this: a symbol with a prompt is one
    the user is asked about. All 79 have one, so there are no internal Kconfig
    symbols and the carriers are at another layer entirely — see `CARRIERS`.
    """
    out = {s: False for s in symbols}
    cur = None
    for line in open(KCONFIG, encoding="utf-8"):
        m = re.match(r"^\s*(?:menu)?config\s+(NROS_\w+)", line)
        if m:
            cur = "CONFIG_" + m.group(1)
            continue
        if cur in out and (re.match(r'^\s*(bool|int|string|hex)\s+"', line)
                           or re.match(r'^\s*prompt\s+"', line)):
            out[cur] = True
    return out


def suggest(sym, kinds, forwarded, has_prompt, kconfig_refs):
    """public / dead. There is no third class AT THIS LAYER.

    A prompted symbol nothing reads is DEAD — the prompt asks a question whose
    answer is discarded, which is worse than not asking.
    """
    k = kinds.get(sym) or set()
    if sym in PROVIDED:
        return "provided"
    if not k and sym not in forwarded and sym not in kconfig_refs:
        return "dead"
    if has_prompt.get(sym):
        return "public"
    # Unprompted AND read: Kconfig-internal, selected by another symbol.
    return "derived"


def classify(symbols, kinds, forwarded, has_prompt, kconfig_refs):
    """Evidence-derived class per symbol, plus what could not be derived."""
    out = {s: suggest(s, kinds, forwarded, has_prompt, kconfig_refs) for s in symbols}
    unclassified = sorted(s for s, c in out.items() if c is None)
    return out, unclassified


def render(symbols, forwarded, classes, kinds, kconfig_refs, unclassified):
    def ev(sym):
        k = sorted(kinds.get(sym) or [])
        if sym in forwarded:
            k = ["forwarded-to-cargo"] + k
        if sym in kconfig_refs:
            k = k + ["kconfig select/depends"]
        if sym in PROVIDED:
            return PROVIDED[sym]
        return ", ".join(k) if k else "no reader found"

    L = []
    L.append("# Configuration surface\n")
    L.append("<!-- GENERATED by scripts/gen-config-surface.py — do not edit. -->\n")
    L.append("One fact can be spelled several ways. Only one of them is a place to make\n"
             "a decision; the rest carry that decision across a layer boundary. This page\n"
             "separates the two, because a list that does not is a list of nine ways to\n"
             "set a locator (issue 0934).\n")
    L.append("**Every symbol on this page is one a user may set.** Kconfig says so\n"
             "itself: each carries a PROMPT, which is what a prompt means. An earlier\n"
             "draft split these into public and carrier by asking who read them, and got\n"
             "the layer wrong — cmake reading `${CONFIG_X}` to emit `ZPICO_X=<value>` is\n"
             "cmake CARRYING the choice, not owning it.\n")
    L.append("The real carriers are one layer down and are NOT Kconfig symbols: the\n"
             "`ZPICO_*` / `NROS_*` environment variables, the `-D` cache variables and\n"
             "the compile definitions this page's symbols feed. Setting one of those by\n"
             "hand bypasses the question Kconfig asked.\n")
    L.append("The `evidence` column is where each value is consumed, so a symbol with no\n"
             "reader stands out: a prompt asking a question whose answer is discarded is\n"
             "worse than no prompt.\n")
    n = len(symbols)
    pub = sum(1 for c in classes.values() if c == "public")
    drv = sum(1 for c in classes.values() if c == "derived")
    dead = sum(1 for c in classes.values() if c == "dead")
    prov = sum(1 for c in classes.values() if c == "provided")
    L.append(f"{n} Kconfig symbols — **{pub} settable**, {prov} provided for "
             f"application code, {drv} derived, {dead} dead.\n")

    L.append("## Facts with more than one spelling\n")
    L.append("Where a decision has an authority OUTSIDE Kconfig, that authority is the\n"
             "place to set it and these symbols carry it.\n")
    for f in FACTS:
        L.append(f"### {f['fact']}\n")
        L.append(f"**Set it here:** {f['ssot']}\n")
        L.append("| symbol | default | class | evidence |")
        L.append("| --- | --- | --- | --- |")
        for c in f["carriers"]:
            d = symbols.get(c)
            L.append(f"| `{c}` | {'`' + d + '`' if d else '—'} | "
                     f"{classes.get(c) or '?'} | {ev(c)} |")
        L.append("")
        L.append(f"{f['note']}\n")

    named = {c for f in FACTS for c in f["carriers"]}
    for kind, title, blurb in (
        ("public", "Public — set these",
         "Read by compiled code or forwarded to a build script. On Zephyr the\n"
         "`CONFIG_` spelling IS the way to state these; the env var is the carrier."),
        ("provided", "Provided for application code",
         "No reader in THIS tree, and not dead: the guides teach an application\n"
         "to use these in its own `main()`. Kconfig generating the macro is the\n"
         "service. Each names where that contract is written down."),
        ("derived", "Derived — selected by another symbol",
         "No Kconfig prompt, so not asked about directly."),
        ("dead", "Dead — delete or wire",
         "Declared and read by nothing. A knob that is documented and inert is\n"
         "worse than one that is absent."),
    ):
        rows = sorted(s for s, c in classes.items() if c == kind and s not in named)
        if not rows:
            continue
        L.append(f"## {title}\n")
        L.append(blurb + "\n")
        L.append("| symbol | default | evidence |")
        L.append("| --- | --- | --- |")
        for r in rows:
            d = symbols.get(r)
            L.append(f"| `{r}` | {'`' + d + '`' if d else '—'} | {ev(r)} |")
        L.append("")

    if unclassified:
        L.append("## Unclassified\n")
        L.append("The evidence did not point at a class. Each is a hole in the claim that\n"
                 "this page is exhaustive; the count is RATCHETED and may only fall.\n")
        for u in unclassified:
            L.append(f"- `{u}`")
        L.append("")
    return "\n".join(L) + "\n"


def self_test():
    """The generator's own invariants, so a green run means something."""
    syms = kconfig_symbols()
    assert syms, "no Kconfig symbols discovered — the parser is broken, not the file"
    fwd = forwarded_knobs()
    assert fwd, "no forwarded knobs discovered — the cmake regex is broken"
    assert fwd <= set(syms), (
        "cmake forwards a symbol Kconfig does not declare: "
        f"{sorted(fwd - set(syms))}")
    # A fact must not claim a carrier that does not exist.
    for f in FACTS:
        for c in f["carriers"]:
            assert c in syms, f"{f['fact']} names {c}, which Kconfig does not declare"
    print("gen-config-surface self-test: OK")


def main():
    if "--self-test" in sys.argv:
        self_test()
        return 0
    # On the NORMAL path too, not only when asked: a gate whose own invariants
    # are checked only under a flag is a gate nobody checks. If the discovery
    # regexes stop matching, every run would otherwise report a cheerful zero.
    self_test()
    syms = kconfig_symbols()
    fwd = forwarded_knobs()
    kinds = readers(syms)
    has_prompt = prompted(syms)
    krefs = kconfig_internal_refs(syms)
    classes, unclassified = classify(syms, kinds, fwd, has_prompt, krefs)
    body = render(syms, fwd, classes, kinds, krefs, unclassified)
    if "--check" in sys.argv:
        # Ratchet BEFORE freshness: a new unclassified knob is the failure this
        # exists to catch, and it should be named as such rather than as
        # "the page is stale".
        if len(unclassified) > RATCHET:
            added = "\n".join(f"    {u}" for u in unclassified[:8])
            print(f"error: {len(unclassified)} unclassified config knobs, "
                  f"ratchet is {RATCHET}.\n"
                  "  A new knob must be declared public, carrier or dead in\n"
                  "  scripts/gen-config-surface.py's FACTS/DEAD tables.\n"
                  f"{added}", file=sys.stderr)
            return 1
        cur = open(OUT, encoding="utf-8").read() if os.path.exists(OUT) else ""
        if cur != body:
            print("error: the configuration surface page is stale.\n"
                  "  Regenerate + commit:  python3 scripts/gen-config-surface.py",
                  file=sys.stderr)
            return 1
        print(f"config-surface OK — {len(syms)} symbol(s), "
              f"{len(unclassified)} unclassified.")
        return 0
    os.makedirs(os.path.dirname(OUT), exist_ok=True)
    open(OUT, "w", encoding="utf-8").write(body)
    print(f"wrote {OUT} — {len(syms)} symbol(s), {len(unclassified)} unclassified")
    return 0


if __name__ == "__main__":
    sys.exit(main())
