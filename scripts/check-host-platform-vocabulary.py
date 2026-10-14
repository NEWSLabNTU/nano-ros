#!/usr/bin/env python3
"""The host board is `linux`, and nothing tracked selects it by another word.

WHAT THE WORDS MEAN (CLAUDE.md "Naming", maintainer ruling 2026-10-14,
phase-500):

    native   every general-purpose host (Linux, macOS, Windows)
    posix    POSIX-compliant systems
    linux    Linux specifically

The host board (`packages/boards/linux/nros-board.toml`) builds on Linux ONLY —
measured: `nros-board-linux::apply_tier_affinity` uses `cpu_set_t` /
`sched_setaffinity`, which libc does not define for apple, windows or freebsd.
So its one true name is `linux`:

  * `native` claims every general-purpose host. phase-500 W1 DROPPED it as a
    board name; the consumers refuse it with a hint naming `linux`
    (`nros_entry_lower::RETIRED_BOARD_KEYS`, `cmake/NanoRosHostBoard.cmake`).
  * `posix` is the PLATFORM (`nros-platform-posix`, whose C sources NuttX also
    compiles), never a board name.

HISTORY. Phase-401 split a descriptor that said `["linux", "native", "posix"]`
(both reaches at once); issue 1109 then ruled `native` the ROLE word a host
image should write. The maintainer rejected that ruling: under the definitions
above `native` is a reach claim too, and a false one for this board.

WHAT IS CHECKED (every population is TRACKED files; CLI test fixtures under a
`tests/` dir are included — the stale `["native", "posix"]` copy lived there,
invisible to the old gate's `tests` exclusion):

  1. Every board descriptor: no `names` entry is `native` or `posix`, and no
     descriptor offers both `linux` and `posix`.
  2. The one host board descriptor (`platform = "posix"`, `entry_kind =
     "hosted-main"`, outside `tests/`) names exactly `["linux"]`.
  3. Every `system.toml` (and `*system.toml.case` fixture): no `[image.*]` or
     `[image_defaults]` `board` is a retired host word.
  4. Every `Cargo.toml`: no `[package.metadata.nros.*] deploy = [...]` list
     holds a retired host word.
  5. Every cmake file: no `DEPLOY` keyword argument is a retired host word.
  6. `nros-sdk-index.toml`: no `[board.native]` / `[board.posix]`, and a
     `[board.linux]` exists (`nros setup linux` once failed for lack of it).
  7. (phase-500 W2) The host LANE, `just` MODULE, setup/build/test/doctor
     SCOPE and the host test binaries are `linux`: no tracked file outside the
     history series (docs/roadmap, docs/issues, CHANGELOG, audit reports)
     spells `lane=native`, `just [setup|build|test|doctor] native`,
     `mod native`, `native::<recipe>`, `NROS_TEST_SCOPE|NROS_FIXTURE_LANE|
     NROS_FIXTURE_SCOPE=native`, `lane-filter.sh native`, `--scope native` or
     a renamed `native_<test>` binary (nextest `binary(...)` filters included),
     and `native` is in neither `_NROS_LANES` nor `_NROS_SCOPE_PLATFORMS`. The
     DEPRECATED alias (one cycle; phase-500 W5 removes it) is allowed only on a
     line carrying the pragma `retired-host-token: alias`.
  8. (phase-500 W3) Host IMAGE IDS are `linux…`: no tracked file outside the
     history series holds an `[image.native…]` header (TOML, or TOML inlined in
     a test or a doc), no `examples/fixtures.toml` row names
     `image = "native…"`, and nothing spells a name DERIVED from such an id —
     the entry package `native…_entry`, or its generated
     `…_entry_nros_main_generated.cpp` / `_link_libs.cmake` /
     `_nros_selection`. No alias: an image id is a label the workspace
     authors, not a CLI front door, so a user's own `[image.native]` still
     builds; only this tree stops spelling it.
  9. (issue 1892) The example lint (`scripts/build/lint-example-leaf.sh`)
     decides from a leaf's SHAPE, never from its platform DIRECTORY. The
     script is run in PLAN mode (`NROS_LINT_LEAF_PLAN=1`: it prints whether it
     renders the leaf with `nros sync`, its `--config` args and its profile
     flags, and runs neither sync nor cargo) over synthetic leaves of each
     shape, each placed under `examples/native/`, `examples/linux/` and a board
     directory. Every plan must equal the shape's expected plan, so a
     `platform != "native"` test (1882's, which left 21 host leaves unrendered
     on a fresh checkout) fails here on a pull request instead of in the
     nightly-only `native-check`. Then every tracked example leaf whose
     `system.toml` declares an `[image.<id>]` — of any id — must plan a render.
 10. (phase-500 W4) The host EXAMPLE DIRECTORY is `examples/linux/`: no
     tracked path lies under `examples/native/`, and no tracked file outside
     the history series spells `examples/native` or a quoted example locator
     relative to `examples/` (`"native/rust/…"`, `'native/c/…'` — the
     `build_example("native/rust/talker", …)` shape the test fixtures use).
     No alias: a moved directory cannot have one.

WHAT IT DELIBERATELY DOES NOT CHECK

Prose, and the deprecated CLI ALIASES (`nros new --platform native|posix`,
`nros setup native`), which exist for released book pages and are retired by
phase-500 W5. The fixture ROW
ids (`workspace-rust-native-…`) are a coordinate vocabulary of their own, like
the interop cell ids. `native` in its other senses (Zephyr
`native_sim`, RTOS-native priorities, interop cell ids, prose) is not a lane.

Usage:  check-host-platform-vocabulary.py
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
sys.path.insert(0, os.path.join(ROOT, "scripts", "lib"))
import file_kinds  # noqa: E402  phase-472 W5 — populations by kind
import git_grep  # noqa: E402  issue 1816 — a failed search raises

HOST = "linux"
# Words that must never select the host board. `native` is retired (phase-500
# W1); `posix` is the platform. Each maps to the word to write instead.
NOT_A_HOST_BOARD = {"native": HOST, "posix": HOST}

NAMES_RE = re.compile(r"^\s*names\s*=\s*\[(.*?)\]", re.M | re.S)
DEPLOY_RE = re.compile(r"^\s*DEPLOY\s+([^)\n#]*)", re.M)


def tracked(*patterns):
    out = subprocess.run(
        ["git", "-C", ROOT, "ls-files", "-z", "--", *patterns],
        check=True, capture_output=True).stdout.decode()
    return sorted(p for p in out.split("\0") if p and "/third-party/" not in f"/{p}")


def strip_comments(text, mark="#"):
    return "\n".join(l for l in text.split("\n") if not l.lstrip().startswith(mark))


def descriptor_problems(names):
    """What is wrong with one descriptor's `names` list."""
    out = [f"`{n}` is not a board name (write `{NOT_A_HOST_BOARD[n]}`)"
           for n in names if n in NOT_A_HOST_BOARD]
    if HOST in names and "posix" in names:
        out.append("claims both `linux` and `posix`")
    return out


def descriptors():
    # Every descriptor, CLI test fixtures included (phase-500 W1: the stale
    # copy lived under `tests/`).
    return file_kinds.files_of_kind("board-descriptor", repo=ROOT)


def check_descriptors():
    problems, hosts = [], []
    for rel in descriptors():
        with open(os.path.join(ROOT, rel), encoding="utf8") as fh:
            text = fh.read()
        for m in NAMES_RE.finditer(strip_comments(text)):
            names = re.findall(r'"([^"]*)"', m.group(1))
            problems += [(rel, p) for p in descriptor_problems(names)]
        if "/tests/" in f"/{rel}":
            continue
        doc = tomllib.loads(text)
        for b in doc.get("board", []):
            if b.get("platform") == "posix" and b.get("entry_kind") == "hosted-main":
                hosts.append((rel, b.get("names", [])))
    if len(hosts) != 1:
        problems.append(("(catalog)", f"expected ONE host board descriptor, found {hosts}"))
    elif hosts[0][1] != [HOST]:
        problems.append((hosts[0][0], f"host board names {hosts[0][1]}, must be [{HOST!r}]"))
    return problems


def selector_problems(doc):
    """(site, word) for every host selector in a parsed system.toml that writes
    a word that is not the host board's name."""
    sites = []
    d = doc.get("image_defaults")
    if isinstance(d, dict) and "board" in d:
        sites.append(("[image_defaults]", d["board"]))
    for iid, body in (doc.get("image") or {}).items():
        if isinstance(body, dict) and "board" in body:
            sites.append((f"[image.{iid}]", body["board"]))
    return [(w, b) for w, b in sites if b in NOT_A_HOST_BOARD]


def deploy_problems(doc):
    """(table, word) for every `[package.metadata.nros.*] deploy` entry that is
    a retired host word."""
    nros = ((doc.get("package") or {}).get("metadata") or {}).get("nros") or {}
    out = []
    for key, body in nros.items():
        if isinstance(body, dict) and isinstance(body.get("deploy"), list):
            out += [(f"[package.metadata.nros.{key}] deploy", w)
                    for w in body["deploy"] if w in NOT_A_HOST_BOARD]
    return out


def cmake_problems(text):
    out = []
    for m in DEPLOY_RE.finditer(strip_comments(text)):
        out += [("DEPLOY", w) for w in m.group(1).split() if w.strip('"') in NOT_A_HOST_BOARD]
    return out


def sdk_index_problems(doc):
    boards = doc.get("board") or {}
    out = [f"[board.{k}] (write [board.{v}])" for k, v in NOT_A_HOST_BOARD.items() if k in boards]
    if HOST not in boards:
        out.append(f"no [board.{HOST}] — `nros setup {HOST}` cannot resolve")
    return out


# Check 7 — phase-500 W2. Every spelling that once named the host lane, module,
# scope or test binaries. ONE list; a new spelling joins it.
RENAMED_TESTS = ("api", "async_roundtrip_e2e", "entry_boot",
                 "example_executor_bound_node_e2e", "example_pubsub_e2e",
                 "example_reqresp_e2e", "main_macro_forms", "main_macro_misuse",
                 "orchestration_tiers")
RETIRED_LANE_RE = re.compile(
    r"\bjust (?:setup |build |test |doctor )?native\b(?![-_])"
    r"|\blane=native\b"
    r"|\bNROS_(?:TEST_SCOPE|FIXTURE_LANE|FIXTURE_SCOPE)=native\b"
    r"|lane-filter\.sh native\b"
    r"|^mod native\b"
    r"|\bnative::[a-z_]"
    r"|--scope native\b"
    r"|just/native\.just"
    r"|\bnative_(?:" + "|".join(RENAMED_TESTS) + r")\b",
    re.M)
ALIAS_PRAGMA = "retired-host-token: alias"
# The history series: they record what WAS typed, and rewriting them would
# falsify the record. Everything else is a live instruction to a reader.
HISTORY = ("docs/roadmap/", "docs/issues/", "CHANGELOG.md", "changelog.d/",
           "docs/development/audit-findings-", "docs/development/build-ux-audit.md",
           "docs/design/archived/", ".superpowers/", "docs/superpowers/")
# This file's own docstring and self-test spell every retired form on purpose.
SELF = "scripts/check-host-platform-vocabulary.py"
TOKEN_LISTS = (("scripts/build/fixture-lane.sh", "_NROS_LANES"),
               ("scripts/build/scope.sh", "_NROS_SCOPE_PLATFORMS"))


def lane_problems(text):
    """(line number, matched spelling) for every retired host-lane spelling in
    `text` that is not on a line carrying the alias pragma."""
    out = []
    for n, line in enumerate(text.split("\n"), 1):
        if ALIAS_PRAGMA in line:
            continue
        out += [(n, m.group(0)) for m in RETIRED_LANE_RE.finditer(line)]
    return out


def token_list_problems(text, var):
    m = re.search(rf'^{var}="([^"]*)"', text, re.M)
    if not m:
        return [f"{var} not found (the gate would pass vacuously)"]
    return [f"{var} lists `native` (write `linux`)"] if "native" in m.group(1).split() else []


def check_lane_vocabulary():
    problems = []
    # `git grep` narrows the population to candidate files fast; the regex
    # above is the judge.
    # issue 1816 — a failed search must raise, never read as "no match".
    for rel in git_grep.lines(["-lIE", r"native(::|\.just|_| |$|\b)", "--",
                               ".", ":!third-party"], ROOT):
        if rel.startswith(HISTORY) or rel == SELF or "/third-party/" in f"/{rel}":
            continue
        try:
            with open(os.path.join(ROOT, rel), encoding="utf8", errors="replace") as fh:
                text = fh.read()
        except OSError:
            continue
        problems += [(f"{rel}:{n}", f"`{w}` — the host lane/module/scope is `linux`")
                     for n, w in lane_problems(text)]
    for rel, var in TOKEN_LISTS:
        with open(os.path.join(ROOT, rel), encoding="utf8") as fh:
            problems += [(rel, p) for p in token_list_problems(fh.read(), var)]
    return problems


# Check 8 — phase-500 W3. A host image id, and every name derived from one.
# `<id>_entry` is the generated entry package (`leaf_system::entry_package_name`)
# and its build products carry it as a prefix (`selection_crate_name`, the cmake
# entry's generated TU and link-libs file).
RETIRED_IMAGE_RE = re.compile(
    r"\[image\.native(?=[\]._])"
    r"|\[\"image\"\]\[\"native(?=[\"_])"
    r"|\bnative(?:_[a-z0-9]+)*_entry"
    r"(?:_(?:nros_main_generated|link_libs|nros_selection))?\b")
FIXTURES = "examples/fixtures.toml"


def image_id_problems(text):
    """(line number, matched spelling) for every retired host image id, or a
    name derived from one, in `text` (alias-pragma lines excepted)."""
    out = []
    for n, line in enumerate(text.split("\n"), 1):
        if ALIAS_PRAGMA in line:
            continue
        out += [(n, m.group(0)) for m in RETIRED_IMAGE_RE.finditer(line)]
    return out


def fixture_image_problems(doc):
    """(row id, image) for every manifest row whose `image` is `native…`."""
    out = []
    for rows in doc.values():
        if not isinstance(rows, list):
            continue
        for row in rows:
            img = row.get("image") if isinstance(row, dict) else None
            if isinstance(img, str) and img.startswith("native"):
                out.append((row.get("id", "?"), img))
    return out


def check_image_ids():
    problems = []
    for rel in git_grep.lines(["-lIE", r"image\.native|\"image\"\]\[\"native|native[a-z0-9_]*_entry", "--",
                               ".", ":!third-party"], ROOT):
        if rel.startswith(HISTORY) or rel == SELF or "/third-party/" in f"/{rel}":
            continue
        try:
            with open(os.path.join(ROOT, rel), encoding="utf8", errors="replace") as fh:
                text = fh.read()
        except OSError:
            continue
        problems += [(f"{rel}:{n}", f"`{w}` — a host image id is `linux…` (phase-500 W3)")
                     for n, w in image_id_problems(text)]
    with open(os.path.join(ROOT, FIXTURES), "rb") as fh:
        problems += [(FIXTURES, f"row `{i}` image = {img!r} (write `linux{img[6:]}`)")
                     for i, img in fixture_image_problems(tomllib.load(fh))]
    return problems


# Check 10 — phase-500 W4. The host example directory, by full path or by the
# `examples/`-relative locator the test fixtures build from.
RETIRED_DIR = "examples/native/"
RETIRED_DIR_RE = re.compile(
    r"\bexamples/native\b(?![-_])"
    r"|[\"']native/(?:rust|c|cpp)\b")


def example_dir_problems(text):
    """(line number, matched spelling) for every reference to the retired host
    example directory in `text`."""
    out = []
    for n, line in enumerate(text.split("\n"), 1):
        out += [(n, m.group(0)) for m in RETIRED_DIR_RE.finditer(line)]
    return out


def check_example_dir():
    problems = [(rel, f"tracked under `{RETIRED_DIR}` — the host examples live in `examples/linux/`")
                for rel in tracked(RETIRED_DIR)]
    for rel in git_grep.lines(["-lIE", r"examples/native|native/(rust|c|cpp)", "--",
                               ".", ":!third-party"], ROOT):
        if rel.startswith(HISTORY) or rel == SELF or "/third-party/" in f"/{rel}":
            continue
        try:
            with open(os.path.join(ROOT, rel), encoding="utf8", errors="replace") as fh:
                text = fh.read()
        except OSError:
            continue
        problems += [(f"{rel}:{n}", f"`{w}` — the host example directory is `examples/linux/` "
                      "(phase-500 W4)") for n, w in example_dir_problems(text)]
    return problems


def check_files():
    problems = []
    for rel in file_kinds.files_of_kind("system-toml", repo=ROOT) + tracked("*system.toml.case"):
        with open(os.path.join(ROOT, rel), "rb") as fh:
            try:
                doc = tomllib.load(fh)
            except tomllib.TOMLDecodeError as e:
                raise SystemExit(f"check-host-platform-vocabulary: {rel}: {e}")
        problems += [(rel, f"{w} board = {b!r}") for w, b in selector_problems(doc)]
    for rel in tracked("Cargo.toml", "*/Cargo.toml"):
        with open(os.path.join(ROOT, rel), "rb") as fh:
            try:
                doc = tomllib.load(fh)
            except tomllib.TOMLDecodeError:
                continue  # not ours to judge; cargo will
        problems += [(rel, f"{w} holds {b!r}") for w, b in deploy_problems(doc)]
    for rel in tracked("*.cmake", "CMakeLists.txt", "*/CMakeLists.txt"):
        with open(os.path.join(ROOT, rel), encoding="utf8", errors="replace") as fh:
            problems += [(rel, f"{k} {w}") for k, w in cmake_problems(fh.read())]
    with open(os.path.join(ROOT, "nros-sdk-index.toml"), "rb") as fh:
        problems += [("nros-sdk-index.toml", p) for p in sdk_index_problems(tomllib.load(fh))]
    return problems


LINT_LEAF = os.path.join(ROOT, "scripts", "build", "lint-example-leaf.sh")
# Three spellings of "which platform directory": the host's current one, its
# phase-500 W4 name, and a board's. A decision that differs between them was
# keyed on the directory.
PLAN_PLATFORMS = ("native", "linux", "mps2-an385-baremetal")

_CARGO = '[package]\nname = "{name}"\nversion = "0.1.0"\nedition = "2024"\n'
_SYSTEM = '[system]\nname = "s"\n\n[image.img0]\nboard = "b"\n'
_PKG = '<?xml version="1.0"?>\n<package format="3"><name>p</name></package>\n'
_SETTINGS = "build/img0/nros-cargo.toml"
# shape -> (files, expected plan). Image ids are arbitrary labels on purpose.
LEAF_SHAPES = {
    "image leaf, not yet rendered": (
        {"system.toml": _SYSTEM, "package.xml": _PKG},
        {"render": "1", "config": ""}),
    "image leaf, settings included by sync (issue 1381)": (
        {"system.toml": _SYSTEM, "package.xml": _PKG, _SETTINGS: "",
         ".cargo/config.toml": 'include = ["../build/img0/nros-cargo.toml"]\n'},
        {"render": "1", "config": ""}),
    "image leaf, settings NOT included (leaf states its own build flags)": (
        {"system.toml": _SYSTEM, _SETTINGS: "",
         ".cargo/config.toml": '[build]\ntarget = "x"\n'},
        {"render": "1", "config": f"--config {_SETTINGS}"}),
    "package.xml-only leaf (issue 1887)": (
        {"package.xml": _PKG},
        {"render": "1", "config": ""}),
    "plain cargo leaf": (
        {},
        {"render": "0", "config": ""}),
}
PLAN_FLAGS = "--profile nros-plan-probe"


def lint_plan(cwd, leaf):
    """The lint script's decisions for `leaf` (relative to `cwd`), as a dict."""
    env = dict(os.environ, NROS_LINT_LEAF_PLAN="1", NROS_EXAMPLE_PROFILE_FLAGS=PLAN_FLAGS)
    r = subprocess.run(["bash", LINT_LEAF, leaf], cwd=cwd, env=env,
                       capture_output=True, text=True)
    if r.returncode != 0:
        return {"error": f"rc={r.returncode}: {r.stderr.strip()}"}
    return dict(l.split("=", 1) for l in r.stdout.splitlines() if "=" in l)


def plan_problems(shape, expected, plans):
    """(where, what) for every platform dir whose plan is not the shape's."""
    want = dict(expected, flags=PLAN_FLAGS)
    out = []
    for platform, plan in plans.items():
        got = {k: plan.get(k) for k in want}
        if got != want:
            out.append((f"examples/{platform}/… ({shape})",
                        f"lint plan {got}, expected {want}"
                        + (f" [{plan['error']}]" if "error" in plan else "")))
    return out


def check_lint_leaf_shape():
    import tempfile
    problems = []
    with tempfile.TemporaryDirectory(prefix="nros-lint-plan.") as tmp:
        for i, (shape, (files, expected)) in enumerate(LEAF_SHAPES.items()):
            plans = {}
            for platform in PLAN_PLATFORMS:
                leaf = f"examples/{platform}/rust/shape{i}"
                for rel, body in {"Cargo.toml": _CARGO.format(name=f"shape{i}"), **files}.items():
                    path = os.path.join(tmp, leaf, rel)
                    os.makedirs(os.path.dirname(path), exist_ok=True)
                    with open(path, "w", encoding="utf8") as fh:
                        fh.write(body)
                plans[platform] = lint_plan(tmp, leaf)
            problems += plan_problems(shape, expected, plans)
    for rel in file_kinds.files_of_kind("system-toml", repo=ROOT):
        leaf = os.path.dirname(rel)
        if not rel.startswith("examples/") or not os.path.isfile(os.path.join(ROOT, leaf, "Cargo.toml")):
            continue
        with open(os.path.join(ROOT, rel), "rb") as fh:
            if not isinstance(tomllib.load(fh).get("image"), dict):
                continue
        plan = lint_plan(ROOT, leaf)
        if plan.get("render") != "1":
            problems.append((leaf, f"declares [image.*] but the lint plans no `nros sync` ({plan})"))
    return problems


def self_test():
    """Prove each check can fail. Runs on EVERY invocation — a negative control
    nobody runs decays into a comment (`check-gate-selftests`)."""
    fails = []
    want = {
        ("linux",): [],
        ("native", "linux"): ["`native` is not a board name (write `linux`)"],
        ("native", "posix"): ["`native` is not a board name (write `linux`)",
                              "`posix` is not a board name (write `linux`)"],
        ("linux", "posix"): ["`posix` is not a board name (write `linux`)",
                             "claims both `linux` and `posix`"],
        ("threadx", "threadx-linux"): [],
    }
    for names, exp in want.items():
        got = descriptor_problems(list(names))
        if got != exp:
            fails.append(f"descriptor {names}: expected {exp}, got {got}")
    for body, exp in [
        ('[image.n]\nboard = "linux"\n', []),
        ('[image.native]\nboard = "linux"\n', []),  # an image ID: check 8, not 3
        ('[image.n]\nboard = "native"\n', [("[image.n]", "native")]),
        ('[image_defaults]\nboard = "posix"\n', [("[image_defaults]", "posix")]),
        ('[image.z]\nboard = "zephyr"\n', []),
    ]:
        got = selector_problems(tomllib.loads(body))
        if got != exp:
            fails.append(f"selector {body!r}: expected {exp}, got {got}")
    for body, exp in [
        ('[package.metadata.nros.application]\ndeploy = ["linux"]\n', []),
        ('[package.metadata.nros.application]\ndeploy = ["native", "freertos"]\n',
         [("[package.metadata.nros.application] deploy", "native")]),
    ]:
        got = deploy_problems(tomllib.loads(body))
        if got != exp:
            fails.append(f"deploy {body!r}: expected {exp}, got {got}")
    for body, exp in [
        ("nano_ros_entry(NAME a\n    DEPLOY  linux)\n", []),
        ("nano_ros_entry(NAME a\n    DEPLOY  native freertos)\n", [("DEPLOY", "native")]),
        ("# DEPLOY native — a comment\n", []),
    ]:
        got = cmake_problems(body)
        if got != exp:
            fails.append(f"cmake {body!r}: expected {exp}, got {got}")
    for body, exp in [
        ("[board.linux]\n", []),
        ("[board.native]\n[board.posix]\n", ["[board.native] (write [board.linux])",
                                            "[board.posix] (write [board.linux])",
                                            "no [board.linux] — `nros setup linux` cannot resolve"]),
    ]:
        got = sdk_index_problems(tomllib.loads(body))
        if got != exp:
            fails.append(f"sdk index {body!r}: expected {exp}, got {got}")
    for body, exp in [
        ("just linux test\n", []),
        ("just native test\n", [(1, "just native")]),
        ("  just setup native\n", [(1, "just setup native")]),
        ("just build-test-fixtures lane=native\n", [(1, "lane=native")]),
        ("mod native 'just/native.just'\n", [(1, "mod native"), (1, "just/native.just")]),
        ("format: native::format\n", [(1, "native::f")]),
        ('filter = "binary(native_api)"\n', [(1, "native_api")]),
        ("env NROS_FIXTURE_LANE=native x\n", [(1, "NROS_FIXTURE_LANE=native")]),
        ("just native test  # retired-host-token: alias\n", []),
        ("just zephyr native_sim\n", []),  # another sense
        ("just check native-check\n", []),  # a gate recipe name, not the module
        ("examples/native/rust/talker\n", []),  # a directory: check 10
        ("native-pubsub-rust-zenoh-n2r\n", []),  # an interop cell id
    ]:
        got = lane_problems(body)
        if got != exp:
            fails.append(f"lane {body!r}: expected {exp}, got {got}")
    for body, exp in [
        ("[image.linux]\nboard = \"linux\"\n", []),
        ("[image.linux_xrce_jazzy]\n[image.linux.knobs.log]\n", []),
        ("[image.native]\n", [(1, "[image.native")]),
        ("x\n[image.native_xrce]\n", [(2, "[image.native")]),
        ('s = "[image.native.knobs.log]\\n"\n', [(1, "[image.native")]),
        ("[image.native_sim_thing]\n", [(1, "[image.native")]),
        ('v["image"]["native"]["knobs"]\n', [(1, '["image"]["native')]),  # a parsed id
        ('v["image"]["linux"]["knobs"]\n', []),
        ("[image.nativeish]\n", []),  # not the retired id
        ("[image.zephyr_native_sim]\n", []),  # a Zephyr image
        ("./build/src/native_entry/native_entry\n",
         [(1, "native_entry"), (1, "native_entry")]),
        ("fn native_rust_entry()\n", [(1, "native_rust_entry")]),
        ("x/native_entry_nros_main_generated.cpp\n", [(1, "native_entry_nros_main_generated")]),
        ('"native_robot1_entry_nros_selection"\n', [(1, "native_robot1_entry_nros_selection")]),
        ("build_native_workspace_rust_entry()\n", []),  # a helper's name, no id
        ("linux_entry linux_robot1_entry\n", []),
        ("[image.native]  # retired-host-token: alias\n", []),
    ]:
        got = image_id_problems(body)
        if got != exp:
            fails.append(f"image id {body!r}: expected {exp}, got {got}")
    for body, exp in [
        ('[[fixture]]\nid = "a"\nimage = "linux_xrce"\n', []),
        ('[[workspace_fixture]]\nid = "w"\nimage = "native"\n', [("w", "native")]),
        ('[[fixture]]\nid = "b"\nimage = "native_cyclonedds"\n[[fixture]]\nid = "c"\n',
         [("b", "native_cyclonedds")]),
        ('[[fixture]]\nid = "z"\nimage = "zephyr_native_sim"\n', []),
    ]:
        got = fixture_image_problems(tomllib.loads(body))
        if got != exp:
            fails.append(f"fixture image {body!r}: expected {exp}, got {got}")
    for body, exp in [
        ('_NROS_LANES="all linux tier1"\n', []),
        ('_NROS_LANES="all native tier1"\n', ["_NROS_LANES lists `native` (write `linux`)"]),
        ("", ["_NROS_LANES not found (the gate would pass vacuously)"]),
    ]:
        got = token_list_problems(body, "_NROS_LANES")
        if got != exp:
            fails.append(f"token list {body!r}: expected {exp}, got {got}")
    for body, exp in [
        ("cd examples/linux/rust/talker\n", []),
        ("cd examples/native/rust/talker\n", [(1, "examples/native")]),
        ("x\nfind examples/native -type d\n", [(2, "examples/native")]),
        ('build_example("native/rust/talker", "talker")\n', [(1, '"native/rust')]),
        ("fmt('native/c/{}')\n", [(1, "'native/c")]),
        ('build_example("linux/rust/talker", "talker")\n', []),
        ("zephyr/native_sim/native/64\n", []),  # a Zephyr board, not the dir
        ("examples/native_sim_demo\n", []),
        ("dist/native/ build/native/\n", []),  # not the example tree
    ]:
        got = example_dir_problems(body)
        if got != exp:
            fails.append(f"example dir {body!r}: expected {exp}, got {got}")
    good = {"render": "1", "config": "", "flags": PLAN_FLAGS}
    for plans, n in [
        ({"native": good, "mps2": good}, 0),
        ({"native": dict(good, render="0"), "mps2": good}, 1),  # 1882's shape
        ({"native": {"error": "rc=1"}, "mps2": good}, 1),
    ]:
        got = plan_problems("t", {"render": "1", "config": ""}, plans)
        if len(got) != n:
            fails.append(f"lint plan {plans}: expected {n} problem(s), got {got}")
    if "packages/cli/nros-cli-core/tests/fixtures/board-workspace/packages/boards/linux/nros-board.toml" \
            not in descriptors():
        fails.append("the CLI test-fixture host descriptor is not in the population")
    if fails:
        for f in fails:
            print(f"check-host-platform-vocabulary self-test: FAIL {f}", file=sys.stderr)
        raise SystemExit(1)


def main():
    shape = check_lint_leaf_shape()
    if shape:
        print("check-host-platform-vocabulary: the example lint decides by the leaf's "
              "platform DIRECTORY, not its shape (issue 1892):\n", file=sys.stderr)
        for rel, what in shape:
            print(f"  {rel}: {what}", file=sys.stderr)
        print("\n  Decide from the files the leaf holds, through the helpers in "
              "scripts/build/leaf-settings.sh\n  (`nros_leaf_sync_renders`, "
              "`nros_leaf_inside_config_args`), never from `examples/<platform>/`.",
              file=sys.stderr)
        return 1
    problems = (check_descriptors() + check_files() + check_lane_vocabulary()
                + check_image_ids() + check_example_dir())
    if problems:
        print("check-host-platform-vocabulary: the host board, lane, module, scope or "
              "image id or example directory is spelled by a word other than `linux` "
              "(phase-500 W1-W4):\n", file=sys.stderr)
        for rel, what in problems:
            print(f"  {rel}: {what}", file=sys.stderr)
        print(
            "\n  native = every general-purpose host (Linux, macOS, Windows)\n"
            "  posix  = POSIX-compliant systems (the PLATFORM's name)\n"
            "  linux  = Linux — the host board, which builds on Linux only\n"
            "  See CLAUDE.md “Naming” and docs/roadmap/phase-500-linux-host-name.md.",
            file=sys.stderr,
        )
        return 1
    print("check-host-platform-vocabulary OK — the host board is `linux` in every "
          "descriptor, selector, deploy list, cmake DEPLOY and sdk-index key, and the "
          "host lane / just module / scope / test binaries / image ids / example directory "
          "say `linux`; the example "
          "lint decides by leaf shape, not platform directory.")
    return 0


if __name__ == "__main__":
    self_test()
    sys.exit(main())
