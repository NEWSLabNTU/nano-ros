#!/usr/bin/env python3
"""A key the source-metadata emitter writes must be DECLARED by the reader —
issue 1522, and issue 0518 before it.

THE FAILURE, TWICE
------------------
`nros`'s `node_metadata.rs` writes each component's sidecar
(`metadata/<component>.json`); the CLI reads it back into the
`deny_unknown_fields` structs of
`nros-cli-core/src/orchestration/source_metadata.rs`. `deny_unknown_fields`
turns an undeclared key into a HARD parse failure of the whole document, four
frames from the writer and naming neither it nor the schema:

    Error: metadata harness emitted invalid JSON at .../listener.json
    Caused by: unknown field `in_place`, expected one of `id`, ...

* issue 0518 — `period_us` was added to the timer emitter, and every
  source-metadata parse failed outright until the field existed here. The
  struct's own doc comment records it.
* phase-457 W3 — `in_place` was added to the subscriber emitter and to ONE
  reader (`leaf_entity_env`), not this one. It sat undetected for a fortnight
  because no producer that reaches this reader emitted the key: the Rust probe
  observes no registration at all. Closing issue 1522 made the Rust road state
  the row, and the first `nros build` of `examples/native/rust/listener` after
  it failed exactly as above.

That second one is the argument for a gate rather than a third fix. The bug is
invisible in proportion to how new the emitting road is, so "we would have
noticed" is precisely what did not happen.

WHAT IT CHECKS
--------------
The emitter's per-endpoint writers pair to the reader's structs BY NAME —
`write_<x>_json` <-> `Source<CamelCase(x)>` — so nothing here is an authored
table of endpoints (an authored pairing is issue 0196's shape, and the point of
this file is the key somebody adds NEXT). For each pair, every JSON key the
writer emits DIRECTLY must be a field of the struct, counting
`#[serde(rename)]`.

REACH, STATED
-------------
Keys written directly by the paired function: `write_json_field(out, "k", ..)`
and a `"k":` inside its own `write!` format string. A key emitted by a shared
HELPER the writer calls (`write_interface`, `write_qos`, `write_source_name`)
is NOT harvested, because inlining a helper would also harvest its NESTED
object's keys, which belong to a different struct and would be false failures.
Both historical defects were direct writes; a helper gaining a top-level key is
the residual hole, and it is written here rather than left to be discovered.

NON-VACUITY. Zero pairs, a writer with no struct, or a pair yielding no keys is
a FAILURE — a scan that found nothing prints the same word as a scan that found
nothing wrong.

Buildless and offline; reads tracked source only.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent

EMITTER = REPO / "packages/api/nros/src/node_metadata.rs"
READER = REPO / "packages/cli/nros-cli-core/src/orchestration/source_metadata.rs"

_WRITER_FN = re.compile(r"^fn (write_([a-z0-9_]+)_json)\s*\(", re.M)
_JSON_FIELD = re.compile(r'write_json_field\(\s*out\s*,\s*"([A-Za-z0-9_]+)"')
_WRITE_MACRO = re.compile(r"write!\((.*?)\)\?;", re.S)
_KEY_IN_FORMAT = re.compile(r'\\"([A-Za-z0-9_]+)\\"\s*:')
_STRUCT = re.compile(r"pub struct (Source[A-Za-z0-9]*)\s*\{(.*?)\n\}", re.S)
_FIELD = re.compile(r"^\s*pub ([a-z0-9_]+)\s*:", re.M)
_RENAME = re.compile(r'rename\s*=\s*"([A-Za-z0-9_]+)"')

# Writers with no endpoint struct of their own. Kept EMPTY on purpose: a name
# that pairs to nothing is a failure, so this constant exists only to make that
# choice visible to the next reader.
UNPAIRED: frozenset[str] = frozenset()


def camel(snake: str) -> str:
    return "".join(part.capitalize() for part in snake.split("_"))


def body_of(text: str, start: int) -> str:
    """The `{...}` block that follows `start`, by brace counting."""
    open_at = text.find("{", start)
    if open_at < 0:
        return ""
    depth = 0
    for i in range(open_at, len(text)):
        if text[i] == "{":
            depth += 1
        elif text[i] == "}":
            depth -= 1
            if depth == 0:
                return text[open_at : i + 1]
    return ""


def emitted_keys(body: str) -> set[str]:
    keys = set(_JSON_FIELD.findall(body))
    for fmt in _WRITE_MACRO.findall(body):
        keys |= set(_KEY_IN_FORMAT.findall(fmt))
    return keys


def writers(emitter_src: str) -> dict[str, set[str]]:
    """`write_<x>_json` -> the keys it emits directly, keyed by `<x>`."""
    out: dict[str, set[str]] = {}
    for m in _WRITER_FN.finditer(emitter_src):
        out[m.group(2)] = emitted_keys(body_of(emitter_src, m.start()))
    return out


def structs(reader_src: str) -> dict[str, set[str]]:
    """`Source<X>` -> the JSON keys it accepts."""
    out: dict[str, set[str]] = {}
    for m in _STRUCT.finditer(reader_src):
        block = m.group(2)
        names = set(_FIELD.findall(block))
        names |= set(_RENAME.findall(block))
        out[m.group(1)] = names
    return out


def analyse(emitter_src: str, reader_src: str) -> list[str]:
    problems: list[str] = []
    ws = writers(emitter_src)
    ss = structs(reader_src)

    if not ws:
        return [
            f"  - no `write_<x>_json` writer found in "
            f"{EMITTER.relative_to(REPO)} — the emitter moved, and every "
            f"clause below would pass vacuously."
        ]
    if not ss:
        return [
            f"  - no `pub struct Source…` found in "
            f"{READER.relative_to(REPO)} — the reader moved."
        ]

    paired = 0
    for name, keys in sorted(ws.items()):
        if name in UNPAIRED:
            continue
        struct = f"Source{camel(name)}"
        if struct not in ss:
            problems.append(
                f"  - `write_{name}_json` pairs by name to `{struct}`, which "
                f"{READER.relative_to(REPO)} does not declare. Either the "
                f"struct was renamed (the pairing is DERIVED from the writer's "
                f"name, so rename both) or this writer's output reaches no "
                f"reader at all."
            )
            continue
        paired += 1
        if not keys:
            problems.append(
                f"  - `write_{name}_json` emits no key this gate can see, so "
                f"`{struct}` is checked against nothing. Its keys are written "
                f"through a helper, which is outside this gate's stated reach "
                f"— see the module docstring before widening it."
            )
            continue
        missing = sorted(keys - ss[struct])
        if missing:
            problems.append(
                f"  - `write_{name}_json` emits {missing}, which `{struct}` "
                f"does not declare. That struct is `deny_unknown_fields`, so "
                f"this is not an ignored key: EVERY source-metadata parse "
                f"fails with `unknown field`, naming neither the emitter nor "
                f"the schema version (issues 0518, 1522). Add the field "
                f"`#[serde(default, skip_serializing_if = …)]`, even if this "
                f"reader never uses it."
            )

    if paired == 0:
        problems.append(
            "  - no writer paired to a struct; the gate examined nothing."
        )
    return problems


def self_test() -> None:
    """Runs on the NORMAL path, against the real files, then mutates them."""
    emitter = EMITTER.read_text(encoding="utf-8")
    reader = READER.read_text(encoding="utf-8")

    # The exact regression this gate exists for: drop `in_place` from the
    # reader while the emitter keeps writing it.
    dropped = reader.replace("    pub in_place: Option<bool>,\n", "", 1)
    assert dropped != reader, "self-test premise gone: `in_place` field not found"
    assert analyse(emitter, dropped), (
        "a key the emitter writes and the reader does not declare must FAIL"
    )

    # Issue 0518's shape, one struct over.
    dropped_us = reader.replace("    pub period_us: Option<u64>,\n", "", 1)
    assert dropped_us != reader, "self-test premise gone: `period_us` field not found"
    assert analyse(emitter, dropped_us), "issue 0518's own defect must FAIL"

    # A renamed struct breaks the derived pairing rather than passing quietly.
    renamed = reader.replace("pub struct SourceSubscriber {", "pub struct SourceSub {", 1)
    assert analyse(emitter, renamed), "an unpairable writer must FAIL"

    # Non-vacuity, both sides.
    assert analyse("// no writers\n", reader), "zero writers must FAIL"
    assert analyse(emitter, "// no structs\n"), "zero structs must FAIL"


def main() -> int:
    for path in (EMITTER, READER):
        if not path.is_file():
            print(
                f"check-sidecar-endpoint-keys: {path.relative_to(REPO)} is "
                f"missing — the seam moved; this gate cannot answer.",
                file=sys.stderr,
            )
            return 1

    emitter = EMITTER.read_text(encoding="utf-8")
    reader = READER.read_text(encoding="utf-8")

    self_test()

    problems = analyse(emitter, reader)
    if problems:
        print(
            "check-sidecar-endpoint-keys: the source-metadata emitter writes a "
            "key its\n`deny_unknown_fields` reader does not declare — every "
            "parse of that sidecar\nfails outright (issues 0518, 1522):\n",
            file=sys.stderr,
        )
        print("\n".join(problems), file=sys.stderr)
        return 1

    ws = writers(emitter)
    total = sum(len(k) for k in ws.values())
    print(
        f"check-sidecar-endpoint-keys: OK — {len(ws)} endpoint writer(s), "
        f"{total} directly-written key(s), each declared by the "
        f"`Source…` struct its name pairs to."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
