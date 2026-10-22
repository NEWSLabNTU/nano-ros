#!/usr/bin/env python3
"""The feature-lane registry: run its lanes, or gate that every gated test runs.

RFC-0051 §7b, phase-510 W1. `.config/feature-lanes.toml` names every cargo
feature that gates a test target and every lane that turns such a feature on.

    feature-lanes.py run <runner> [--verbose]   execute that recipe's lanes
    feature-lanes.py check                      the gate
    feature-lanes.py --self-test                negative controls only

WHY PER TARGET. The gate this replaces asked whether each gating FEATURE was
enabled by some recipe. That is true of `component-runtime-test` while
`executor_arena_advisory`, gated on it, was in no lane's `--test` list, and of
`platform-posix` while two `nros-rmw-zenoh` files gated on it ran nowhere: a
feature reachable for one target says nothing about the next (phase-510
inventory). The unit is the (crate, target) pair.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import shlex
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
REGISTRY = ROOT / ".config" / "feature-lanes.toml"
sys.path.insert(0, str(ROOT / "scripts" / "lib"))
import comments  # noqa: E402
import check_skip  # noqa: E402  issue 1043's middle outcome, NOT VERIFIED

try:
    import tomllib  # type: ignore[import-not-found]
except ModuleNotFoundError:  # Python < 3.11
    import tomli as tomllib  # type: ignore[no-redef]

CLASSES = ("config", "harness")
MATRIX_RS = ROOT / "packages" / "testing" / "nros-tests" / "src" / "matrix.rs"


def matrix_config_variants(rust_text: str) -> set[str]:
    """The variants of `pub enum Config` in matrix.rs (code only)."""
    code = comments.strip_comments(rust_text, "rust")
    m = re.search(r"pub enum Config\s*\{([^}]*)\}", code)
    if not m:
        return set()
    return set(re.findall(r"^\s*([A-Z][A-Za-z0-9]*)\s*,?\s*$", m.group(1), re.M))


def config_vocabulary_problems(reg: dict, variants: set[str]) -> list[str]:
    """`matrix::Config` and the registry's config rows name ONE set."""
    named = {e["config"] for e in {**reg.get("feature", {}), **reg.get("knob", {})}.values()
             if e.get("class") == "config" and e.get("config")}
    out = []
    for v in sorted(named - variants):
        out.append(f"`.config/feature-lanes.toml` classifies a configuration as `{v}`, "
                   "which `matrix::Config` has no variant for")
    for v in sorted(variants - named):
        out.append(f"`matrix::Config::{v}` has no `class = \"config\"` row in "
                   ".config/feature-lanes.toml naming what builds it")
    return out


def load(path: Path = REGISTRY) -> dict:
    with open(path, "rb") as f:
        return tomllib.load(f)


# ── what the tree gates ────────────────────────────────────────────────────

def _git_ls(*patterns: str) -> list[str]:
    return subprocess.run(
        ["git", "-C", str(ROOT), "ls-files", "--", *patterns],
        capture_output=True, text=True, check=True,
    ).stdout.split()


def crate_name(manifest: Path) -> str | None:
    try:
        with open(manifest, "rb") as f:
            doc = tomllib.load(f)
    except (OSError, tomllib.TOMLDecodeError):
        return None
    return doc.get("package", {}).get("name")


FILE_CFG = re.compile(r"#!\[cfg\((.*?)\)\]", re.S)


def file_cfg_features(rust_text: str) -> list[str] | None:
    """The features a test file's crate-level `#![cfg(...)]` requires, or None.

    Only a conjunction of `feature = "x"` (and platform predicates, which a
    host lane satisfies) is understood. `any(...)` / `not(...)` over features is
    refused, so a gate it cannot read becomes a finding rather than a pass.
    """
    head = comments.strip_comments(rust_text, "rust")[:2000]
    m = FILE_CFG.search(head)
    if not m:
        return None
    expr = m.group(1)
    if re.search(r"\b(any|not)\s*\(", expr) and "feature" in expr:
        raise ValueError(f"cfg too complex for this gate: #![cfg({expr})]")
    return re.findall(r'feature\s*=\s*"([^"]+)"', expr) or None


def gated_targets() -> dict[tuple[str, str], tuple[list[str], str]]:
    """(crate, target) -> (features it needs, where that is declared)."""
    out: dict[tuple[str, str], tuple[list[str], str]] = {}
    for rel in _git_ls("*Cargo.toml"):
        manifest = ROOT / rel
        crate = crate_name(manifest)
        if not crate:
            continue
        with open(manifest, "rb") as f:
            doc = tomllib.load(f)
        for t in doc.get("test", []):
            if t.get("required-features"):
                out[(crate, t["name"])] = (list(t["required-features"]), rel)
        tests = manifest.parent / "tests"
        if not tests.is_dir():
            continue
        declared = {t.get("name") for t in doc.get("test", [])}
        auto = doc.get("package", {}).get("autotests", True)
        for p in sorted(tests.glob("*.rs")):
            name = p.stem
            if not auto and name not in declared:
                # Not a test target at all: an `autotests = false` crate builds
                # only what it declares, so an undeclared file is reached by no
                # cargo command. That is the `wake_latency_cortex_m3` shape.
                try:
                    feats = file_cfg_features(p.read_text(encoding="utf-8"))
                except ValueError:
                    feats = None
                out.setdefault((crate, name), (["<undeclared>"] + (feats or []),
                                               str(p.relative_to(ROOT))))
                continue
            try:
                feats = file_cfg_features(p.read_text(encoding="utf-8"))
            except ValueError as e:
                out[(crate, name)] = (["<unreadable>"], f"{p.relative_to(ROOT)}: {e}")
                continue
            if feats:
                prev = out.get((crate, name))
                merged = sorted(set((prev[0] if prev else []) + feats))
                out[(crate, name)] = (merged, str(p.relative_to(ROOT)))
    return out


def unified_features() -> dict[str, set[str]]:
    """crate -> features the ROOT workspace's own graph resolves for it.

    This is what `test-unit` (`cargo nextest run --workspace`, no
    `--features`) builds a member with, because cargo unifies features across
    the workspace. `--offline`: the gate must not reach the network.
    """
    out = subprocess.run(
        ["cargo", "metadata", "--format-version", "1", "--offline",
         "--manifest-path", str(ROOT / "Cargo.toml")],
        capture_output=True, text=True, check=True,
    ).stdout
    meta = json.loads(out)
    names = {p["id"]: p["name"] for p in meta["packages"]}
    members = set(meta["workspace_members"])
    res: dict[str, set[str]] = {}
    for node in meta["resolve"]["nodes"]:
        if node["id"] in members:
            res[names[node["id"]]] = set(node.get("features", []))
    return res


# ── recipes ────────────────────────────────────────────────────────────────

def recipe_body(runner: str) -> str | None:
    """The CODE of one just recipe, wherever it is defined."""
    files = [ROOT / "justfile"] + sorted((ROOT / "just").glob("*.just")) \
        + sorted((ROOT / "just" / "check").glob("*.just"))
    head = re.compile(rf"^{re.escape(runner)}(\s[^:]*)?:(?!=)", re.M)
    for path in files:
        try:
            text = comments.strip_comments(path.read_text(encoding="utf-8"), "just")
        except OSError:
            continue
        m = head.search(text)
        if not m:
            continue
        body = []
        for line in text[m.end():].splitlines()[1:]:
            if line and not line[0].isspace():
                break
            body.append(line)
        return "\n".join(body)
    return None


# ── run ────────────────────────────────────────────────────────────────────

def lane_args(lane: dict) -> list[str]:
    args = ["-p", lane["crate"]]
    if lane.get("no_default"):
        args.append("--no-default-features")
    if lane.get("features"):
        args += ["--features", ",".join(lane["features"])]
    if lane.get("lib"):
        args.append("--lib")
    for t in lane.get("targets", []):
        args += ["--test", t]
    if lane.get("filter"):
        args.append(lane["filter"])
    return args


def run(runner: str, verbose: bool) -> int:
    reg = load()
    lanes = [l for l in reg.get("lane", []) if l["runner"] == runner
             and l.get("generated", True) and not l.get("unified")]
    if not lanes:
        sys.stderr.write(f"feature-lanes: no generated lane has runner `{runner}`\n")
        return 2
    base = subprocess.run(
        ["bash", "-c", "source scripts/build/cargo.sh && nros_cargo_nextest_args"],
        cwd=ROOT, capture_output=True, text=True, check=True,
    ).stdout.split()
    rc = 0
    for lane in lanes:
        env = dict(os.environ)
        env.update({k: str(v) for k, v in lane.get("env", {}).items()})
        if lane.get("scoped_target_dir"):
            env["CARGO_TARGET_DIR"] = subprocess.run(
                ["bash", "-c", "source scripts/build/cargo.sh && "
                 f"nros_scoped_target_dir {shlex.quote(lane['scoped_target_dir'])}"],
                cwd=ROOT, capture_output=True, text=True, check=True,
            ).stdout.strip()
        if lane.get("via") == "platform":
            cmd = ["just", "_nextest-platform", " ".join(lane["targets"]),
                   "1" if verbose else "",
                   " ".join(["--features", ",".join(lane["features"])])]
        elif lane.get("tolerant"):
            args = lane_args(lane) + ["--no-fail-fast"]
            if not verbose:
                args += ["--success-output", "never", "--failure-output",
                         os.environ.get("NROS_TEST_FAILURE_OUTPUT", "never")]
            cmd = ["just", "_nextest-tolerant", *args]
        else:
            cmd = ["cargo", "nextest", "run", *base, *lane_args(lane)]
        print(f"==> feature lane: {' '.join(shlex.quote(c) for c in cmd)}", flush=True)
        r = subprocess.run(cmd, cwd=ROOT, env=env)
        if r.returncode != 0:
            # Keep going: one red lane must not hide the verdicts behind it.
            print(f"==> feature lane FAILED (rc={r.returncode}): {lane['crate']} "
                  f"{' '.join(lane.get('targets', [])) or '--lib'}", flush=True)
            rc = 1
    return rc


# ── check ──────────────────────────────────────────────────────────────────

def problems(reg: dict, gated: dict, unified: dict[str, set[str]] | None,
             body_of=recipe_body) -> list[str]:
    out: list[str] = []
    classified = set(reg.get("feature", {}))
    for name, entry in {**reg.get("feature", {}), **reg.get("knob", {})}.items():
        if entry.get("class") not in CLASSES:
            out.append(f"`{name}`: class must be one of {CLASSES}")
        if entry.get("class") == "config" and not entry.get("config"):
            out.append(f"`{name}`: a config-class entry names its `matrix::Config` value")
        if not entry.get("reason"):
            out.append(f"`{name}`: every classification states its reason")

    reached: dict[tuple[str, str], list[dict]] = {}
    for lane in reg.get("lane", []):
        for t in lane.get("targets", []):
            reached.setdefault((lane["crate"], t), []).append(lane)
        for f in lane.get("features", []):
            if f not in classified:
                out.append(f"lane {lane['runner']}/{lane['crate']} enables `{f}`, "
                           "which [feature.*] does not classify")

    for (crate, target), (needs, where) in sorted(gated.items()):
        if "<unreadable>" in needs:
            out.append(f"{crate}::{target}: {where}")
            continue
        if "<undeclared>" in needs:
            out.append(
                f"{crate}::{target} ({where}): the crate sets `autotests = false` and "
                "has no `[[test]]` entry for this file, so NO cargo command builds it. "
                "Declare it (with its `required-features`) or delete it.")
            continue
        for f in needs:
            if f not in classified:
                out.append(f"{crate}::{target} is gated on `{f}`, which [feature.*] does "
                           "not classify (RFC-0051 §7a: config or harness?)")
        lanes = reached.get((crate, target), [])
        ok = []
        for lane in lanes:
            have = set(lane.get("features", []))
            if lane.get("unified"):
                # None: not verifiable here (reported NOT VERIFIED), so the
                # lane's own claim stands for this run.
                have = unified.get(crate, set()) if unified is not None else have
            if set(needs) <= have:
                ok.append(lane)
        if not ok:
            out.append(
                f"{crate}::{target} ({where}) needs feature(s) {needs} and NO registered "
                "lane runs it with them. Cargo skips such a target silently, so it "
                "reads as coverage while running nothing: add it to a lane in "
                ".config/feature-lanes.toml, or delete it.")

    told: set[str] = set()
    for lane in reg.get("lane", []):
        crate = lane["crate"]
        if lane.get("unified"):
            if unified is not None:
                missing = set(lane.get("features", [])) - unified.get(crate, set())
                if missing:
                    out.append(
                        f"lane test-unit/{crate} claims the workspace resolves "
                        f"{sorted(missing)} for it, and `cargo metadata` says it does "
                        "not — its targets are built with the feature OFF.")
            continue
        body = body_of(lane["runner"])
        if body is None:
            out.append(f"lane runner `{lane['runner']}` is no recipe in justfile/just/")
            continue
        if lane.get("generated", True):
            if lane["runner"] in told:
                continue
            told.add(lane["runner"])
            if not re.search(rf"feature-lanes\.py\s+run\s+{re.escape(lane['runner'])}\b", body):
                out.append(f"recipe `{lane['runner']}` has generated lanes but never runs "
                           f"`scripts/feature-lanes.py run {lane['runner']}`")
        else:
            if f"-p {crate}" not in body:
                out.append(f"recipe `{lane['runner']}` does not run `-p {crate}`, which "
                           "its registered lane claims")
            for f in lane.get("features", []):
                if not re.search(rf"--features[= ][^\n]*\b{re.escape(f)}\b", body):
                    out.append(f"recipe `{lane['runner']}` never enables `{f}` for "
                               f"{crate}, which its registered lane claims")
    return out


def self_test() -> None:
    comments.self_test()
    assert file_cfg_features('#![cfg(feature = "a")]\n') == ["a"]
    assert file_cfg_features('#![cfg(all(feature = "a", target_os = "linux"))]') == ["a"]
    assert file_cfg_features('//! #![cfg(feature = "doc")]\nfn x() {}\n') is None
    try:
        file_cfg_features('#![cfg(any(feature = "a", feature = "b"))]')
        raise AssertionError("an any() over features must be refused")
    except ValueError:
        pass
    reg = {
        "feature": {"a": {"class": "harness", "reason": "r"},
                    "b": {"class": "config", "config": "B", "reason": "r"}},
        "lane": [{"runner": "r1", "crate": "c", "features": ["a"], "targets": ["t1"]}],
    }
    body = lambda _r: "    python3 scripts/feature-lanes.py run r1\n"  # noqa: E731
    # A gated target in a lane with its feature: clean.
    assert problems(reg, {("c", "t1"): (["a"], "x")}, None, body) == []
    # The phase-510 inventory shape: the feature IS enabled, the TARGET is not.
    p = problems(reg, {("c", "t1"): (["a"], "x"), ("c", "t2"): (["a"], "y")}, None, body)
    assert any("c::t2" in m for m in p), p
    # In a lane, but without the feature it needs.
    p = problems(reg, {("c", "t1"): (["a", "b"], "x")}, None, body)
    assert any("NO registered lane" in m for m in p), p
    # An unclassified gating feature.
    p = problems(reg, {("c", "t1"): (["a", "z"], "x")}, None, body)
    assert any("`z`" in m for m in p), p
    # A generated runner whose recipe does not call the generator.
    p = problems(reg, {("c", "t1"): (["a"], "x")}, None, lambda _r: "    cargo test\n")
    assert any("never runs" in m for m in p), p
    # A unified lane the workspace graph does not back.
    reg_u = {"feature": reg["feature"],
             "lane": [{"runner": "test-unit", "unified": True, "crate": "c",
                       "features": ["a"], "targets": ["t1"]}]}
    p = problems(reg_u, {("c", "t1"): (["a"], "x")}, {"c": set()}, body)
    assert p, "a unified lane whose feature the workspace does not resolve must fail"
    assert problems(reg_u, {("c", "t1"): (["a"], "x")}, {"c": {"a"}}, body) == []
    # The Config vocabulary: one set, both directions.
    v = matrix_config_variants("pub enum Config {\n    /// doc\n    B,\n    Extra,\n}\n")
    assert v == {"B", "Extra"}, v
    p = config_vocabulary_problems(reg, v)
    assert any("Config::Extra" in m for m in p), p
    p = config_vocabulary_problems(reg, set())
    assert any("`B`" in m for m in p), p
    assert config_vocabulary_problems(reg, {"B"}) == []
    # An undeclared file in an autotests=false crate.
    p = problems(reg, {("c", "t3"): (["<undeclared>"], "z")}, None, body)
    assert any("autotests" in m for m in p), p


def check() -> int:
    self_test()
    reg = load()
    gated = gated_targets()
    if not gated:
        sys.stderr.write("[FAIL] feature-lanes: no gated targets found — the scan broke\n")
        return 1
    skipped = 0
    try:
        unified = unified_features()
    except (subprocess.CalledProcessError, FileNotFoundError) as e:
        # A checkout with no cargo cache cannot resolve the graph offline. That
        # leaves the `test-unit` (unified) claims unverified, not false: report
        # it, and still check everything else.
        unified = None
        skipped = check_skip.unverified(
            "feature-lanes",
            f"the test-unit lanes' feature-unification claims: `cargo metadata --offline` "
            f"failed ({type(e).__name__}) — fetch the root workspace's dependencies")
    found = problems(reg, gated, unified)
    variants = matrix_config_variants(MATRIX_RS.read_text(encoding="utf-8"))
    if not variants:
        found.append(f"no `pub enum Config` found in {MATRIX_RS.relative_to(ROOT)}")
    found += config_vocabulary_problems(reg, variants)
    if found:
        sys.stderr.write("[FAIL] feature-lanes: a feature-gated test target nothing runs "
                         "(RFC-0051 §7b):\n")
        for m in found:
            sys.stderr.write(f"  - {m}\n")
        return 1
    n_cfg = sum(1 for e in reg.get("feature", {}).values() if e["class"] == "config")
    print(f"feature-lanes: OK — {len(gated)} gated target(s), each in a lane with its "
          f"features; {len(reg.get('lane', []))} lane(s); "
          f"{len(reg.get('feature', {}))} gating feature(s) classified ({n_cfg} config)")
    return skipped


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--self-test", action="store_true")
    sub = ap.add_subparsers(dest="cmd")
    r = sub.add_parser("run")
    r.add_argument("runner")
    r.add_argument("--verbose", action="store_true")
    sub.add_parser("check")
    a = ap.parse_args()
    if a.self_test:
        self_test()
        print("feature-lanes self-test: OK")
        return 0
    if a.cmd == "run":
        return run(a.runner, a.verbose)
    if a.cmd == "check":
        return check()
    ap.print_help()
    return 2


if __name__ == "__main__":
    sys.exit(main())
