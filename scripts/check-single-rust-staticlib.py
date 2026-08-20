#!/usr/bin/env python3
"""issue 0734 — one image links exactly ONE nano-ros Rust staticlib.

`cmake/NanoRosRuntimeCrate.cmake:6` states the invariant:

    Single-runtime invariant: a binary links exactly ONE Rust staticlib (one
    std, one nros-rmw-cffi C ABI + REGISTRY).

It matters because a cargo `staticlib` bundles its ENTIRE dependency closure.
Link two of them and the shared closure is linked twice — and because the two
archives are separate cargo builds with different `-C metadata`, the duplicated
statics do not even collide. Nothing folds, `--allow-multiple-definition` never
fires, and the linker allocates both. Measured on mr_canhubk3/s32k344:

    0x20474cf1 0x20000 libnros_c.a  (…Cs*ewqHElJteY4*…SUBSCRIBER_BUFFERS)
    0x2049ccfb 0x20000 libnros_cpp.a(…Cs*hvwoP2UscId*…SUBSCRIBER_BUFFERS)

~195 KiB of duplicated `.bss` on a 320 KiB part, and the subscriber ring state
present in two divergent copies. On a host this is invisible, which is why it
survived: Phase 241.D3-rev removed the double link from the crate graph
(`nros-cpp` deps `nros-c` as an RLIB so ONE archive carries both) and issue
0425 swept the generic cmake path, but `zephyr/CMakeLists.txt` kept doing it
for two more phases.

THE RULE: no single branch may link more than one umbrella.

Sibling branches are fine and are the CORRECT shape — a file legitimately links
the C umbrella in its C-API arm and the C++ umbrella in its C++ arm, because at
most one runs. So this tracks the if/elseif/else path and only complains when
two different umbrellas are linked on the SAME path.

Comments are stripped first. `check-cmake-image-policy.py` records why (issue
0196): a mechanical grep once excluded a site because a COMMENT there mentioned
the name it was looking for, reporting a clean sweep over a file it never
examined. This file's own header names every umbrella; without stripping, it
would flag itself.

Run: python3 scripts/check-single-rust-staticlib.py
"""
import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent / "lib"))
from tracked import tracked  # noqa: E402  (path set above)

ROOT = Path(__file__).resolve().parent.parent

# The umbrellas. Each is a cargo `staticlib` (or an alias for one); linking two
# into one image is the defect. Grouped so an alias and its target count as the
# same umbrella.
UMBRELLAS = {
    "c": {"nros_c_cargo", "NanoRos::NanoRos", "nros_c::nros_c", "nros_c-static"},
    "cpp": {"nros_cpp_cargo", "NanoRos::NanoRosCpp", "nros_cpp-static"},
    "ws": {"nros_ws_runtime_cargo", "nros_ws_runtime-static"},
}
LINK_CALLS = ("target_link_libraries", "zephyr_library_link_libraries")

SEARCH = ["cmake", "zephyr", "integrations"]


def strip_comments(text: str) -> str:
    """Drop `#` comments, keeping line count so numbers stay honest."""
    out = []
    for line in text.splitlines():
        out.append(re.sub(r"#.*$", "", line))
    return "\n".join(out)


def umbrella_of(token: str):
    for name, members in UMBRELLAS.items():
        if token in members:
            return name
    return None


def check(path: Path):
    text = strip_comments(path.read_text(encoding="utf-8", errors="replace"))
    violations = []
    # Branch path: a list of (if-id, arm-index). Two links share a path only if
    # every enclosing conditional is on the same arm.
    path_stack = []
    next_if = 0
    # EVERY link site, not just the first per umbrella. Keeping only the first
    # is how the first draft of this gate reported OK against the very defect it
    # was written for: `zephyr/CMakeLists.txt` links the C umbrella legitimately
    # in its C-API arm at line 272, that site was recorded, and the SECOND,
    # buggy C link was then never compared against anything.
    sites = []  # (umbrella, line, branch-path)

    for lineno, line in enumerate(text.splitlines(), 1):
        low = line.strip()
        if re.match(r"^if\s*\(", low):
            path_stack.append([next_if, 0])
            next_if += 1
            continue
        if re.match(r"^(elseif|else)\s*\(", low):
            if path_stack:
                path_stack[-1][1] += 1
            continue
        if re.match(r"^endif\s*\(", low):
            if path_stack:
                path_stack.pop()
            continue

        for call in LINK_CALLS:
            if f"{call}(" not in line:
                continue
            for token in re.findall(r"[A-Za-z_][A-Za-z0-9_:.-]*", line):
                which = umbrella_of(token)
                if which is None:
                    continue
                key = tuple(tuple(p) for p in path_stack)
                for other, other_line, other_key in sites:
                    if other == which:
                        continue
                    # Same branch path, or one ENCLOSES the other: both run in
                    # the same image. Sibling arms (differing arm index at some
                    # level) are the correct shape and are not flagged.
                    if key[: len(other_key)] == other_key or other_key[: len(key)] == key:
                        violations.append(
                            f"{path.relative_to(ROOT)}:{lineno}: links the "
                            f"'{which}' umbrella ({token}) on the same branch as "
                            f"the '{other}' umbrella linked at line {other_line}. "
                            f"A staticlib bundles its whole closure — linking two "
                            f"duplicates it. See issue 0734."
                        )
                sites.append((which, lineno, key))
    return violations


def main() -> int:
    # issue 0721 — these are TRACKED files, so they come from the git index.
    # `rglob` under `cmake`/`zephyr`/`integrations` also descends every build
    # tree that happens to sit there, which is the walk that rule forbids and
    # what `check-no-tracked-file-find` flagged here.
    files = sorted(
        set(tracked(*SEARCH, suffix=".cmake")) | set(tracked(*SEARCH, name="CMakeLists.txt"))
    )
    problems = []
    for f in files:
        problems += check(f)
    if problems:
        print("check-single-rust-staticlib: FAIL\n", file=sys.stderr)
        for p in problems:
            print(f"  {p}", file=sys.stderr)
        print(
            "\n  A binary links exactly ONE Rust staticlib "
            "(cmake/NanoRosRuntimeCrate.cmake:6). Prefer the bundling umbrella:\n"
            "  NanoRosCpp bundles nros-c; the ws-runtime bundles nros-cpp.",
            file=sys.stderr,
        )
        return 1
    print(f"check-single-rust-staticlib: OK ({len(files)} cmake files)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
