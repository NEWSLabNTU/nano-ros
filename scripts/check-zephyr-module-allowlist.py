#!/usr/bin/env python3
"""The west manifests' module allowlists must be exactly what the index declares.

phase-447 F1, issue 1275, RFC-0099 D10.

WHAT THIS IS FOR
----------------
The Zephyr module set used to live in ONE place that was not the index:
`west.yml`'s `name-allowlist`. `nros-sdk-index.toml` had zero `hal_` mentions,
so `west update` fetched `hal_nxp` (1.3 G), `hal_stm32` (764 M) and `hal_nordic`
(224 M) for silicon no board in this repo targets, and `nros setup zephyr
--dry-run` could not price one byte of it — the cost was in a file the
provisioner never opened. D10 says the provisioner reads manifest/index files as
its SSoT; for this set it did not.

`[zephyr_module.*]` in the index is the SSoT now and the manifests are DERIVED:

    a module appears in <manifest>'s name-allowlist
      IFF  its `lines` contains that manifest's Zephyr line

This gate asserts that in BOTH directions, for every line. Both directions
matter and they catch different mistakes:

  * index -> manifest catches "declared but never fetched" — an author adds a
    `[zephyr_module.foo]` with `lines = ["3.7"]`, `--dry-run` prices it, and
    `west update` never pulls it, so the price is fiction.
  * manifest -> index catches the ORIGINAL defect, which is the one that
    actually happened: a module in the manifest that the index has never heard
    of. That is `hal_nxp` for as long as this repo has existed.

WHY THE MANIFESTS ARE NOT GENERATED
-----------------------------------
They stay committed files. `west init -m <url>` reads `west.yml` out of a bare
clone of this repo, before any `nros` exists to generate anything, and
`examples/templates/zephyr-byo/west.yml` shows a downstream doing exactly that.
A generated manifest would be absent at the one moment it is needed. So the
index is the SSoT and this gate is what makes the mirror honest, which is the
same trade `check-abi-bindings` makes for the committed bindgen output.

WHAT IT DOES NOT CHECK
----------------------
Whether a `needed_by` board really needs the module. That is a BUILD question —
phase-447 F1 answered it by building a native_sim and an mps2_an385 leaf against
the narrowed manifest — and a gate that grepped for it would be asserting the
weaker thing while reading as if it asserted the stronger one. `approx_mb`'s
presence for a fetched module is checked by `SdkIndex::validate`, not here.

Run:  python3 scripts/check-zephyr-module-allowlist.py [--self-test]
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
INDEX = REPO / "nros-sdk-index.toml"

# Mirrors `ZEPHYR_MANIFEST_LINES` in
# `packages/cli/nros-cli-core/src/orchestration/sdk_index.rs`. Two spellings of
# a two-entry table is a real duplication, so it is ASSERTED rather than
# trusted: `_read_cli_lines()` parses the Rust constant and the check below
# fails if the two disagree. A new Zephyr line therefore cannot be added on one
# side only — which is the shape of defect this whole gate exists to stop, one
# level up.
LINE_TO_MANIFEST = {"3.7": "west.yml", "4.4": "west-4.4.yml"}

CLI_CONST = REPO / "packages/cli/nros-cli-core/src/orchestration/sdk_index.rs"


def _read_cli_lines(text: str) -> dict:
    """Parse `ZEPHYR_MANIFEST_LINES` out of the Rust source."""
    m = re.search(
        r"ZEPHYR_MANIFEST_LINES:\s*&\[\(&str,\s*&str\)\]\s*=\s*&\[(.*?)\];",
        text,
        re.S,
    )
    if not m:
        return {}
    return dict(re.findall(r'\("([^"]+)",\s*"([^"]+)"\)', m.group(1)))


def parse_index_modules(text: str) -> dict:
    """`[zephyr_module.<name>]` -> its `lines` list.

    A hand parser rather than `tomllib`, for one reason that is not taste:
    `tomllib` is 3.11+ and the gates run under whatever `python3` the host has.
    The shape read here is a fixed one the index authors by convention, and the
    self-test below pins it.
    """
    modules: dict[str, list] = {}
    current = None
    for raw in text.splitlines():
        line = raw.strip()
        if line.startswith("[") and line.endswith("]"):
            section = line[1:-1]
            if section.startswith("zephyr_module."):
                current = section[len("zephyr_module.") :]
                modules[current] = []
            else:
                current = None
            continue
        if current is None:
            continue
        m = re.match(r"lines\s*=\s*\[(.*)\]\s*$", line)
        if m:
            modules[current] = re.findall(r'"([^"]+)"', m.group(1))
    return modules


def parse_allowlist(text: str) -> list:
    """The `name-allowlist:` entries of a west manifest, in order.

    Reads the block by INDENTATION rather than by "lines starting with `-`":
    a manifest has other list keys (`projects:`, `remotes:`), and a scan that
    stopped at the first non-`-` line would silently return a prefix of the
    allowlist whenever a comment sat inside it — which is exactly where these
    manifests keep their reasoning.
    """
    out: list = []
    indent = None
    for raw in text.splitlines():
        if indent is None:
            if raw.strip() == "name-allowlist:":
                indent = len(raw) - len(raw.lstrip())
            continue
        stripped = raw.strip()
        if not stripped:
            continue
        cur = len(raw) - len(raw.lstrip())
        if cur <= indent:
            break  # dedented out of the block
        if stripped.startswith("#"):
            continue
        m = re.match(r"-\s*(\S+)\s*$", stripped)
        if m:
            out.append(m.group(1))
    return out


def self_test() -> None:
    """Run on the NORMAL path, not behind a flag.

    A selftest nobody runs decays into a comment, and both readers here are the
    kind that fail toward OK: `parse_allowlist` returning `[]` for a manifest it
    could not understand would make every module look "correctly absent", and
    `parse_index_modules` returning `{}` would make every manifest entry look
    undeclared. Each fixture below is a mutation that a naive reader passes.
    """
    # A manifest whose allowlist has comments INSIDE it and another list after
    # it. A reader that stops at the first non-`-` line returns ["a"]; one that
    # takes every `- x` in the file returns the projects too.
    manifest = (
        "manifest:\n"
        "  projects:\n"
        "    - name: zephyr\n"
        "      import:\n"
        "        name-allowlist:\n"
        "          # Core\n"
        "          - a\n"
        "          # HALs\n"
        "          - b\n"
        "    - name: other\n"
        "  self:\n"
        "    path: nros\n"
    )
    got = parse_allowlist(manifest)
    assert got == ["a", "b"], f"parse_allowlist: {got!r}"

    # An index where a NON-module section also carries a `lines =` key. A
    # reader that does not reset `current` on leaving the class attributes it
    # to the previous module.
    index = (
        '[zephyr_module.a]\nwhy = "x"\nlines = ["3.7", "4.4"]\n\n'
        "[zephyr_module.b]\nlines = []\n\n"
        '[tool.q]\nlines = ["9.9"]\n'
    )
    got_idx = parse_index_modules(index)
    assert got_idx == {"a": ["3.7", "4.4"], "b": []}, f"parse_index_modules: {got_idx!r}"

    # An empty allowlist block must read as empty, not as "unparsed".
    assert parse_allowlist("name-allowlist:\nother: 1\n") == []
    # A manifest with NO allowlist at all is distinguishable only by the caller;
    # the reader returns [] for both, which is why main() requires the key.
    assert parse_allowlist("projects:\n  - name: zephyr\n") == []
    sys.stdout.write("check-zephyr-module-allowlist self-test: OK\n")


def main() -> int:
    self_test()
    if "--self-test" in sys.argv:
        return 0

    index_text = INDEX.read_text(encoding="utf-8")
    modules = parse_index_modules(index_text)
    if not modules:
        sys.stderr.write(
            "check-zephyr-module-allowlist: FAIL\n\n"
            "  nros-sdk-index.toml declares no `[zephyr_module.*]` at all.\n"
            "  That is the pre-phase-447 state RFC-0099 D10 describes: the\n"
            "  Zephyr module set living only in west.yml, unpriceable by\n"
            "  `nros setup zephyr --dry-run`. Restore the table.\n"
        )
        return 1

    # The two spellings of the line table must agree (see LINE_TO_MANIFEST).
    cli_lines = _read_cli_lines(CLI_CONST.read_text(encoding="utf-8"))
    if cli_lines != LINE_TO_MANIFEST:
        sys.stderr.write(
            "check-zephyr-module-allowlist: FAIL\n\n"
            "  The Zephyr manifest-line table disagrees between:\n"
            "    %s  ->  %r\n"
            "    %s  ->  %r\n"
            "  A line added on one side only means the other side silently\n"
            "  stops covering it. Update both.\n"
            % (
                CLI_CONST.relative_to(REPO),
                cli_lines,
                Path(__file__).name,
                LINE_TO_MANIFEST,
            )
        )
        return 1

    problems: list = []
    counts: list = []
    for line, manifest_name in sorted(LINE_TO_MANIFEST.items()):
        path = REPO / manifest_name
        text = path.read_text(encoding="utf-8")
        if "name-allowlist:" not in text:
            problems.append(
                "  %s has no `name-allowlist:` — with no allowlist west imports\n"
                "    EVERY Zephyr module (~90 of them), which is the cost this\n"
                "    table exists to bound." % manifest_name
            )
            continue
        allow = set(parse_allowlist(text))
        want = {name for name, lines in modules.items() if line in lines}

        for missing in sorted(want - allow):
            problems.append(
                "  %s: `[zephyr_module.%s]` declares lines containing %r, but the\n"
                "    module is NOT in %s's name-allowlist. `west update` will not\n"
                "    fetch it, so `nros setup zephyr --dry-run` prices a module\n"
                "    nobody gets. Add `- %s` to the allowlist, or drop %r from its\n"
                "    `lines`."
                % (manifest_name, missing, line, manifest_name, missing, line)
            )
        for extra in sorted(allow - want):
            known = extra in modules
            problems.append(
                "  %s: name-allowlist has `%s`, which the index does %s.\n"
                "    %s\n"
                "    The index is the SSoT (RFC-0099 D10) — a module west fetches\n"
                "    that the index has never heard of is issue 1275 exactly: an\n"
                "    unpriced download in a file the provisioner never opens."
                % (
                    manifest_name,
                    extra,
                    (
                        "not carry on line %r" % line
                        if known
                        else "not declare at all"
                    ),
                    (
                        "Add %r to `[zephyr_module.%s].lines`, or remove it here."
                        % (line, extra)
                        if known
                        else "Add `[zephyr_module.%s]` (why / needed_by / approx_mb /\n"
                        "    lines) to nros-sdk-index.toml, or remove it here." % extra
                    ),
                )
            )
        counts.append("%s: %d" % (manifest_name, len(allow)))

    if problems:
        sys.stderr.write("check-zephyr-module-allowlist: FAIL\n\n")
        sys.stderr.write("\n\n".join(problems))
        sys.stderr.write("\n")
        return 1

    withheld = sorted(n for n, lines in modules.items() if not lines)
    sys.stdout.write(
        "check-zephyr-module-allowlist: OK — %d module(s) declared, "
        "allowlists match the index (%s)%s\n"
        % (
            len(modules),
            "; ".join(counts),
            ("; withheld: " + ", ".join(withheld)) if withheld else "",
        )
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
