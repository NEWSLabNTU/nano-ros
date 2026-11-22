#!/usr/bin/env python3
"""Every Zephyr patch script a patch set runs has a CHECK mode (issue 2831).

`check-zephyr-pin-drift.py` asks a workspace whether it carries this checkout's
edits by running the line's set under `NROS_PATCH_CHECK=1`: each patch script
calls `nros_patch_would_apply` (scripts/lib/zephyr-patchset.sh) before each
write, and in check mode that exits 1, naming the missing edit, instead of
writing. A script with no call ignores check mode and APPLIES its edit, to a
workspace every session on the host shares. The probe refuses such a set at
run time; this gate refuses it at review, so a new patch cannot arrive
without one.

Checked: every `scripts/zephyr/*.sh` named by a `scripts/zephyr/patches/*.sh`
set, except the scripts that edit no tree (`NOT_TREE_EDITS`), calls the
helper. Presence only: that EVERY write is preceded by a call is what each
script's own check run proves (the probe's self-test proves the mechanism).

Run:  python3 scripts/check-zephyr-patch-check-mode.py [--self-test]
"""
from __future__ import annotations

import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
NOT_TREE_EDITS = {"provision-py312-venv.sh"}
CALL = re.compile(r"^\s*[^#\n]*\bnros_patch_would_apply\b", re.M)


def named(set_text: str) -> set[str]:
    """The scripts a set RUNS: named on a non-comment line."""
    code = "\n".join(l for l in set_text.splitlines() if not l.lstrip().startswith("#"))
    return set(re.findall(r"scripts/zephyr/([A-Za-z0-9._-]+\.sh)", code))


def offenders(sets: dict[str, str], scripts: dict[str, str]) -> list[str]:
    out = []
    for set_name, text in sorted(sets.items()):
        for n in sorted(named(text) - NOT_TREE_EDITS):
            body = scripts.get(n)
            if body is None:
                out.append(f"{set_name}: names scripts/zephyr/{n}, which does not exist")
            elif not CALL.search(body):
                out.append(f"{set_name}: scripts/zephyr/{n} never calls nros_patch_would_apply")
    return out


def self_test() -> None:
    sets = {"3.7.sh": "# see scripts/zephyr/setup.sh\nbash ./scripts/zephyr/a-patch.sh\n"
            "bash ./scripts/zephyr/provision-py312-venv.sh\n"}
    good = {"a-patch.sh": 'nros_patch_would_apply a "$f"\necho x >> "$f"\n'}
    assert offenders(sets, good) == []
    # negative control: the call only in a comment does not count
    assert len(offenders(sets, {"a-patch.sh": "# nros_patch_would_apply\necho x\n"})) == 1
    assert len(offenders(sets, {})) == 1
    print("check-zephyr-patch-check-mode self-test: OK (3 cases)")


def main() -> int:
    self_test()
    if "--self-test" in sys.argv:
        return 0
    sets = {p.name: p.read_text() for p in sorted((REPO / "scripts/zephyr/patches").glob("*.sh"))}
    scripts = {p.name: p.read_text() for p in (REPO / "scripts/zephyr").glob("*.sh")}
    bad = offenders(sets, scripts)
    if bad:
        print("check-zephyr-patch-check-mode: FAIL")
        for b in bad:
            print(f"  {b}")
        print("  Call `nros_patch_would_apply <tag> <what>` before each write (scripts/lib/"
              "zephyr-patchset.sh); without it, a check of a shared workspace writes to it.")
        return 1
    n = len(set().union(*(named(t) for t in sets.values())) - NOT_TREE_EDITS)
    print(f"check-zephyr-patch-check-mode: OK — {n} patch script(s) across {len(sets)} set(s), "
          "each with a check mode")
    return 0


if __name__ == "__main__":
    sys.exit(main())
