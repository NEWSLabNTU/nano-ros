#!/usr/bin/env python3
"""Read examples/fixtures.toml — the SSOT for fixture build options (Phase 177.9).

Consumed by both the fixture build recipes and the test-all staleness probe so
they build each fixture with identical features/target-dir/env.

  fixtures-manifest.py list --platform native --lang rust [--rmw zenoh]

emits one record per matching entry, fields separated by the unit-separator
byte 0x1F (NOT tab — tab is IFS-whitespace, so bash `read` would collapse the
empty <env> field and shift the columns):

  <dir>\x1f<env>\x1f<cargo-args>

Read it in bash with `IFS=$'\x1f' read -r dir env args`. <env> is space-joined
`KEY=VAL` (or empty), <cargo-args> is the cargo build flags
(--no-default-features / --features a,b / --target-dir D / --target TRIPLE) —
the profile is added by the caller; word-split <cargo-args> into an argv array.
"""
import argparse
import sys

SEP = "\x1f"

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


def cmake_defs(entry):
    # `rmw` shorthand expands to -DNROS_RMW=<rmw>; explicit cmake_defs override.
    defs = {}
    if entry.get("rmw"):
        defs["NROS_RMW"] = entry["rmw"]
    defs.update(entry.get("cmake_defs") or {})
    return " ".join(f"-D{k}={v}" for k, v in defs.items())


def main():
    p = argparse.ArgumentParser()
    p.add_argument("command", choices=["list"])
    p.add_argument("--manifest", default=DEFAULT_MANIFEST)
    p.add_argument("--platform")
    p.add_argument("--lang")
    p.add_argument("--rmw")
    # The test-all staleness probe builds with the default (stable) toolchain
    # and can't replicate a recipe-injected platform toolchain (e.g. the ESP32
    # nightly + build-std). Such cells set `skip_probe = true` so --for-probe
    # omits them — otherwise the probe rebuilds them under the wrong toolchain
    # every preflight (toolchain-fingerprint thrash → permanent false-stale).
    p.add_argument("--for-probe", action="store_true")
    a = p.parse_args()

    for e in load(a.manifest):
        if a.platform and e.get("platform") != a.platform:
            continue
        if a.lang and e.get("lang") != a.lang:
            continue
        if a.rmw and e.get("rmw") != a.rmw:
            continue
        if a.for_probe and e.get("skip_probe"):
            continue
        if e.get("lang") in ("c", "cpp"):
            # cmake record: <dir>\x1f<build-subdir>\x1f<cmake -D defs>\x1f<target>
            sub = e.get("build_subdir") or (f"build-{e['rmw']}" if e.get("rmw") else "build")
            sys.stdout.write(
                f"{e['dir']}{SEP}{sub}{SEP}{cmake_defs(e)}{SEP}{e.get('target', '')}\n"
            )
        else:
            # cargo record: <dir>\x1f<env>\x1f<cargo-args>
            sys.stdout.write(f"{e['dir']}{SEP}{env_str(e)}{SEP}{cargo_args(e)}\n")


if __name__ == "__main__":
    main()
