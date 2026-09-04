#!/usr/bin/env python3
"""phase-421 W4 — the serdes descriptors are well-formed and unambiguous.

Family `serdes` (RFC-0088 D6) is the first provider family born after RFC-0087
D4, so its descriptor is SMALL by construction: the `<nano_ros_provides>`
announcement carries the name, and `nros-serdes.toml` carries only the two facts
no convention can derive — how the format is produced (`impl`) and the
image-local discriminant (`format_id`). A provider with nothing non-derivable to
say needs no descriptor at all.

That shape decides what this gate can honestly assert. Its sibling
`check-rmw-descriptors.py` learned the hard way that **a gate that cannot fail
is worse than no gate** — it began as an agreement check and had to be rewritten
once the descriptors became the source, because comparing a generated table to
its own input is comparing a thing to itself. So this file never compares the
descriptor to something derived from it. It checks what can still go wrong:

  S1  every descriptor is well-formed: `impl` is exactly "schema" or "codegen",
      `format_id` is an integer in 1..=255, and there are no unknown keys under
      `[serdes]` — an unknown key is a typo, and a typo silently lowers to
      nothing while reading like a decision;
  S2  no two providers claim one name, and no two claim one `format_id`. The
      discriminant half is the one that matters: an ambiguous `u8` is a
      wire-visible bug with a compile-time-looking cause (RFC-0088 D2);
  S3  the in-tree reserved discriminants agree with
      `nros_serdes::format::SerializationFormatId`. `cdr = 1` and `uorb = 2` are
      written twice — once in the Rust enum, once in a descriptor — and this is
      the only thing stopping them diverging. The pairs are PARSED out of
      `format.rs`, never hardcoded here: a copy in this file would be a third
      spelling of the same fact and would agree with neither;
  S4  a descriptor exists for every package announcing `kind="serdes"`, so
      adding a provider and forgetting the descriptor fails here rather than at
      a consumer, where the symptom is a defaulted `impl` nobody chose.

Why S3 checks BOTH directions. A reserved name must hold its reserved value
(`cdr` may not become 7), and a reserved value must not be taken under another
name (`flatbuf = 2` would collide with uORB inside any image that links both).
Only the first direction is obvious, and only the second is the wire bug.

Note that uORB has NO serdes provider package: its wire is the PX4 struct, a
property of that backend rather than a serialization library (RFC-0011). It is a
reserved name with no descriptor, which S3 tolerates by design — it constrains
descriptors that USE a reserved name or value, and never demands one exist.

`kind="serdes"` announcement agreement — "a package.xml beside a descriptor
announces its family, and no two announce one name" — is NOT here. It lives in
`scripts/check-provider-announcements.py`, one gate covering every family, which
is where S5 moved when boards needed the same rule (#282 → #326). What stays
here is what is serdes-SPECIFIC. The name-uniqueness pass below is the belt to
that gate's braces and covers a strictly larger set: this one sees providers
whose descriptor is missing (S4) or whose package.xml the other gate skips.

Buildless — TOML plus a regex over one Rust file. No cargo, no cmake.

Usage:
    scripts/check-serdes-descriptors.py
    scripts/check-serdes-descriptors.py --self-test
"""

import glob
import importlib.util
import os
import re
import sys

try:
    import tomllib  # 3.11+
except ModuleNotFoundError:  # 3.10 backport, same spelling as the sibling gates
    import tomli as tomllib

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

FORMAT_RS = os.path.join(ROOT, "packages/core/nros-serdes/src/format.rs")

# `impl` values the toolchain knows how to lower (RFC-0088 D7). A third value is
# a provider asking for a strategy that does not exist.
IMPLS = {"schema", "codegen"}

# Every key `[serdes]` may carry. Anything else is a typo: the reader takes its
# default and the authored line has no effect, which is the failure mode this
# repo keeps paying for in Kconfig fragments (issue 0876).
KNOWN_KEYS = {"impl", "format_id"}


def _load(name, path):
    """Import a sibling gate by file path (hyphens are not module names)."""
    spec = importlib.util.spec_from_file_location(name, os.path.join(ROOT, path))
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


_ANN = _load("_announcements", "scripts/check-provider-announcements.py")

# ONE spelling of "where serdes descriptors live" and of "how a package.xml
# announces a provision", shared with the family gate rather than re-written.
DESCRIPTOR_GLOB = _ANN.FAMILIES["serdes"][0]
declared_provisions = _ANN.declared_provisions

# Where a package.xml may announce `serdes` from. Wider than DESCRIPTOR_GLOB on
# purpose: S4's whole job is the package that announces and has no descriptor,
# so it cannot search only where descriptors already are.
PACKAGE_XML_GLOB = "packages/*/*/package.xml"

ENUM_RE = re.compile(r"pub enum SerializationFormatId\s*\{(.*?)\n\}", re.DOTALL)
VARIANT_RE = re.compile(r"^\s*([A-Z][A-Za-z0-9]*)\s*=\s*(\d+)\s*,", re.MULTILINE)
AS_STR_RE = re.compile(r"pub const fn as_str\(self\)[^{]*\{(.*?)\n    \}", re.DOTALL)
ARM_RE = re.compile(r"Self::([A-Za-z0-9]+)\s*=>\s*\"([^\"]+)\"")


def reserved_formats(text):
    """{name: discriminant} parsed out of `format.rs`.

    Two independent readings that must agree: the enum body gives variant ->
    number, `as_str` gives variant -> string. Requiring both means the enum
    cannot grow a variant with no cross-image name, which is the identity that
    actually crosses an image boundary (RFC-0088 D2).

    Returns (mapping, problems).
    """
    problems = []
    body = ENUM_RE.search(text)
    if not body:
        return {}, ["could not find `pub enum SerializationFormatId` — did it move?"]
    variants = {m.group(1): int(m.group(2)) for m in VARIANT_RE.finditer(body.group(1))}
    if not variants:
        return {}, ["`SerializationFormatId` parsed with no explicit discriminants"]

    arms_src = AS_STR_RE.search(text)
    if not arms_src:
        return {}, ["could not find `SerializationFormatId::as_str` — did it move?"]
    arms = dict(ARM_RE.findall(arms_src.group(1)))

    mapping = {}
    for variant, value in sorted(variants.items(), key=lambda kv: kv[1]):
        name = arms.get(variant)
        if name is None:
            problems.append(
                f"SerializationFormatId::{variant} has no `as_str` arm — the "
                f"string, not the number, is the cross-image identity"
            )
            continue
        mapping[name] = value
    return mapping, problems


def read_format_rs():
    """(reserved mapping, problems) — never raises.

    A missing or moved `format.rs` is a real failure mode (the crate is
    reorganised, this gate is not) and must READ as one, not as a traceback: a
    stack trace on a gate reads as "the gate is broken" and gets ignored.
    """
    try:
        with open(FORMAT_RS, encoding="utf-8") as fh:
            text = fh.read()
    except OSError as e:  # report, do not raise
        return {}, [f"cannot read {os.path.relpath(FORMAT_RS, ROOT)}: {e}"]
    return reserved_formats(text)


def read_descriptor(path):
    """([serdes] table, problems) for one descriptor."""
    rel = os.path.relpath(path, ROOT)
    with open(path, "rb") as fh:
        try:
            data = tomllib.load(fh)
        except Exception as e:  # noqa: BLE001 — report, do not raise
            return None, [f"{rel}: not valid TOML: {e}"]
    table = data.get("serdes")
    if table is None:
        return None, [f"{rel}: no [serdes] table"]
    if not isinstance(table, dict):
        return None, [f"{rel}: [serdes] is not a table"]
    return table, []


def check_descriptor(rel, table):
    """S1 for one descriptor: impl, format_id, no unknown keys."""
    problems = []

    unknown = sorted(set(table) - KNOWN_KEYS)
    if unknown:
        problems.append(
            f"{rel}: unknown key(s) under [serdes]: {unknown} — known keys are "
            f"{sorted(KNOWN_KEYS)}; an unrecognised key lowers to nothing while "
            f"reading like a decision"
        )

    impl = table.get("impl")
    if impl is None:
        problems.append(
            f"{rel}: [serdes].impl is missing — write \"codegen\" or \"schema\" "
            f"(RFC-0088 D7); a descriptor that has nothing to say should not exist"
        )
    elif impl not in IMPLS:
        problems.append(
            f"{rel}: [serdes].impl is {impl!r}, not one of {sorted(IMPLS)}"
        )

    fid = table.get("format_id")
    if fid is None:
        problems.append(f"{rel}: [serdes].format_id is missing")
    elif isinstance(fid, bool) or not isinstance(fid, int):
        problems.append(
            f"{rel}: [serdes].format_id is {fid!r} — must be an integer, since it "
            f"lowers to a u8 discriminant"
        )
    elif not 1 <= fid <= 255:
        problems.append(
            f"{rel}: [serdes].format_id is {fid} — must be in 1..=255 (0 is "
            f"reserved for 'unset' and the discriminant is a u8)"
        )
    return problems


def scan():
    """Every serdes provider found in the tree.

    Returns (providers, problems) where a provider is
    (package dir rel, descriptor rel or None, announced names, [serdes] table).
    """
    problems = []
    by_dir = {}

    for desc in sorted(glob.glob(os.path.join(ROOT, DESCRIPTOR_GLOB))):
        d = os.path.dirname(desc)
        table, probs = read_descriptor(desc)
        problems += probs
        by_dir[d] = [os.path.relpath(desc, ROOT), [], table]

    for pkg_xml in sorted(glob.glob(os.path.join(ROOT, PACKAGE_XML_GLOB))):
        names = declared_provisions(pkg_xml, "serdes")
        if not names:
            continue
        d = os.path.dirname(pkg_xml)
        by_dir.setdefault(d, [None, [], None])[1] = names

    providers = [
        (os.path.relpath(d, ROOT), desc, names, table)
        for d, (desc, names, table) in sorted(by_dir.items())
    ]
    return providers, problems


def main(argv):
    if "--self-test" in argv:
        return self_test()

    # phase-395: a negative control nobody runs decays into a comment, so the
    # selftest runs on the NORMAL path too — quietly when it passes. The flag
    # above is only the verbose entry point.
    rc = self_test(quiet=True)
    if rc:
        return rc

    providers, problems = scan()
    if not providers:
        sys.exit(
            "check-serdes-descriptors: no serdes provider found — refusing to "
            "pass on an empty set (the gate would be vacuous)"
        )

    reserved, probs = read_format_rs()
    problems += [f"packages/core/nros-serdes/src/format.rs: {p}" for p in probs]
    reserved_by_id = {v: k for k, v in reserved.items()}

    by_name = {}
    by_id = {}
    descriptors = 0

    for pkg, desc_rel, names, table in providers:
        # S4 — announced, but no descriptor beside the announcement.
        if desc_rel is None:
            problems.append(
                f"{pkg}: announces serdes name(s) {names} but ships no "
                f"nros-serdes.toml — `impl` and `format_id` cannot be derived "
                f"from a name, so a consumer would silently get the defaults"
            )
            continue
        if table is None:
            continue  # already reported by read_descriptor
        descriptors += 1
        problems += check_descriptor(desc_rel, table)

        # S2 — one name, one provider; one discriminant, one format.
        for n in names:
            if n in by_name:
                problems.append(
                    f"two providers claim serdes name {n!r}: {by_name[n]} and "
                    f"{desc_rel} — a name resolves to exactly one provider"
                )
            by_name[n] = desc_rel
        fid = table.get("format_id")
        if isinstance(fid, int) and not isinstance(fid, bool):
            if fid in by_id:
                problems.append(
                    f"two providers claim format_id {fid}: {by_id[fid]} and "
                    f"{desc_rel} — an ambiguous discriminant is a wire-visible "
                    f"bug with a compile-time-looking cause (RFC-0088 D2)"
                )
            by_id[fid] = desc_rel

            # S3 — both directions against the Rust enum.
            for n in names:
                if n in reserved and reserved[n] != fid:
                    problems.append(
                        f"{desc_rel}: format_id {fid} for {n!r}, but "
                        f"SerializationFormatId reserves {reserved[n]} for it — "
                        f"the enum and the descriptor are two spellings of one "
                        f"number and must agree"
                    )
            holder = reserved_by_id.get(fid)
            if holder is not None and holder not in names:
                problems.append(
                    f"{desc_rel}: format_id {fid} is reserved for {holder!r} by "
                    f"SerializationFormatId, but this provider announces {names} "
                    f"— a reserved discriminant may not be taken under another "
                    f"name"
                )

    if problems:
        sys.stderr.write("check-serdes-descriptors: FAILED\n")
        for p in problems:
            sys.stderr.write(f"  {p}\n")
        return 1

    print(
        f"serdes descriptors: OK ({descriptors} descriptor(s), "
        f"{len(by_name)} name(s) claimed, {len(by_id)} discriminant(s), no "
        f"duplicates; {len(reserved)} reserved format(s) in format.rs agree)"
    )
    return 0


def self_test(quiet=False):
    """Exercise each check against a synthetic input, both directions."""
    bad = []

    def expect(label, problems, want):
        """`want` is a substring of the message this case must produce."""
        if want is None:
            if problems:
                bad.append(f"{label}: expected clean, got {problems}")
        elif not any(want in p for p in problems):
            bad.append(f"{label}: expected a problem containing {want!r}, got {problems}")

    # S1
    good = {"impl": "codegen", "format_id": 1}
    expect("S1 well-formed", check_descriptor("d", good), None)
    expect("S1 bad impl", check_descriptor("d", {"impl": "cdr", "format_id": 1}), "not one of")
    expect("S1 missing impl", check_descriptor("d", {"format_id": 1}), "impl is missing")
    expect("S1 typo key", check_descriptor("d", dict(good, imp1="codegen")), "unknown key")
    expect("S1 id 0", check_descriptor("d", {"impl": "schema", "format_id": 0}), "1..=255")
    expect("S1 id 256", check_descriptor("d", {"impl": "schema", "format_id": 256}), "1..=255")
    expect(
        "S1 id not an int",
        check_descriptor("d", {"impl": "schema", "format_id": "1"}),
        "must be an integer",
    )
    # A TOML bool is an int in Python; it must not read as a discriminant.
    expect(
        "S1 id is a bool",
        check_descriptor("d", {"impl": "schema", "format_id": True}),
        "must be an integer",
    )

    # S3 — the enum parser, against a miniature format.rs and its failure modes.
    src = (
        "pub enum SerializationFormatId {\n    /// doc\n    Cdr = 1,\n    Uorb = 2,\n}\n"
        "impl SerializationFormatId {\n"
        "    pub const fn as_str(self) -> &'static str {\n"
        "        match self {\n"
        "            Self::Cdr => \"cdr\",\n"
        "            Self::Uorb => \"uorb\",\n"
        "        }\n"
        "    }\n"
    )
    mapping, probs = reserved_formats(src)
    if mapping != {"cdr": 1, "uorb": 2} or probs:
        bad.append(f"S3 enum parse: got {mapping} / {probs}")
    _, probs = reserved_formats(src.replace("Self::Uorb => \"uorb\",\n", ""))
    if not any("no `as_str` arm" in p for p in probs):
        bad.append("S3 a variant with no string arm was accepted")
    _, probs = reserved_formats("fn unrelated() {}")
    if not probs:
        bad.append("S3 a format.rs with no enum was accepted")

    # The real file must parse, and must still reserve what the descriptors use.
    live, probs = read_format_rs()
    if probs or not live:
        bad.append(f"S3 live format.rs did not parse: {probs}")

    # S2/S4 depend on the tree, so the self-test asserts only that the scan
    # sees something — a green run over an empty scan is the vacuous gate.
    providers, _ = scan()
    if not providers:
        bad.append("S2/S4 scan found no serdes provider at all")

    if bad:
        for b in bad:
            sys.stderr.write("check-serdes-descriptors --self-test: " + b + "\n")
        return 2
    if not quiet:
        print(
            f"check-serdes-descriptors --self-test: OK (13 case(s), "
            f"{len(providers)} provider(s) visible)"
        )
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
