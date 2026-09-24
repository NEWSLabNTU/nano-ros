#!/usr/bin/env python3
"""A generated message crate's IDENTITY — its name, its version, the wire types
it claims, and the `links` channel it occupies.

Issues 1428 + 1455. Five rules, one scan, no build.

WHY ONE GATE AND NOT FIVE

All five rules are the same mistake seen from different sides: nothing in the
tree knew which crate *is* `builtin_interfaces`. It WAS generated three times
(rcl-interfaces, diagnostic-msgs and rosgraph-msgs each carried the whole
transitive closure of their own package), the three Rust sources were
byte-identical, and all three declared
`TYPE_NAME = "builtin_interfaces/msg/Time"` — one type on the wire, three types
in Rust, with no conversion between them, and `nros-tests` path-depping two at
once.

phase-465 collapsed the four output trees into ONE, so each ament package is
emitted exactly once and rule 3's baseline is empty. That was NOT a codegen
change, contrary to what this docstring and the issue both said until
2026-09-22: one invocation already deduplicates, because
`resolve_transitive_dependencies` returns a `HashSet`. What the gate forbids is
a SECOND copy of any wire type appearing without anyone deciding, which is
exactly how the third `builtin_interfaces` arrived.

RULE 1 — a generated message crate's version is the CONSTANT `0.0.0`

CLAUDE.md, from issue 0394, which broke root-workspace resolution twice: a
generated crate carries `version = "0.0.0"` and consumers path-dep it with NO
version. Three of the then-eight tracked generated crates predate that rule and
were hand-adapted as workspace members, so they carried
`version.workspace = true` — the release version, 0.5.0. phase-465 reshaped
all six survivors to the emitted shape, so none is hand-adapted any more.

RULE 2 — no consumer may pin that version

Which is the live half. Five dep rows read `version = "0.5.0", path = "..."`,
across `packages/core`, `packages/rmw` and the generated tree itself. A path
dep's version field is still a REQUIREMENT: `^0.5.0` stops matching the moment
the workspace version bumps to 0.6.0, and because it is a resolve-time failure
it takes every cargo command in the tree rather than the one consumer. The
issue reported one of these five rows; the class was five (CLAUDE.md's 0196
rule — check the gate's reach, not the reported site), so this rule reads EVERY
tracked manifest in the repo, not the interfaces tree.

RULE 3 — two shipped crates may not claim the same wire type

Ratcheted, because the `builtin_interfaces` triple was real when the rule landed
and removing it was the phase-sized half. The baseline may only shrink: a new
duplicate fails, and a baselined duplicate that stops duplicating fails as stale
(the issue-0743 class, where a stale override degrades silently into a no-op).
That is what made phase-465's collapse PROVABLE rather than claimed — the six
rows had to go in the same commit, and the file is empty now.

`#[cfg(test)]` declarations are EXCLUDED, and that exclusion is the difference
between a useful gate and one that gets baselined into uselessness. Measured on
2026-09-21: four type names were claimed by more than one crate, but two of the
four (`std_msgs/msg/Header`, `std_msgs/msg/Int32`) and three of the extra
claimants (`nros-serdes`, `nros-rmw-cyclonedds`) are hand-written fixture
structs inside `#[cfg(test)] mod tests`. A fixture is not linked into an image,
so it cannot collide on the wire; flagging it would have put two non-problems in
the baseline beside the real one.

RULE 4 — a generated crate's `links` is a function of its own `[package] name`

Issue 1455, RFC-0067 §D4. `links` is the THIRD identity axis a `path` dep
resolves on, beside the name and the version, and like the other two it is
global to the dependency graph — cargo refuses two packages that declare the
same one. Codegen wrote `nros_msgs_<ament package>` on the assumption that a
generated crate is named after its ament package; the committed core set is ALL
renames, so `nros-builtin-interfaces-clock` shipped
`links = "nros_msgs_builtin_interfaces"` and a consumer's own unrenamed
`builtin_interfaces` collided with it at RESOLVE time — every cargo command in
that leaf, not one build.

The rule is deliberately about the crate's OWN name rather than about any rename
map, because that is the invariant a reader of the shipped file can check: two
crates shipping under different names cannot collide, and two shipping under the
same name still do (which is a real collision, and stays reported).

RULE 5 — a shipped generated crate is in the `nros-` namespace

phase-465 W4. The same collision as rule 4, one axis over: two `path` packages
sharing a NAME and a version are a hard cargo error with no workaround at all
("package collision in the lockfile: … only one can be written to lockfile
unambiguously"), and a consumer's own `nros sync` emits a crate named after each
ament package verbatim. So the `nros-` prefix on the committed set is
load-bearing, not cosmetic.

It is not hypothetical: `geometry_msgs` arrives in the interfaces closure via
diagnostic_msgs' ament deps, unrenamed and unused, and only one `rm -rf` line in
`just generate-interfaces` keeps it out of the tree. Checking the tree is what
makes that line's failure visible; trusting the recipe is what would not.

Usage::

    check-message-crate-identity.py             # the gate (self-tests first)
    check-message-crate-identity.py --audit     # full picture, never fails
    check-message-crate-identity.py --write-baseline
"""

import os
import re
import subprocess
import sys

try:
    import tomllib
except ModuleNotFoundError:  # Python < 3.11
    import tomli as tomllib

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
BASELINE = os.path.join(ROOT, ".config", "duplicate-wire-type-baseline.txt")

BASELINE_HEADER = """\
# Wire type names claimed by MORE THAN ONE tracked crate (issue 1428).
#
# EMPTY, and that is the end state. It held six rows -- `builtin_interfaces`'s
# `Time` and `Duration` times three crates -- because codegen emitted each
# driver package's whole transitive closure into its own flat directory, and
# three of the four closures contained `builtin_interfaces`. phase-465
# collapsed the four trees into ONE, so each ament package is emitted once and
# there is nothing left to baseline.
#
# A RATCHET, not an allowlist: this file may only shrink. That is what made the
# collapse provable rather than merely claimed -- a baselined duplicate that
# stops duplicating fails as STALE, so the six rows had to go in the same
# commit as the collapse, and a shipped duplicate that came BACK would fail as
# new. It is also still what keeps a fourth copy from arriving unnoticed, which
# is exactly how the third arrived.
#
# Format: <wire type name><TAB><crate directory>, one line per claimant.
# Regenerate: python3 scripts/check-message-crate-identity.py --write-baseline
"""

DEP_TABLES = ("dependencies", "dev-dependencies", "build-dependencies")

TYPE_NAME_RE = re.compile(r"const\s+TYPE_NAME\s*:\s*&'static\s+str\s*=\s*\"([^\"]*)\"")
CFG_TEST_RE = re.compile(r"#!?\[cfg\(test\)\]")


# --------------------------------------------------------------------------
# Rust scanning
# --------------------------------------------------------------------------


def blank_comments(src: str):
    """Replace comment bodies with spaces; report which offsets sit in a string.

    Offsets are preserved, so a regex match on the result indexes the original.
    Brace counting must ignore braces inside string and char literals, hence the
    mask rather than a second pass.
    """
    out = list(src)
    in_string = [False] * len(src)
    i, n = 0, len(src)
    state = None  # None | "line" | "block" | "str" | "char" | "raw"
    raw_hashes = 0
    while i < n:
        c = src[i]
        nxt = src[i + 1] if i + 1 < n else ""
        if state is None:
            if c == "/" and nxt == "/":
                state = "line"
                out[i] = out[i + 1] = " "
                i += 2
                continue
            if c == "/" and nxt == "*":
                state = "block"
                out[i] = out[i + 1] = " "
                i += 2
                continue
            if c == "r" and nxt in ('"', "#"):
                j = i + 1
                h = 0
                while j < n and src[j] == "#":
                    h += 1
                    j += 1
                if j < n and src[j] == '"':
                    state, raw_hashes = "raw", h
                    in_string[i] = True
                    i = j + 1
                    continue
            if c == '"':
                state = "str"
                in_string[i] = True
                i += 1
                continue
            if c == "'":
                # A lifetime (`&'static`) is not a char literal. Only treat it
                # as one when it closes within a few chars.
                m = re.match(r"'(\\.|[^'\\])'", src[i:])
                if m:
                    state = "char"
                    in_string[i] = True
                    i += 1
                    continue
            i += 1
            continue
        if state == "line":
            if c == "\n":
                state = None
            else:
                out[i] = " "
            i += 1
            continue
        if state == "block":
            out[i] = " " if c != "\n" else "\n"
            if c == "*" and nxt == "/":
                out[i + 1] = " "
                state = None
                i += 2
                continue
            i += 1
            continue
        if state == "raw":
            in_string[i] = True
            if c == '"' and src[i + 1 : i + 1 + raw_hashes] == "#" * raw_hashes:
                for k in range(i, min(n, i + 1 + raw_hashes)):
                    in_string[k] = True
                i += 1 + raw_hashes
                state = None
                continue
            i += 1
            continue
        # "str" / "char"
        in_string[i] = True
        if c == "\\":
            if i + 1 < n:
                in_string[i + 1] = True
            i += 2
            continue
        if (state == "str" and c == '"') or (state == "char" and c == "'"):
            state = None
        i += 1
    return "".join(out), in_string


def cfg_test_spans(code: str, in_string) -> list:
    """Half-open [start, end) offsets covered by a `#[cfg(test)]` item."""
    spans = []
    for m in CFG_TEST_RE.finditer(code):
        if m.group(0).startswith("#!"):
            return [(0, len(code))]  # inner attribute — the whole file is test
        j = m.end()
        while j < len(code) and (code[j] != "{" or in_string[j]):
            if code[j] == ";" and not in_string[j]:
                j = -1
                break
            j += 1
        if j < 0 or j >= len(code):
            continue
        depth, k = 0, j
        while k < len(code):
            if not in_string[k]:
                if code[k] == "{":
                    depth += 1
                elif code[k] == "}":
                    depth -= 1
                    if depth == 0:
                        break
            k += 1
        spans.append((m.start(), k + 1))
    return spans


def wire_claims_in(src: str) -> list:
    """ROS-form wire type names declared OUTSIDE any `#[cfg(test)]` item."""
    code, in_string = blank_comments(src)
    spans = cfg_test_spans(code, in_string)
    found = []
    for m in TYPE_NAME_RE.finditer(code):
        name = m.group(1)
        if "/" not in name:
            continue  # the `pkg::msg::dds_::T_` spelling, same type
        if any(a <= m.start() < b for a, b in spans):
            continue
        found.append(name.rstrip("\0"))
    return found


# --------------------------------------------------------------------------
# Tree scanning
# --------------------------------------------------------------------------


def tracked(pattern: str) -> list:
    out = subprocess.run(
        ["git", "ls-files", "-z", pattern],
        cwd=ROOT,
        capture_output=True,
        check=True,
    ).stdout.decode()
    return [p for p in out.split("\0") if p]


def is_generated_crate(manifest_path: str) -> bool:
    """A committed generated message crate.

    Keyed on a `generated` path component, which is the convention every
    codegen output tree follows. A USER's generated tree is gitignored and
    per-host, so a tracked-file scan sees exactly the core pre-generated set
    (`packages/interfaces/*`), which is CLAUDE.md's deliberate exception to the
    "never commit `generated/`" rule.
    """
    return "generated" in manifest_path.split(os.sep)


def load(manifest_path: str):
    try:
        with open(os.path.join(ROOT, manifest_path), "rb") as fh:
            return tomllib.load(fh)
    except (FileNotFoundError, Exception):  # noqa: B014 - parse is another gate's job
        return None


def ships_prefixed(crate_name: str) -> bool:
    """RULE 5 — is this a name a consumer's own generated tree cannot collide with?

    `nros sync` names a crate after its ament package verbatim, so anything in
    the `nros-` namespace is safe by construction and anything outside it is a
    name a consumer may also produce.
    """
    return crate_name.startswith("nros-")


def links_key(crate_name: str) -> str:
    """The `links` a generated crate named `crate_name` must declare.

    The one Rust spelling is `rosidl_codegen::BoundInventory::links_key`; this
    is the gate's copy of a four-token formula, and the crates it reads are the
    cross-check that the two agree.
    """
    return "nros_msgs_" + re.sub(r"[-./]", "_", crate_name)


def scan_manifests(manifests: list):
    gen_names, bad_version, versioned_rows, bad_links, with_links = {}, [], [], [], []
    unprefixed = []
    docs = {}
    for path in manifests:
        doc = load(path)
        if doc is None:
            continue
        docs[path] = doc
        if not is_generated_crate(path):
            continue
        pkg = doc.get("package") or {}
        name = pkg.get("name")
        if not name:
            continue
        gen_names[name] = path
        # RULE 5 — a SHIPPED generated crate is in the `nros-` namespace.
        #
        # Two `path` packages sharing a name and a version are a hard cargo
        # error with no workaround ("package collision in the lockfile: … only
        # one can be written to lockfile unambiguously"), and a consumer's own
        # `nros sync` produces a crate named after each ament package verbatim.
        # So a committed crate named `geometry_msgs` would make every leaf that
        # also generates `geometry_msgs` unresolvable. It is not hypothetical:
        # `geometry_msgs` arrives in the interfaces closure via diagnostic_msgs'
        # ament deps, unrenamed and unused, and only the recipe's `rm -rf`
        # keeps it out of the tree (phase-465 W4). That is one line standing
        # between a regeneration and a consumer-facing resolve failure, so the
        # tree is checked rather than the recipe trusted.
        if not ships_prefixed(name):
            unprefixed.append((path, name))
        version = pkg.get("version")
        if version != "0.0.0":
            shown = "version.workspace = true" if version is None else repr(version)
            bad_version.append((path, name, shown))
        # RULE 4 — a crate with NO `links` is fine (pre-phase-403 vintages emit
        # no bounds `build.rs`, so there is no channel to carry). One that HAS
        # it must have derived it from the name it ships under.
        links = pkg.get("links")
        if links is not None:
            with_links.append(name)
            if links != links_key(name):
                bad_links.append((path, name, links, links_key(name)))

    for path, doc in docs.items():
        for table in DEP_TABLES:
            for key, spec in (doc.get(table) or {}).items():
                if not isinstance(spec, dict):
                    continue
                dep_name = spec.get("package", key)
                if dep_name in gen_names and "version" in spec:
                    versioned_rows.append((path, table, key, spec["version"]))
        for _plat, tdoc in (doc.get("target") or {}).items():
            if not isinstance(tdoc, dict):
                continue
            for table in DEP_TABLES:
                for key, spec in (tdoc.get(table) or {}).items():
                    if not isinstance(spec, dict):
                        continue
                    dep_name = spec.get("package", key)
                    if dep_name in gen_names and "version" in spec:
                        versioned_rows.append(
                            (path, "target." + table, key, spec["version"])
                        )
    return gen_names, bad_version, versioned_rows, bad_links, with_links, unprefixed


def crate_of(rs_path: str, manifest_dirs) -> str:
    d = os.path.dirname(rs_path)
    while d:
        if d in manifest_dirs:
            return d
        d = os.path.dirname(d)
    return ""


def scan_wire_claims(manifests: list):
    manifest_dirs = {os.path.dirname(p) for p in manifests}
    claims = {}
    for rs in tracked("*.rs"):
        parts = rs.split(os.sep)
        if "tests" in parts or "benches" in parts:
            continue  # a test target is not linked into an image
        try:
            with open(os.path.join(ROOT, rs), encoding="utf-8", errors="replace") as fh:
                src = fh.read()
        except OSError:
            continue
        if "TYPE_NAME" not in src:
            continue
        owner = crate_of(rs, manifest_dirs)
        if not owner:
            continue
        for name in wire_claims_in(src):
            claims.setdefault(name, set()).add(owner)
    return {k: v for k, v in claims.items() if len(v) > 1}


# --------------------------------------------------------------------------
# Baseline
# --------------------------------------------------------------------------


def read_baseline():
    pairs = set()
    if not os.path.exists(BASELINE):
        return pairs
    with open(BASELINE, encoding="utf-8") as fh:
        for line in fh:
            line = line.rstrip("\n")
            if not line.strip() or line.lstrip().startswith("#"):
                continue
            name, _, crate = line.partition("\t")
            pairs.add((name.strip(), crate.strip()))
    return pairs


def write_baseline(dupes):
    rows = sorted((n, c) for n, cs in dupes.items() for c in cs)
    with open(BASELINE, "w", encoding="utf-8") as fh:
        fh.write(BASELINE_HEADER)
        for name, crate in rows:
            fh.write(f"{name}\t{crate}\n")
    print(f"wrote {BASELINE} ({len(rows)} row(s))")


# --------------------------------------------------------------------------
# Selftest — runs on the NORMAL path (phase-395: a negative control nobody
# runs decays into a comment).
# --------------------------------------------------------------------------


def selftest() -> None:
    src_test_only = """
        struct Real;
        impl Message for Real { const TYPE_NAME: &'static str = "a/msg/Real"; }
        #[cfg(test)]
        mod tests {
            struct Fixture;
            impl Message for Fixture { const TYPE_NAME: &'static str = "a/msg/Fixture"; }
        }
    """
    got = wire_claims_in(src_test_only)
    assert got == ["a/msg/Real"], f"cfg(test) exclusion broken: {got}"

    # A commented-out declaration is not a claim.
    src_comment = """
        // const TYPE_NAME: &'static str = "b/msg/Commented";
        /* const TYPE_NAME: &'static str = "b/msg/Blocked"; */
        impl Message for X { const TYPE_NAME: &'static str = "b/msg/Live"; }
    """
    got = wire_claims_in(src_comment)
    assert got == ["b/msg/Live"], f"comment handling broken: {got}"

    # The `dds_` spelling is the same type, counted once via the ROS form.
    src_dds = """
        impl T for X { const TYPE_NAME: &'static str = "c::msg::dds_::X_"; }
        impl Message for X { const TYPE_NAME: &'static str = "c/msg/X"; }
    """
    assert wire_claims_in(src_dds) == ["c/msg/X"], "dds_ spelling not skipped"

    # A lifetime must not be read as a char literal and swallow the line.
    assert wire_claims_in(
        'impl M for Y { const TYPE_NAME: &\'static str = "d/msg/Y"; }'
    ) == ["d/msg/Y"], "lifetime handling broken"

    # A nested cfg(test) module inside a live module still excludes only itself.
    src_nested = """
        mod outer {
            impl M for A { const TYPE_NAME: &'static str = "e/msg/A"; }
            #[cfg(test)]
            mod t { impl M for B { const TYPE_NAME: &'static str = "e/msg/B"; } }
            impl M for C { const TYPE_NAME: &'static str = "e/msg/C"; }
        }
    """
    got = wire_claims_in(src_nested)
    assert got == ["e/msg/A", "e/msg/C"], f"nested cfg(test) broken: {got}"

    # The manifest rules, on planted documents.
    assert is_generated_crate(
        "packages/interfaces/x/generated/humble/nros-y/Cargo.toml"
    )
    assert not is_generated_crate("packages/core/nros-node/Cargo.toml")

    # RULE 4 — the formula, and the two directions that matter. The value 1455
    # measured is what the shipped `-clock` crate carried; the value the rule
    # demands is derived from the name it ships under.
    assert links_key("builtin_interfaces") == "nros_msgs_builtin_interfaces"
    assert (
        links_key("nros-builtin-interfaces-clock")
        == "nros_msgs_nros_builtin_interfaces_clock"
    ), links_key("nros-builtin-interfaces-clock")
    # A renamed crate and a consumer's own copy must NOT share a key...
    assert links_key("nros-builtin-interfaces-clock") != links_key("builtin_interfaces")
    # ...and two copies shipping under ONE name must, or the gate has stopped
    # describing what cargo does.
    assert links_key("nros-builtin-interfaces") == links_key("nros-builtin-interfaces")

    # RULE 5 — both directions, including the crate that would actually arrive.
    # `geometry_msgs` is the one the interfaces closure emits unrenamed, and the
    # only thing keeping it out of the tree is one line in the recipe.
    assert ships_prefixed("nros-builtin-interfaces")
    assert not ships_prefixed("geometry_msgs")
    assert not ships_prefixed("builtin_interfaces")
    # Not a substring test: a name that merely CONTAINS the prefix still collides.
    assert not ships_prefixed("my-nros-msgs")


# --------------------------------------------------------------------------


def main() -> int:
    audit = "--audit" in sys.argv
    selftest()

    manifests = tracked("*Cargo.toml")
    gen_names, bad_version, versioned_rows, bad_links, with_links, unprefixed = (
        scan_manifests(manifests)
    )
    dupes = scan_wire_claims(manifests)

    if "--write-baseline" in sys.argv:
        write_baseline(dupes)
        return 0

    baseline = read_baseline()
    observed = {(n, c) for n, cs in dupes.items() for c in cs}
    new_dupes = sorted(observed - baseline)
    stale = sorted(baseline - observed)

    if audit:
        print(f"generated message crates (tracked): {len(gen_names)}")
        for name, path in sorted(gen_names.items()):
            print(f"  {name}  {path}")
        print(f"\nwire types claimed by >1 shipped crate: {len(dupes)}")
        for name in sorted(dupes):
            print(f"  {name}")
            for c in sorted(dupes[name]):
                print(f"      {c}")
        print(f"\nversion-carrying dep rows: {len(versioned_rows)}")
        print(f"crates not at 0.0.0: {len(bad_version)}")
        print(f"crates whose `links` does not follow their name: {len(bad_links)}")
        print(f"crates shipping an unprefixed ament name: {len(unprefixed)}")
        return 0

    failed = False

    if bad_version:
        failed = True
        print(
            "generated message crate(s) whose version is not the constant `0.0.0`:",
            file=sys.stderr,
        )
        for path, name, shown in bad_version:
            print(f"  {name}: {shown}    {path}", file=sys.stderr)
        print(
            "\nA generated crate's version must be the CONSTANT `0.0.0` (issue 0394):\n"
            "the release version in this field is what consumers then pin, and a\n"
            "pinned `^0.5.0` stops resolving when the workspace version bumps.",
            file=sys.stderr,
        )

    if versioned_rows:
        failed = True
        print(
            "\ndep row(s) pinning a generated message crate's version:", file=sys.stderr
        )
        for path, table, key, ver in versioned_rows:
            print(
                f"  {path}  [{table}] {key} = {{ version = {ver!r}, ... }}",
                file=sys.stderr,
            )
        print(
            "\nA path dep's `version` is still a REQUIREMENT. Drop the field and keep\n"
            "`path` alone — CLAUDE.md, issue 0394 (which broke root-workspace\n"
            "resolution twice). This is resolve-time, so it takes every cargo command\n"
            "in the tree, not just this consumer.",
            file=sys.stderr,
        )

    if bad_links:
        failed = True
        print(
            "\ngenerated message crate(s) whose `links` does not follow their name:",
            file=sys.stderr,
        )
        for path, name, got, want in bad_links:
            print(
                f"  {name}: links = {got!r}, must be {want!r}    {path}",
                file=sys.stderr,
            )
        print(
            "\n`links` is global to the dependency graph, so it is an identity axis\n"
            "like the name and the version (RFC-0067 §D4, issue 1455). A crate that\n"
            "keeps its ament package's key after being renamed into `nros-` collides\n"
            "with a consumer's own generated copy of that package, at RESOLVE time —\n"
            "every cargo command in that leaf. Regenerate the tree (`just\n"
            "generate-interfaces`); the `--rename` pass recomputes this value now.",
            file=sys.stderr,
        )

    if unprefixed:
        failed = True
        print(
            "\ntracked generated message crate(s) shipping an UNPREFIXED name:",
            file=sys.stderr,
        )
        for path, name in unprefixed:
            print(f"  {name}    {path}", file=sys.stderr)
        print(
            "\nA shipped crate must be in the `nros-` namespace (RFC-0067 §D4). A\n"
            "consumer's own `nros sync` emits a crate named after each ament package\n"
            "verbatim, and two `path` packages sharing a name and a version are a hard\n"
            "cargo error with no workaround — 'package collision in the lockfile: …\n"
            "only one can be written to lockfile unambiguously'. `geometry_msgs` reaches\n"
            "the interfaces closure via diagnostic_msgs' ament deps and is dropped by\n"
            "`just generate-interfaces`; if one is here, that drop did not happen.",
            file=sys.stderr,
        )

    if new_dupes:
        failed = True
        print(
            "\nwire type(s) newly claimed by more than one shipped crate:",
            file=sys.stderr,
        )
        for name, crate in new_dupes:
            print(f"  {name}  claimed by  {crate}", file=sys.stderr)
        print(
            "\nOne type on the wire, two types in Rust, and no conversion between\n"
            "them. If this copy is deliberate, it needs a decision recorded in issue\n"
            "1428 and a line in\n"
            f"  {os.path.relpath(BASELINE, ROOT)}\n"
            "which is a shrink-only ratchet, not an allowlist.",
            file=sys.stderr,
        )

    if stale:
        failed = True
        print("\nbaselined duplicate(s) that no longer duplicate:", file=sys.stderr)
        for name, crate in stale:
            print(f"  {name}  {crate}", file=sys.stderr)
        print(
            "\nThe debt shrank -- delete these lines. A stale entry is the issue-0743\n"
            "class: it silently stops meaning anything while still reading as cover.",
            file=sys.stderr,
        )

    if failed:
        return 1

    print(
        f"check-message-crate-identity: OK ({len(gen_names)} generated crate(s), "
        f"{len(with_links)} declaring `links`, "
        f"{len(manifests)} manifest(s), {len(baseline)} baselined duplicate claim(s))"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
