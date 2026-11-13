#!/usr/bin/env python3
"""Every reader of a bringup's or a leaf's capability `features` routes through
`nros_orchestration_ir::node_interfaces::effective_features`.

phase-556 W13. Which node interfaces an image gets (the six parameter services,
`/rosout`) is decided in ONE function, so the default can change in one
constant and an image's `node_interfaces = "stock" | "none"` switch reaches
every road: the overlay beside a model, the leaf reader, the CLI's strict
`SystemToml`, and what they carry onward (the sizing descriptor's
`infra_families`, the Zephyr Kconfig symbols, `NROS_FEATURES`). A reader that
takes the DECLARED list straight off the file or the parsed struct builds the
image without the switch, silently: it agrees with the routed readers on every
image that states no switch and diverges on the first one that does.

What counts as a read of the declared list (outside `#[cfg(test)]` code and
`//` comments):

  * `.system.features`            the strict parser's `[system] features`
  * `overlay.features` / `overlay.images`   the overlay's declared lists
  * `.declared_features`          the leaf reader's pre-switch list
  * a raw `.get("features")` within three lines after a `.get("system")`,
    `.get("image_defaults")` or `.get("image")`, i.e. a `system.toml` table

A read is ROUTED when `effective_features(` appears within WINDOW lines of it
(the read feeds the call), or when it sits in `node_interfaces.rs`. Anything
else must be in ALLOWED with a reason, and every ALLOWED entry must still
match, so the list cannot outlive the code it excuses.
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent

HOME = "packages/core/nros-orchestration-ir/src/node_interfaces.rs"
WINDOW = 12

FIELD_READS = [
    (re.compile(r"\.system\s*\.\s*features\b"), "`SystemToml.system.features`"),
    (re.compile(r"\boverlay\s*\.\s*(features|images)\b"), "the overlay's declared lists"),
    (re.compile(r"\.declared_features\b"), "`LeafSystem.declared_features`"),
]
TABLE = re.compile(r"\.get\(\s*\"(system|image_defaults|image)\"\s*\)")
FEATURES_KEY = re.compile(r"\.get\(\s*\"features\"\s*\)")
ROUTED = re.compile(r"\beffective_features\s*\(")

# (path, a substring of the offending line) -> reason. Each must still match.
ALLOWED: dict[tuple[str, str], str] = {
    # The overlay STORES the declared list (from `system.toml`, and back from
    # the `nros.toml` it wrote) beside the switch; its two readers,
    # `system_features` and `features_for_image`, are the routed ones.
    (
        "packages/core/nros-orchestration-ir/src/overlay.rs",
        'and_then(|s| s.get("features"))',
    ): "the overlay's parsers store the declared list; its readers route",
    # `with_image_features` merges the image's declared list into the
    # system's and resolves the image's switch; `effective_features()` (the
    # routed reader) applies it to the merged list.
    (
        "packages/cli/nros-cli-core/src/orchestration/cargo_metadata_schema.rs",
        "self.system.features.contains(f)",
    ): "a merge of declared lists; the switch is applied by the routed reader",
    (
        "packages/cli/nros-cli-core/src/orchestration/cargo_metadata_schema.rs",
        "owned.system.features.extend(extra);",
    ): "a merge of declared lists; the switch is applied by the routed reader",
    # A source REWRITE: it asks whether the file's own list names an axis so
    # it can add the line, which is a question about the text, not the image.
    (
        "packages/cli/nros-cli-core/src/cmd/migrate_bringup.rs",
        'and_then(|s| s.get("features"))',
    ): "a rewrite of the file's own list, not a build decision",
}


def code_of(line: str) -> str:
    i = line.find("//")
    return line if i < 0 else line[:i]


def non_test_lines(text: str) -> list[str]:
    """The lines before the first `#[cfg(test)]` that opens a `mod`. Test
    modules sit at the end of a file here; a test constructs and inspects the
    declared list on purpose."""
    lines = text.splitlines()
    for i, line in enumerate(lines):
        if line.strip() == "#[cfg(test)]":
            nxt = next((l for l in lines[i + 1:] if l.strip()), "")
            if re.match(r"\s*(pub(\([^)]*\))?\s+)?mod\s", nxt):
                return lines[:i]
    return lines


def scan_text(text: str) -> list[tuple[int, str, str]]:
    """(line number, line, why) for every UNROUTED read."""
    lines = [code_of(l) for l in non_test_lines(text)]
    hits = []
    for n, code in enumerate(lines):
        why = None
        for rx, w in FIELD_READS:
            if rx.search(code):
                why = w
                break
        if why is None and FEATURES_KEY.search(code):
            lo = max(0, n - 3)
            if any(TABLE.search(l) for l in lines[lo:n + 1]):
                why = "a raw `features` key of a `system.toml` table"
        if why is None:
            continue
        lo, hi = max(0, n - WINDOW), min(len(lines), n + WINDOW + 1)
        if any(ROUTED.search(l) for l in lines[lo:hi]):
            continue
        hits.append((n + 1, code.strip(), why))
    return hits


def tracked_rust() -> list[str]:
    out = subprocess.run(
        ["git", "ls-files", "-z", "--cached", "--others", "--exclude-standard", "--",
         "packages/**/*.rs", "packages/*.rs"],
        cwd=REPO, check=True, capture_output=True,
    ).stdout.decode()
    return sorted(p for p in out.split("\0") if p and "/tests/" not in f"/{p}")


def self_test() -> bool:
    cases = [
        # A bypass of each shape is caught.
        ("fn f(s: &SystemToml) -> bool { s.system.features.is_empty() }", True),
        ("let f = &loaded.overlay.features;", True),
        ("let d = leaf.declared_features.clone();", True),
        ('let v = doc.get("system")\n    .and_then(|s| s.get("features"));', True),
        # A routed read passes.
        ("let e = effective_features(&s.system.features, s.system.node_interfaces);", False),
        ('let v = doc.get("system").and_then(|s| s.get("features"));\n'
         "let e = node_interfaces::effective_features(&v, sw);", False),
        # Not a system.toml read: a Cargo manifest's `[features]`.
        ('let f = manifest.get("features").and_then(|f| f.as_table());', False),
        # A comment and a test module are not readers.
        ("// s.system.features is the declared list", False),
        ("fn x() {}\n#[cfg(test)]\nmod tests {\n    fn t() { s.system.features.clear(); }\n}", False),
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
        print("check-node-interface-readers: self-test FAILED")
        return 1
    files = tracked_rust()
    if not files:
        print("check-node-interface-readers: examined 0 Rust files, refusing to pass")
        return 1
    if HOME not in files:
        print(f"check-node-interface-readers: {HOME} is gone; the gate has no subject")
        return 1
    allowed = ALLOWED
    used: set[tuple[str, str]] = set()
    bad: list[str] = []
    for rel in files:
        if rel == HOME:
            continue
        try:
            text = (REPO / rel).read_text(encoding="utf-8")
        except (OSError, UnicodeDecodeError):
            continue
        for n, line, why in scan_text(text):
            key = next((k for k in allowed if k[0] == rel and k[1] in line), None)
            if key is not None:
                used.add(key)
                continue
            bad.append(f"  {rel}:{n}: {why}: {line}")
    stale = sorted(set(allowed) - used)
    if bad or stale:
        print("check-node-interface-readers: FAIL\n")
        if bad:
            print("A reader takes the declared capability list without the node-interface")
            print("switch. Route it through")
            print("`nros_orchestration_ir::node_interfaces::effective_features` (or a reader")
            print("that already does: `Overlay::features_for_image`, `LoadedModel::features`,")
            print("`LeafSystem::features`, `SystemToml::capability_enabled` /")
            print("`effective_features`), or add an ALLOWED entry with a reason:\n")
            print("\n".join(bad))
        if stale:
            print("\nALLOWED entries that match nothing any more; delete them:")
            for p, s in stale:
                print(f"  {p}: {s!r}")
        return 1
    print(f"check-node-interface-readers: OK ({len(files)} Rust file(s); "
          f"{len(allowed)} allowlisted read(s), each with a reason)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
