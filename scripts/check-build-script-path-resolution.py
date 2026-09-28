#!/usr/bin/env python3
"""A build script that resolves a path-valued SDK variable uses `nros_build_paths`.

phase-471 W3 — the build-script half of issue 1280.

# The failure this prevents

An inherited absolute path outranks the checkout you are building. Every
path-valued variable here resolves ENV-FIRST, which is how a real out-of-tree
SDK gets used, so a `build.rs` that reads one with a bare `env::var` compiles
whatever another checkout's shell exported. `nros_build_paths` is the ONE
resolver that applies issue 1280's three-valued rule — outside any checkout
KEEP, a DIFFERENT checkout RE-ROOT, this one keep.

Issue 1280 landed the rule for both halves and gated only one.
`check-inherited-checkout-paths` holds the 21 `just/sdk-env.just` exports to the
re-root wrapper; it contains zero references to `build.rs`, `nros_build_paths`
or `env::var`. Its reach is narrower than the rule it enforces — the 0196 shape
the 2026-07-28 audit found in four gates — and that is how issue 1527's FIVE
sites drifted with every gate green, one of them a submodule probe that measured
one checkout while the compile below it built another.

# Why this imports the census instead of re-deriving

`scripts/nros-build-wiring.py` already computes this list, and its subject is
`just/sdk-env.just` read through the same wrapper test the shell gate applies.
Re-deriving here would make a SECOND subject, which is the defect one level up:
issue 1280's own census was an authored 19-name copy and was already short by
five when it was written. So this gate enforces the census's number rather than
computing a rival one, and a change to what counts as a path variable moves both
halves at once or neither.

# Two things it must get right, both measured in phase-471

* A **private helper** takes the variable NAME as an argument, so no literal
  match can see which variables it resolves. The rule is about the helper's
  BODY. The census finds those bodies by NAME (`^fn env_path\\w*`), which is
  narrower than the rule — a helper called `sdk_dir()` would be invisible — so
  this gate widens it: ANY local fn whose body reads `env::var` of its own
  string parameter and never reaches `nros_build_paths`.
* `env::var("X").is_err()` is a PRESENCE test, not a path read. Re-rooting it
  would change nothing, and counting it reports a crate with no defect while
  burying the ones that have it.

Exempt at the site with `// nros-build-paths-exempt: <reason>`, read by whoever
next changes the thing it excuses.

Run: `just check build-script-path-resolution`  (also `--self-test`)
"""

from __future__ import annotations

import importlib.util
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CENSUS = ROOT / "scripts/nros-build-wiring.py"

EXEMPT_RE = re.compile(r"//\s*nros-build-paths-exempt:\s*(\S.*)")

# A local helper that resolves a path from a NAME it was handed. The census
# matches these by function name; the rule is about the body, so this asks the
# body directly: a `&str` parameter, an `env::var` of that parameter, and no
# delegation to the shared resolver anywhere inside.
HELPER_DEF = re.compile(
    r"^\s*fn\s+(\w+)\s*\(\s*(\w+)\s*:\s*&str[^\n]*\n((?:.*\n)*?)^\s*\}", re.M)


def load_census():
    spec = importlib.util.spec_from_file_location("nros_build_wiring", CENSUS)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def strip_comments(text: str) -> str:
    return re.sub(r"^\s*//.*$", "", text, flags=re.M)


def body_resolving_helpers(code: str) -> list[str]:
    """Names of local fns that resolve a path from a `&str` name themselves.

    Widened past the census's `^fn env_path\\w*`: the hazard is the SHAPE, and
    naming it `env_path` is a courtesy the next person need not extend.
    """
    out = []
    for name, param, body in HELPER_DEF.findall(code):
        if "nros_build_paths::" in body:
            continue
        if re.search(rf"env::var(?:_os)?\(\s*{re.escape(param)}\s*\)", body):
            out.append(name)
    return out


def self_test() -> int:
    bad = 0

    def chk(label: str, ok: bool) -> None:
        nonlocal bad
        print(f"  {'ok' if ok else 'FAIL'}  {label}")
        if not ok:
            bad = 1

    # The widening this gate exists to add, both directions.
    hidden = (
        "fn sdk_dir(name: &str) -> PathBuf {\n"
        "    PathBuf::from(env::var(name).unwrap())\n"
        "}\n"
    )
    chk("a differently-NAMED private helper is found (the census's name rule misses it)",
        body_resolving_helpers(hidden) == ["sdk_dir"])

    delegating = (
        "fn sdk_dir(name: &str) -> PathBuf {\n"
        "    nros_build_paths::env_path(name).unwrap()\n"
        "}\n"
    )
    chk("a helper that DELEGATES is not a finding — it is the fix",
        body_resolving_helpers(delegating) == [])

    # The presence test. Asserted through the census's own regex so the two
    # cannot drift: if it ever starts counting `.is_err()`, this fails here
    # rather than in a confusing report about a crate with no defect.
    census = load_census()
    presence = 'if env::var("FREERTOS_DIR").is_err() { return; }'
    reads = re.findall(
        r'env::var(?:_os)?\(\s*"([A-Z_0-9]+)"\s*\)(?!\s*\.is_(?:err|ok)\(\))', presence)
    chk("`env::var(..).is_err()` is a presence test, not a path read", reads == [])

    valued = 'let d = env::var("FREERTOS_DIR").unwrap();'
    reads = re.findall(
        r'env::var(?:_os)?\(\s*"([A-Z_0-9]+)"\s*\)(?!\s*\.is_(?:err|ok)\(\))', valued)
    chk("a read whose VALUE is used IS a path read", reads == ["FREERTOS_DIR"])

    chk("an exemption is read with its reason",
        (EXEMPT_RE.search("// nros-build-paths-exempt: NuttX builds in place")
         or [None, ""])[1].strip() == "NuttX builds in place")
    chk("an exemption with NO reason does not count",
        EXEMPT_RE.search("// nros-build-paths-exempt:") is None)

    # The subject is the census's, and it must be non-empty — a gate whose
    # variable set came up empty would pass over every script in the tree.
    chk("the census yields a non-empty path-variable set",
        len(census.sdk_path_vars()) > 0)
    return bad


def main() -> int:
    if "--self-test" in sys.argv:
        return self_test()

    # The control runs on the NORMAL path. A negative control nobody invokes
    # reads as coverage while proving nothing about the run that matters.
    if self_test() != 0:
        print("[FAIL] the self-test did not pass, so nothing below is trustworthy.",
              file=sys.stderr)
        return 1

    if not CENSUS.is_file():
        print(f"[FAIL] missing {CENSUS.relative_to(ROOT)}", file=sys.stderr)
        return 1

    census = load_census()
    path_vars = set(census.sdk_path_vars())
    if not path_vars:
        print("[FAIL] no path-valued variables derived from just/sdk-env.just —", file=sys.stderr)
        print("       with an empty subject this gate passes over everything,", file=sys.stderr)
        print("       which is how it would go quiet instead of red.", file=sys.stderr)
        return 1

    rows = census.build_scripts()
    if not rows:
        print("[FAIL] the census found no build scripts at all", file=sys.stderr)
        return 1

    fail = 0
    checked = 0
    exempted = 0
    for r in rows:
        if not r["sdk_path_vars"]:
            continue
        checked += 1
        text = (ROOT / r["path"]).read_text(errors="replace")
        code = strip_comments(text)

        reason = EXEMPT_RE.search(text)
        raw = r["raw_path_reads"]
        hidden = body_resolving_helpers(code)

        if not raw and not hidden:
            continue
        if reason:
            exempted += 1
            print(f"  exempt  {r['path']}: {reason.group(1).strip()}")
            continue

        fail = 1
        print(f"[FAIL] {r['path']} resolves an SDK path without nros_build_paths", file=sys.stderr)
        if raw:
            print(f"       raw env::var of: {', '.join(raw)}", file=sys.stderr)
        if hidden:
            print(f"       private helper(s) resolving a name themselves: "
                  f"{', '.join(hidden)}", file=sys.stderr)
        print("       An inherited absolute path outranks the checkout you are", file=sys.stderr)
        print("       building (issue 1280): another checkout's value wins and", file=sys.stderr)
        print("       the build compiles ITS sources. Use", file=sys.stderr)
        print("       `nros_build_paths::env_path(<NAME>)` — it applies the", file=sys.stderr)
        print("       three-valued rule and canonicalises (keeping issue 0491's", file=sys.stderr)
        print("       reason for these helpers). If the raw read is deliberate,", file=sys.stderr)
        print("       say why: `// nros-build-paths-exempt: <reason>`.", file=sys.stderr)

    if fail:
        return 1

    print(f"check-build-script-path-resolution: OK — {checked} build script(s) name a "
          f"path-valued SDK variable ({len(path_vars)} such variables, read from "
          f"just/sdk-env.just), every one routed through nros_build_paths"
          + (f"; {exempted} exempted with a reason." if exempted else "."))
    return 0


if __name__ == "__main__":
    sys.exit(main())
