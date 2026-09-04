#!/usr/bin/env python3
"""Derive the zpico run-time configuration key table from zenoh-pico's config.h.

zenoh-pico's *primary* configuration method is run-time options —
``zp_config_insert(config, Z_CONFIG_<X>_KEY, value)``. There is no config-file
format for the pico client (JSON5/YAML belongs to ``zenohd``, the router), so
this table is the whole surface: a key nano-ros does not map is a key a user
cannot set.

That map used to be hand-written in ``zpico.c`` and covered 10 of the 23 keys
upstream defines. A closed hand-written list drifts silently on every upstream
bump — the same failure phase-347 W3 retired four other closed lists for — so
the list is DERIVED here instead, from the one place the keys are defined:

    packages/rmw/zenoh/zpico-sys/zenoh-pico/include/zenoh-pico/config.h

The output is committed (``c/zpico/zpico_config_keys.h``) rather than generated
at build time on purpose: ``zpico.c`` is compiled by TWO build systems — cargo
(``nros-zpico-build``) and CMake (``zephyr/cmake/nros_rmw_zenoh.cmake``) — and a
build-time generator would have to exist in both, which is the two-producer
hazard CLAUDE.md warns about. Same shape as ``scripts/gen-abi-bindings.sh``:
generate once, commit, gate for staleness (``just check zpico-config-keys``).

Naming rule, mechanical so it cannot drift:

    Z_CONFIG_<X>_KEY  ->  lowercase(<X>)

e.g. ``Z_CONFIG_TLS_LISTEN_PRIVATE_KEY_KEY`` -> ``tls_listen_private_key``.

Four property names predate that rule and are still accepted, as ALIASES listed
explicitly below. They are the only authored rows in the file; everything else
falls out of config.h.

Usage:
    python3 scripts/gen-zpico-config-keys.py            # write the header
    python3 scripts/gen-zpico-config-keys.py --check    # exit 1 if it is stale
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
CONFIG_H = REPO / "packages/rmw/zenoh/zpico-sys/zenoh-pico/include/zenoh-pico/config.h"
OUT_H = REPO / "packages/rmw/zenoh/zpico-sys/c/zpico/zpico_config_keys.h"

KEY_RE = re.compile(r"^#define\s+(Z_CONFIG_([A-Z0-9_]+)_KEY)\s+(0x[0-9A-Fa-f]+|\d+)\s*$")

# Property names that predate the mechanical rule. Kept working so an existing
# `properties = [("root_ca_certificate", …)]` caller does not break; the
# canonical (derived) spelling is what new code should use.
#
# alias -> the Z_CONFIG_* macro it resolves to
ALIASES = {
    "scouting_timeout_ms": "Z_CONFIG_SCOUTING_TIMEOUT_KEY",
    "root_ca_certificate": "Z_CONFIG_TLS_ROOT_CA_CERTIFICATE_KEY",
    "root_ca_certificate_base64": "Z_CONFIG_TLS_ROOT_CA_CERTIFICATE_BASE64_KEY",
    "verify_name_on_connect": "Z_CONFIG_TLS_VERIFY_NAME_ON_CONNECT_KEY",
}


def parse_keys(text: str) -> list[tuple[str, str]]:
    """Return [(macro, derived property name)] in config.h order."""
    out: list[tuple[str, str]] = []
    for line in text.splitlines():
        m = KEY_RE.match(line)
        if m:
            out.append((m.group(1), m.group(2).lower()))
    return out


def render(keys: list[tuple[str, str]], with_aliases: bool = True) -> str:
    """Emit the header. `with_aliases=False` is for the self-test, whose sample
    holds none of the alias targets — a self-test that could not reach the
    emission path would be the negative control the gate-selftest rule exists
    to prevent."""
    rows: list[tuple[str, str, bool]] = []
    for macro, name in keys:
        rows.append((name, macro, name.startswith("tls_")))
    if with_aliases:
        known = {m for m, _ in keys}
        missing = sorted(m for m in ALIASES.values() if m not in known)
        if missing:
            raise SystemExit(
                f"ERROR: config.h no longer defines {', '.join(missing)}, which "
                f"ALIASES names. Fix ALIASES in {Path(__file__).name}."
            )
        for alias, macro in sorted(ALIASES.items()):
            derived = next(n for m, n in keys if m == macro)
            rows.append((alias, macro, derived.startswith("tls_")))

    lines = []
    for name, macro, tls in rows:
        lines.append(f'    {{"{name}", {macro}, {str(tls).lower()}}},')
    table = "\n".join(lines)

    n_derived = len(keys)
    n_alias = len(rows) - n_derived
    return f"""/* GENERATED FILE — DO NOT EDIT.
 *
 * Regenerate with:  python3 scripts/gen-zpico-config-keys.py
 * Staleness gate:   just check zpico-config-keys
 *
 * Source of truth:
 *   packages/rmw/zenoh/zpico-sys/zenoh-pico/include/zenoh-pico/config.h
 *
 * zenoh-pico's run-time options ARE its configuration surface — there is no
 * config-file format for the pico client. This table maps the nano-ros
 * property name a caller writes onto the `Z_CONFIG_*_KEY` constant
 * `zp_config_insert()` takes. {n_derived} keys are derived mechanically
 * (`Z_CONFIG_<X>_KEY` -> `lowercase(<X>)`); {n_alias} legacy aliases are
 * authored in the generator.
 *
 * `needs_tls` marks the keys that only mean anything when zenoh-pico is built
 * with `Z_FEATURE_LINK_TLS`. They stay in the table on every build so that
 * supplying one to a build WITHOUT TLS is a reported error rather than a
 * silent no-op.
 */

#ifndef ZPICO_CONFIG_KEYS_H
#define ZPICO_CONFIG_KEYS_H

#include <stdbool.h>
#include <stdint.h>

/* Requires <zenoh-pico.h> (for the Z_CONFIG_*_KEY macros) to be included
 * first; this header deliberately does not include it, so it can be parsed by
 * the coverage test without a zenoh-pico include path. */

typedef struct zpico_config_key_entry {{
    const char* name; /* nano-ros property name */
    uint8_t key;      /* Z_CONFIG_*_KEY */
    bool needs_tls;   /* meaningful only with Z_FEATURE_LINK_TLS */
}} zpico_config_key_entry;

static const zpico_config_key_entry ZPICO_CONFIG_KEYS[] = {{
{table}
}};

#define ZPICO_CONFIG_KEY_COUNT (sizeof(ZPICO_CONFIG_KEYS) / sizeof(ZPICO_CONFIG_KEYS[0]))

#endif /* ZPICO_CONFIG_KEYS_H */
"""


def self_test() -> bool:
    """Prove the derivation actually derives before trusting it on config.h.

    The whole point of this script is that a hand-written key list drifts; a
    parser that quietly matches nothing would reintroduce exactly that, with a
    green gate on top. So: a `_KEY` define is picked up with its name lowered,
    and the three near-misses that must NOT be (a `_DEFAULT`, a mode VALUE, a
    non-Z_CONFIG define) are checked by name.
    """
    sample = "\n".join(
        [
            "#define Z_CONFIG_MODE_KEY 0x40",
            '#define Z_CONFIG_MODE_CLIENT "client"',
            "#define Z_CONFIG_SCOUTING_TIMEOUT_KEY 0x47",
            '#define Z_CONFIG_SCOUTING_TIMEOUT_DEFAULT "1000"',
            "#define Z_CONFIG_TLS_LISTEN_PRIVATE_KEY_KEY 0x4D",
            "#define Z_FEATURE_PUBLICATION 1",
        ]
    )
    got = parse_keys(sample)
    want = [
        ("Z_CONFIG_MODE_KEY", "mode"),
        ("Z_CONFIG_SCOUTING_TIMEOUT_KEY", "scouting_timeout"),
        # the trailing `_KEY` is the SUFFIX, not the whole word: this key's own
        # name ends in `_KEY`, and stripping greedily would name it
        # `tls_listen_private`.
        ("Z_CONFIG_TLS_LISTEN_PRIVATE_KEY_KEY", "tls_listen_private_key"),
    ]
    if got != want:
        print(f"self-test FAILED: parse_keys -> {got}, want {want}", file=sys.stderr)
        return False

    # An emitted table must name every key it parsed, and a `tls_` key must be
    # marked as needing TLS while a non-TLS one is not — that flag is what makes
    # a TLS key on a non-TLS build an error rather than a silent no-op.
    rendered = render(got, with_aliases=False)
    for _, name in got:
        if f'{{"{name}", ' not in rendered:
            print(f"self-test FAILED: '{name}' missing from the emitted table", file=sys.stderr)
            return False
    if '{"tls_listen_private_key", Z_CONFIG_TLS_LISTEN_PRIVATE_KEY_KEY, true}' not in rendered:
        print("self-test FAILED: a tls_ key was not marked needs_tls", file=sys.stderr)
        return False
    if '{"mode", Z_CONFIG_MODE_KEY, false}' not in rendered:
        print("self-test FAILED: a non-TLS key was marked needs_tls", file=sys.stderr)
        return False
    print("gen-zpico-config-keys self-test: OK (3 key shapes + 3 near-misses + emission)")
    return True


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--check", action="store_true", help="fail if the header is stale")
    args = ap.parse_args()

    if not self_test():
        return 1

    if not CONFIG_H.is_file():
        print(
            f"ERROR: {CONFIG_H} is missing — the zenoh-pico submodule is not "
            f"checked out. Run: git submodule update --init "
            f"packages/rmw/zenoh/zpico-sys/zenoh-pico",
            file=sys.stderr,
        )
        return 2

    keys = parse_keys(CONFIG_H.read_text())
    if not keys:
        print(f"ERROR: no Z_CONFIG_*_KEY defines found in {CONFIG_H}", file=sys.stderr)
        return 2

    rendered = render(keys)
    current = OUT_H.read_text() if OUT_H.is_file() else None

    if args.check:
        if current != rendered:
            print(
                "ERROR: packages/rmw/zenoh/zpico-sys/c/zpico/zpico_config_keys.h is "
                "stale — zenoh-pico's config.h defines a different key set than the "
                "committed table. Run: python3 scripts/gen-zpico-config-keys.py "
                "and commit the result.",
                file=sys.stderr,
            )
            return 1
        print(f"zpico config-key table covers all {len(keys)} Z_CONFIG_*_KEY constants.")
        return 0

    if current != rendered:
        OUT_H.write_text(rendered)
        print(f"wrote {OUT_H.relative_to(REPO)} ({len(keys)} keys + {len(ALIASES)} aliases)")
    else:
        print(f"{OUT_H.relative_to(REPO)} already up to date")
    return 0


if __name__ == "__main__":
    sys.exit(main())
