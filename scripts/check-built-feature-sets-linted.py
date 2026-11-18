#!/usr/bin/env python3
"""Every feature set a `just` recipe BUILDS is also LINTED (issue 2722).

A crate built with an explicit `--features` set compiles code that no other
set reaches: `cfg(feature = ...)` arms, and for `cargo test` the test modules
behind them. When the one recipe that builds a set does `cargo test` or
`cargo check` and no recipe runs `cargo clippy` over the same set, a clippy
error there sits on `main` with nothing asking. Measured on 2026-11-19: the
`nros-cpp` census set (`alloc,env,log,rmw-cffi,metadata-mode,param-services,
lifecycle-services`) was built only by `census-hooks-complete` and carried two
`-D warnings` errors.

So the lint list is DERIVED from the build sites, never written down beside
them: this script reads every recipe through `just --dump --dump-format json`
(so `{{C_API_SHIPPED_FEATURES}}` is resolved by `just` itself), finds each
`cargo test|build|check|rustc|nextest run -p <crate> --features <set>`, and
lists the sets no `cargo clippy` site covers. A clippy site covers a build
site when the crate, `--no-default-features` and the feature SET are the same,
and, for a test build, the clippy site also lints the test targets (`--tests`
or `--all-targets`). The environment the build site sets before `cargo` (for
example `NROS_PARAM_STORE=1`) is carried, because a set may need it to
compile at all.

  --list    one JSON object per uncovered set: crate, features, no_default,
            tests, env, and the sites that build it. The lint lane runs this.
  --check   exit 1 if a build site's feature set cannot be resolved
            statically (it would be skipped by the lane without anyone
            knowing), naming the site; otherwise print the counts.
  --run DIR lint each listed set with `cargo clippy -D warnings`, in the
            Cargo target dir DIR, reporting every red set (the lane).
  --self-test  synthetic recipes: an unlinted set is listed, a covered one is
            not, a test build needs a `--tests` clippy, env is carried.

Run: python3 scripts/check-built-feature-sets-linted.py --check
"""

from __future__ import annotations

import json
import re
import shlex
import subprocess
import sys
from dataclasses import dataclass, field
from pathlib import Path

BUILD_VERBS = {"test", "build", "check", "rustc", "nextest"}
CARGO = re.compile(r"\bcargo\s+(?:\+\S+\s+)?(test|build|check|clippy|rustc|nextest)\b")
ENV_ASSIGN = re.compile(r"^[A-Z_][A-Z0-9_]*=")
# A build site's own target dir is the build's business; the lane uses its own.
DROPPED_ENV = {"CARGO_TARGET_DIR"}
UNRESOLVED = "\0unresolved\0"
# Recipes whose cargo builds are not this tree's to lint, each with why.
EXEMPT_RECIPES = {
    # play_launch's Python extension, built from the play_launch submodule
    # (`$_pyexec_ws`) as a tool: that repository lints its own crates.
    "setup-launch-resolve": "a submodule crate built as a host tool",
    # zenoh-c, cloned and built as the CAN router's library: upstream's crate.
    "scripts/can/build-zenohc-can.sh": "an upstream crate built as a host library",
    # Reads its set from NanoRosPx4Module.cmake at run time; the same set is
    # spelled literally in `px4::build-sitl-example`, which this lane lints.
    "scripts/stage-px4-posix-archives.sh": "the PX4 archive set, linted via px4::build-sitl-example",
}
# Ad hoc debugging aids, not lanes: nothing runs them unattended.
EXEMPT_PREFIXES = ("scripts/debug/",)


@dataclass
class Site:
    where: str
    verb: str
    crate: str
    features: frozenset[str]
    no_default: bool
    tests: bool
    env: tuple[tuple[str, str], ...] = ()
    manifest: str | None = None
    target: str | None = None

    @property
    def key(self) -> tuple:
        return (
            self.crate,
            self.no_default,
            tuple(sorted(self.features)),
            self.manifest,
            self.target,
        )


@dataclass
class Harvest:
    builds: list[Site] = field(default_factory=list)
    lints: list[Site] = field(default_factory=list)
    unresolved: list[str] = field(default_factory=list)


def render(body: list, scope: dict[str, str]) -> list[str]:
    """A recipe body as text lines, `{{var}}` resolved where `just` can."""
    lines = []
    for line in body:
        out = []
        for frag in line:
            out.append(_fragment(frag, scope))
        lines.append("".join(out))
    return lines


def _fragment(frag, scope: dict[str, str]) -> str:
    # `just --dump` nests an interpolation: `[["variable", "NAME"]]`.
    if isinstance(frag, str):
        return frag
    if isinstance(frag, list) and len(frag) == 2 and frag[0] == "variable":
        return scope.get(frag[1], UNRESOLVED)
    if isinstance(frag, list) and len(frag) == 1:
        return _fragment(frag[0], scope)
    return UNRESOLVED


def join_continuations(lines: list[str]) -> list[tuple[int, str]]:
    out: list[tuple[int, str]] = []
    buf, start = "", 0
    for n, line in enumerate(lines, 1):
        if not buf:
            start = n
        stripped = line.rstrip()
        if stripped.endswith("\\"):
            buf += stripped[:-1] + " "
            continue
        out.append((start, buf + line))
        buf = ""
    if buf:
        out.append((start, buf))
    return out


def parse_command(text: str, where: str, harvest: Harvest) -> None:
    if text.lstrip().startswith("#"):
        return
    m = CARGO.search(text)
    if not m:
        return
    verb = m.group(1)
    head, tail = text[: m.start()], text[m.end():]
    # The command ends at the first shell separator; `--` ends cargo's args.
    tail = re.split(r"\s--(?:\s|$)|\|\||&&|;|\|(?!\|)", tail)[0]
    try:
        toks = shlex.split(tail)
        head_toks = shlex.split(head.split("&&")[-1].split(";")[-1])
    except ValueError:
        toks, head_toks = tail.split(), []
    crates: list[str] = []
    feats: str | None = None
    manifest: str | None = None
    target: str | None = None
    no_default = tests = False
    i = 0
    while i < len(toks):
        t = toks[i]
        if t in ("-p", "--package") and i + 1 < len(toks):
            crates.append(toks[i + 1])
            i += 2
            continue
        if t.startswith("--package="):
            crates.append(t.split("=", 1)[1])
        elif t in ("-F", "--features") and i + 1 < len(toks):
            feats = toks[i + 1]
            i += 2
            continue
        elif t.startswith("--features="):
            feats = t.split("=", 1)[1]
        elif t == "--manifest-path" and i + 1 < len(toks):
            manifest = toks[i + 1]
            i += 2
            continue
        elif t == "--target" and i + 1 < len(toks):
            target = toks[i + 1]
            i += 2
            continue
        elif t == "--no-default-features":
            no_default = True
        elif t in ("--tests", "--all-targets"):
            tests = True
        i += 1
    if feats is None or not crates:
        return
    recipe = where.rsplit(":", 1)[0]  # a recipe name, or a script path
    if recipe in EXEMPT_RECIPES or recipe.startswith(EXEMPT_PREFIXES):
        return
    # A target only the running shell knows (a loop over triples): the set is
    # linted for the host. Its cross-only `cfg` arms are the site's own to
    # compile; the feature set is what this lane is about.
    if target is not None and ("$" in target or UNRESOLVED in target):
        target = None
    if manifest is not None and ("$" in manifest or UNRESOLVED in manifest):
        if verb != "clippy":
            harvest.unresolved.append(f"{where}: cargo {verb} --manifest-path {manifest!r}")
        return
    if UNRESOLVED in feats or "$" in feats:
        if verb != "clippy":
            harvest.unresolved.append(f"{where}: cargo {verb} --features {feats!r}".replace(UNRESOLVED, "{{?}}"))
        return
    if verb == "test":
        tests = True
    env = tuple(
        (k, v)
        for k, v in (t.split("=", 1) for t in head_toks if ENV_ASSIGN.match(t))
        # A value only the running shell knows (a loop variable) cannot be
        # carried; the lane lints the set without it.
        if k not in DROPPED_ENV and "$" not in v and UNRESOLVED not in v
    )
    fs = frozenset(x for x in re.split(r"[,\s]+", feats) if x)
    for crate in crates:
        site = Site(where, verb, crate, fs, no_default, tests, env, manifest, target)
        (harvest.lints if verb == "clippy" else harvest.builds).append(site)


def harvest_dump(dump: dict) -> Harvest:
    harvest = Harvest()

    def walk(module: dict, scope: dict[str, str], path: str) -> None:
        scope = dict(scope)
        for name, a in module.get("assignments", {}).items():
            if isinstance(a.get("value"), str):
                scope[name] = a["value"]
        for name, recipe in module.get("recipes", {}).items():
            lines = render(recipe.get("body", []), scope)
            for n, text in join_continuations(lines):
                parse_command(text, f"{path}{name}:{n}", harvest)
        for sub_name, sub in module.get("modules", {}).items():
            walk(sub, scope, f"{path}{sub_name}::")

    walk(dump, {}, "")
    return harvest


SHELL_ASSIGN = re.compile(r'^\s*(?:export\s+)?([A-Z_][A-Z0-9_]*)="?([^"$`]*)"?\s*$')


def harvest_scripts(root: Path, harvest: Harvest) -> None:
    """Shell scripts under `scripts/`: a `$NAME` is resolved from the script's
    own single literal `NAME="..."`; a name assigned twice, or from another
    variable, stays unresolved and is reported."""
    files = subprocess.run(
        ["git", "ls-files", "scripts/*.sh", "scripts/**/*.sh"],
        cwd=root, capture_output=True, text=True, check=True,
    ).stdout.split()
    for rel in files:
        text = (root / rel).read_text(errors="replace")
        assigned: dict[str, list[str]] = {}
        for line in text.splitlines():
            m = SHELL_ASSIGN.match(line)
            if m:
                assigned.setdefault(m.group(1), []).append(m.group(2))
            else:
                m2 = re.match(r"^\s*(?:export\s+)?([A-Z_][A-Z0-9_]*)=", line)
                if m2:
                    assigned.setdefault(m2.group(1), []).append(UNRESOLVED)
        literal = {k: v[0] for k, v in assigned.items() if len(v) == 1 and v[0] != UNRESOLVED}

        def sub(m: re.Match) -> str:
            return literal.get(m.group(1) or m.group(2), m.group(0))

        lines = [
            re.sub(r"\$\{([A-Z_][A-Z0-9_]*)\}|\$([A-Z_][A-Z0-9_]*)", sub, line)
            for line in text.splitlines()
        ]
        for n, cmd in join_continuations(lines):
            parse_command(cmd, f"{rel}:{n}", harvest)


def harvest_feature_lanes(root: Path, harvest: Harvest) -> None:
    """The `.config/feature-lanes.toml` lanes: `scripts/feature-lanes.py`
    spells their `cargo nextest run` in Python, so no recipe text holds them."""
    try:
        import tomllib
    except ModuleNotFoundError:  # Python < 3.11
        import tomli as tomllib  # type: ignore[no-redef]
    reg = tomllib.loads((root / ".config" / "feature-lanes.toml").read_text())
    for n, lane in enumerate(reg.get("lane", []), 1):
        if not lane.get("features"):
            continue
        env = tuple((k, str(v)) for k, v in lane.get("env", {}).items() if k not in DROPPED_ENV)
        harvest.builds.append(
            Site(
                f".config/feature-lanes.toml:lane {n} ({lane.get('runner', '?')})",
                "nextest",
                lane["crate"],
                frozenset(lane["features"]),
                bool(lane.get("no_default")),
                True,
                env,
            )
        )


def uncovered(harvest: Harvest) -> list[dict]:
    jobs: dict[tuple, dict] = {}
    for b in harvest.builds:
        if any(
            lint.key == b.key and (lint.tests or not b.tests) for lint in harvest.lints
        ):
            continue
        job = jobs.setdefault(
            b.key,
            {
                "crate": b.crate,
                "features": sorted(b.features),
                "no_default": b.no_default,
                "manifest_path": b.manifest,
                "target": b.target,
                "tests": False,
                "env": {},
                "sites": [],
            },
        )
        job["tests"] = job["tests"] or b.tests
        job["env"].update(dict(b.env))
        job["sites"].append(f"{b.where} (cargo {b.verb})")
    return list(jobs.values())


def self_test() -> int:
    def recipe(*lines: str) -> dict:
        return {"body": [[line] for line in lines]}

    dump = {
        "assignments": {"SET": {"value": "alloc,log"}},
        "recipes": {
            "unlinted": recipe("cargo test -p a --no-default-features --features x,y --quiet"),
            "covered-build": recipe(
                "cargo check -p b --features p", "cargo clippy -p b --features p -- -D warnings"
            ),
            "test-needs-tests": recipe(
                "cargo test -p c --features q", "cargo clippy -p c --features q -- -D warnings"
            ),
            "env": recipe("NROS_PARAM_STORE=1 CARGO_TARGET_DIR=x cargo check -p d --features r"),
            "comment": recipe("# cargo test -p e --features s"),
        },
        "modules": {
            "sub": {
                "assignments": {},
                "recipes": {
                    "var": {"body": [["cargo test -p f --features ", [["variable", "SET"]]]]},
                    "param": {"body": [["cargo test -p g --features ", [["variable", "args"]]]]},
                },
                "modules": {},
            }
        },
    }
    h = harvest_dump(dump)
    got = {j["crate"]: j for j in uncovered(h)}
    problems = []
    if set(got) != {"a", "c", "d", "f"}:
        problems.append(f"uncovered crates {sorted(got)} != ['a', 'c', 'd', 'f']")
    if "a" in got and not (got["a"]["no_default"] and got["a"]["tests"]):
        problems.append("a: --no-default-features / test build not carried")
    if "d" in got and got["d"]["env"] != {"NROS_PARAM_STORE": "1"}:
        problems.append(f"d: env {got['d']['env']} (CARGO_TARGET_DIR must drop, the knob stay)")
    if "f" in got and got["f"]["features"] != ["alloc", "log"]:
        problems.append("f: an inherited `{{SET}}` is not resolved")
    if not any("sub::param" in u for u in h.unresolved):
        problems.append("a recipe parameter set is not reported unresolved")
    for p in problems:
        print(f"check-built-feature-sets-linted self-test: {p}", file=sys.stderr)
    return 1 if problems else 0


def load_dump(root: Path) -> dict:
    out = subprocess.run(
        ["just", "--dump", "--dump-format", "json"],
        cwd=root,
        capture_output=True,
        text=True,
        check=True,
    )
    return json.loads(out.stdout)


def clippy_command(job: dict) -> list[str]:
    cmd = ["cargo", "clippy", "--quiet", "-p", job["crate"]]
    if job.get("manifest_path"):
        cmd += ["--manifest-path", job["manifest_path"]]
    if job.get("target"):
        cmd += ["--target", job["target"]]
    if job["no_default"]:
        cmd.append("--no-default-features")
    cmd += ["--features", ",".join(job["features"])]
    if job["tests"]:
        cmd += ["--lib", "--tests"]
    return cmd + ["--", "-D", "warnings"]


def run(root: Path, jobs: list[dict], target_dir: str) -> int:
    """Lint every job; keep going, so one red set cannot hide the next."""
    import os
    import time

    red = []
    for job in jobs:
        cmd = clippy_command(job)
        env = dict(os.environ, CARGO_TARGET_DIR=target_dir, **job["env"])
        t0 = time.monotonic()
        r = subprocess.run(cmd, cwd=root, env=env)
        took = time.monotonic() - t0
        verdict = "ok" if r.returncode == 0 else "RED"
        print(f"==> {verdict} {took:5.0f}s {' '.join(cmd[3:])}   <- {job['sites'][0]}", flush=True)
        if r.returncode != 0:
            red.append(job)
    if red:
        print(f"built-feature-sets-clippy: {len(red)} of {len(jobs)} set(s) RED:", file=sys.stderr)
        for job in red:
            print(f"  {' '.join(clippy_command(job))}", file=sys.stderr)
            for s in job["sites"]:
                print(f"      built at {s}", file=sys.stderr)
        return 1
    print(f"built-feature-sets-clippy: OK ({len(jobs)} set(s), each linted -D warnings)")
    return 0


def main(argv: list[str]) -> int:
    if "--self-test" in argv:
        return self_test()
    root = Path(__file__).resolve().parent.parent
    h = harvest_dump(load_dump(root))
    harvest_scripts(root, h)
    harvest_feature_lanes(root, h)
    jobs = uncovered(h)
    if "--run" in argv:
        if h.unresolved:
            print("built-feature-sets-clippy: unresolved build sites; run --check", file=sys.stderr)
            return 1
        return run(root, jobs, argv[argv.index("--run") + 1])
    if "--list" in argv:
        for j in jobs:
            print(json.dumps(j, sort_keys=True))
        return 0
    if self_test():
        return 1
    if h.unresolved:
        print(
            "check-built-feature-sets-linted: these build sites name a feature set no static "
            "reader can resolve, so the lint lane cannot cover them. Spell the set literally "
            "or through a justfile variable:",
            file=sys.stderr,
        )
        for u in h.unresolved:
            print(f"  {u}", file=sys.stderr)
        return 1
    print(
        f"check-built-feature-sets-linted: OK ({len(h.builds)} build sites, {len(h.lints)} "
        f"clippy sites; {len(jobs)} set(s) linted by the derived lane)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
