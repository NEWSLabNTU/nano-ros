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
is classified. 55 of 74 are unclassified today, so a gate that demanded zero
would be switched off on day one (the failure mode CLAUDE.md records for
`api-parity --check`). Instead the count RATCHETS: it may fall, never rise. A
new knob must be classified, and the backlog can only shrink. That is the
property the survey found missing -- nothing anywhere records which spellings
are decisions.

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
# Unclassified knobs allowed today. May only DECREASE -- lower it whenever the
# backlog shrinks. Set from the first run rather than chosen, so it records
# reality rather than an aspiration.
RATCHET = 55

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


def classify(symbols):
    known, rows = set(), []
    for f in FACTS:
        known |= set(f["carriers"])
    known |= set(DEAD)
    unclassified = sorted(s for s in symbols if s not in known)
    return unclassified, rows


def render(symbols, forwarded, unclassified):
    L = []
    L.append("# Configuration surface\n")
    L.append("<!-- GENERATED by scripts/gen-config-surface.py — do not edit. -->\n")
    L.append("One fact can be spelled several ways. Only one of them is a place to\n"
             "make a decision; the rest carry that decision across a layer boundary.\n"
             "This page separates the two, because a list that does not is a list of\n"
             "nine ways to set a locator (issue 0934).\n")
    L.append(f"{len(symbols)} Kconfig symbols; {len(forwarded)} are forwarded to the\n"
             "Rust build by `zephyr/cmake/nros_cargo_build.cmake`.\n")

    for f in FACTS:
        L.append(f"## {f['fact']}\n")
        L.append(f"**Set it here:** {f['ssot']}\n")
        L.append("| carrier | default | forwarded to cargo |")
        L.append("| --- | --- | --- |")
        for c in f["carriers"]:
            d = symbols.get(c)
            L.append(f"| `{c}` | {'`' + d + '`' if d else '—'} | "
                     f"{'yes' if c in forwarded else 'no'} |")
        L.append("")
        L.append(f"{f['note']}\n")

    if DEAD:
        L.append("## Dead\n")
        L.append("Declared, and read by nothing. Delete or wire — a knob that is\n"
                 "documented and inert is worse than one that is absent.\n")
        L.append("| symbol | why |")
        L.append("| --- | --- |")
        for k, why in sorted(DEAD.items()):
            L.append(f"| `{k}` | {why} |")
        L.append("")

    if unclassified:
        L.append("## Unclassified\n")
        L.append("Discovered, not yet declared public, carrier or dead. Each is a hole\n"
                 "in the claim that this page is exhaustive. The count is RATCHETED by\n"
                 "`--check`: it may fall, never rise.\n")
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
    unclassified, _ = classify(syms)
    body = render(syms, fwd, unclassified)
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
