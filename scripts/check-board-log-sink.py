#!/usr/bin/env python3
"""Issues 0708/0710 — a crate that supplies the platform console must compile in
`nros-log`'s dispatch auto-install.

`nros_log::dispatch_to_sinks` drops every record until a sink list is published.
An application notices its own missing output; a LIBRARY record cannot, because
its author has no way to know whether the board initialised the facade — issue
0589 put the zenoh session-pool diagnostic in exactly that position, and on
ThreadX and NuttX it reached nothing.

# What this used to check, and why it stopped

Issue 0708's rule was "every `pub fn run*` in a board crate reaches
`init_default()`". That is a search for boot paths, and it kept missing them:
NuttX's funnel is `pub extern "C" fn nsh_main`, the bare-metal one is
`#[entry] fn main()`, and three board crates did not link `nros-log` at all in
the configuration holding the funnel. Every miss surfaced from a booted image,
never from the gate. Issue 0710 stopped searching: dispatch installs the platform
sink itself on first use, so no funnel has to be found.

That install references `nros_platform_log_write`/`_flush` from a path every
record reaches, which would make a platform port a LINK requirement for every
consumer of `nros-log` — including host tools and the test harness, which have no
port. (It did, for one commit: every `nros-tests` target that links `nros-log`
without a port failed to link.) So the install sits behind `nros-log`'s
`platform-sink` feature, and the crates that SUPPLY the symbol turn it on.

# The rule this checks

That set is bounded and greppable, unlike a boot funnel:

  * a crate under `packages/boards/` that depends on `nros-log` — a board IS the
    platform console, and the RTOS ports' symbols are C, linked at board level;
  * a `packages/platform/` crate whose `cffi-export` feature emits the canonical
    `nros_platform_*` symbols from Rust;
  * `nros-platform-cffi`'s `posix-c-port`, which compiles the POSIX C port.

Cargo unifies features, so any image holding one of these gets the auto-install
without a consumer opting in, and a graph with no port neither gets it nor needs
it.

Run: python3 scripts/check-board-log-sink.py [--self-test]
"""

import sys
from pathlib import Path

try:
    import tomllib  # Python 3.11+
except ModuleNotFoundError:  # pragma: no cover - 3.10 hosts
    import tomli as tomllib

ROOT = Path(__file__).resolve().parent.parent
LOG_CRATE = "nros-log"
FEATURE = "platform-sink"
QUALIFIED = f"{LOG_CRATE}/{FEATURE}"

# Features that mean "this crate supplies `nros_platform_log_write`". Named
# rather than pattern-matched: `cffi-export` on a crate with no C port would be
# a different claim, and there is none in the tree.
PROVIDER_FEATURES = ("cffi-export", "posix-c-port")

# `nros-board-common` is a BUILD-SCRIPT crate: it drives image links and platform
# compiles at build time and boots nothing. Excluded by name, with the reason
# here rather than as a bare entry in a list.
EXCLUDED_BOARDS = {"nros-board-common"}


def dep_entry(manifest, name):
    """The dependency table row for `name`, from any dependency section."""
    for key, table in manifest.items():
        if key.endswith("dependencies") and isinstance(table, dict) and name in table:
            return table[name]
        # `[target.'cfg(...)'.dependencies]`
        if key == "target" and isinstance(table, dict):
            for cfg in table.values():
                if not isinstance(cfg, dict):
                    continue
                for sub, deps in cfg.items():
                    if sub.endswith("dependencies") and name in deps:
                        return deps[name]
    return None


def board_problems(manifest, rel):
    entry = dep_entry(manifest, LOG_CRATE)
    if entry is None:
        # No `nros-log` edge is fine: the crate publishes no records and pulls
        # no sink. What is NOT fine is having the edge without the feature.
        return []
    feats = entry.get("features", []) if isinstance(entry, dict) else []
    if FEATURE in feats:
        return []
    return [f"{rel}: depends on {LOG_CRATE} without features = [\"{FEATURE}\"]"]


def forwards_to_provider(enables):
    """True when the feature only RE-EXPORTS another crate's provider feature.

    `nros-platform`'s `cffi-export` is the selector, not the emitter: it turns on
    `nros-platform-mps2-an385?/cffi-export` and friends, and each of those is
    checked on its own. Requiring the enable here as well would demand an
    `nros-log` edge from a crate that supplies no symbol — the same "fix it where
    the symptom showed" mistake the boot-funnel rule kept making.
    """
    return any(
        "/" in e and e.split("/", 1)[1] in PROVIDER_FEATURES
        for e in enables
        if isinstance(e, str)
    )


def provider_problems(manifest, rel):
    out = []
    for name, enables in (manifest.get("features") or {}).items():
        if name not in PROVIDER_FEATURES:
            continue
        if QUALIFIED in enables or forwards_to_provider(enables):
            continue
        out.append(f"{rel}: feature `{name}` neither enables `{QUALIFIED}` nor "
                   "forwards to a provider crate's own provider feature")
    return out


def audit():
    problems = []
    for path in sorted((ROOT / "packages/boards").glob("*/Cargo.toml")):
        if path.parent.name in EXCLUDED_BOARDS:
            continue
        manifest = tomllib.loads(path.read_text(encoding="utf-8"))
        problems += board_problems(manifest, path.relative_to(ROOT))
    for path in sorted((ROOT / "packages/platform").glob("*/Cargo.toml")):
        manifest = tomllib.loads(path.read_text(encoding="utf-8"))
        problems += provider_problems(manifest, path.relative_to(ROOT))
    return problems


def self_test():
    """The shapes this check has to get right, each written both ways."""
    ok = True

    def case(label, got, want):
        nonlocal ok
        if bool(got) != want:
            print(f"  FAIL  {label}")
            ok = False
        else:
            print(f"  ok    {label}")

    plain = tomllib.loads(
        '[dependencies]\nnros-log = { path = "../x", default-features = false }\n'
    )
    case("a board dep without the feature is reported",
         board_problems(plain, "x"), True)

    good = tomllib.loads(
        '[dependencies]\n'
        'nros-log = { path = "../x", features = ["platform-sink"] }\n'
    )
    case("a board dep with the feature passes", board_problems(good, "x"), False)

    none = tomllib.loads('[dependencies]\nlibc = "0.2"\n')
    case("a board with no nros-log edge is not reported",
         board_problems(none, "x"), False)

    # The shape that cost a commit: the edge declared under a `cfg` target table
    # rather than the plain one. A scan of `[dependencies]` alone reads it as
    # "no edge" and passes a crate that has one.
    targeted = tomllib.loads(
        "[target.'cfg(target_os = \"none\")'.dependencies]\n"
        'nros-log = { path = "../x" }\n'
    )
    case("a cfg-target dep is seen, not skipped",
         board_problems(targeted, "x"), True)

    bare = tomllib.loads('[dependencies]\nnros-log = "0.5"\n')
    case("a bare version-string dep is reported", board_problems(bare, "x"), True)

    prov_bad = tomllib.loads('[features]\ncffi-export = ["dep:nros-platform-cffi"]\n')
    case("a provider feature without the enable is reported",
         provider_problems(prov_bad, "x"), True)

    prov_ok = tomllib.loads(
        '[features]\ncffi-export = ["dep:nros-log", "nros-log/platform-sink"]\n'
    )
    case("a provider feature with the enable passes",
         provider_problems(prov_ok, "x"), False)

    unrelated = tomllib.loads('[features]\ndds-heap = []\n')
    case("an unrelated feature is not a provider",
         provider_problems(unrelated, "x"), False)

    unrelated_fwd = tomllib.loads(
        '[features]\ncffi-export = ["nros-platform-mps2-an385?/cffi-export"]\n'
    )
    case("a selector that forwards to a provider passes",
         provider_problems(unrelated_fwd, "x"), False)

    fwd_wrong = tomllib.loads(
        '[features]\ncffi-export = ["nros-platform-mps2-an385?/dds-heap"]\n'
    )
    case("forwarding some OTHER feature does not count",
         provider_problems(fwd_wrong, "x"), True)

    return ok


def main():
    if "--self-test" in sys.argv:
        sys.exit(0 if self_test() else 1)
    problems = audit()
    if problems:
        print("[FAIL] a platform-console crate does not compile in the nros_log "
              "auto-install:", file=sys.stderr)
        for p in problems:
            print(f"  {p}", file=sys.stderr)
        print(f"\n  Without `{FEATURE}`, `dispatch` drops every record raised before "
              "some code", file=sys.stderr)
        print("  calls `nros_log::init` — silently, and a library record's author "
              "cannot know", file=sys.stderr)
        print("  (issues 0708, 0710, 0589).", file=sys.stderr)
        print(f"\n  Fix: add `{FEATURE}` to the crate's `{LOG_CRATE}` features, or "
              f"`{QUALIFIED}`", file=sys.stderr)
        print("  to the provider feature that emits `nros_platform_log_write`.",
              file=sys.stderr)
        sys.exit(1)
    print("check-board-log-sink: OK (every platform-console crate enables "
          f"`{QUALIFIED}`)")


if __name__ == "__main__":
    main()
