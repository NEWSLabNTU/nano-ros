#!/usr/bin/env python3
"""issue 0529 / phase-405 W6 — zephyr/Kconfig's tx defaults are DERIVED from the
zephyr platform descriptor, and this renders them and gates them.

On Zephyr the zenoh tx knobs feed two lanes:

  * `zephyr/Kconfig` defaults, forwarded to the C lane by
    `zephyr/cmake/nros_rmw_zenoh.cmake` as `zephyr_compile_definitions(...)`;
  * `nros-platform-zephyr/nros-platform.toml`'s `[knobs.zenoh.tx]`, which the
    RFC-0049 ladder resolves for the Rust lane.

THE DIRECTION IS DECLARED: the TOML is the authority, the Kconfig `default`
lines are its mirror. That was always the intent — the descriptor has said so
since phase-290 — but this script previously described the pair as two
independent sources awaiting a merge, and the two statements disagreed in
writing for two phases. `--write` renders the Kconfig side from the TOML, so
the mirror is now GENERATED rather than asserted, which is the difference
between a stopgap and an end state (the `check-abi-bindings` shape).

Kconfig stays HAND-WIRED otherwise, per RFC-0049 — only the three `default`
lines are derived, in place, leaving prompts and help text alone.

phase-405 W6 also folded in `nros-tests/tests/kconfig_platform_default_drift.rs`
(phase-290 W3.b), which compared the same three pairs across the same two files
in 80 lines of Rust on a later lane. Two checkers for one pair is the drift this
file exists to prevent, one level up; the surviving one is here because it runs
on `check fast` (the pre-push hook) and because the generator belongs beside the
gate that enforces it.

Buildless: two file reads and a regex.

Usage::

    check-zephyr-knob-agreement.py           # the gate
    check-zephyr-knob-agreement.py --write   # render Kconfig's defaults from the TOML
"""

import argparse
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


def rewrite_default(text, symbol, value):
    """`text` with `config <symbol>`'s `default` line set to `value`.

    Surgical on purpose: Kconfig is hand-wired (RFC-0049), so only the one line
    moves. Returns the text unchanged if the symbol has no `default`, which the
    caller reports rather than silently inventing one.
    """
    m = re.search(rf"^config {re.escape(symbol)}\s*$(.*?)(?=^config |\Z)", text, re.M | re.S)
    if not m:
        return text
    body = m.group(1)
    fixed = re.sub(r"^(\s+default\s+)\S+(\s*)$", rf"\g<1>{value}\g<2>", body, count=1, flags=re.M)
    return text[: m.start(1)] + fixed + text[m.end(1) :]


def selftest():
    """Exercise the parse, the compare and the rewrite on every run — phase-395.

    A negative control nobody runs decays into a comment, so this is on the
    normal path rather than behind a flag.
    """
    sample = (
        "config NROS_ZENOH_TX_BATCH\n"
        '    bool "batching"\n'
        "    default y\n"
        "    help\n"
        "      words\n"
        "\n"
        "config OTHER\n"
        "    int \"other\"\n"
        "    default 3\n"
    )
    assert kconfig_default(sample, "NROS_ZENOH_TX_BATCH") == "y"
    assert kconfig_default(sample, "NROS_ABSENT") is None, "absent symbol must read None"
    assert normalise(True) == "y" and normalise(False) == "n" and normalise(50) == "50"

    # The rewrite moves the named symbol and nothing else.
    out = rewrite_default(sample, "NROS_ZENOH_TX_BATCH", "n")
    assert kconfig_default(out, "NROS_ZENOH_TX_BATCH") == "n", "rewrite did not take"
    assert kconfig_default(out, "OTHER") == "3", "rewrite touched a sibling symbol"
    assert '    bool "batching"' in out and "      words" in out, "rewrite ate hand-wired text"

    # A symbol with no `default` comes back untouched rather than gaining one.
    bare = "config NROS_BARE\n    bool \"x\"\n"
    assert rewrite_default(bare, "NROS_BARE", "y") == bare, "rewrite invented a default"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--write", action="store_true",
                    help="render Kconfig's tx defaults from the platform TOML")
    args = ap.parse_args()

    selftest()

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

    problems, wrote = [], []
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
        elif got != want and args.write:
            kconfig = rewrite_default(kconfig, symbol, want)
            wrote.append(f"{symbol}: {got} -> {want}")
        elif got != want:
            problems.append(
                f"[knobs.zenoh.tx].{key} = {tx[key]!r} (→ {want}) but "
                f"zephyr/Kconfig {symbol} defaults to {got}. The TOML is the "
                f"authority and the Kconfig default its mirror, so re-render "
                f"it: `python3 {os.path.basename(__file__)} --write`"
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

    if wrote:
        with open(KCONFIG, "w", encoding="utf-8") as fh:
            fh.write(kconfig)
        for w in wrote:
            print(f"zephyr knob agreement: rewrote {w}")

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
