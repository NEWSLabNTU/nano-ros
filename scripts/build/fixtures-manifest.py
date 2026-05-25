#!/usr/bin/env python3
"""Read examples/fixtures.toml — the SSOT for fixture build options (Phase 177.9).

Consumed by both the fixture build recipes and the test-all staleness probe so
they build each fixture with identical features/target-dir/env.

  fixtures-manifest.py list --platform native --lang rust [--rmw zenoh]

emits one TAB-separated record per matching entry:

  <dir>\t<env>\t<cargo-args>

where <env> is space-joined `KEY=VAL` (or empty) and <cargo-args> is the
cargo build flags (--no-default-features / --features a,b / --target-dir D /
--target TRIPLE) — the profile is added by the caller. Word-split <cargo-args>
on whitespace into an argv array.
"""
import argparse
import sys

try:
    import tomllib  # Python 3.11+
except ModuleNotFoundError:  # 3.10 and earlier
    import tomli as tomllib

DEFAULT_MANIFEST = "examples/fixtures.toml"


def load(path):
    with open(path, "rb") as f:
        return tomllib.load(f).get("fixture", [])


def cargo_args(entry):
    args = []
    if entry.get("no_default_features"):
        args.append("--no-default-features")
    feats = entry.get("features")
    if feats:
        args += ["--features", ",".join(feats)]
    if entry.get("target_dir"):
        args += ["--target-dir", entry["target_dir"]]
    if entry.get("target"):
        args += ["--target", entry["target"]]
    return " ".join(args)


def env_str(entry):
    return " ".join(f"{k}={v}" for k, v in (entry.get("env") or {}).items())


def main():
    p = argparse.ArgumentParser()
    p.add_argument("command", choices=["list"])
    p.add_argument("--manifest", default=DEFAULT_MANIFEST)
    p.add_argument("--platform")
    p.add_argument("--lang")
    p.add_argument("--rmw")
    a = p.parse_args()

    for e in load(a.manifest):
        if a.platform and e.get("platform") != a.platform:
            continue
        if a.lang and e.get("lang") != a.lang:
            continue
        if a.rmw and e.get("rmw") != a.rmw:
            continue
        sys.stdout.write(f"{e['dir']}\t{env_str(e)}\t{cargo_args(e)}\n")


if __name__ == "__main__":
    main()
