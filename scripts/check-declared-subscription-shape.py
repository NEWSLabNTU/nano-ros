#!/usr/bin/env python3
"""The declarative registrar and the metadata probe must read ONE classifier —
issue 1522.

WHY A CLASSIFIER IS ALLOWED HERE AT ALL
---------------------------------------
phase-457 W3 made a subscription's `registration_path` an OBSERVATION.
`SubscriptionRequest::in_place_capable` is stated at each of the executor's
eleven registration entry points and read at one site,
`Executor::open_subscription`, which reports it through
`registration_observer`. What W3 DELETED was a second opinion: the CLI used to
compose that row from the entry's LANGUAGE, and measured against the eleven
entry points the inference was wrong for nine of them.

The Rust producer observes nothing — `record_node_metadata::<C>` runs a
component's `register()` against a recording `NodeContext` and opens no
executor — so every Rust endpoint's row refused. Its registrar, though, is ONE
function (`nros::node_runtime`'s `EntityKind::Subscription` arm) lowering to
one of two entry points, so the shape is a function of the DECLARATION. Issue
1522 takes that road: the registrar and the recorder read one classifier,
`DeclaredSubscriptionShape`.

That is cheaper than teaching the probe to register, and it is NOT a second
opinion **only if the classifier is the thing the registrar branches on**. This
gate is that "only if".

WHAT IT CHECKS, ALL OF IT DERIVED
---------------------------------
Nothing here is an authored table of entry points or lowerings — an authored
table is the shape issue 0196 keeps finding, and it is what W3 removed. Every
expectation is harvested from the enum itself:

1. VARIANTS. `DeclaredSubscriptionShape`'s variants, and for each one the
   `register_subscription_*` entry point and `create_generic_subscription*`
   lowering its own doc comment names. A variant documenting neither is a
   failure: the doc comment is load-bearing here, not prose.

2. THE VALUE HAS ONE DEFINITION. Inside each variant's documented entry point
   in `spin.rs`, the `SubscriptionRequest`'s `in_place_capable` must be spelled
   `DeclaredSubscriptionShape::<V>.in_place_capable()` — never a literal. That
   is what makes the bool the probe states and the bool the executor acts on
   the same expression, so issue 1340's candidate moves both together.

3. THE REGISTRAR BRANCHES ON IT. The `EntityKind::Subscription` arm must call
   `declared_subscription_shape()`, must name every variant, and must NOT test
   the declaration's `safety` flag itself — a second test of the same input is
   a second opinion with extra steps. The set of `create_generic_subscription*`
   calls in the arm must equal the set the variants document, so a THIRD
   lowering cannot fall through to the basic path while the probe keeps
   stating the basic path's answer.

4. THE RECORDER READS THE SAME CALL. `MetadataRecorder`'s `NodeRuntime::
   create_entity` — the one seam a Rust declaration crosses — must state
   `in_place_capable` from `declared_subscription_shape()`.

5. ONE MASK. `safety-e2e` off means the runtime ignores the flag, so the
   classifier must too. That `#[cfg]` lives in exactly one function
   (`EntityMetadata::declared_subscription_shape`), which must be the only
   caller of `DeclaredSubscriptionShape::of_declaration` outside tests.

6. NON-VACUITY. Zero variants, a missing file or an unfindable function is a
   FAILURE, not an OK — a scan that found nothing prints the same word as a
   scan that found nothing wrong.

Buildless and offline; reads tracked source only.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent

SHAPE_RS = REPO / "packages/core/nros-node/src/executor/declared_shape.rs"
SPIN_RS = REPO / "packages/core/nros-node/src/executor/spin.rs"
RUNTIME_RS = REPO / "packages/api/nros/src/node_runtime.rs"
RECORDER_RS = REPO / "packages/api/nros/src/node.rs"
METADATA_RS = REPO / "packages/api/nros/src/node_metadata.rs"

ENUM_NAME = "DeclaredSubscriptionShape"
CLASSIFIER = "declared_subscription_shape"

_ENUM_BLOCK = re.compile(
    r"pub enum " + ENUM_NAME + r"\s*\{(.*?)\n\}",
    re.S,
)
_DOC_LINE = re.compile(r"^\s*///(.*)$")
_VARIANT_LINE = re.compile(r"^\s*([A-Z][A-Za-z0-9]*)\s*,\s*$")
_ENTRY_POINT = re.compile(r"\bregister_subscription_[a-z0-9_]+\b")
_LOWERING = re.compile(r"\bcreate_generic_subscription[a-z0-9_]*\b")


def code_only(text: str) -> str:
    """`text` with `//` comments blanked out.

    Two clauses below are about what the code DOES (does it test the raw flag,
    what does it lower to), and prose that names a symbol in order to say
    "not this" must not read as doing it — which is exactly how the first
    version of this gate failed on the comment explaining itself.
    """
    return "\n".join(line.split("//", 1)[0] for line in text.splitlines())


def find_block(text: str, header: str) -> str | None:
    """The `{...}` body that follows the first occurrence of `header`.

    A brace counter, not a parser. Good enough for Rust bodies that hold no
    brace inside a string or char literal, which is true of every block this
    gate reads; a body that gains one shows up as an over-long or truncated
    block and fails a clause rather than passing quietly.
    """
    start = text.find(header)
    if start < 0:
        return None
    open_at = text.find("{", start)
    if open_at < 0:
        return None
    depth = 0
    for i in range(open_at, len(text)):
        if text[i] == "{":
            depth += 1
        elif text[i] == "}":
            depth -= 1
            if depth == 0:
                return text[open_at : i + 1]
    return None


def variants(shape_src: str) -> dict[str, dict[str, set[str]]]:
    """Variant name -> the entry point(s) and lowering(s) its doc names."""
    m = _ENUM_BLOCK.search(shape_src)
    if not m:
        return {}
    out: dict[str, dict[str, set[str]]] = {}
    doc: list[str] = []
    for line in m.group(1).splitlines():
        d = _DOC_LINE.match(line)
        if d:
            doc.append(d.group(1))
            continue
        v = _VARIANT_LINE.match(line)
        if v:
            blob = "\n".join(doc)
            out[v.group(1)] = {
                "entry_points": set(_ENTRY_POINT.findall(blob)),
                "lowerings": set(_LOWERING.findall(blob)),
            }
        doc = []
    return out


def analyse(
    shape_src: str,
    spin_src: str,
    runtime_src: str,
    recorder_src: str,
    metadata_src: str,
) -> list[str]:
    problems: list[str] = []

    vs = variants(shape_src)
    if not vs:
        return [
            f"  - no variants of `{ENUM_NAME}` found in "
            f"{SHAPE_RS.relative_to(REPO)} — the enum moved or was renamed, and "
            f"every clause below would pass vacuously."
        ]

    # ---- 1/2: each variant's documented entry point takes ITS value ---------
    documented_lowerings: set[str] = set()
    for name, refs in sorted(vs.items()):
        documented_lowerings |= refs["lowerings"]
        if not refs["entry_points"]:
            problems.append(
                f"  - `{ENUM_NAME}::{name}` names no `register_subscription_*` "
                f"entry point in its doc comment. That doc is what this gate "
                f"derives the expectation from, so an undocumented variant is "
                f"an unchecked one."
            )
            continue
        if not refs["lowerings"]:
            problems.append(
                f"  - `{ENUM_NAME}::{name}` names no "
                f"`create_generic_subscription*` lowering in its doc comment; "
                f"the registrar clause cannot be derived for it."
            )
        for entry in sorted(refs["entry_points"]):
            body = find_block(spin_src, f"fn {entry}")
            if body is None:
                problems.append(
                    f"  - `{ENUM_NAME}::{name}` documents entry point "
                    f"`{entry}`, which is not a `fn` in "
                    f"{SPIN_RS.relative_to(REPO)}."
                )
                continue
            wanted = re.compile(
                r"in_place_capable:\s*[\w:]*"
                + ENUM_NAME
                + r"::"
                + name
                + r"\s*\.in_place_capable\(\)"
            )
            if not wanted.search(body):
                problems.append(
                    f"  - `{entry}` does not spell its "
                    f"`SubscriptionRequest::in_place_capable` as "
                    f"`{ENUM_NAME}::{name}.in_place_capable()`. A literal there "
                    f"is a SECOND definition of the bool: the probe would go on "
                    f"stating the classifier's answer while the executor acted "
                    f"on another. Issue 1340's candidate must move both at once."
                )

    # ---- 3: the registrar branches on the classifier -----------------------
    arm = find_block(runtime_src, "EntityKind::Subscription => {")
    if arm is None:
        problems.append(
            f"  - no `EntityKind::Subscription => {{` arm in "
            f"{RUNTIME_RS.relative_to(REPO)} — the declarative registrar moved, "
            f"and the classifier is no longer known to be what it branches on."
        )
    else:
        if f"{CLASSIFIER}()" not in arm:
            problems.append(
                f"  - the `EntityKind::Subscription` arm does not call "
                f"`{CLASSIFIER}()`. The recorder states this endpoint's "
                f"`in_place_capable` from that call; a registrar that decides "
                f"some other way makes the probe a second opinion about a site "
                f"it cannot see (issue 0196's class, and what phase-457 W3 "
                f"removed)."
            )
        for name in sorted(vs):
            if f"{ENUM_NAME}::{name}" not in arm:
                problems.append(
                    f"  - the `EntityKind::Subscription` arm never names "
                    f"`{ENUM_NAME}::{name}`. Every shape must be handled there "
                    f"(a comment naming it counts, for the arm the cfg leaves "
                    f"out) — an unnamed variant falls through to the basic "
                    f"path while the probe states that variant's own answer."
                )
        arm_code = code_only(arm)
        if "metadata.safety" in arm_code:
            problems.append(
                "  - the `EntityKind::Subscription` arm tests `metadata.safety` "
                "directly. That input belongs to the classifier: a second test "
                "of it is a second opinion, and the `safety-e2e` mask then "
                "exists in two places that can disagree."
            )
        found = set(_LOWERING.findall(arm_code))
        extra = found - documented_lowerings
        if extra:
            problems.append(
                f"  - the `EntityKind::Subscription` arm lowers to "
                f"{sorted(extra)}, which no `{ENUM_NAME}` variant documents. A "
                f"new lowering is a new variant with its own "
                f"`in_place_capable()`; without one the probe keeps stating the "
                f"old shape's answer for it."
            )
        missing = documented_lowerings - found
        if missing:
            problems.append(
                f"  - the `EntityKind::Subscription` arm does not call "
                f"{sorted(missing)}, which a `{ENUM_NAME}` variant documents as "
                f"its lowering. Either the arm or the doc is stale, and the doc "
                f"is what this gate derives from."
            )

    # ---- 4: the recorder reads the same call -------------------------------
    create_entity = find_block(recorder_src, "fn create_entity(")
    if create_entity is None:
        problems.append(
            f"  - no `fn create_entity(` in {RECORDER_RS.relative_to(REPO)} — "
            f"the recorder's `NodeRuntime` seam moved."
        )
    elif f"{CLASSIFIER}()" not in create_entity or "in_place_capable" not in create_entity:
        problems.append(
            f"  - `MetadataRecorder`'s `create_entity` does not state "
            f"`in_place_capable` from `{CLASSIFIER}()`. That is the ONE seam a "
            f"Rust declaration crosses, and nothing observes this road — "
            f"without it every Rust endpoint's registration path refuses "
            f"(issue 1522)."
        )

    # ---- 5: one mask -------------------------------------------------------
    mask = find_block(metadata_src, f"pub fn {CLASSIFIER}(")
    if mask is None:
        problems.append(
            f"  - no `pub fn {CLASSIFIER}(` in "
            f"{METADATA_RS.relative_to(REPO)} — the `safety-e2e` mask has no "
            f"single home, so the registrar and the recorder each carry their "
            f"own `#[cfg]` and can disagree."
        )
    else:
        if 'feature = "safety-e2e"' not in mask:
            problems.append(
                f"  - `{CLASSIFIER}` does not apply the `safety-e2e` mask. With "
                f"the capability off the runtime IGNORES the declaration's "
                f"flag and registers the basic path, so a probe that does not "
                f"mask it states the wrong shape."
            )
        for rel, src in (
            (RUNTIME_RS, runtime_src),
            (RECORDER_RS, recorder_src),
        ):
            if "of_declaration(" in src:
                problems.append(
                    f"  - {rel.relative_to(REPO)} calls "
                    f"`{ENUM_NAME}::of_declaration` directly. The one caller is "
                    f"`{CLASSIFIER}`, which is where the mask lives; a second "
                    f"caller is a second mask."
                )
        outside = metadata_src.count("of_declaration(") - mask.count("of_declaration(")
        if outside > 0:
            problems.append(
                f"  - {METADATA_RS.relative_to(REPO)} calls "
                f"`of_declaration` {outside} time(s) outside `{CLASSIFIER}`; "
                f"the mask must have exactly one application."
            )

    return problems


def self_test() -> None:
    """Runs on the NORMAL path, against the real tree, then mutates it.

    A gate whose self-test builds only synthetic inputs answers "does my regex
    work", not "does the rule hold here" — and a mutation that the real text
    survives is the one that matters.
    """
    real = {
        "shape_src": SHAPE_RS.read_text(encoding="utf-8"),
        "spin_src": SPIN_RS.read_text(encoding="utf-8"),
        "runtime_src": RUNTIME_RS.read_text(encoding="utf-8"),
        "recorder_src": RECORDER_RS.read_text(encoding="utf-8"),
        "metadata_src": METADATA_RS.read_text(encoding="utf-8"),
    }

    # The positive control is `main`'s own run, so only the mutations here.
    def mutated(key: str, old: str, new: str) -> list[str]:
        assert old in real[key], f"self-test premise gone: {old!r} not in {key}"
        m = dict(real)
        m[key] = m[key].replace(old, new, 1)
        return analyse(**m)

    # (2) the entry point goes back to a literal.
    assert mutated(
        "spin_src",
        f"in_place_capable: super::declared_shape::{ENUM_NAME}::BufferedRaw",
        "in_place_capable: false, //",
    ), "a literal `in_place_capable` at a documented entry point must FAIL"

    # (3) the registrar branches on the raw flag again.
    assert mutated(
        "runtime_src",
        f"if shape == {ENUM_NAME}::BufferedRawSafety {{",
        "if metadata.safety {",
    ), "a registrar testing `metadata.safety` itself must FAIL"

    # (3) a variant nothing in the arm names.
    assert mutated(
        "shape_src",
        "    BufferedRawSafety,",
        "    BufferedRawSafety,\n    /// `Node::create_generic_subscription_viewable` →\n"
        "    /// `Executor::register_subscription_buffered_borrowed_on`.\n    BufferedView,",
    ), "a variant the registrar never names must FAIL"

    # (4) the recorder stops stating it.
    assert mutated(
        "recorder_src",
        "Some(metadata.declared_subscription_shape().in_place_capable())",
        "None",
    ), "a recorder that does not state the path must FAIL"

    # (5) a second mask.
    assert mutated(
        "runtime_src",
        "let shape = metadata.declared_subscription_shape();",
        f"let shape = {ENUM_NAME}::of_declaration(metadata.safety);",
        # NOTE: also removes the classifier call, so this asserts two clauses.
    ), "a second caller of `of_declaration` must FAIL"

    # (6) non-vacuity.
    empty = dict(real)
    empty["shape_src"] = "// the enum is gone\n"
    assert analyse(**empty), "zero variants must FAIL, not pass vacuously"


def main() -> int:
    texts = {}
    for key, path in (
        ("shape_src", SHAPE_RS),
        ("spin_src", SPIN_RS),
        ("runtime_src", RUNTIME_RS),
        ("recorder_src", RECORDER_RS),
        ("metadata_src", METADATA_RS),
    ):
        if not path.is_file():
            print(
                f"check-declared-subscription-shape: {path.relative_to(REPO)} is "
                f"missing — the seam moved; this gate cannot answer.",
                file=sys.stderr,
            )
            return 1
        texts[key] = path.read_text(encoding="utf-8")

    self_test()

    problems = analyse(**texts)
    if problems:
        print(
            "check-declared-subscription-shape: the declarative registrar and "
            "the metadata\nprobe no longer read ONE classifier (issue 1522):\n",
            file=sys.stderr,
        )
        print("\n".join(problems), file=sys.stderr)
        return 1

    names = sorted(variants(texts["shape_src"]))
    print(
        f"check-declared-subscription-shape: OK — {len(names)} shape(s) "
        f"({', '.join(names)}): the registrar branches on them, each entry "
        f"point takes its value from them, and the recorder states it."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
