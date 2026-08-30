#!/usr/bin/env python3
"""issue 0529 — Zephyr's Kconfig defaults and its platform TOML must agree.

On Zephyr the zenoh tx knobs have TWO sources and both are load-bearing for a
different lane:

  * `zephyr/Kconfig` defaults, forwarded to the C lane by
    `zephyr/cmake/nros_rmw_zenoh.cmake` as `zephyr_compile_definitions(...)`;
  * `nros-platform-zephyr/nros-platform.toml`'s `[knobs.zenoh.tx]`, which the RFC-0049
    ladder resolves for the Rust lane.

They agree today — `y / y / 50` against `true / true / 50` — but only by
coincidence: nothing derives one from the other and nothing noticed when the
resolver could not even reach the TOML (issue 0529). Two spellings of one fact
is the drift this repo keeps paying for, so this compares them.

**Not a substitute for merging the sources.** The right end state is one
authority with the other as a rung of the ladder. Until then this makes a
divergence loud instead of silent, which is the cheap half.

Buildless: two file reads and a regex.
"""

import os
import re
import sys

try:
    import tomllib  # 3.11+
except ModuleNotFoundError:  # 3.10 backport, as the sibling gates spell it
    import tomli as tomllib

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
KCONFIG = os.path.join(ROOT, "zephyr/Kconfig")
# phase-400 W1 — the descriptor moved beside its crate. Search the same path
# the resolver does rather than hardcoding one root, so this gate keeps working
# whichever home a platform's descriptor lives in.
def _find_platform(root: str, name: str) -> str:
    for rel in ("packages/platform/nros-platform-" + name, "config/" + name):
        cand = os.path.join(root, rel, "nros-platform.toml")
        if os.path.isfile(cand):
            return cand
    return os.path.join(root, "config", name, "nros-platform.toml")


PLATFORM = _find_platform(ROOT, "zephyr")

# platform TOML key -> Kconfig symbol. Both halves of each row are read below;
# a row whose Kconfig symbol is absent is a failure, not a skip, because a
# renamed symbol is exactly the drift this exists to catch.
PAIRS = {
    "batch": "NROS_ZENOH_TX_BATCH",
    "split_lock": "NROS_ZENOH_TX_SPLIT_LOCK",
    "flush_ms": "NROS_ZENOH_TX_BATCH_FLUSH_MS",
}


def kconfig_default(text, symbol):
    """The `default` line of `config <symbol>`, or None if absent."""
    m = re.search(
        rf"^config {re.escape(symbol)}\s*$(.*?)(?=^config |\Z)",
        text,
        re.M | re.S,
    )
    if not m:
        return None
    d = re.search(r"^\s+default\s+(\S+)\s*$", m.group(1), re.M)
    return d.group(1) if d else None


def normalise(value):
    """Kconfig `y`/`n` and TOML `true`/`false` are the same fact."""
    if isinstance(value, bool):
        return "y" if value else "n"
    return str(value)


def main():
    for path in (KCONFIG, PLATFORM):
        if not os.path.exists(path):
            sys.exit(f"check-zephyr-knob-agreement: missing {path}")

    with open(KCONFIG, encoding="utf-8") as fh:
        kconfig = fh.read()
    with open(PLATFORM, "rb") as fh:
        toml_doc = tomllib.load(fh)

    tx = toml_doc.get("knobs", {}).get("zenoh", {}).get("tx")
    if not tx:
        # Deleting the table is a legitimate way to end the duplication — but it
        # has to be deliberate, so say so rather than passing silently.
        print(
            "zephyr knob agreement: OK (no [knobs.zenoh.tx] in the platform "
            "TOML — Kconfig is the sole source)"
        )
        return 0

    problems = []
    for key, symbol in PAIRS.items():
        if key not in tx:
            continue
        want = normalise(tx[key])
        got = kconfig_default(kconfig, symbol)
        if got is None:
            problems.append(
                f"the zephyr platform descriptor sets [knobs.zenoh.tx].{key} "
                f"but zephyr/Kconfig has no `config {symbol}` with a default — "
                f"renamed or removed?"
            )
        elif got != want:
            problems.append(
                f"[knobs.zenoh.tx].{key} = {tx[key]!r} (→ {want}) but "
                f"zephyr/Kconfig {symbol} defaults to {got} — the C lane takes "
                f"Kconfig and the Rust lane takes the TOML, so they would "
                f"disagree"
            )

    # A knob in the TOML with no row here is unchecked, which is how a pair
    # silently stops being compared.
    for key in tx:
        if key not in PAIRS:
            problems.append(
                f"[knobs.zenoh.tx].{key} has no Kconfig counterpart in PAIRS "
                f"({os.path.basename(__file__)}) — add one, or say why the knob "
                f"has no Zephyr Kconfig"
            )

    if problems:
        sys.stderr.write("check-zephyr-knob-agreement: FAILED\n")
        for p in problems:
            sys.stderr.write(f"  {p}\n")
        return 1

    checked = ", ".join(f"{k}={normalise(tx[k])}" for k in PAIRS if k in tx)
    print(f"zephyr knob agreement: OK ({checked})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
