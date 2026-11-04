#!/usr/bin/env python3
"""phase-348 W2 — discovery and resolution claim the same names.

A provider says what it IS twice: its `package.xml` announces
`<nano_ros_provides kind=… name=…/>` so a source-time scan can FIND it, and its
descriptor (`nros-rmw.toml`, `nros-board.toml`) declares the names a consumer
RESOLVES against. Nothing structural keeps the two equal, so this compares
them.

  A1  a package.xml sitting beside a descriptor announces provisions of that
      kind — otherwise it is invisible to the scan while looking migrated;
  A2  for a NAMED family, its provision names equal the descriptor's declared
      names EXACTLY and in order, canonical first, since names[0] is what
      error messages list;
  A2n for a NAMELESS family, the descriptor declares no names at all — the
      announcement is the only spelling (RFC-0087 D4);
  A3  a package.xml ANNOUNCING a kind has a sibling descriptor of that kind —
      A1 read from the other side, and the inverse of the board family's
      `board_descriptor.rs::require_announcement`;
  A4  every family's globs REACH every descriptor of its kind in the tree —
      the rule's coverage equals the rule;
  A5  a package.xml is well-formed XML, because every rule here is a REGEX and
      a regex reads a file no parser accepts.

**A2 and A2n are the same rule seen from two sides.** A family is nameless
once its readers DERIVE the names from the announcement instead of reading
them out of the descriptor; then a `names` key in a descriptor is not a
duplicate to be compared, it is a duplicate that nothing reads — worse than a
disagreement, because it can be edited with no effect. A2n refuses it, so the
rule "the announcement is the only spelling" keeps a gate after the comparison
it used to have stops existing.

**One gate for every family, not one per family.** This started as S5 inside
`check-rmw-descriptors.py`, covering rmw alone. Extending the rule to boards by
adding a second copy next to the board descriptors is exactly the
second-spelling antipattern this repo keeps paying for (see the Zephyr
unset-variable guard, #282 → #326), so S5 moved here instead and
`check-rmw-descriptors.py` kept only what is rmw-SPECIFIC (S1–S4). Adding a
`platform` family later means one row in FAMILIES, not another script.

**A3 and A4 are issue 1220, and they are the same defect at two altitudes.**
A1 starts at a DESCRIPTOR and asks about the package.xml, so it can only see
what its family's glob reaches; for `platform` that glob was `config/*` while
five of the eight descriptors had moved to `packages/platform/` (phase-400 W1)
and four announcements had stayed behind in `config/` with nothing beside them.
Both halves of that split were invisible and the gate printed OK. The rule was
right; its reach was not, which is this repo's recurring shape (the 2026-07-28
audit found four, issue 1226 was a fifth).

So A3 does not glob at all: it enumerates TRACKED `package.xml` and starts from
the announcement, which is a set no path pattern can be narrower than. And A4
holds the A1/A2 globs to the tree — every tracked `nros-<kind>.toml` is either
reached by its family's globs or named in `REACH_EXEMPT` with a reason, and an
exemption that matches nothing is an error. This used to say "a package.xml
with no descriptor beside it is NOT checked: the migration proceeds one
provider at a time". The migration finished; that skip is A3 now, the same way
phase-375 W6 turned A1's identical skip into a rule.

**A5 exists because writing A3's fix broke it.** Every rule above is a regex
over `package.xml`, and a regex is happy with a document no XML parser accepts
— so a provider whose file will not parse passes this gate and is INVISIBLE to
`provider_scan`, which demotes a parse failure to a `ScanError` warning by
design (*"one malformed package.xml somewhere in a large tree must not make
every provider undiscoverable ... the CLI prints them, and a gate can make them
fatal"*). No gate did. Measured 2026-09-10: a `<x>` inside a `<description>`
took all five platform providers out of `nros dev ws providers` while this gate
printed OK, which is exactly the harm A1's own message describes. All 417
tracked package.xml parse; A5 keeps it that way.

**A6–A8 are the host-tool kinds of RFC-0119 (phase-527 W1).** `runner` and
`command` packages are built for the HOST, which follows from their kind:

  A6  a package announces host kinds (`runner`, `command`) or target kinds
      (`rmw`, `board`, `platform`, `serdes`), never both — it would need two
      build roles and so two target directories (RFC-0119 D3);
  A7  a `command` is not named like a built-in or reserved verb — announcing
      one is a refusal at discovery, not a shadow (RFC-0119 D7). The verb list
      is `packages/cli/nros-cli-core/builtin-verbs.txt`, generated from the
      clap tree by a test in that crate, because this gate cannot ask clap;
  A8  a stated descriptor field equals its derived value. The one derivable
      field of these kinds is `bin` (the package's single Cargo bin target);
      a stated `bin` naming none of several targets is refused too.

`BORN_EMPTY` lists the families that exist before any provider of them does,
each with the work item that ships the first one. The empty-set refusal is
waived for them, and an entry whose glob has started matching is an error, so
the waiver retires itself.

Buildless — TOML, a regex, and stdlib XML. No cmake, no cargo.
"""

import fnmatch
import glob
import os
import re
import sys
from collections import namedtuple
from pathlib import Path
from xml.etree import ElementTree

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "lib"))
from tracked import tracked  # noqa: E402 — issue 0721: index lookup, not a walk

try:
    import tomllib  # 3.11+
except ModuleNotFoundError:  # 3.10 backport, same spelling as the sibling gates
    import tomli as tomllib

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# kind -> (descriptor glob, how to pull the declared names out of it).
#
# `extract=None` means a NAMELESS family: its readers derive the names from the
# `<nano_ros_provides>` announcements, so the descriptor must not declare any
# (A2n). Everything else about the row is unchanged — A1 still applies, because
# a provider that announces nothing is invisible to the scan whatever its
# descriptor says.
#
# The named families disagree in shape and that is not an accident: an rmw
# descriptor is ONE backend (`[rmw]`), while a board descriptor is an ARRAY
# (`[[board]]`) because one package can ship several boards — nros-board-nuttx-qemu
# declares both the ARM and RISC-V variants, disambiguated by `target_contains`.
# So the board reader flattens entries in declaration order.
FAMILIES = {
    # phase-420 W5 — rmw went NAMELESS. `cargo-nano-ros/build.rs` reads the
    # announcements (via `derived_descriptor::announced_names`) and derives the
    # rest of the lowering from names[0], so `[rmw].names` is deleted from all
    # four descriptors. A2 has nothing left to compare; A2n keeps it that way.
    "rmw": ("packages/rmw/*/*/nros-rmw.toml", None),
    # Board is still NAMED, and phase-420 W5 measured why rather than assuming:
    # `nros-board-nuttx-qemu` declares TWO `[[board]]` entries and announces
    # seven names in one flat list, so the announcement cannot say which four
    # belong to the ARM variant and which three to the RISC-V one. Deriving
    # per-entry names needs a boundary the tag has no way to carry. The board
    # reader also lives in `nros-cli-core`, not here.
    # phase-375 W6 / RFC-0064 R5 D2: TWO globs, because a bundle board lives one
    # level deeper (`nros-board-zephyr/boards/<bundle>/`). The Rust-side walk
    # became recursive; this gate does not walk, so it states both depths.
    #
    # This line used to end "and `check-board-glob-depth` holds them equal to
    # what the walk finds". There is no such gate, and there never was —
    # measured 2026-09-10, `grep -rn check-board-glob-depth` matches this
    # comment and nothing else. A third board depth would have gone unglobbed
    # under a comment saying otherwise, which is issue 1220 one family over. A4
    # is what holds it now, and A4 exists rather than being cited.
    "board": (
        ["packages/boards/*/nros-board.toml", "packages/boards/*/boards/*/nros-board.toml"],
        lambda d: [n for b in d.get("board", []) for n in b.get("names", [])],
    ),
    # TWO roots, in `PlatformsTree::default_search_path` order, and neither is
    # residue. phase-400 W1 moved the descriptor of every platform that HAS a
    # package beside that package — `posix`, `zephyr`, `freertos`, `nuttx`,
    # `threadx` — and deliberately left `bare-metal` and `generic` in `config/`,
    # because neither is a port and neither names a package to sit beside. So
    # `packages/platform/` is where a platform provider lives and `config/` is
    # where the two fallbacks live; the search path says the same, with
    # `packages/platform` first.
    #
    # issue 1220 — this row read `config/*` alone, under a comment asserting
    # "platform descriptors live under `config/`, not `packages/platform/`".
    # True when phase-349 W1 wrote it, false from phase-400 W1 on, and the row
    # covered 3 of 8 descriptors while reporting on a family. `names` is
    # top-level, not in a table, and the loader falls back to the directory name
    # for a file that declares none.
    "platform": (
        [
            "packages/platform/*/nros-platform.toml",
            "config/*/nros-platform.toml",
        ],
        lambda d: list(d.get("names", [])),
    ),
    # phase-421 W4 / RFC-0088 D6 — the first family BORN nameless, where rmw was
    # made nameless afterwards by W5. `nros-serdes.toml` carries `impl` and
    # `format_id`, the two facts no convention can derive, and the announcement
    # is the name. Everything serdes-SPECIFIC — descriptor well-formedness, the
    # `format_id` discriminant, and the descriptor a package announcing `serdes`
    # must have — lives in `scripts/check-serdes-descriptors.py`, the split that
    # keeps this gate one gate for every family rather than one per family.
    "serdes": ("packages/*/*/nros-serdes.toml", None),
    # phase-527 W1 / RFC-0119 D1–D2 — the two HOST-TOOL kinds, both born
    # nameless: the verb or runner name is the announcement, and the
    # descriptor holds `protocol`, `verbs`/`needs` or `summary`/`tier`, and an
    # optional `bin`. Runners live in `packages/runners/` (D5); commands live
    # beside the domain they serve (`packages/rmw/zenoh/nros_zenoh_tools`),
    # one or two levels below a `packages/<dir>/`.
    "runner": ("packages/runners/*/nros-runner.toml", None),
    "command": (
        ["packages/*/*/nros-command.toml", "packages/*/*/*/nros-command.toml"],
        None,
    ),
}

# Families with no provider in the tree YET, and the work item that ships the
# first. Without this the "refusing to pass on an empty set" arm would make a
# new family unlandable before its first provider, which is RFC-0087 D5's
# order: a family is defined, then a package that needs it is written. A row
# whose glob has started matching is an ERROR — the waiver is not a place to
# park a family, and it must leave the moment it stops being true.
BORN_EMPTY = {}

# RFC-0119 D3 — what a package is built for follows from its kinds.
HOST_KINDS = {"runner", "command"}
TARGET_KINDS = {"rmw", "board", "platform", "serdes"}

# A7's verb list. GENERATED from the clap tree (see the module docstring).
BUILTIN_VERBS = "packages/cli/nros-cli-core/builtin-verbs.txt"


# A4 — the files named like a descriptor that a family glob is RIGHT not to
# reach, each with the reason. Every entry must match at least one tracked
# file: a stale exemption is an error, not a no-op, because it is how a rule
# gets reclaimed by accident (`check-derived-descriptor-fields` D3, and the
# same ratchet shape as `build-type-spelling-baseline.json`).
#
# `forbid_key` makes an exemption a MEASUREMENT rather than a claim: where the
# reason is "this is a name collision, not a descriptor", the file is parsed
# and must NOT carry the family's descriptor table. If `nros configure` ever started
# writing a real board descriptor into a leaf's `.cargo/`, the exemption would
# fail instead of hiding it.
#
# (pattern, reason, forbid_key) — `pattern` is fnmatch against the repo-relative
# path.
#
# phase-445 W6 deleted the row this list was written for — `*/.cargo/
# nros-board.toml`, the per-leaf cargo-config PROJECTION, a name collision with
# the board descriptor rather than one of them. The projection is retired
# (RFC-0098 D1), and this gate's own stale-exemption arm said so in exactly the
# words it was given: "matched nothing … delete the row". The `forbid_key`
# MECHANISM it exercised does NOT retire with it — the self-test installs a
# synthetic row (`SELF_TEST_EXEMPT`) so that control keeps running on the
# normal path.
REACH_EXEMPT = (
    (
        "packages/cli/nros-cli-core/tests/fixtures/*",
        "a test workspace's own provider tree, built and asserted by the tests "
        "that own it; it is a FIXTURE, not a provider this tree ships",
        None,
    ),
)

# SELF-TEST ONLY, prepended to [`REACH_EXEMPT`] for the duration of the control
# run. It is the shape no shipped row has any more: an exemption whose PREMISE
# is "this file's name collides with a descriptor and it is not one". Without
# it the `forbid_key` arm would still be code and would be exercised by nothing,
# which is the "a negative control nobody runs decays into a comment" failure
# `check-gate-selftests` exists for.
SELF_TEST_EXEMPT = (
    (
        "leaf/nros-board.toml",
        "SELF-TEST ONLY — a name collision with the board descriptor, not one",
        "board",
    ),
)

# What A3 and A4 examined, so the OK line can say it. A gate that prints a count
# of what it CHECKED and not of what it REACHED is exactly how issue 1220 read
# as covering a family it covered three-eighths of.
Reach = namedtuple("Reach", "announcements descriptors")

PROVIDES_RE = r'<nano_ros_provides\s+kind="{kind}"\s+name="([^"]+)"\s*/?>'
# Any `kind="..."` on a provision tag, whatever the family. A3 harvests this
# rather than iterating FAMILIES, so a kind announced in the tree with no
# FAMILIES row is REPORTED instead of silently uncovered.
PROVIDES_KIND_RE = re.compile(r'<nano_ros_provides\s+kind="([^"]+)"')
COMMENT_RE = re.compile(r"<!--([^-]|-[^-])*-->")


def declared_provisions(path, kind):
    """Provision names of one kind, in file order, comments stripped.

    The strip is not optional (issue 0516): a provider's package.xml documents
    the provision tag in a comment, and a regex cannot tell that from a
    declaration. Without it a commented-out example counts as a claimed name.
    """
    with open(path, encoding="utf-8") as fh:
        body = COMMENT_RE.sub("", fh.read())
    return re.findall(PROVIDES_RE.format(kind=kind), body)


def restated_names(data):
    """Every `names` key anywhere in a descriptor, flattened.

    A nameless family's descriptor must declare none, and the search is over
    the WHOLE document rather than one known table: the two live shapes put
    `names` in `[rmw]`, in `[[board]]` entries and at top level
    (`nros-platform.toml`), and a rule that only looked where today's families
    happen to keep it would pass the next family's restatement.
    """
    out = []

    def walk(node):
        if isinstance(node, dict):
            for k, v in node.items():
                if k == "names" and isinstance(v, list):
                    out.extend(v)
                else:
                    walk(v)
        elif isinstance(node, list):
            for v in node:
                walk(v)

    walk(data)
    return out


def scan(root):
    """(problems, checked, announced, Reach) for the tree at `root`.

    Split out of `main` so the negative control below can run the real rule
    against a fixture tree instead of asserting about it in prose. The gate had
    no control at all, which is how the "not migrated yet" skip below survived
    its own migration.
    """
    return _scan_impl(root)


def _write(path, body):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w", encoding="utf-8") as fh:
        fh.write(body)


def _pkg_xml(*kinds):
    tags = "".join(f'<nano_ros_provides kind="{k}" name="x"/>' for k in kinds)
    return f"<package><export>{tags}</export></package>"


def _descriptor_body(kind):
    """The minimal well-formed descriptor of a family, by shape."""
    return {
        "board": '[[board]]\nnames = ["x"]\n',
        "rmw": "[rmw]\n",
        "platform": 'names = ["x"]\n',
        "runner": 'protocol = 1\nverbs = ["run"]\n',
        "command": 'protocol = 1\nsummary = "x"\n',
    }.get(kind, "")


def load_builtin_verbs(root):
    """{tier: set(verbs)} from BUILTIN_VERBS, or None when the file is absent.

    Lines are `<tier> <verb>`, tier one of `user`, `dev`, `reserved`; `#`
    starts a comment.
    """
    path = os.path.join(root, BUILTIN_VERBS)
    if not os.path.exists(path):
        return None
    tiers = {"user": set(), "dev": set(), "reserved": set()}
    with open(path, encoding="utf-8") as fh:
        for raw in fh:
            line = raw.split("#", 1)[0].strip()
            if not line:
                continue
            tier, _, verb = line.partition(" ")
            tiers.setdefault(tier, set()).add(verb.strip())
    return tiers


def cargo_bins(pkg_dir):
    """Every bin target of the package's Cargo.toml, explicit or implicit.

    The Python twin of `extension_descriptor::cargo_bins`: `[[bin]]` entries
    name themselves, and a manifest with none has the implicit `src/main.rs`
    target named after the package. Empty for a package with no Cargo.toml (a
    CMake package's executable is not derivable).
    """
    manifest = os.path.join(pkg_dir, "Cargo.toml")
    if not os.path.exists(manifest):
        return []
    try:
        with open(manifest, "rb") as fh:
            data = tomllib.load(fh)
    except Exception:  # noqa: BLE001 — not this gate's file to judge
        return []
    explicit = [b["name"] for b in data.get("bin", []) if "name" in b]
    if explicit:
        return explicit
    name = data.get("package", {}).get("name")
    if name and os.path.exists(os.path.join(pkg_dir, "src", "main.rs")):
        return [name]
    return []


def check_host_package(root, pkg_xml, kinds, verbs):
    """A6–A8 for one package. -> problems"""
    rel = os.path.relpath(pkg_xml, root)
    pkg_dir = os.path.dirname(pkg_xml)
    problems = []
    host = [k for k in kinds if k in HOST_KINDS]
    target = [k for k in kinds if k in TARGET_KINDS]
    if host and target:
        problems.append(
            f"{rel}: announces host-tool kinds {host} and target kinds {target} "
            f"in one package. A runner or command is built for the HOST and a "
            f"provider of the others is linked into an image, so one package "
            f"would need two build roles and two target directories.\n"
            f"    Split it: a board vendor ships a board package and a runner "
            f"package, two packages in one repository (RFC-0119 D3)."
        )
    for kind in host:
        desc = os.path.join(pkg_dir, f"nros-{kind}.toml")
        if not os.path.exists(desc):
            continue  # A3 reports it
        desc_rel = os.path.relpath(desc, root)
        try:
            with open(desc, "rb") as fh:
                data = tomllib.load(fh)
        except Exception as e:  # noqa: BLE001 — report, do not raise
            problems.append(f"{desc_rel}: not valid TOML: {e}")
            continue
        # A8 — `bin` is the one derivable field.
        stated = data.get("bin")
        bins = cargo_bins(pkg_dir)
        if stated is not None and len(bins) == 1 and stated != bins[0]:
            problems.append(
                f"{desc_rel}: states bin = {stated!r}, but the package's only bin "
                f"target is {bins[0]!r}. A stated field must equal its derived "
                f"value (RFC-0087 D4): drop the line, or rename the target."
            )
        elif stated is not None and len(bins) > 1 and stated not in bins:
            problems.append(
                f"{desc_rel}: states bin = {stated!r}, which is none of the "
                f"package's bin targets ({', '.join(bins)})."
            )
        # A7 — a command may not take a built-in or reserved verb.
        if kind != "command":
            continue
        if verbs is None:
            problems.append(
                f"{rel}: announces a command, and {BUILTIN_VERBS} is missing, so "
                f"A7 cannot check its verb. Regenerate it (see that file's "
                f"header, or `nros_cli_core::builtin_verbs`)."
            )
            continue
        tier = data.get("tier", "user")
        taken = verbs.get("user", set()) | verbs.get("reserved", set())
        if tier == "dev":
            taken = taken | verbs.get("dev", set())
        for name in declared_provisions(pkg_xml, "command"):
            if name in taken:
                problems.append(
                    f"{rel}: announces command {name!r}, which is a built-in or "
                    f"reserved `nros` verb. A built-in cannot be provided, and "
                    f"announcing one is a refusal at discovery, not a shadow "
                    f"(RFC-0119 D7).\n"
                    f"    Name the verb something else."
                )
    return problems


def self_test(quiet=False):
    """Every rule must FIRE on the tree it exists to refuse.

    Runs on the NORMAL path, not behind a flag: a control nobody runs decays
    into a comment. Same shape as `scripts/check-board-tiers.py`.

    A1 had a control from the start. A3 and A4 get one here because issue 1220
    is precisely a rule that could not fail — this gate exited 0 on a tree where
    five descriptors and four announcements were split across two directories,
    and the reason was reach, which no amount of RUNNING the gate reveals.
    """
    import shutil
    import tempfile

    # Install the synthetic row. Restored after the control block below; an
    # assertion in between ends the process, so there is no path that leaves it
    # installed for the real scan.
    global REACH_EXEMPT
    shipped_exempt = REACH_EXEMPT
    REACH_EXEMPT = SELF_TEST_EXEMPT + shipped_exempt

    def build(tmp):
        """A minimal tree that PASSES every rule, so a mutation names itself."""
        for kind, (pattern, _) in FAMILIES.items():
            if kind in BORN_EMPTY:
                continue  # an empty family must pass; its stale arm is tested below
            pat = pattern if isinstance(pattern, str) else pattern[0]
            # One minimal descriptor per family, so no family trips the
            # "refusing to pass on an empty set" arm...
            dest = os.path.join(tmp, pat.replace("*", "x"))
            # Two families whose globs share a shape (`serdes` and `command`
            # are both `packages/*/*/…`) would write one package.xml over the
            # other; the later one takes a directory named after itself.
            if os.path.exists(os.path.join(os.path.dirname(dest), "package.xml")):
                dest = os.path.join(tmp, pat.replace("*", kind, 1).replace("*", "x"))
            _write(dest, _descriptor_body(kind))
            # ... and its announcement, so the baseline is clean and each
            # assertion below names exactly one offender.
            _write(os.path.join(os.path.dirname(dest), "package.xml"), _pkg_xml(kind))
        # A7's verb list, one verb per tier.
        _write(
            os.path.join(tmp, BUILTIN_VERBS),
            "# test\nuser build\ndev ws\nreserved run\n",
        )
        # One file per REACH_EXEMPT row — the shipped one and the synthetic
        # one — so the "stale exemption" arm is exercised by taking them away
        # rather than only asserted about.
        _write(os.path.join(tmp, "leaf/nros-board.toml"), "[target.x]\n")
        _write(
            os.path.join(
                tmp, "packages/cli/nros-cli-core/tests/fixtures/w/nros-board.toml"
            ),
            '[[board]]\nnames = ["x"]\n',
        )

    def only(problems, needle, why):
        hits = [p for p in problems if needle in p]
        assert len(hits) == 1, f"{why}; got {problems}"

    def none(problems, needle, why):
        hits = [p for p in problems if needle in p]
        assert not hits, f"{why}; got {problems}"

    with tempfile.TemporaryDirectory() as tmp:
        build(tmp)
        problems = scan(tmp)[0]
        assert not problems, f"the baseline tree must pass; got {problems}"

        # A1 — a descriptor whose package.xml is gone.
        board = os.path.join(tmp, "packages/boards/x")
        os.remove(os.path.join(board, "package.xml"))
        only(
            scan(tmp)[0],
            "no sibling package.xml",
            "A1 must name the descriptor with no package.xml, and only it",
        )
        _write(os.path.join(board, "package.xml"), _pkg_xml("board"))
        none(scan(tmp)[0], "no sibling package.xml", "announcing it must silence A1")

        # A3 — an announcement whose descriptor is gone. This is issue 1220's
        # `config/posix/`: a package.xml alone in a directory.
        _write(os.path.join(tmp, "orphan/package.xml"), _pkg_xml("rmw"))
        only(
            scan(tmp)[0],
            "no sibling nros-rmw.toml",
            "A3 must name the announcement with no descriptor",
        )
        shutil.rmtree(os.path.join(tmp, "orphan"))

        # A3 — a kind no FAMILIES row covers is REPORTED, not skipped. Without
        # this a fifth provider family could be announced and unchecked.
        _write(os.path.join(tmp, "unknown/package.xml"), _pkg_xml("transport"))
        only(
            scan(tmp)[0],
            "not a family this gate knows",
            "A3 must refuse an announced kind with no FAMILIES row",
        )
        shutil.rmtree(os.path.join(tmp, "unknown"))

        # A4 — a descriptor outside every glob. This is issue 1220's
        # `packages/platform/*/nros-platform.toml` before the row was widened.
        _write(os.path.join(tmp, "elsewhere/nros-serdes.toml"), "[serdes]\n")
        _write(os.path.join(tmp, "elsewhere/package.xml"), _pkg_xml("serdes"))
        only(
            scan(tmp)[0],
            "no family glob reaches",
            "A4 must name the descriptor the globs miss",
        )
        shutil.rmtree(os.path.join(tmp, "elsewhere"))

        # A4 — an exemption that claims "name collision" is MEASURED. A file
        # that really did declare a board would otherwise ride the exemption
        # out of every rule here.
        _write(
            os.path.join(tmp, "leaf/nros-board.toml"),
            '[[board]]\nnames = ["x"]\n',
        )
        only(
            scan(tmp)[0],
            "it IS a descriptor",
            "A4 must refuse an exemption whose premise stopped holding",
        )
        _write(os.path.join(tmp, "leaf/nros-board.toml"), "[target.x]\n")

        # A5 — a package.xml a regex reads and a parser rejects. Written with
        # the exact mistake that produced it: a bare `<x>` in prose.
        _write(
            os.path.join(tmp, "malformed/package.xml"),
            '<package><description>lives in config/<x>/</description>'
            '<export><nano_ros_provides kind="rmw" name="x"/></export></package>',
        )
        _write(os.path.join(tmp, "malformed/nros-rmw.toml"), "[rmw]\n")
        only(
            scan(tmp)[0],
            "not well-formed XML",
            "A5 must refuse a package.xml no parser accepts",
        )
        shutil.rmtree(os.path.join(tmp, "malformed"))

        # A6 — one package announcing a host kind and a target kind.
        _write(os.path.join(tmp, "mixed/package.xml"), _pkg_xml("board", "runner"))
        _write(os.path.join(tmp, "mixed/nros-board.toml"), _descriptor_body("board"))
        _write(os.path.join(tmp, "mixed/nros-runner.toml"), _descriptor_body("runner"))
        only(
            scan(tmp)[0],
            "host-tool kinds ['runner'] and target kinds ['board']",
            "A6 must refuse a package mixing host and target kinds",
        )
        shutil.rmtree(os.path.join(tmp, "mixed"))

        # A7 — a command named like a built-in verb, a reserved verb, and a
        # dev-tier command named like a `nros dev` verb. A user-tier command
        # may reuse a dev verb's name: it sits at a different level.
        def command_pkg(name, tier):
            _write(
                os.path.join(tmp, "cmd/package.xml"),
                f'<package><export><nano_ros_provides kind="command" name="{name}"/>'
                f"</export></package>",
            )
            _write(
                os.path.join(tmp, "cmd/nros-command.toml"),
                f'protocol = 1\nsummary = "x"\ntier = "{tier}"\n',
            )

        for name, tier in (("build", "user"), ("run", "user"), ("ws", "dev")):
            command_pkg(name, tier)
            only(
                scan(tmp)[0],
                f"announces command {name!r}, which is a built-in or reserved",
                f"A7 must refuse a {tier}-tier command named {name!r}",
            )
        command_pkg("ws", "user")
        none(scan(tmp)[0], "built-in or reserved", "a user-tier `ws` is not `nros dev ws`")
        shutil.rmtree(os.path.join(tmp, "cmd"))

        # A8 — a stated `bin` that differs from the one derived bin target.
        _write(os.path.join(tmp, "packages/runners/r/package.xml"), _pkg_xml("runner"))
        _write(
            os.path.join(tmp, "packages/runners/r/Cargo.toml"),
            '[package]\nname = "r"\n[[bin]]\nname = "nros-runner-r"\n',
        )
        _write(
            os.path.join(tmp, "packages/runners/r/nros-runner.toml"),
            _descriptor_body("runner") + 'bin = "other"\n',
        )
        problems = scan(tmp)[0]
        only(
            problems,
            "states bin = 'other', but the package's only bin target is 'nros-runner-r'",
            "A8 must refuse a stated bin that differs from the derived one",
        )
        # BORN_EMPTY's stale arm: a family the waiver calls empty, once it has a
        # provider, must have its waiver refused. Exercised on whichever family
        # is still waived (the runner family left the table when phase-528 W3
        # wrote the first runner packages); with none left, the arm guards
        # nothing and there is nothing to test.
        waived = sorted(BORN_EMPTY)
        if waived:
            fam = waived[0]
            pattern = FAMILIES[fam][0]
            pattern = pattern[0] if isinstance(pattern, list) else pattern
            desc = os.path.join(tmp, pattern.replace("*", "zz"))
            _write(os.path.join(os.path.dirname(desc), "package.xml"), _pkg_xml(fam))
            _write(desc, _descriptor_body(fam))
            only(
                [p for p in scan(tmp)[0] if "BORN_EMPTY" in p],
                "BORN_EMPTY says it has no provider yet",
                "a stale BORN_EMPTY row",
            )
            shutil.rmtree(os.path.dirname(desc))
        _write(
            os.path.join(tmp, "packages/runners/r/nros-runner.toml"),
            _descriptor_body("runner") + 'bin = "nros-runner-r"\n',
        )
        none(scan(tmp)[0], "states bin", "a stated bin equal to the derived one passes")
        shutil.rmtree(os.path.join(tmp, "packages/runners"))

        # A4 — an exemption that covers nothing is stale, and stale is an error.
        shutil.rmtree(os.path.join(tmp, "leaf"))
        only(
            scan(tmp)[0],
            "matched nothing",
            "A4 must refuse a REACH_EXEMPT row with no files left under it",
        )

    REACH_EXEMPT = shipped_exempt
    if not quiet:
        print("check-provider-announcements self-test: OK")
    return 0


def announced_kinds(pkg_xml):
    """Every `kind` a package.xml announces, deduped, in file order.

    Comments stripped for the issue-0516 reason `declared_provisions` gives.
    """
    body = COMMENT_RE.sub("", Path(pkg_xml).read_text(encoding="utf-8"))
    return list(dict.fromkeys(PROVIDES_KIND_RE.findall(body)))


def check_announcements_have_descriptors(root):
    """A3 — an announcement with no sibling descriptor of the kind it claims.

    The inverse of A1, and the direction the board family already enforces in
    Rust (`board_descriptor.rs::require_announcement`, RFC-0064 R5 D2). Kept
    glob-FREE on purpose: it walks the ANNOUNCEMENTS, so unlike A1 it cannot be
    narrower than the tree. Issue 1220 was four `config/<x>/package.xml` whose
    descriptor had moved to `packages/platform/` — a package announcing a
    platform with nothing beside it to lower, which `provider_scan`'s
    `descriptor_path(kind)` (`self.dir.join("nros-{kind}.toml")`) resolves to a
    path that does not exist.

    -> (problems, announcements examined)
    """
    problems = []
    seen = 0
    verbs = load_builtin_verbs(root)
    for pkg_xml in tracked(root, name="package.xml"):
        if not os.path.exists(pkg_xml):
            continue  # staged deletion: in the index, gone from the worktree
        rel = os.path.relpath(pkg_xml, root)
        try:
            ElementTree.parse(pkg_xml)  # A5
        except (ElementTree.ParseError, OSError, UnicodeDecodeError) as e:
            problems.append(
                f"{rel}: not well-formed XML: {e}. Every rule in this gate is a "
                f"REGEX, which reads this file happily; `provider_scan` uses a "
                f"real parser and demotes the failure to a warning, so a "
                f"provider here is silently absent from `nros dev ws providers` "
                f"while every gate says OK.\n"
                f"    A bare `<` in prose is the usual cause — a path written "
                f"`config/<x>/` inside a <description> is an open tag."
            )
            continue
        kinds = announced_kinds(pkg_xml)
        problems.extend(check_host_package(root, pkg_xml, kinds, verbs))
        for kind in kinds:
            seen += 1
            if kind not in FAMILIES:
                problems.append(
                    f"{rel}: announces kind {kind!r}, which is not a family this "
                    f"gate knows ({', '.join(sorted(FAMILIES))}). Either it is a "
                    f"typo, or a family was added to the tree and not to "
                    f"FAMILIES — in which case nothing checks it."
                )
                continue
            desc = os.path.join(os.path.dirname(pkg_xml), f"nros-{kind}.toml")
            if not os.path.exists(desc):
                problems.append(
                    f"{rel}: announces "
                    f'<nano_ros_provides kind="{kind}"/> with no sibling '
                    f"nros-{kind}.toml. `ProviderPackage::descriptor_path` looks "
                    f"beside the package.xml, so the scan FINDS this provider "
                    f"and then resolves it to a file that is not there.\n"
                    f"    Put the descriptor at {os.path.relpath(desc, root)}, "
                    f"or drop the announcement."
                )
    return problems, seen


def check_glob_reach(root, family_paths):
    """A4 — the family globs reach every descriptor of their kind in the tree.

    A1 and A2 start from a glob, so their coverage is whatever someone typed.
    Issue 1220 is what that costs: `config/*/nros-platform.toml` covered 3 of 8
    and the gate reported on the family as though the number were 8. This
    compares the globbed set against the TRACKED files of the same name, so the
    reach is measured rather than asserted.

    `family_paths` is {kind: [globbed descriptor paths]}, so both sets come from
    the same run and cannot drift.

    -> (problems, descriptors reached)
    """
    problems = []
    reached = 0
    used = {pat: 0 for pat, _, _ in REACH_EXEMPT}
    for kind in sorted(FAMILIES):
        globbed = {os.path.relpath(p, root) for p in family_paths.get(kind, [])}
        reached += len(globbed)
        for path in tracked(root, name=f"nros-{kind}.toml"):
            rel = os.path.relpath(path, root)
            if rel in globbed:
                continue
            if not os.path.exists(path):
                # In the index, gone from the worktree — a deletion in flight,
                # not a descriptor out of reach. `globbed` comes from the
                # filesystem and `tracked` from the index, so the two disagree
                # for exactly as long as a `git rm` is unstaged; reporting that
                # as an unreached descriptor would name the wrong defect.
                continue
            hit = next((e for e in REACH_EXEMPT if fnmatch.fnmatch(rel, e[0])), None)
            if hit is None:
                pats = FAMILIES[kind][0]
                pats = [pats] if isinstance(pats, str) else list(pats)
                problems.append(
                    f"{rel}: a {kind} descriptor no family glob reaches "
                    f"({', '.join(pats)}). A1/A2 never evaluate it, so its "
                    f"announcement is unchecked while the family reports OK — "
                    f"the issue-1220 shape.\n"
                    f"    Widen the {kind!r} row's glob, or add the path to "
                    f"REACH_EXEMPT with the reason it is not a descriptor."
                )
                continue
            used[hit[0]] += 1
            if hit[2] is None:
                continue
            # The exemption claims a NAME COLLISION; measure it.
            try:
                with open(path, "rb") as fh:
                    data = tomllib.load(fh)
            except Exception as e:  # noqa: BLE001 — report, do not raise
                problems.append(f"{rel}: not valid TOML: {e}")
                continue
            if hit[2] in data:
                problems.append(
                    f"{rel}: exempted from A4 as {hit[1]}, but it declares "
                    f"[{hit[2]}] — it IS a descriptor, and the exemption is "
                    f"hiding it from every rule in this gate."
                )
    for pat, count in sorted(used.items()):
        if count == 0:
            problems.append(
                f"REACH_EXEMPT pattern {pat!r} matched nothing. A stale "
                f"exemption is how a rule gets reclaimed by accident — delete "
                f"the row now that the files it covered are gone."
            )
    return problems, reached


def _scan_impl(root):
    problems = []
    checked = 0
    announced = 0
    family_paths = {}

    for kind, (pattern, extract) in sorted(FAMILIES.items()):
        # A family may state several globs (boards do: a bundle board sits one
        # level deeper). `dict.fromkeys` rather than `set`, so a path matched by
        # two globs is reported once and the order stays deterministic.
        patterns = [pattern] if isinstance(pattern, str) else list(pattern)
        paths = sorted(
            dict.fromkeys(
                m for pat in patterns for m in glob.glob(os.path.join(root, pat))
            )
        )
        family_paths[kind] = paths
        if kind in BORN_EMPTY:
            if paths:
                problems.append(
                    f"family {kind!r}: BORN_EMPTY says it has no provider yet "
                    f"({BORN_EMPTY[kind]}), but {len(paths)} descriptor(s) match "
                    f"{patterns!r}. Delete the BORN_EMPTY row — the waiver is "
                    f"stale, and the family now has a set to check."
                )
            else:
                continue
        if not paths:
            # A family whose descriptors all vanished would otherwise make this
            # gate quietly vacuous for that family.
            problems.append(
                f"family {kind!r}: no descriptor matched {patterns!r} — refusing to "
                f"pass on an empty set"
            )
            continue

        for desc_path in paths:
            desc_rel = os.path.relpath(desc_path, root)
            pkg_xml = os.path.join(os.path.dirname(desc_path), "package.xml")
            if not os.path.exists(pkg_xml):
                # RFC-0064 R5 D5 / phase-375 W6. This used to `continue`, with
                # the comment "not migrated yet; not discoverable, still
                # builds". The migration finished; the skip did not, and it
                # outlived its reason by long enough that phase-385 landed
                # `nros-board-mps3-an536-freertos` -- the NEWEST board in the
                # tree -- with a descriptor and no announcement, and no gate
                # said so. An optional step is a step the next person omits.
                problems.append(
                    f"{desc_rel}: descriptor with no sibling package.xml. A "
                    f"provider announces what it IS in package.xml and what it "
                    f"LOWERS TO in the descriptor; with only the second half it "
                    f"is invisible to `provider_scan` and to every consumer "
                    f"that reaches providers through it.\n"
                    f"    Add {os.path.relpath(pkg_xml, root)} with "
                    f'<nano_ros_provides kind="{kind}" .../> matching the '
                    f"descriptor's names, canonical first."
                )
                continue
            checked += 1
            rel = os.path.relpath(pkg_xml, root)

            with open(desc_path, "rb") as fh:
                try:
                    data = tomllib.load(fh)
                except Exception as e:  # noqa: BLE001 — report, do not raise
                    problems.append(f"{desc_rel}: not valid TOML: {e}")
                    continue
            names = None if extract is None else extract(data)
            if extract is None:
                # A2n — the announcement is the only spelling.
                restated = restated_names(data)
                if restated:
                    problems.append(
                        f"{desc_rel}: declares names {restated} in a NAMELESS "
                        f"family — {kind} names come from the "
                        f'<nano_ros_provides kind="{kind}"/> announcements and '
                        f"nothing reads this key, so it can drift with no "
                        f"symptom. Delete it (RFC-0087 D4)"
                    )
            elif not names:
                problems.append(
                    f"{desc_rel}: declares no names — nothing could resolve to it"
                )
                continue

            found = declared_provisions(pkg_xml, kind)
            announced += len(found)
            if not found:
                problems.append(
                    f'{rel}: sits beside a {kind} descriptor but announces no '
                    f'<nano_ros_provides kind="{kind}"/> — it would be invisible '
                    f"to the phase-348 scan"
                )
            elif names is not None and found != names:
                problems.append(
                    f"{rel}: provides {found} but {desc_rel} declares {names} — "
                    f"discovery and resolution must claim the same names, "
                    f"canonical first"
                )

    a3, seen = check_announcements_have_descriptors(root)
    problems.extend(a3)
    a4, reached = check_glob_reach(root, family_paths)
    problems.extend(a4)

    return problems, checked, announced, Reach(seen, reached)


def main():
    rc = self_test(quiet=True)
    if rc:
        return rc

    problems, checked, announced, reach = scan(ROOT)

    if problems:
        sys.stderr.write("check-provider-announcements: FAILED\n")
        for p in problems:
            sys.stderr.write(f"  {p}\n")
        return 1

    nameless = sorted(k for k, (_, e) in FAMILIES.items() if e is None)
    # The REACH numbers are printed beside the CHECKED ones on purpose. Issue
    # 1220's gate said "23 migrated provider(s) across 4 famil(ies)" on a tree
    # where its platform row saw 3 of 8 descriptors, and no reader could tell:
    # a count of what a glob matched cannot report what it missed.
    print(
        f"provider announcements: OK ({checked} migrated provider(s) across "
        f"{len(FAMILIES)} famil(ies), {announced} name(s) announced; "
        f"named families match their descriptor, nameless "
        f"({', '.join(nameless) or 'none'}) restate nothing; reach: "
        f"{reach.descriptors} descriptor(s) globbed = every tracked "
        f"nros-<kind>.toml but {len(REACH_EXEMPT)} exempted pattern(s), "
        f"{reach.announcements} announcement(s) each have their descriptor)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
