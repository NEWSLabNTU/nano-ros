#!/usr/bin/env python3
"""No nano-ros source reads a SystemModel field that is not the model's to carry.

phase-486 W3 (RFC-0060 amendment 2026-10-10, revised 2026-10-12). The
SystemModel holds what every realizer reads with the same meaning. Three of its
fields held facts only nano-ros reads, and rlm stopped writing them in
`v0.1.55` (its `docs/model-boundary.md`, W6):

- `execution.features` — nano-ros's capability switches. They are read from the
  overlay beside the model (`nros.toml`, `nros_orchestration_ir::overlay`),
  through `LoadedModel::features`.
- `Deploy.extra` — one realizer's build fields, plus the placing table's name.
  Nothing reads it.
- `Deploy.target` (`Target::Mcu` / `Target::Linux`) — the board a
  `[deploy.<n>] board` named. The two board-slice readers that consumed it
  (`codegen::entry` and `nros::main!`, issue 0358's duplicate) are deleted;
  placement is `overlay::placed_here`, which reads only membership.

And one rule about the lifecycle default: the image's single boot transition
comes from `overlay::reduce_lifecycle_autostart`, never from the first node's
value (`find_map(|n| n.lifecycle_autostart)`), which three readers used to
spell.

Before rlm `v0.1.55` reaches nano-ros (phase-486 W7) the fields are still
POPULATED, so a reader that slips back would work today and silently read
nothing after the bump. That is why this is a gate and not a review note.

ALLOWED lists the sites that may name a field, each with its reason. An entry
that no longer matches anything fails too, so the list cannot rot into
permission for code that is gone. The list empties at phase-486 W8.
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent

PATTERNS = [
    (re.compile(r"execution\s*\.\s*features\b"), "`execution.features` (read the overlay: `LoadedModel::features`)"),
    (re.compile(r"Execution::features\b"), "`Execution::features` (read the overlay)"),
    (re.compile(r"\.extra\s*(\.\s*(get|contains_key|insert|iter|is_empty|len|keys|values)\b|\[)"),
     "`Deploy.extra` (rlm no longer writes it; nothing may read it)"),
    (re.compile(r"\bTarget::(Mcu|Linux)\b"), "`Deploy.target` (`placed_here` is the placement rule)"),
    (re.compile(r"find_map\(\s*\|\s*\w+\s*\|\s*\w+\.lifecycle_autostart\s*\)"),
     "first-node-wins lifecycle (use `reduce_lifecycle_autostart`)"),
]

# (path, reason). Every entry must still match, or the gate fails.
ALLOWED: dict[str, str] = {
    "packages/core/nros-orchestration-ir/src/overlay.rs":
        "the migration guard: a model that still carries switches with no overlay "
        "beside it is refused, naming `nros sync` (phase-486 W3)",
    "packages/cli/nros-cli-core/src/orchestration/model_ingest.rs":
        "a TEST that writes `Deploy.target`/`extra` (`edf`) into a model to prove "
        "the EDF slicing no longer reads them (issue 0951)",
}


def code_of(line: str) -> str:
    """The line with a `//` comment removed. String literals are not parsed:
    a pattern inside one is rare enough that a reviewer should see it."""
    i = line.find("//")
    return line if i < 0 else line[:i]


def scan_text(text: str) -> list[tuple[int, str]]:
    hits = []
    for n, line in enumerate(text.splitlines(), 1):
        code = code_of(line)
        for rx, why in PATTERNS:
            if rx.search(code):
                hits.append((n, why))
    return hits


def tracked_rust() -> list[str]:
    out = subprocess.run(
        ["git", "ls-files", "-z", "--", "packages/**/*.rs", "packages/*.rs"],
        cwd=REPO, check=True, capture_output=True,
    ).stdout.decode()
    return sorted({p for p in out.split("\0") if p})


def self_test() -> bool:
    cases = [
        ("let f = &model.execution.features;", True),
        ("model . execution . features.iter()", True),
        ("d.extra.get(\"kind\")", True),
        ("dep.extra[\"profile\"]", True),
        ("Some(Target::Mcu { board })", True),
        (".find_map(|n| n.lifecycle_autostart)", True),
        ("// execution.features in a comment", False),
        ("let f = model.features(None);", False),
        ("let extra = BTreeMap::new();", False),
        ("model.lifecycle_autostart()?", False),
    ]
    ok = True
    for text, want in cases:
        got = bool(scan_text(text))
        if got != want:
            print(f"  self-test: {text!r}: expected {'a hit' if want else 'no hit'}")
            ok = False
    return ok


def main() -> int:
    if not self_test():
        print("check-launch-model-boundary: self-test FAILED")
        return 1
    files = tracked_rust()
    if not files:
        print("check-launch-model-boundary: examined 0 Rust files — refusing to pass")
        return 1
    bad: list[str] = []
    used: set[str] = set()
    for rel in files:
        try:
            text = (REPO / rel).read_text(encoding="utf-8")
        except (OSError, UnicodeDecodeError):
            continue
        hits = scan_text(text)
        if not hits:
            continue
        if rel in ALLOWED:
            used.add(rel)
            continue
        for n, why in hits:
            bad.append(f"  {rel}:{n}: {why}")
    stale = sorted(set(ALLOWED) - used)
    if bad or stale:
        print("check-launch-model-boundary: FAIL\n")
        if bad:
            print("A nano-ros reader names a field the SystemModel no longer carries for it")
            print("(phase-486, RFC-0060 amendment). Read the overlay through")
            print("`nros_orchestration_ir::overlay::LoadedModel` instead:\n")
            print("\n".join(bad))
        if stale:
            print("\nALLOWED entries that match nothing any more — delete them:")
            for s in stale:
                print(f"  {s}")
        return 1
    print(f"check-launch-model-boundary: OK ({len(files)} Rust file(s); "
          f"{len(ALLOWED)} allowlisted site(s), each with a reason)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
