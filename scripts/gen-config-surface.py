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
                "(NanoRosEntry.cmake:485), so the Kconfig spelling is the carrier "
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

DEAD = {
    "CONFIG_NROS_TRANSPORT_SERIAL": "zero references anywhere in the tree",
    "CONFIG_NROS_INIT_DELAY_MS": "zero source readers; two guides still document it as live",
}


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


def suggest(sym, kinds, forwarded):
    """The class the evidence points at. `None` where evidence is absent."""
    k = kinds.get(sym) or set()
    if not k and sym not in forwarded:
        return "dead"
    if sym in forwarded:
        # Reaches a build script through the 0460 bridge. On Zephyr this IS how
        # a user states a build-time knob -- the env var is the carrier, not
        # this. An earlier draft had it backwards and called these carriers,
        # which contradicted the FACTS note two screens up saying the opposite.
        return "public"
    if "c-source" in k or "rust-source" in k:
        # Consumed by compiled code: the value the user picked reaches a
        # decision, so this is where it is stated.
        return "public"
    if k <= {"cmake", "c-define"}:
        # Only ever turned into a compile definition or read by cmake to decide
        # what to build: that is transport, not a decision.
        return "carrier"
    return None


def classify(symbols, kinds, forwarded):
    """Evidence-derived class per symbol, plus what could not be derived."""
    out = {s: suggest(s, kinds, forwarded) for s in symbols}
    unclassified = sorted(s for s, c in out.items() if c is None)
    return out, unclassified


def render(symbols, forwarded, classes, kinds, unclassified):
    def ev(sym):
        k = sorted(kinds.get(sym) or [])
        if sym in forwarded:
            k = ["forwarded-to-cargo"] + k
        return ", ".join(k) if k else "no reader found"

    L = []
    L.append("# Configuration surface\n")
    L.append("<!-- GENERATED by scripts/gen-config-surface.py — do not edit. -->\n")
    L.append("One fact can be spelled several ways. Only one of them is a place to make\n"
             "a decision; the rest carry that decision across a layer boundary. This page\n"
             "separates the two, because a list that does not is a list of nine ways to\n"
             "set a locator (issue 0934).\n")
    L.append("**The class is derived from WHO READS the symbol**, not asserted: a symbol\n"
             "forwarded to a build script or consumed by compiled code is where a value is\n"
             "stated; one that only becomes a compile definition is transport; one nothing\n"
             "reads is dead. The `evidence` column is that derivation, so a wrong class is\n"
             "a visible disagreement rather than an opinion.\n")
    n = len(symbols)
    pub = sum(1 for c in classes.values() if c == "public")
    car = sum(1 for c in classes.values() if c == "carrier")
    dead = sum(1 for c in classes.values() if c == "dead")
    L.append(f"{n} Kconfig symbols — **{pub} public**, {car} carrier, {dead} dead.\n")

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
        ("carrier", "Carriers — do not set by hand",
         "Only ever read by cmake or turned into a compile definition. Setting one\n"
         "directly is at best redundant and at worst a second authority."),
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
    classes, unclassified = classify(syms, kinds, fwd)
    body = render(syms, fwd, classes, kinds, unclassified)
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
