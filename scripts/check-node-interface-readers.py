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

A read is ROUTED when `effective_features(` or `image_features(` appears
within WINDOW lines of it (the read feeds the call), or when it sits in
`node_interfaces.rs`. Anything else must be in ALLOWED with a reason, and every
ALLOWED entry must still match, so the list cannot outlive the code it excuses.

Issue 2700 widened the rule from "through the switch" to "for THE IMAGE": an
image's own `features` and `node_interfaces` reached some roads and not
others, so a generated workspace entry built and sized from the system list.
Two more shapes are refused:

  * an IMAGE-LESS LOAD -- a call that reads a model (or a system) for an image
    (`LoadedModel::with_overlay_beside`, `LoadedModel::load`, `load_model`,
    `plan_from_model`, `leaf_model`, `leaf_env`, `with_leaf_frame`,
    `with_image_features`) with a literal `None` as an argument. Every switch
    reader downstream of such a load answers for the system list. A load that
    reads topology only says so in IMAGELESS_ALLOWED, with a reason.
  * an UNSCOPED DERIVATION -- the facade's `derive(` (the cargo features a
    selection builds with) with no `with_image_features(` within WINDOW lines
    above it.
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
ROUTED = re.compile(r"\b(effective_features|image_features)\s*\(")

# Issue 2700 -- loads that bind a model or a system to an image.
IMAGE_LOADS = re.compile(
    r"(?<![\w:])(?:\w+::)*(?:LoadedModel::with_overlay_beside|LoadedModel::load|"
    r"load_model|plan_from_model|leaf_model|leaf_env|with_leaf_frame|"
    r"with_image_features)\s*\("
)
DERIVE = re.compile(r"\}\s*=\s*derive\(")
SCOPED = re.compile(r"\bwith_image_features\s*\(")

# Issue 2700 -- (path, a substring of the call's first line) -> reason, for an
# image-less load that reads no capability switch. Each must still match.
IMAGELESS_ALLOWED: dict[tuple[str, str], str] = {
    (
        "packages/cli/nros-cli-core/src/orchestration/probe_seeds.rs",
        "load_model(&path, None)",
    ): "launch values only: the probe seeds read each instance's parameters, never a switch",
    (
        "packages/core/nros-orchestration-ir/src/router_rule.rs",
        "LoadedModel::load(model_path, None)",
    ): "topic rates only: the router rule reads topology, never a switch",
    (
        "packages/cli/nros-cli-core/src/cmd/plan.rs",
        "load_model(model_path, None)",
    ): "the plan record is topology",
    (
        "packages/cli/nros-cli-core/src/cmd/ws.rs",
        "load_model(&model_path, None)",
    ): "the bridge plan record is topology",
    (
        "packages/cli/nros-build/src/lib.rs",
        "load_model(&model_path, None)",
    ): "the plan record is topology",
    (
        "packages/cli/nros-cli-core/src/codegen/entry/mod.rs",
        "load_model(&plan.launch_file, None)",
    ): "sched tasks, spin periods and blocking calls: the plan already holds the switches",
    (
        "packages/cli/nros-cli-core/src/cmd/build.rs",
        "plan_from_model(model_path, board, None)",
    ): "the launch closure only (which packages), never a switch",
    (
        "packages/cli/nros-cli-core/src/cmd/build.rs",
        "plan_from_model(&model, image.board.clone(), None)",
    ): "the launch closure only (which packages), never a switch",
    (
        "packages/cli/nros-cli-core/src/cmd/build.rs",
        "LoadedModel::with_overlay_beside(model, &path, None)",
    ): "a shared cmake configure names no single image; its own producers read "
       "none, and this precomposition must match what they read",
}

# (path, a substring of the offending line) -> reason. Each must still match.
ALLOWED: dict[tuple[str, str], str] = {
    # The overlay STORES the declared list (from `system.toml`, and back from
    # the `nros.toml` it wrote) beside the switch; its two readers,
    # `system_features` and `features_for_image`, are the routed ones.
    (
        "packages/core/nros-orchestration-ir/src/overlay.rs",
        'and_then(|s| s.get("features"))',
    ): "the overlay's parsers store the declared list; its readers route",
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
    """The lines before the first `#[cfg(test)]` that opens an INLINE `mod`
    (`mod tests {`). Test modules sit at the end of a file here; a test
    constructs and inspects the declared list on purpose.

    A `#[cfg(test)] mod golden;` DECLARATION is not one: it names another file
    and the code after it is the road. Cutting there hid the whole of
    `codegen/entry/mod.rs` (the C/C++ entry's switch reader) from this gate
    (issue 2700)."""
    lines = text.splitlines()
    for i, line in enumerate(lines):
        if line.strip() == "#[cfg(test)]":
            nxt = next((l for l in lines[i + 1:] if l.strip()), "")
            if re.match(r"\s*(pub(\([^)]*\))?\s+)?mod\s+\w+\s*\{", nxt):
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


def call_args(text: str, start: int) -> tuple[list[str], int]:
    """The top-level arguments of the call whose `(` is at `start`, and the
    index past its `)`."""
    depth, args, cur, i = 0, [], [], start
    while i < len(text):
        c = text[i]
        if c in "([{":
            depth += 1
            if depth > 1:
                cur.append(c)
        elif c in ")]}":
            depth -= 1
            if depth == 0:
                args.append("".join(cur).strip())
                return args, i + 1
            cur.append(c)
        elif c == "," and depth == 1:
            args.append("".join(cur).strip())
            cur = []
        elif depth >= 1:
            cur.append(c)
        i += 1
    return args, i


def scan_imageless(text: str) -> list[tuple[int, str, str]]:
    """(line number, first line, why) for every image-less load and every
    unscoped derivation (issue 2700)."""
    lines = [code_of(l) for l in non_test_lines(text)]
    code = "\n".join(lines)
    hits = []
    for m in IMAGE_LOADS.finditer(code):
        line_start = code.rfind("\n", 0, m.start()) + 1
        if re.search(r"\bfn\s+$", code[line_start:m.start()]):
            continue
        args, _ = call_args(code, m.end() - 1)
        if "None" in args:
            n = code.count("\n", 0, m.start())
            hits.append((n + 1, lines[n].strip(), "an image-less load"))
    for n, line in enumerate(lines):
        if DERIVE.search(line):
            lo = max(0, n - WINDOW)
            if not any(SCOPED.search(l) for l in lines[lo:n + 1]):
                hits.append((n + 1, line.strip(),
                             "a selection derived without `with_image_features`"))
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
        # A test module DECLARED from another file cuts nothing.
        ("#[cfg(test)]\nmod golden;\nfn f(s: &SystemToml) -> bool { s.system.features.is_empty() }",
         True),
        ("fn x() {}\n#[cfg(test)]\nmod tests {\n    fn t() { s.system.features.clear(); }\n}", False),
        # Issue 2700: the composition routes through `image_features` too.
        ("let f = image_features(&self.system.features, &img, sw);", False),
    ]
    imageless = [
        # An image-less load of each shape is caught, on one line or several.
        ("let m = LoadedModel::with_overlay_beside(model, &p, None)?;", True),
        ("let m = crate::overlay::LoadedModel::load(p, None)?;", True),
        ("let m = model_ingest::load_model(\n    &p,\n    None,\n)?;", True),
        ("let p = plan_from_model(&m, Some(b.into()), None)?;", True),
        ("let m = leaf_entity_env::leaf_model(leaf, None);", True),
        ("let s = sys.with_image_features(None);", True),
        # A load that names the image passes, as does `None` nested deeper.
        ("let m = load_model(p, Some(id))?;", False),
        ("let m = load_model(p, args.image.as_deref())?;", False),
        ("let m = load_model(p, img.or(None))?;", False),
        # A definition is not a call.
        ("pub fn load_model(path: &Path, image: Option<&str>) -> R {", False),
        # An unscoped derivation is caught; a scoped one passes.
        ("let Derived {\n    a,\n} = derive(\n    rmws,\n    sys,\n)?;", True),
        ("let image_sys = sys.with_image_features(id);\nlet Derived {\n    a,\n} = derive(\n"
         "    rmws,\n    &image_sys,\n)?;", False),
        # A test module is not a road.
        ("fn x() {}\n#[cfg(test)]\nmod tests {\n    fn t() { load_model(p, None); }\n}", False),
    ]
    ok = True
    for text, want in cases:
        got = bool(scan_text(text))
        if got != want:
            print(f"  self-test: {text!r}: expected {'a hit' if want else 'no hit'}")
            ok = False
    for text, want in imageless:
        got = bool(scan_imageless(text))
        if got != want:
            print(f"  self-test (image): {text!r}: expected {'a hit' if want else 'no hit'}")
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
    used_imageless: set[tuple[str, str]] = set()
    bad: list[str] = []
    bad_imageless: list[str] = []
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
        for n, line, why in scan_imageless(text):
            key = next((k for k in IMAGELESS_ALLOWED if k[0] == rel and k[1] in line), None)
            if key is not None:
                used_imageless.add(key)
                continue
            bad_imageless.append(f"  {rel}:{n}: {why}: {line}")
    stale = sorted(set(allowed) - used) + sorted(set(IMAGELESS_ALLOWED) - used_imageless)
    if bad_imageless:
        print("check-node-interface-readers: FAIL\n")
        print("A build or sizing road reads a model or a system for NO image, so an")
        print("image's own `features` and `node_interfaces` never reach it. Pass the")
        print("image being built (`Some(image_id)`, `args.image`), scope the system")
        print("with `SystemToml::with_image_features`, or, for a load that reads")
        print("topology only, add an IMAGELESS_ALLOWED entry with a reason:\n")
        print("\n".join(bad_imageless))
        print()
    if bad or stale or bad_imageless:
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
          f"{len(allowed)} allowlisted read(s) and {len(IMAGELESS_ALLOWED)} "
          f"image-less load(s), each with a reason)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
