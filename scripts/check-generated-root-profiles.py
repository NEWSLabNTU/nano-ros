#!/usr/bin/env python3
"""A cargo root nano-ros generates carries the root workspace's profiles — issue 1953.

A generated root (the RFC-0071 D9 image crate, the workspace runtime umbrella)
is its own `[workspace]`, so cargo reads profiles from that manifest and from
nothing else. The `nros-*` presets also arrive as `CARGO_PROFILE_*` variables,
but `release` and `dev` never do (nros-cargo-profile's ownership rule), so a
root that states no profile builds `--profile release` with cargo's DEFAULT:
`panic = "unwind"`. That broke every threadx-linux C/C++ image at the nightly's
`CMAKE_BUILD_TYPE=Release` ("unwinding panics are not supported without std")
while every lane that builds `nros-relwithdebinfo` stayed green.

Three checks:

1. Every cmake file that EMITS a `[workspace]` table calls
   `nros_root_cargo_profiles()`, or is listed in EXEMPT with its reason. A new
   generated root that forgets is the defect this gate exists for.
2. `cmake -P cmake/NanoRosRootProfiles.cmake` prints exactly the root
   `Cargo.toml`'s `[profile.*]` tables, and both built-in profiles abort.
3. The image crate generator, run in script mode (the NuttX apps road's
   spelling, no configure), writes a manifest whose `[profile.release]` and
   `[profile.dev]` abort. It reads the backend's facts THROUGH the CLI, so it
   needs the in-tree `nros` (`just setup-cli`); a checkout without one (a
   fresh worktree, the pristine clone) reports it NOT VERIFIED through the
   `nros_check_skip` ledger, never fails, and 1 and 2 still run.

No compiler, no SDK; a few cmake script runs. Fast line.

Usage::

    check-generated-root-profiles.py
"""

from __future__ import annotations

import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

try:
    import tomllib
except ModuleNotFoundError:  # 3.10 backport, as the sibling gates spell it
    import tomli as tomllib

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "scripts" / "lib"))
import check_skip  # noqa: E402  issue 1043's middle outcome, NOT VERIFIED

# The in-tree CLI the image crate's backend dispatch must ask. Passed
# explicitly: left to resolve itself, the dispatch finds whatever `nros` the
# PATH holds, and an older one emits variables this tree does not know.
CLI = ROOT / "packages" / "cli" / "target" / "release" / "nros"
HELPER = ROOT / "cmake" / "NanoRosRootProfiles.cmake"
IMAGE_CRATE = ROOT / "cmake" / "NanoRosImageCrate.cmake"
CALL = "nros_root_cargo_profiles("

# A generated root that legitimately states its own profiles. Each entry is a
# reason, not a convenience.
EXEMPT = {
    "cmake/cpp_ffi_Cargo.toml.in": (
        "the C++ FFI glue is built only at the `cpp-ffi-glue` carve-out "
        "(nros-minsizerel), a preset injected through CARGO_PROFILE_*, so its "
        "own `[profile.release]` is never selected; it also generates from an "
        "INSTALLED prefix, which has no root Cargo.toml to copy from"
    ),
}

WORKSPACE_TABLE = re.compile(r"^\s*\"?(\\n)*\[workspace\]")


def tracked_cmake(root: Path) -> list[str]:
    """The tracked `cmake/` sources, from the index rather than a walk."""
    out = subprocess.run(
        ["git", "-C", str(root), "ls-files", "--", "cmake/*.cmake", "cmake/*.in",
         "cmake/**/*.cmake", "cmake/**/*.in"],
        capture_output=True, text=True, check=True,
    )
    return sorted(set(out.stdout.split()))


def emitters(root: Path, files: list[str] | None = None) -> list[str]:
    """Repo-relative cmake files with a non-comment line opening `[workspace]`."""
    found = []
    for rel in tracked_cmake(root) if files is None else files:
        path = root / rel
        if not path.is_file():
            continue
        for line in path.read_text(encoding="utf-8", errors="replace").splitlines():
            if line.lstrip().startswith("#"):
                continue
            if "[workspace]" in line and ("\\n[workspace]" in line or WORKSPACE_TABLE.match(line)):
                found.append(path.relative_to(root).as_posix())
                break
    return found


def check_emitters(root: Path, exempt: dict[str, str],
                   files: list[str] | None = None) -> list[str]:
    errors = []
    for rel in emitters(root, files):
        if rel in exempt:
            continue
        if CALL not in (root / rel).read_text(encoding="utf-8"):
            errors.append(
                f"{rel} emits a `[workspace]` table and never calls {CALL}...). "
                "A generated root states every profile it is built with, or "
                "`--profile release` gets cargo's `panic = \"unwind\"` (issue 1953)."
            )
    for rel in exempt:
        if not (root / rel).exists():
            errors.append(f"EXEMPT names {rel}, which does not exist; drop the entry.")
    return errors


def run_helper(repo: Path) -> str:
    out = subprocess.run(
        ["cmake", f"-DNROS_ROOT_PROFILES_REPO={repo}", "-P", str(HELPER)],
        capture_output=True, text=True, check=False,
    )
    if out.returncode != 0:
        raise RuntimeError(f"helper failed on {repo}:\n{out.stderr}")
    return out.stdout


def profiles_of(text: str) -> dict:
    return tomllib.loads(text).get("profile", {})


def check_helper(repo: Path) -> list[str]:
    errors = []
    try:
        copied = profiles_of(run_helper(repo))
    except (RuntimeError, tomllib.TOMLDecodeError) as exc:
        return [f"NanoRosRootProfiles.cmake output does not parse: {exc}"]
    want = profiles_of((repo / "Cargo.toml").read_text(encoding="utf-8"))
    if copied != want:
        errors.append(
            "NanoRosRootProfiles.cmake does not reproduce the root profiles:\n"
            f"  root:   {want}\n  copied: {copied}"
        )
    for name in ("release", "dev"):
        if copied.get(name, {}).get("panic") != "abort":
            errors.append(f"copied [profile.{name}] does not abort: {copied.get(name)}")
    return errors


def check_image_crate(tmp: Path) -> list[str]:
    out_dir = tmp / "image"
    run = subprocess.run(
        ["cmake", f"-D_NANO_ROS_CODEGEN_TOOL={CLI}", "-DNROS_IMAGE_FLAVOR=c", "-DNROS_IMAGE_RMW=zenoh",
         "-DNROS_IMAGE_EDITION=humble", f"-DNROS_IMAGE_OUT_DIR={out_dir}",
         "-DNROS_IMAGE_API_FEATURES=alloc;rmw-cffi", "-P", str(IMAGE_CRATE)],
        capture_output=True, text=True, check=False,
    )
    manifest = out_dir / "Cargo.toml"
    if run.returncode != 0 or not manifest.exists():
        return [f"image crate script mode failed (rc={run.returncode}):\n{run.stderr[-2000:]}"]
    got = profiles_of(manifest.read_text(encoding="utf-8"))
    errors = []
    for name in ("release", "dev"):
        if got.get(name, {}).get("panic") != "abort":
            errors.append(
                f"the generated image crate's [profile.{name}] does not abort "
                f"({got.get(name)}); `--profile {name}` would unwind (issue 1953)."
            )
    return errors


def self_test() -> None:
    """Negative controls: each check must catch what it claims to."""
    with tempfile.TemporaryDirectory() as d:
        repo = Path(d)
        (repo / "cmake").mkdir()
        # A comment with `;` and an unbalanced `[` — the shapes that made a
        # file(STRINGS) list emit comment fragments as TOML keys.
        (repo / "Cargo.toml").write_text(
            "[workspace]\nmembers = []\n\n"
            "[profile.release]\n# one; two [three\nopt-level = 3  # trailing; [x\n"
            "panic = \"abort\"\n\n[profile.dev]\npanic = \"abort\"\n\n[patch.crates-io]\n",
            encoding="utf-8",
        )
        assert check_helper(repo) == [], check_helper(repo)
        (repo / "Cargo.toml").write_text(
            "[profile.release]\nopt-level = 3\n\n[profile.dev]\npanic = \"abort\"\n",
            encoding="utf-8",
        )
        assert any("[profile.release] does not abort" in e for e in check_helper(repo))

        (repo / "cmake" / "Bare.cmake").write_text(
            'file(WRITE x "[package]\\nname = \\"x\\"\\n\\n[workspace]\\n")\n', encoding="utf-8")
        (repo / "cmake" / "Commented.cmake").write_text(
            "# a generated root is its own `[workspace]`\n", encoding="utf-8")
        (repo / "cmake" / "Good.cmake").write_text(
            'nros_root_cargo_profiles(_p)\nfile(WRITE x "\\n[workspace]\\n${_p}")\n', encoding="utf-8")
        files = ["cmake/Bare.cmake", "cmake/Commented.cmake", "cmake/Good.cmake"]
        errs = check_emitters(repo, {}, files)
        assert len(errs) == 1 and "Bare.cmake" in errs[0], errs
        assert check_emitters(repo, {"cmake/Bare.cmake": "r"}, files) == []
        assert any("does not exist" in e
                   for e in check_emitters(repo, {"cmake/Gone.in": "r"}, files))


def main() -> int:
    if shutil.which("cmake") is None:
        print("check-generated-root-profiles: cmake not on PATH", file=sys.stderr)
        return 1
    self_test()
    errors = check_emitters(ROOT, EXEMPT) + check_helper(ROOT)
    skipped = 0
    if CLI.is_file():
        with tempfile.TemporaryDirectory() as d:
            errors += check_image_crate(Path(d))
    else:
        skipped = check_skip.unverified(
            "generated-root-profiles",
            f"the image crate's generated profiles: no in-tree CLI at {CLI.relative_to(ROOT)} "
            "(the generator asks it for the backend's facts) — run `just setup-cli`")
    if errors:
        for e in errors:
            print(f"  - {e}", file=sys.stderr)
        print("check-generated-root-profiles: FAILED (issue 1953)", file=sys.stderr)
        return 1
    print(f"check-generated-root-profiles: OK ({len(emitters(ROOT))} generated root(s), "
          f"{len(EXEMPT)} exempt with a reason; self-test ok)")
    return skipped


if __name__ == "__main__":
    sys.exit(main())
