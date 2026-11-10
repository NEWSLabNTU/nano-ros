#!/usr/bin/env python3
"""Every road asks cargo for the image's STATICLIB ONLY — issue 2407.

cargo passes no `-C lto` to a crate whose crate-type also includes `lib` or
`rlib`, and it says nothing. nros-cpp is `["staticlib", "lib"]` (other crates
depend on its rlib) and every generated Zephyr entry is
`["staticlib", "rlib"]`, so building them with `cargo build` silently drops the
profile's LTO: the nucleo action-server image is 59,800 B (16 %) larger in
`.text` that way. The manifests keep their `lib`; each ROAD instead asks for
the archive alone, which is when cargo applies LTO:

  * cmake: `corrosion_import_crate(... CRATE_TYPES staticlib)`, which Corrosion
    turns into `cargo rustc --crate-type=staticlib`;
  * Zephyr west: `scripts/zephyr/cargo-features-patch.sh` rewrites
    zephyr-lang-rust's build command to `cargo rustc --crate-type staticlib`
    (3.7's `CARGO_ARGS build`, 4.4's literal `cargo build`);
  * Zephyr C/C++: `nros_cargo_build()` (nros-c, nros-cpp, the runtime crate)
    runs `cargo rustc --lib --crate-type staticlib`;
  * PX4: the archive builds (the placer command, `just px4`, the release
    staging script) say `cargo rustc -p nros-cpp --lib --crate-type staticlib`.

A road that forgets loses LTO with no error and no warning, so this gate reads
each road's source. The cargo road (`nros build`) builds `[[bin]]` entries,
which get LTO whatever the package's lib types, and is not a road here.

Run: python3 scripts/check-staticlib-only-roads.py
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent

# A corrosion import that is not an image archive, with the reason.
CORROSION_EXEMPT = {
    "packages/cli/nros-cli/CMakeLists.txt": "the host `nros` CLI, a [[bin]]; no image links it",
}
WEST_PATCH = "scripts/zephyr/cargo-features-patch.sh"
WEST_MARKER = "nano-ros: staticlib only (issue 2407)"
WEST_REWRITES = ("CARGO_ARGS rustc --crate-type staticlib", "cargo rustc --crate-type staticlib")
# The Zephyr C/C++ lane's own cargo step (nros-c, nros-cpp, the runtime crate).
ZEPHYR_C_LANE = "zephyr/cmake/nros_cargo_build.cmake"
ZEPHYR_C_LANE_ARGS = re.compile(
    r"set\(CARGO_ARGS\s+rustc\s+--lib\s+--crate-type\s+staticlib\b")
PX4_FILES_GLOBS = ("integrations/px4/*.cmake", "just/px4.just", "scripts/stage-px4-*.sh")
NROS_ARCHIVE_PKG = re.compile(r"-p\s+nros-c(pp)?\b")

IMPORT = re.compile(r"corrosion_import_crate\s*\(", re.I)


STRING = re.compile(r'"(?:[^"\\]|\\.)*"', re.S)


def strip_cmake_comments(text: str) -> str:
    """Comments and string literals out: a `message("... corrosion_import_crate()")`
    names the command without calling it."""
    text = STRING.sub('""', text)
    return "\n".join(line.split("#", 1)[0] if not line.lstrip().startswith("#") else ""
                     for line in text.splitlines())


def corrosion_problems(path: str, text: str) -> list[str]:
    out = []
    body = strip_cmake_comments(text)
    for m in IMPORT.finditer(body):
        depth, i = 1, m.end()
        while i < len(body) and depth:
            depth += {"(": 1, ")": -1}.get(body[i], 0)
            i += 1
        args = body[m.end():i - 1]
        if not re.search(r"\bCRATE_TYPES\s+staticlib\b", args):
            line = body[:m.start()].count("\n") + 1
            out.append(f"{path}:{line}: corrosion_import_crate without `CRATE_TYPES staticlib` "
                       "-- Corrosion then runs `cargo build`, and cargo drops LTO for a crate "
                       "that also emits an rlib (issue 2407)")
    return out


def west_problems(text: str) -> list[str]:
    missing = [s for s in (WEST_MARKER, *WEST_REWRITES) if s not in text]
    return [f"{WEST_PATCH}: no staticlib-only hunk ({', '.join(repr(s) for s in missing)} "
            "missing) -- the Zephyr Rust image would build the rlib too and lose LTO "
            "(issue 2407)"] if missing else []


def zephyr_c_lane_problems(body: str) -> list[str]:
    if ZEPHYR_C_LANE_ARGS.search(body):
        return []
    return [f"{ZEPHYR_C_LANE}: nros_cargo_build()'s CARGO_ARGS do not start `rustc --lib "
            "--crate-type staticlib` -- every Zephyr C/C++ image's nros-c / nros-cpp would "
            "build the rlib too and lose LTO (issue 2407)"]


def px4_problems(path: str, text: str) -> list[str]:
    out = []
    lines = text.splitlines()
    for n, line in enumerate(lines, 1):
        code = line.split("#", 1)[0] if path.endswith((".cmake", ".sh", ".just")) else line
        # a command may continue onto the next line; join the continuation
        joined = code
        k = n
        while joined.rstrip().endswith("\\") and k < len(lines):
            joined = joined.rstrip()[:-1] + " " + lines[k].split("#", 1)[0]
            k += 1
        if re.search(r"\bcargo\s+build\b", joined) and NROS_ARCHIVE_PKG.search(joined):
            out.append(f"{path}:{n}: `cargo build` of the nros archive -- write `cargo rustc "
                       "-p nros-cpp --lib --crate-type staticlib`, or cargo drops LTO "
                       "(issue 2407)")
    return out


def tracked(globs) -> list[str]:
    r = subprocess.run(["git", "ls-files", "--", *globs], cwd=REPO, check=True,
                       capture_output=True, text=True)
    return [p for p in r.stdout.splitlines() if p]


def self_test() -> list[str]:
    bad = []
    if corrosion_problems("a", "corrosion_import_crate(\n  MANIFEST_PATH x\n  CRATE_TYPES staticlib\n)"):
        bad.append("a staticlib import was refused")
    if not corrosion_problems("a", "corrosion_import_crate(MANIFEST_PATH x CRATES y)"):
        bad.append("an import without CRATE_TYPES was accepted")
    if not corrosion_problems("a", "corrosion_import_crate(MANIFEST_PATH x # CRATE_TYPES staticlib\n)"):
        bad.append("a CRATE_TYPES in a comment was credited")
    if not corrosion_problems("a", "corrosion_import_crate(MANIFEST_PATH x CRATE_TYPES staticlib cdylib)") == []:
        bad.append("staticlib among several types was refused")
    if corrosion_problems("a", 'message(FATAL_ERROR "call it after corrosion_import_crate()")'):
        bad.append("a mention inside a string was taken for a call")
    if west_problems(f"{WEST_MARKER}\n{WEST_REWRITES[0]}\n{WEST_REWRITES[1]}\n"):
        bad.append("a complete west hunk was refused")
    if not west_problems("no hunk here"):
        bad.append("a missing west hunk was accepted")
    if zephyr_c_lane_problems("set(CARGO_ARGS\n    rustc\n    --lib\n    --crate-type staticlib\n    -p x"):
        bad.append("a staticlib C lane was refused")
    if not zephyr_c_lane_problems("set(CARGO_ARGS\n    build\n    -p x"):
        bad.append("a `build` C lane was accepted")
    if not px4_problems("x.just", "    cargo build -p nros-cpp --no-default-features \\\n        --release\n"):
        bad.append("a `cargo build -p nros-cpp` was accepted")
    if not px4_problems("x.sh", "cargo build --manifest-path m \\\n  -p nros-cpp --release\n"):
        bad.append("a continued `cargo build ... -p nros-cpp` was accepted")
    if px4_problems("x.just", "    cargo rustc -p nros-cpp --lib --crate-type staticlib --release\n"):
        bad.append("a `cargo rustc --crate-type staticlib` was refused")
    if px4_problems("x.cmake", "# cargo build -p nros-cpp in a comment\n"):
        bad.append("a comment was refused")
    return bad


def main() -> int:
    bad = self_test()
    if bad:
        print("check-staticlib-only-roads: self-test FAILED:\n  " + "\n  ".join(bad), file=sys.stderr)
        return 1
    problems: list[str] = []
    imports = 0
    for path in tracked(["*.cmake", "CMakeLists.txt", "**/CMakeLists.txt"]):
        if path.startswith("third-party/") or path in CORROSION_EXEMPT:
            continue
        text = (REPO / path).read_text(errors="replace")
        imports += len(IMPORT.findall(strip_cmake_comments(text)))
        problems += corrosion_problems(path, text)
    problems += west_problems((REPO / WEST_PATCH).read_text())
    problems += zephyr_c_lane_problems(strip_cmake_comments((REPO / ZEPHYR_C_LANE).read_text()))
    px4 = tracked(list(PX4_FILES_GLOBS))
    for path in px4:
        problems += px4_problems(path, (REPO / path).read_text(errors="replace"))
    if imports == 0 or not px4:
        problems.append(f"found {imports} corrosion import(s) and {len(px4)} PX4 file(s): the "
                        "scan reached nothing, so it proves nothing")
    if problems:
        print("check-staticlib-only-roads: FAIL\n  " + "\n  ".join(problems), file=sys.stderr)
        return 1
    print(f"check-staticlib-only-roads: OK -- {imports} corrosion import(s), the west patch, the "
          f"Zephyr C/C++ lane, and "
          f"{len(px4)} PX4 file(s) all ask cargo for the staticlib only "
          f"({len(CORROSION_EXEMPT)} exempt: {', '.join(CORROSION_EXEMPT)})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
