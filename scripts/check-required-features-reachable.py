#!/usr/bin/env python3
"""Issue 0652 — a `required-features` target that no recipe enables is invisible.

Cargo skips a `[[test]]` whose `required-features` are off. Silently: it is not
reported as filtered, it simply is not built. So a target behind a feature no
recipe enables is indistinguishable from a deleted one — except that it still
LOOKS like coverage when you read the tests directory, which is how one of them
sat failing without anyone noticing.

This is issue 0319's shape one level down. There the gate existed and the lane
did not run it; here the lane would run it and the TARGET is unreachable. Both
end the same way: a green line that stands for nothing.

WHAT THIS CHECKS

Every `required-features` value declared anywhere in the workspace appears in at
least one `just` recipe as a feature actually enabled. Not "is mentioned" — the
word must appear in a `--features`/`features = ` position, because `rmw` occurs
250 times in the justfiles as a substring of `rmw-zenoh`, `check-rmw-*` and
friends while being enabled as a feature exactly never.

WHY A BASELINE

Five features are unreachable today, covering nine test targets. Gating them all
at once would fail on day one and get bypassed, so they are listed as a
SHRINKING BACKLOG — the same shape `check-leaf-lockfiles` uses and says out
loud: a baseline is not a permanent exemption. What the gate buys immediately is
that the SIXTH one cannot be added silently.
"""

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

# Unreachable today. Remove an entry when its targets join a lane — or when the
# targets are deleted, which for an obsolete test is the more honest of the two.
#
# Nine targets hide behind these:
#   trigger-test            trigger_conditions, wake_latency
#   component-runtime-test  component_runtime, tier_filter, component_dispatch,
#                           component_param
#   loan-e2e                loan_e2e
#   phase216-substrate      dispatch_strategy
#   rmw                     custom_transport_loopback
BASELINE = {
    "trigger-test",
    "component-runtime-test",
    "loan-e2e",
    "phase216-substrate",
    "rmw",
}

# `--features a,b`, `--features "a b"`, `features = ["a"]`, `--all-features`.
FEATURE_CONTEXT = re.compile(
    r"--features[= ]\s*\"?([A-Za-z0-9_,\- ]+)\"?|features\s*=\s*\[([^\]]*)\]"
)


def declared_required_features() -> dict[str, list[str]]:
    """feature -> manifests declaring a target behind it."""
    out: dict[str, list[str]] = {}
    listing = subprocess.run(
        ["git", "-C", str(ROOT), "ls-files", "--", "*Cargo.toml"],
        capture_output=True, text=True, check=True,
    ).stdout.split()
    for rel in listing:
        try:
            text = (ROOT / rel).read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        for line in text.splitlines():
            stripped = line.strip()
            if stripped.startswith("#") or "required-features" not in stripped:
                continue
            for feat in re.findall(r'"([^"]+)"', stripped):
                out.setdefault(feat, []).append(rel)
    return out


def features_enabled_by_recipes() -> set[str]:
    enabled: set[str] = set()
    files = [ROOT / "justfile"] + sorted((ROOT / "just").glob("*.just"))
    for path in files:
        try:
            text = path.read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        if "--all-features" in text:
            # Enables everything, including targets nobody named.
            return {"*"}
        for m in FEATURE_CONTEXT.finditer(text):
            blob = m.group(1) or m.group(2) or ""
            for feat in re.split(r"[,\s\"]+", blob):
                if feat:
                    enabled.add(feat)
    return enabled


def main() -> int:
    declared = declared_required_features()
    if not declared:
        sys.stderr.write(
            "[FAIL] no `required-features` targets found — this gate would pass\n"
            "       vacuously. Either they are all gone (delete this check) or the\n"
            "       manifest scan broke.\n"
        )
        return 1

    enabled = features_enabled_by_recipes()
    wildcard = "*" in enabled

    unreachable = {
        f: m for f, m in declared.items() if not wildcard and f not in enabled
    }
    new = {f: m for f, m in unreachable.items() if f not in BASELINE}
    fixed = sorted(BASELINE - set(unreachable))

    rc = 0
    if new:
        sys.stderr.write(
            "[FAIL] `required-features` value(s) that no recipe enables (issue 0652):\n"
        )
        for feat, manifests in sorted(new.items()):
            sys.stderr.write(f"         {feat}  ({', '.join(sorted(set(manifests)))})\n")
        sys.stderr.write(
            "\n       Cargo skips such a target SILENTLY — it is not reported as\n"
            "       filtered, it is simply never built, so it looks like coverage\n"
            "       while running nothing. Put it in a lane, or delete the target.\n"
        )
        rc = 1
    if fixed:
        sys.stderr.write(
            "[FAIL] baselined feature(s) now reachable — remove them from BASELINE\n"
            "       in this script; it is a shrinking backlog, not an exemption:\n"
        )
        for feat in fixed:
            sys.stderr.write(f"         {feat}\n")
        rc = 1

    if rc == 0:
        n = len(declared)
        sys.stderr.write("")
        print(
            f"required-features reachable: OK ({n} declared, "
            f"{len(BASELINE)} baselined backlog)"
        )
    return rc


if __name__ == "__main__":
    sys.exit(main())
