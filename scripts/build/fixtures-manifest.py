#!/usr/bin/env python3
"""Read examples/fixtures.toml — the SSOT for fixture build options (Phase 177.9).

Consumed by both the fixture build recipes and the test-all staleness probe so
they build each fixture with identical features/target-dir/env.

  fixtures-manifest.py list --platform native --lang rust [--rmw zenoh] [--id ID]
  fixtures-manifest.py list-workspaces --platform native [--lang rust] [--id ID]
  fixtures-manifest.py validate-workspaces --platform native

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


def load(path):
    with open(path, "rb") as f:
        return tomllib.load(f).get("fixture", [])


def load_workspace_fixtures(path):
    with open(path, "rb") as f:
        return tomllib.load(f).get("workspace_fixture", [])


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


def matches_filters(entry, args, *, for_probe=False):
    if args.platform and entry.get("platform") != args.platform:
        return False
    if args.lang and entry.get("lang") != args.lang:
        return False
    if args.rmw and entry.get("rmw") != args.rmw:
        return False
    if args.id and entry.get("id") != args.id:
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
    default_launch = _system_default_launch(entry, system_toml)
    if not default_launch:
        _fail(entry, f"{system_toml}: missing [system].default_launch")
    _require_file(
        entry,
        bringup_dir / "launch" / default_launch,
        "default launch file",
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
        choices=["list", "list-workspaces", "validate-workspaces"],
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
    a = p.parse_args()

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
