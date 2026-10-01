#!/usr/bin/env python3
"""phase-432 W3.3 / phase-469 W1 — a codegen pack that is half-wired must fail LOUDLY.

"Adding a language is cheap" is only good news if adding a BROKEN language is
not equally cheap. A pack can be half-wired in several ways, and every one of
them is quiet:

  * a directory under a pack root with no `pack.toml` — a pack nobody
    describes, so nothing says what it emits or how to build it;
  * a manifest naming a template that does not exist, or omitting one that
    does — minijinja resolves templates at RENDER time, so this surfaces on
    some user's build rather than here;
  * a language pack missing the fields its consumer needs;
  * a `Language` variant with no pack at all — the enumeration says the
    language exists and nothing renders it;
  * a language with no GOLDEN coordinate, so its bytes are recorded nowhere and
    a change to them is invisible.

TWO PACK ROOTS, ONE GATE
------------------------
The ENTRY packs (`codegen/entry/packs/entry/`) have had manifests since
phase-432 W3.2; the MESSAGE packs (`rosidl-codegen/packs/`) got theirs in
phase-469 W1. They are the same rule about two pack families, so this grew a
second root rather than acquiring a sibling script: a second gate over one rule
is the "one fact, two authored spellings" defect phase-469 exists to remove, one
level up, and a gate whose reach is narrower than its rule is the 0196 shape the
tree has paid for repeatedly.

Both roots' manifests now name FILES (`templates = [{ key, file }]`) beside
themselves — the entry packs since phase-474 W1, when their registry stopped
being an authored `include_str!` list in `render.rs` and became generated from
these rows, as the message packs' had in phase-469 W1. So file existence and
the reverse direction — a `.jinja` no manifest claims — are READ from the
manifest rather than re-spelled, and are checked here for both. What still
differs per root is carried by `PackRoot`.

THE NAME
--------
It still says `entry`, and that is deliberate: the name is one the growth-only
`.config/gate-registry-baseline.txt` ratchet carries, and regenerating that
baseline is reserved for a deliberate RETIREMENT (issue 1071). A rename would
mean writing "we retired a gate" about a gate that grew. Read it as
"pack conformance"; the recipe comment says so too.

WHY THIS IS A SEPARATE FILE FROM THE UNIT TESTS

`check-cli-tests` runs the unit tests and is on the required PR context, so the
Rust half is merge-gating already; on the message side `build.rs` refuses most
of this at COMPILE time, which is earlier still. This half is on the FAST line,
which is what a contributor runs before pushing — the point of a conformance
gate is that it answers early, and a cargo build is not early. It also sees two
things neither can: an UNTRACKED pack directory, and the `Language` enumeration.

Usage::

    check-entry-pack-conformance.py            # the gate
    check-entry-pack-conformance.py --audit    # show what it found, never fails
"""

from __future__ import annotations

import re
import subprocess
import sys
from dataclasses import dataclass, field
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
LANG_SRC = ROOT / "packages/cli/nros-lang/src/lib.rs"


class ManifestError(Exception):
    """A manifest this parser cannot read. Never a silent skip."""


def _parse_inline_table(body: str) -> dict:
    """`key = "value"` pairs inside a `{ … }`, which is all a `templates` row is."""
    out: dict = {}
    for part in body.split(","):
        part = part.strip()
        if not part:
            continue
        m = re.match(r'^([A-Za-z_][\w-]*)\s*=\s*"([^"]*)"$', part)
        if not m:
            raise ManifestError(f"cannot parse inline-table field: {part!r}")
        out[m.group(1)] = m.group(2)
    return out


def _parse_array(body: str) -> list:
    """The items between `[` and `]`: quoted strings, or inline tables."""
    items: list = []
    i = 0
    while i < len(body):
        ch = body[i]
        if ch in " \t\r\n,":
            i += 1
            continue
        if ch == '"':
            j = body.find('"', i + 1)
            if j < 0:
                raise ManifestError(f"unterminated string in array: {body[i:i + 40]!r}")
            items.append(body[i + 1 : j])
            i = j + 1
            continue
        if ch == "{":
            j = body.find("}", i + 1)
            if j < 0:
                raise ManifestError(f"unterminated inline table: {body[i:i + 40]!r}")
            items.append(_parse_inline_table(body[i + 1 : j]))
            i = j + 1
            continue
        raise ManifestError(f"cannot parse array item at {body[i:i + 40]!r}")
    return items


def parse_manifest(text: str) -> dict:
    """The flat subset of TOML a `pack.toml` uses, parsed without a dependency.

    This host's Python is 3.10, which has no `tomllib`, and the repo's rule is
    that a gate brings no dependencies (see `check-provider-announcements.py`).
    A pack manifest is deliberately flat — bare `key = value` plus one array,
    whose items are strings (an entry `filters` list) or inline tables (the
    `templates` rows), no nesting — so the subset needed is small.

    It REFUSES anything outside that subset rather than skipping the line. A
    parser that silently ignores what it does not understand is how a gate ends
    up reporting green over a manifest it never read.
    """
    out: dict = {}
    pending_key: str | None = None
    pending_raw = ""
    for raw in text.splitlines():
        line = raw.split("#", 1)[0].strip() if not raw.strip().startswith("#") else ""
        if not line:
            continue
        if pending_key is not None:
            close = line.find("]")
            if close >= 0:
                pending_raw += line[:close]
                out[pending_key] = _parse_array(pending_raw)
                pending_key, pending_raw = None, ""
            else:
                pending_raw += line + "\n"
            continue
        m = re.match(r"^([A-Za-z_][\w-]*)\s*=\s*(.*)$", line)
        if not m:
            raise ManifestError(f"cannot parse line: {line!r}")
        key, value = m.group(1), m.group(2).strip()
        if value.startswith("["):
            close = value.find("]")
            if close >= 0:
                out[key] = _parse_array(value[1:close])
            else:
                pending_key, pending_raw = key, value[1:] + "\n"
        elif value.startswith('"') and value.endswith('"') and len(value) >= 2:
            out[key] = value[1:-1]
        elif value in ("true", "false"):
            out[key] = value == "true"
        elif re.fullmatch(r"-?\d+", value):
            out[key] = int(value)
        else:
            raise ManifestError(f"unsupported value for `{key}`: {value!r}")
    if pending_key is not None:
        raise ManifestError(f"unterminated list for `{pending_key}`")
    return out


@dataclass(frozen=True)
class PackRoot:
    """One pack family: where its packs live and what a manifest must hold."""

    #: Short name used in messages.
    name: str
    #: Directory whose immediate children are packs.
    dir: Path
    #: Fields a LANGUAGE pack must declare and a SHARED pack must not.
    language_fields: tuple[str, ...]
    #: Fields that are one row — all present or all absent.
    paired_fields: tuple[tuple[str, ...], ...] = ()
    #: `templates` rows name files beside the manifest, so check both directions.
    templates_name_files: bool = False
    #: An integer field that must be unique across this root's packs.
    unique_int_field: str | None = None
    #: Directory of `<name>.<ext>.golden` files, when this root has a corpus
    #: recorded per language.
    goldens: Path | None = None
    #: Extra suffixes whose files a manifest must claim (message packs).
    claimed_suffixes: tuple[str, ...] = field(default=())
    #: A root of TEST-FIXTURE packs (phase-474 W5): each must declare
    #: `fixture = true` and must NOT name a `language` — a real language is a
    #: `Language` variant, which a fixture must not add — so the
    #: variant<->pack checks do not apply, and its golden is keyed on its
    #: `extension` instead.
    fixture: bool = False


ENTRY = PackRoot(
    name="entry",
    dir=ROOT / "packages/cli/nros-cli-core/src/codegen/entry/packs/entry",
    language_fields=("language", "extension", "c_family", "entry_template", "context"),
    goldens=ROOT / "packages/cli/nros-cli-core/testdata/entry",
    templates_name_files=True,
    claimed_suffixes=(".jinja",),
)

MESSAGE = PackRoot(
    name="message",
    dir=ROOT / "packages/cli/rosidl-codegen/packs",
    language_fields=("language",),
    # `generator::naming` derives an artifact's header name, its include guard
    # and its translation unit from these; a header extension with no guard
    # suffix is a surface whose guards collide with another surface's.
    paired_fields=(("header_extension", "guard_suffix"),),
    templates_name_files=True,
    # Hashed by `codegen_fingerprint` as a SEQUENCE, so it must be total.
    unique_int_field="registry_order",
    claimed_suffixes=(".jinja",),
)

#: phase-474 W5 — the data-only toy entry pack(s): the proof that an entry
#: language is a pack, not a Rust emitter. Held to the entry root's rules — a
#: manifest, files claimed both ways, a golden — so the proof cannot rot into
#: a directory nothing checks.
ENTRY_FIXTURE = PackRoot(
    name="entry-fixture",
    dir=ROOT / "packages/cli/nros-cli-core/testdata/entry-packs",
    language_fields=("fixture", "extension", "c_family", "entry_template", "context"),
    goldens=ROOT / "packages/cli/nros-cli-core/testdata/entry",
    templates_name_files=True,
    claimed_suffixes=(".jinja",),
    fixture=True,
)

ROOTS = (ENTRY, MESSAGE, ENTRY_FIXTURE)

#: phase-474 — where an entry EMITTER would live, and the one that may.
EMITTER_DIR = ROOT / "packages/cli/nros-cli-core/src/codegen/entry"
#: `emit_rust.rs` is the Rust parity renderer RFC-0091 §7 keeps BY DECISION:
#: the second rendering the parity corpus compares the `nros::main!`
#: proc-macro against (issue 0083). Every other entry language is a pack.
ALLOWED_EMITTERS = {"emit_rust.rs"}


def tracked_pack_dirs(root: PackRoot) -> list[Path]:
    """Every `<root>/<pack>/` directory, from git rather than a walk.

    `--others --exclude-standard` alongside `--cached` so a pack added but not
    yet committed reds NOW, while its author is looking at it, rather than on
    someone else's push. That is the same choice `check-entry-locator-ssot`
    makes, and for the same reason.
    """
    rel = root.dir.relative_to(ROOT)
    out = subprocess.run(
        ["git", "ls-files", "--cached", "--others", "--exclude-standard", str(rel)],
        cwd=ROOT,
        capture_output=True,
        text=True,
        check=True,
    ).stdout.split()
    dirs = {ROOT / Path(p).parent for p in out}
    return sorted(d for d in dirs if d != root.dir)


def declared_languages() -> list[str]:
    """The `Language` variants, read from the crate that owns the enumeration.

    Parsed rather than restated: a second list of "which languages exist" is
    exactly the drift phase-432 removes, and this gate would be a poor place to
    reintroduce it.
    """
    text = LANG_SRC.read_text(encoding="utf-8")
    m = re.search(r"pub enum Language\s*\{(.*?)\n\}", text, re.S)
    if not m:
        raise SystemExit(
            f"check-entry-pack-conformance: cannot find `pub enum Language` in "
            f"{LANG_SRC.relative_to(ROOT)} — the parse this gate depends on has "
            "moved, so it would report green having checked nothing."
        )
    body = re.sub(r"//[^\n]*", "", m.group(1))
    return [v.lower() for v in re.findall(r"^\s*([A-Z]\w*)\s*,", body, re.M)]


def golden_languages(root: PackRoot) -> set[str]:
    """Languages that have at least one recorded golden.

    Keyed on the file EXTENSION rather than the name prefix: a row is named for
    what it exercises, and tying this to a naming convention would make it a
    check on names instead of on coverage.
    """
    seen: set[str] = set()
    if root.goldens is None:
        return seen
    for p in root.goldens.glob("*.golden"):
        # `<name>.<ext>.golden`
        parts = p.name.split(".")
        if len(parts) >= 3:
            seen.add(parts[-2])
    return seen


def check_root(root: PackRoot) -> tuple[list[str], dict[str, dict]]:
    """Everything wrong with `root`, and the manifests it managed to read."""
    problems: list[str] = []
    rel_root = root.dir.relative_to(ROOT)

    dirs = tracked_pack_dirs(root)
    if not dirs:
        problems.append(
            f"no pack directories under {rel_root} — this gate would pass "
            "having checked nothing"
        )
        return problems, {}

    manifests: dict[str, dict] = {}
    for d in dirs:
        mf = d / "pack.toml"
        rel = d.relative_to(ROOT)
        if not mf.is_file():
            problems.append(
                f"{rel}: a pack directory with no `pack.toml`. Nothing says what "
                "it emits, how to build it, or whether it is a language pack or "
                "shared partials."
            )
            continue
        try:
            manifests[d.name] = parse_manifest(mf.read_text(encoding="utf-8"))
        except ManifestError as e:
            problems.append(f"{rel}/pack.toml: {e}")

    for pack, m in sorted(manifests.items()):
        rel = f"{rel_root}/{pack}/pack.toml"
        if m.get("shared"):
            stray = [f for f in root.language_fields if f in m]
            stray += [f for pair in root.paired_fields for f in pair if f in m]
            if stray:
                problems.append(
                    f"{rel}: shared pack declares {', '.join(sorted(set(stray)))} — "
                    "a shared pack renders no artifact of its own, so declaring a "
                    "language pack's fields makes the two kinds confusable."
                )
            if not m.get("templates"):
                problems.append(f"{rel}: shared pack declares no templates, so it is nothing.")
        else:
            missing = [f for f in root.language_fields if f not in m]
            if missing:
                problems.append(
                    f"{rel}: language pack is missing {', '.join(missing)}. "
                    "A pack its consumer cannot describe is a pack that emits "
                    "nothing and looks wired."
                )
        for pair in root.paired_fields:
            present = [f for f in pair if f in m]
            if present and len(present) != len(pair):
                problems.append(
                    f"{rel}: {', '.join(present)} declared without "
                    f"{', '.join(f for f in pair if f not in m)} — these are one "
                    "row, so declare all of them or none."
                )

    if root.unique_int_field:
        seen: dict[int, str] = {}
        for pack, m in sorted(manifests.items()):
            rel = f"{rel_root}/{pack}/pack.toml"
            value = m.get(root.unique_int_field)
            if value is None:
                problems.append(
                    f"{rel}: no `{root.unique_int_field}`. It is hashed as part of "
                    "the registry's ORDER, so it cannot be left to directory order."
                )
            elif not isinstance(value, int):
                problems.append(f"{rel}: `{root.unique_int_field}` is not an integer.")
            elif value in seen:
                problems.append(
                    f"{rel}: `{root.unique_int_field} = {value}` is also declared by "
                    f"`{seen[value]}` — it decides a hashed order, so it must be total."
                )
            else:
                seen[value] = pack

    if root.templates_name_files:
        # Both directions. A manifest names FILES here, so neither half
        # re-spells a key→path map that lives somewhere else.
        claimed: set[Path] = set()
        for pack, m in sorted(manifests.items()):
            rel = f"{rel_root}/{pack}/pack.toml"
            rows = m.get("templates")
            if not rows:
                if not m.get("shared"):
                    problems.append(f"{rel}: no `templates`, so this pack renders nothing.")
                continue
            for row in rows:
                if not isinstance(row, dict) or "key" not in row or "file" not in row:
                    problems.append(
                        f"{rel}: a `templates` row is not "
                        '`{ key = "…", file = "…" }`: ' + repr(row)
                    )
                    continue
                path = root.dir / pack / row["file"]
                if not path.is_file():
                    problems.append(
                        f"{rel}: row `{row['key']}` names `{row['file']}`, which "
                        "does not exist beside the manifest — the render fails at "
                        "someone's build, not here."
                    )
                else:
                    claimed.add(path)
        keys: dict[str, str] = {}
        for pack, m in sorted(manifests.items()):
            for row in m.get("templates") or []:
                if isinstance(row, dict) and "key" in row:
                    if row["key"] in keys:
                        problems.append(
                            f"{rel_root}/{pack}/pack.toml: registry key "
                            f"`{row['key']}` is also declared by `{keys[row['key']]}` — "
                            "the loader resolves the first, so the second never renders."
                        )
                    else:
                        keys[row["key"]] = pack
        for d in dirs:
            for path in sorted(d.iterdir()):
                if path.suffix in root.claimed_suffixes and path not in claimed:
                    problems.append(
                        f"{path.relative_to(ROOT)}: no `pack.toml` claims this "
                        "template, so nothing renders it. A file nobody renders "
                        "parses, looks wired and emits nothing."
                    )

    return problems, manifests


def check() -> list[str]:
    problems: list[str] = []
    langs = set(declared_languages())
    if not langs:
        problems.append("`Language` parsed to zero variants — refusing to report green.")

    problems += check_no_entry_emitters()

    for root in ROOTS:
        root_problems, manifests = check_root(root)
        problems += root_problems
        if not manifests:
            continue

        if root.fixture:
            problems += check_fixture_root(root, manifests)
            continue

        # Every declared Language has a pack in this root, and every language
        # pack names a declared Language.
        pack_langs = {m["language"] for m in manifests.values() if "language" in m}
        for lang in sorted(langs - pack_langs):
            problems.append(
                f"language `{lang}` is a `Language` variant with no {root.name} "
                "pack. The enumeration says it exists; nothing renders it."
            )
        for lang in sorted(pack_langs - langs):
            problems.append(
                f"a {root.name} pack declares language `{lang}`, which is not a "
                "`Language` variant — one of the two is wrong."
            )

        # Every language has a golden coordinate, where this root records one.
        # Without it its bytes are recorded nowhere and a change is invisible.
        goldens = golden_languages(root)
        if root.goldens is not None:
            # `.get`, not `[...]`: a manifest missing `extension` has ALREADY
            # been reported above, and a gate that crashes on the second
            # finding tells you about one problem when it found two.
            ext_by_lang = {
                m["language"]: m.get("extension")
                for m in manifests.values()
                if "language" in m
            }
            for lang in sorted(pack_langs):
                ext = ext_by_lang.get(lang)
                if ext and ext not in goldens:
                    problems.append(
                        f"language `{lang}` has no golden with extension `.{ext}` in "
                        f"{root.goldens.relative_to(ROOT)} — its generated bytes are "
                        "recorded nowhere, so a change to them is invisible."
                    )
    return problems


def check_no_entry_emitters() -> list[str]:
    """phase-474 — an entry language is a PACK, never a Rust emitter.

    `emit_c.rs` and `emit_cpp.rs` were deleted when `LoweredEntry` became the
    template context (RFC-0091 §6b); a new `codegen/entry/emit_<x>.rs` would be
    the per-language projection coming back. Read from git (plus untracked
    files), like the pack directories, so a new one reds while its author is
    looking at it.
    """
    rel = EMITTER_DIR.relative_to(ROOT)
    out = subprocess.run(
        ["git", "ls-files", "--cached", "--others", "--exclude-standard", str(rel)],
        cwd=ROOT,
        capture_output=True,
        text=True,
        check=True,
    ).stdout.split()
    problems = []
    for p in sorted(out):
        path = ROOT / p
        if path.parent != EMITTER_DIR or not path.exists():
            continue
        if re.fullmatch(r"emit_\w+\.rs", path.name) and path.name not in ALLOWED_EMITTERS:
            problems.append(
                f"{p}: an entry EMITTER. An entry language is a pack — a `pack.toml`, "
                "its templates, and a row in `filters.rs` if it needs a spelling no "
                "filter provides — rendered from `LoweredEntry` by `emit/mod.rs` "
                "(phase-474). A per-language projection here is what RFC-0091 §6b "
                "retired."
            )
    return problems


def check_fixture_root(root: PackRoot, manifests: dict[str, dict]) -> list[str]:
    """A fixture pack declares itself, names no language, and has a golden."""
    problems: list[str] = []
    rel_root = root.dir.relative_to(ROOT)
    goldens = golden_languages(root)
    for pack, m in sorted(manifests.items()):
        rel = f"{rel_root}/{pack}/pack.toml"
        if m.get("fixture") is not True:
            problems.append(f"{rel}: a fixture-root pack must declare `fixture = true`.")
        if "language" in m:
            problems.append(
                f"{rel}: a fixture pack names `language = \"{m['language']}\"`. A "
                "real language is a `Language` variant; a fixture proving a pack "
                "can be DATA must not need one."
            )
        ext = m.get("extension")
        if ext and ext not in goldens:
            problems.append(
                f"{rel}: no golden with extension `.{ext}` in "
                f"{root.goldens.relative_to(ROOT)} — what the fixture renders is "
                "recorded nowhere, so it proves nothing."
            )
    return problems


def selftest() -> None:
    """The predicates must catch what they exist for, and pass what they must.

    Run on the NORMAL path, not behind a flag: a negative control nobody runs
    decays into a comment. Every case here is a shape this gate was written for,
    driven through the pure functions so it needs no files on disk.
    """
    # The manifest parser refuses what it cannot read, rather than skipping it.
    for bad in (
        "language = c\n",
        'partials = ["a"\n',
        "language = 'c'\n",
        "templates = [ message.h ]\n",
        'templates = [ { key = message.h } ]\n',
    ):
        try:
            parse_manifest(bad)
        except ManifestError:
            pass
        else:
            raise SystemExit(
                f"SELFTEST FAIL: parse_manifest accepted {bad!r} — a parser that "
                "ignores what it does not understand reports green over a file it "
                "never read."
            )

    # ...and reads the shapes a real manifest uses, including a multi-line list
    # and a comment on its own line.
    got = parse_manifest(
        "# a comment\n"
        'language = "c"   # trailing comment\n'
        "c_family = true\n"
        'partials = [\n    "a.jinja",\n    "b.jinja",\n]\n'
    )
    want = {"language": "c", "c_family": True, "partials": ["a.jinja", "b.jinja"]}
    if got != want:
        raise SystemExit(f"SELFTEST FAIL: parse_manifest gave {got!r}, want {want!r}")

    # ...including the message packs' integer and inline-table rows.
    got = parse_manifest(
        "registry_order = 20\n"
        "templates = [\n"
        '    { key = "message.h", file = "message.h.jinja" },\n'
        '    { key = "_field.jinja", file = "_field.jinja" },\n'
        "]\n"
    )
    want = {
        "registry_order": 20,
        "templates": [
            {"key": "message.h", "file": "message.h.jinja"},
            {"key": "_field.jinja", "file": "_field.jinja"},
        ],
    }
    if got != want:
        raise SystemExit(f"SELFTEST FAIL: parse_manifest gave {got!r}, want {want!r}")

    # The `Language` parse must find variants. If it silently found none, every
    # "variant with no pack" check below would vacuously pass.
    if not declared_languages():
        raise SystemExit(
            "SELFTEST FAIL: declared_languages() found no variants, so the "
            "language checks would pass having compared nothing."
        )

    # A golden extension the corpus really has, and one it cannot have.
    goldens = golden_languages(ENTRY)
    if not goldens:
        raise SystemExit("SELFTEST FAIL: golden_languages() found nothing.")
    if "nosuchext" in goldens:
        raise SystemExit("SELFTEST FAIL: golden_languages() invented an extension.")

    # The emitter rule catches a new per-language emitter and spares the one
    # RFC-0091 §7 keeps.
    if not re.fullmatch(r"emit_\w+\.rs", "emit_zig.rs") or "emit_zig.rs" in ALLOWED_EMITTERS:
        raise SystemExit("SELFTEST FAIL: the emitter rule would not catch `emit_zig.rs`.")
    if "emit_rust.rs" not in ALLOWED_EMITTERS:
        raise SystemExit("SELFTEST FAIL: the parity renderer is not exempt.")

    # Every root must exist. A root whose path has moved would report "no pack
    # directories", which is a finding — but a root silently pointing at an
    # empty tree while the packs live elsewhere is the 0196 shape.
    for root in ROOTS:
        if not root.dir.is_dir():
            raise SystemExit(
                f"SELFTEST FAIL: pack root {root.dir.relative_to(ROOT)} does not "
                "exist, so this gate covers one family while claiming two."
            )


def main() -> int:
    selftest()

    if "--audit" in sys.argv:
        for root in ROOTS:
            print(f"{root.name} packs under {root.dir.relative_to(ROOT)}:")
            for d in tracked_pack_dirs(root):
                has = "pack.toml" if (d / "pack.toml").is_file() else "NO MANIFEST"
                print(f"  {d.name:10} {has}")
        print(f"Language variants: {', '.join(declared_languages())}")
        print(f"entry golden extensions: {', '.join(sorted(golden_languages(ENTRY)))}")
        return 0

    problems = check()
    if problems:
        print(
            "FAIL: a codegen pack is half-wired.\n"
            "  A pack that is described incompletely does not fail to build — it\n"
            "  emits nothing, or emits into a file the toolchain will not compile,\n"
            "  and the first symptom is on someone's board.\n",
            file=sys.stderr,
        )
        for p in problems:
            print(f"  {p}", file=sys.stderr)
        return 1

    counts = ", ".join(f"{len(tracked_pack_dirs(r))} {r.name}" for r in ROOTS)
    print(
        f"check-entry-pack-conformance: OK ({counts} pack(s), "
        f"{len(declared_languages())} language(s), each described by a manifest)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
