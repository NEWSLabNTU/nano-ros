#!/usr/bin/env python3
"""Read examples/fixtures.toml — the SSOT for fixture build options (Phase 177.9).

Consumed by both the fixture build recipes and the test-all staleness probe so
they build each fixture with identical features/target-dir/env.

  fixtures-manifest.py list --platform linux --lang rust [--rmw zenoh] [--id ID]
  fixtures-manifest.py list-workspaces --platform linux [--lang rust] [--id ID]
  fixtures-manifest.py validate-workspaces --platform linux

emits one record per matching entry, fields separated by the unit-separator
byte 0x1F (NOT tab — tab is IFS-whitespace, so bash `read` would collapse the
empty <env> field and shift the columns):

  <dir>\x1f<env>\x1f<cargo-args>

Read it in bash with `IFS=$'\x1f' read -r dir env args`. <env> is space-joined
`KEY=VAL` (or empty), <cargo-args> is the cargo build flags
(--no-default-features / --features a,b / --target-dir D / --target TRIPLE) —
the profile is added by the caller; word-split <cargo-args> into an argv array.
"""
import argparse
import re
import sys
from pathlib import Path

SEP = "\x1f"

try:
    import tomllib  # Python 3.11+
except ModuleNotFoundError:  # 3.10 and earlier
    import tomli as tomllib

DEFAULT_MANIFEST = "examples/fixtures.toml"

# The launch file a bringup resolves to when it declares no
# `[system].default_launch`. Mirrors the fallback in
# `nros_orchestration_ir::model_location::launch_to_model_rel`, which is the
# SSoT for the rule — see the note in `validate_workspace_fixture`.
DEFAULT_LAUNCH_FILE = "system.launch.xml"


def load(path):
    with open(path, "rb") as f:
        return tomllib.load(f).get("fixture", [])


def load_workspace_fixtures(path):
    with open(path, "rb") as f:
        return tomllib.load(f).get("workspace_fixture", [])


# phase-319 W2 (issue 0351) — the compile-check lane's inventory.
#
# These are the fixtures `scripts/build/compile-check-fixtures.sh` builds. They
# used to live in six hardcoded arrays inside that script, each with its own
# colon-delimited positional format — which is why `check-fixtures-stale.sh`,
# which enumerates THIS manifest, could not see them (issue 0350 hid there for
# three days). AGENTS.md:79 already said they belong here.
#
# They are their own table rather than `[[fixture]]` rows because `list`'s record
# format is per-language and consumed positionally by `fixtures-build.sh`;
# overloading it would change that contract for 251 existing rows. The table is
# named for the LANE, not for a claim about the rows: ten are compile-intent
# checks with no runtime artifact, the other sixteen produce binaries and JSON
# that tests read or execute.
COMPILE_CHECK_BUILDERS = (
    "cargo-check",       # stage the tree, `cargo check`, stamp `.compile-ok`
    "cargo-build",       # stage the tree, `cargo build`, keep the binary
    "cmake-configure",   # cmake configure (+ build) into build/cmake-fixtures/<id>
    "cross-build",       # `cargo build --target <target>` for one or more profiles
    "cxx-syntax",        # `c++ -fsyntax-only` over a snippet; no artifact
)


def load_compile_check_fixtures(path):
    with open(path, "rb") as f:
        return tomllib.load(f).get("compile_check_fixture", [])


def validate_compile_check_fixture(entry):
    """Shape-check one compile-check row. Raises ValueError via `_fail`."""
    for key in ("id", "builder"):
        if not entry.get(key):
            _fail(entry, f"missing required key {key!r}")

    builder = entry["builder"]
    if builder not in COMPILE_CHECK_BUILDERS:
        _fail(
            entry,
            f"unsupported builder {builder!r} "
            f"(expected one of: {', '.join(COMPILE_CHECK_BUILDERS)})",
        )

    # `cxx-syntax` probes a snippet resolved by id, so it carries no dir; every
    # other builder needs a source tree that exists.
    if builder == "cxx-syntax":
        if entry.get("dir"):
            _fail(entry, "cxx-syntax rows take no 'dir' (the snippet is resolved by id)")
    else:
        if not entry.get("dir"):
            _fail(entry, f"missing required key 'dir' for builder {builder!r}")
        _require_dir(entry, Path(entry["dir"]), "fixture dir")

    if builder == "cross-build" and not entry.get("target"):
        _fail(entry, "missing required key 'target' for builder 'cross-build'")


def validate_compile_check_fixtures(entries):
    seen = {}
    for e in entries:
        validate_compile_check_fixture(e)
        fid = e["id"]
        if fid in seen:
            _fail(e, f"duplicate compile-check fixture id {fid!r}")
        seen[fid] = True
    return len(seen)


def compile_check_record(entry):
    """One \x1f-separated record: id, builder, dir, pkg, manifest_dir, target,
    profiles, output. Empty field = absent; the script defaults them."""
    return SEP.join(
        (
            entry.get("id", ""),
            entry.get("builder", ""),
            entry.get("dir", ""),
            entry.get("pkg", ""),
            entry.get("manifest_dir", ""),
            entry.get("target", ""),
            ",".join(entry.get("profiles", [])),
            entry.get("output", ""),
        )
    )


def cargo_args(entry, *, include_target_dir=True):
    args = []
    if entry.get("no_default_features"):
        args.append("--no-default-features")
    feats = entry.get("features")
    if feats:
        args += ["--features", ",".join(feats)]
    if include_target_dir and entry.get("target_dir"):
        args += ["--target-dir", entry["target_dir"]]
    if entry.get("target"):
        args += ["--target", entry["target"]]
    return " ".join(args)


def env_str(entry):
    return " ".join(f"{k}={v}" for k, v in (entry.get("env") or {}).items())


def cmake_defs(entry):
    # `rmw` shorthand expands to -DNROS_RMW=<rmw>; explicit cmake_defs override.
    defs = {}
    if entry.get("rmw"):
        defs["NROS_RMW"] = entry["rmw"]
    defs.update(entry.get("cmake_defs") or {})
    return " ".join(f"-D{k}={v}" for k, v in defs.items())


def workspace_record(entry):
    # workspace record:
    # <id>\x1f<lang>\x1f<dir>\x1f<bringup>\x1f<entry>\x1f<build-subdir>
    # \x1f<target-dir>\x1f<codegen-out>\x1f<cmake -D defs>\x1f<env>\x1f<cargo-args>
    # \x1f<board>\x1f<conf-files>
    # board/conf_files are zephyr-only; non-zephyr rows emit empty strings so the
    # field count stays uniform (13 columns).
    return SEP.join(
        [
            entry["id"],
            entry["lang"],
            entry["dir"],
            entry["bringup"],
            entry["entry"],
            entry.get("build_subdir", ""),
            entry.get("target_dir", ""),
            entry.get("codegen_out", ""),
            cmake_defs(entry),
            env_str(entry),
            cargo_args(entry, include_target_dir=False),
            entry.get("board", ""),
            ";".join(entry.get("conf_files", [])),
        ]
    )


_COORDS_CACHE = {}


def _coords_for(path):
    """Parse a lane-coords file into a set of (platform, lang, rmw) triples."""
    if path not in _COORDS_CACHE:
        coords = set()
        with open(path, encoding="utf-8") as fh:
            for line in fh:
                line = line.strip()
                if not line or line.startswith("#"):
                    continue
                parts = [x.strip() for x in line.split(",")]
                if len(parts) != 3:
                    raise SystemExit(
                        f"{path}: expected `platform,lang,rmw`, got {line!r}"
                    )
                coords.add(tuple(parts))
        if not coords:
            # An empty file would silently select NOTHING and the lane would look
            # instant rather than broken.
            raise SystemExit(f"{path}: no coordinates — refusing to select nothing")
        _COORDS_CACHE[path] = coords
    return _COORDS_CACHE[path]


def matches_filters(entry, args, *, for_probe=False):
    # `skip_build` rows stay in the manifest for documentation/inventory but
    # are intentionally NOT built as fixtures (e.g. an incomplete example).
    # Exclude them from both the build list and the stale probe — a row that
    # is never built can never be stale.
    if entry.get("skip_build"):
        return False
    if args.platform and entry.get("platform") != args.platform:
        return False
    if args.lang and entry.get("lang") != args.lang:
        return False
    if args.rmw and entry.get("rmw") != args.rmw:
        return False
    if args.id and entry.get("id") != args.id:
        return False
    coords_from = getattr(args, "coords_from", None)
    if coords_from:
        coord = (
            entry.get("platform"),
            entry.get("lang"),
            entry.get("rmw"),
        )
        if coord not in _coords_for(coords_from):
            return False
    # Issue #29 — `--core-only` excludes the isolated-`target_dir` variant cells
    # (the RMW/feature rebuilds that duplicate the dep graph + overrun disk).
    if getattr(args, "core_only", False) and entry.get("target_dir"):
        return False
    if for_probe and entry.get("skip_probe"):
        return False
    return True


def _fail(entry, message):
    fixture_id = entry.get("id", "<missing id>")
    raise ValueError(f"{fixture_id}: {message}")


def _require_file(entry, path, label):
    if not path.is_file():
        _fail(entry, f"missing {label}: {path}")


def _require_dir(entry, path, label):
    if not path.is_dir():
        _fail(entry, f"missing {label}: {path}")


def _load_toml(entry, path):
    try:
        with open(path, "rb") as f:
            return tomllib.load(f)
    except tomllib.TOMLDecodeError as exc:
        _fail(entry, f"{path}: invalid TOML: {exc}")


def _package_name(entry, path):
    package = (_load_toml(entry, path).get("package") or {})
    return package.get("name")


def _workspace_members(entry, path):
    workspace = (_load_toml(entry, path).get("workspace") or {})
    return workspace.get("members") or []


def _system_default_launch(entry, path):
    system = (_load_toml(entry, path).get("system") or {})
    return system.get("default_launch")


def _cmake_has_entry_target(text, entry_name):
    escaped = re.escape(entry_name)
    patterns = [
        # RFC-0048 / phase-287 W3 — the CURRENT spelling. Every C/C++/mixed entry
        # uses it; the detector knew only the older verbs, so all 47 of those rows
        # failed validation while building fine. Same drift as issue 0350: a verb
        # migration swept the CMakeLists and a checker that reads them did not
        # follow.
        rf"\bnano_ros_add_executable\s*\(\s*{escaped}\b",
        rf"\bnano_ros_entry\s*\([^)]*\bNAME\s+{escaped}\b",
        rf"\badd_executable\s*\(\s*{escaped}\b",
        rf"\badd_library\s*\(\s*{escaped}\b",
    ]
    return any(re.search(pattern, text, re.DOTALL) for pattern in patterns)


def _validate_rust_workspace(entry, root, entry_dir):
    workspace_manifest = root / "Cargo.toml"
    _require_file(entry, workspace_manifest, "workspace Cargo.toml")

    member_names = set()
    member_basenames = set()
    for member in _workspace_members(entry, workspace_manifest):
        member_basenames.add(Path(member).name)
        member_manifest = root / member / "Cargo.toml"
        if member_manifest.is_file():
            name = _package_name(entry, member_manifest)
            if name:
                member_names.add(name)

    expected = entry["entry"]
    if expected not in member_names and expected not in member_basenames:
        _fail(
            entry,
            f"Rust entry {expected!r} is not listed in workspace Cargo.toml "
            "members or package names",
        )

    _require_file(entry, entry_dir / "Cargo.toml", "entry Cargo.toml")


def _validate_zephyr_workspace(entry, root, entry_dir):
    # A Zephyr west app is neither a cargo member nor a plain
    # add_executable/add_library target — it is driven by
    # find_package(Zephyr) + project() and links the entry via
    # rust_cargo_application() (Rust) or target_sources(app ...) (C/C++).
    entry_cmake = entry_dir / "CMakeLists.txt"
    _require_file(entry, entry_cmake, "entry CMakeLists.txt")

    text = entry_cmake.read_text(encoding="utf-8")
    if "project(" not in text:
        _fail(entry, "entry CMakeLists.txt does not call project(...)")
    has_rust_app = "rust_cargo_application" in text
    has_app_sources = bool(
        re.search(r"\btarget_sources\s*\(\s*app\b", text, re.DOTALL)
    )
    if not (has_rust_app or has_app_sources):
        _fail(
            entry,
            "entry CMakeLists.txt does not link a Zephyr app "
            "(expected rust_cargo_application() or target_sources(app ...))",
        )

    _require_file(entry, entry_dir / "prj.conf", "entry prj.conf")
    for name in entry.get("conf_files", []):
        _require_file(entry, entry_dir / name, f"conf file {name}")


def _validate_cmake_workspace(entry, root, entry_dir):
    root_cmake = root / "CMakeLists.txt"
    entry_cmake = entry_dir / "CMakeLists.txt"
    _require_file(entry, root_cmake, "workspace CMakeLists.txt")
    _require_file(entry, entry_cmake, "entry CMakeLists.txt")

    text = entry_cmake.read_text(encoding="utf-8")
    if not _cmake_has_entry_target(text, entry["entry"]):
        _fail(
            entry,
            "entry CMakeLists.txt does not define an obvious target "
            f"for {entry['entry']!r}",
        )


def validate_workspace_fixture(entry):
    required_keys = ("id", "platform", "lang", "dir", "rmw", "bringup", "entry")
    for key in required_keys:
        if not entry.get(key):
            _fail(entry, f"missing required key {key!r}")

    lang = entry["lang"]
    if lang not in ("rust", "c", "cpp", "mixed"):
        _fail(entry, f"unsupported workspace fixture lang {lang!r}")

    platform = entry["platform"]
    if platform == "zephyr" and not entry.get("board"):
        _fail(entry, "missing required key 'board' for zephyr workspace fixture")

    root = Path(entry["dir"])
    _require_dir(entry, root, "workspace dir")

    bringup_dir = root / entry["bringup"]
    _require_dir(entry, bringup_dir, "bringup dir")
    _require_file(entry, bringup_dir / "package.xml", "bringup package.xml")

    system_toml = bringup_dir / "system.toml"
    _require_file(entry, system_toml, "bringup system.toml")
    # A bringup declares its topology through LAUNCH, and only through launch.
    #
    # This used to accept a second way — a `config/system_model.yaml` on disk —
    # because phase-296 R4 had retired the launch bake. phase-330 reversed
    # that: the SystemModel is a BUILD ARTIFACT, `check-no-tracked-models`
    # rejects a committed one, and W7 deleted the last of them. That left the
    # model arm satisfiable only by an untracked build leftover, so this gate
    # passed or failed depending on whether someone had run `nros sync` in the
    # tree — and disagreed with `check-no-tracked-models` about the very file
    # it was accepting. A gate a stale artifact can satisfy is worse than no
    # gate.
    #
    # The default launch resolves the way the real consumers resolve it:
    # explicit `[system].default_launch`, else the conventional
    # `system.launch.xml`. That fallback is SSoT'd in
    # `nros_orchestration_ir::model_location::launch_to_model_rel` (shared by
    # `nros::main!(launch = …)`, `nros-build` and `nano_ros_entry(LAUNCH …)`) —
    # keep this line in step with it. It cannot delegate: that function is a
    # pure name mapping and never touches the filesystem, and existence is
    # exactly what this gate is for.
    default_launch = _system_default_launch(entry, system_toml) or DEFAULT_LAUNCH_FILE
    _require_file(
        entry,
        bringup_dir / "launch" / default_launch,
        f"default launch file (declare `[system].default_launch` in "
        f"{system_toml} to name a different one)",
    )

    entry_dir = root / "src" / entry["entry"]
    _require_dir(entry, entry_dir, "entry dir")
    _require_file(entry, entry_dir / "package.xml", "entry package.xml")

    if platform == "zephyr":
        _validate_zephyr_workspace(entry, root, entry_dir)
    elif lang == "rust":
        _validate_rust_workspace(entry, root, entry_dir)
    else:
        _validate_cmake_workspace(entry, root, entry_dir)


def validate_workspace_fixtures(entries):
    count = 0
    for entry in entries:
        validate_workspace_fixture(entry)
        count += 1
    return count


def main():
    p = argparse.ArgumentParser()
    p.add_argument(
        "command",
        choices=[
            "list",
            "list-workspaces",
            "validate-workspaces",
            # phase-319 W2 — the compile-check lane's inventory.
            "list-compile-checks",
            "validate-compile-checks",
            # Issue 0406 — classify one id across ALL row kinds, so a builder
            # that matched nothing can say WHY instead of exiting 0 silently.
            "describe-id",
            # Issue 0406 — the platform vocabulary, so a builder can reject a
            # typo instead of sweeping zero rows successfully.
            "list-platforms",
        ],
    )
    p.add_argument("--manifest", default=DEFAULT_MANIFEST)
    p.add_argument("--platform")
    p.add_argument("--lang")
    p.add_argument("--rmw")
    p.add_argument("--id")
    # The test-all staleness probe builds with the default (stable) toolchain
    # and can't replicate a recipe-injected platform toolchain (e.g. the ESP32
    # nightly + build-std). Such cells set `skip_probe = true` so --for-probe
    # omits them — otherwise the probe rebuilds them under the wrong toolchain
    # every preflight (toolchain-fingerprint thrash → permanent false-stale).
    p.add_argument("--for-probe", action="store_true")
    # Phase 226.D — prepend `<platform>\x1f` to each rust cargo record so
    # the stale probe (scripts/test/rust-fixture-stale.sh) can feed the
    # shared fixture-target-dir resolver, which keys on platform. The
    # build path (fixtures-build.sh) already knows the platform from its
    # CLI arg, so it does NOT pass this flag and keeps the 3-field record.
    p.add_argument("--with-platform", action="store_true")
    # Issue #29 — `--core-only` restricts to the default-config fixtures: rows
    # that do NOT declare an isolated `target_dir`. The RMW/feature variant
    # cells (TLS, safety-e2e, zero-copy, zenoh, xrce, large-buf) each author an
    # isolated `target_dir`, so each is a full standalone rebuild of the dep
    # graph — the duplication that overruns the host-integration runner disk.
    # Those variants are exercised by other lanes (platform-ci native cells,
    # the RMW-specific lanes); the host-integration lane only needs the
    # default-RMW per-example fixtures + the workspace fixtures, so it builds
    # with `--core-only` and the variant-spawning tests `skip!` here.
    # phase-318 W4.d — restrict to a CI lane's fixture coordinates. The file
    # holds `platform,lang,rmw` lines (see `lane-coords`), so a lane's build, its
    # staleness gate and its test selection all derive from ONE computation
    # instead of three hand-kept lists.
    p.add_argument(
        "--coords-from",
        metavar="FILE",
        help="only rows whose (platform,lang,rmw) appears in FILE (one triple per line)",
    )
    p.add_argument("--core-only", action="store_true")
    # phase-319 W2 — narrow `list-compile-checks` to one builder, so the shell
    # lane can keep its per-builder loops.
    p.add_argument("--builder")
    a = p.parse_args()

    if a.command == "list-platforms":
        # Issue 0406. Every platform naming at least one buildable row, from
        # both the single-node and workspace tables. Compile-check rows carry
        # no platform.
        platforms = set()
        for e in load(a.manifest):
            if e.get("platform"):
                platforms.add(e["platform"])
        for e in load_workspace_fixtures(a.manifest):
            if e.get("platform"):
                platforms.add(e["platform"])
        for name in sorted(platforms):
            sys.stdout.write(f"{name}\n")
        return

    if a.command == "describe-id":
        # Issue 0406. Prints one `kind<SEP>platform<SEP>lang<SEP>rmw` line per
        # row carrying this id, across every kind, IGNORING --platform/--lang
        # (the caller already knows those did not match; what it needs is where
        # the id actually lives). No output = the id exists nowhere.
        if not a.id:
            sys.stderr.write("fixtures-manifest.py: describe-id needs --id\n")
            sys.exit(2)
        for e in load(a.manifest):
            if e.get("id") == a.id:
                sys.stdout.write(
                    SEP.join(
                        (
                            "fixture",
                            str(e.get("platform", "")),
                            str(e.get("lang", "")),
                            str(e.get("rmw", "")),
                        )
                    )
                    + "\n"
                )
        for e in load_workspace_fixtures(a.manifest):
            if e.get("id") == a.id:
                sys.stdout.write(
                    SEP.join(
                        (
                            "workspace_fixture",
                            str(e.get("platform", "")),
                            str(e.get("lang", "")),
                            str(e.get("rmw", "")),
                        )
                    )
                    + "\n"
                )
        for e in load_compile_check_fixtures(a.manifest):
            if e.get("id") == a.id:
                sys.stdout.write(
                    SEP.join(
                        ("compile_check_fixture", "", "", str(e.get("builder", "")))
                    )
                    + "\n"
                )
        return

    if a.command in ("list-compile-checks", "validate-compile-checks"):
        entries = []
        for e in load_compile_check_fixtures(a.manifest):
            if a.id and e.get("id") != a.id:
                continue
            if a.builder and e.get("builder") != a.builder:
                continue
            entries.append(e)

        if a.command == "validate-compile-checks":
            try:
                # Validate the WHOLE table, not the filtered view — a filter must
                # never hide a malformed row.
                count = validate_compile_check_fixtures(
                    load_compile_check_fixtures(a.manifest)
                )
            except ValueError as exc:
                sys.stderr.write(f"fixtures-manifest.py: {exc}\n")
                sys.exit(1)
            sys.stdout.write(f"validated {count} compile-check fixture(s)\n")
            return

        for e in entries:
            sys.stdout.write(f"{compile_check_record(e)}\n")
        return

    if a.command in ("list-workspaces", "validate-workspaces"):
        entries = []
        for e in load_workspace_fixtures(a.manifest):
            if not matches_filters(e, a):
                continue
            if a.for_probe and e.get("skip_probe"):
                continue
            entries.append(e)

        if a.command == "validate-workspaces":
            try:
                count = validate_workspace_fixtures(entries)
            except ValueError as exc:
                sys.stderr.write(f"fixtures-manifest.py: {exc}\n")
                sys.exit(1)
            sys.stdout.write(f"validated {count} workspace fixture(s)\n")
            return

        for e in entries:
            sys.stdout.write(f"{workspace_record(e)}\n")
        return

    for e in load(a.manifest):
        if not matches_filters(e, a, for_probe=a.for_probe):
            continue
        if e.get("lang") in ("c", "cpp"):
            # cmake record: <dir>\x1f<build-subdir>\x1f<cmake -D defs>\x1f<target>
            sub = e.get("build_subdir") or (f"build-{e['rmw']}" if e.get("rmw") else "build")
            sys.stdout.write(
                f"{e['dir']}{SEP}{sub}{SEP}{cmake_defs(e)}{SEP}{e.get('target', '')}\n"
            )
        else:
            # cargo record: <dir>\x1f<env>\x1f<cargo-args>
            # With --with-platform: <platform>\x1f<dir>\x1f<env>\x1f<cargo-args>
            prefix = f"{e.get('platform', '')}{SEP}" if a.with_platform else ""
            sys.stdout.write(f"{prefix}{e['dir']}{SEP}{env_str(e)}{SEP}{cargo_args(e)}\n")


if __name__ == "__main__":
    main()
