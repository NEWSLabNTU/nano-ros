#!/usr/bin/env python3
"""Every `std::` C-library name a shim-served source uses must be exported by that shim.

Two directories in this tree stand in for the C++ standard library on targets
that ship none:

  packages/boards/nros-board-threadx-qemu-riscv64/cxx-compat
      put on the include path with `-nostdinc++` for EVERY C++ translation unit
      of a riscv64-threadx build (`cmake/toolchain/riscv64-threadx.cmake`):
      nros-cpp headers, codegen output, examples, and the Cyclone backend.
  zephyr/cxx-compat
      layered over Zephyr's minimal libcpp; its `#else` fallback is the library
      when the toolchain's real `<cstring>` is unreachable.

Each shim header is a hand-written list of `using ::name;` declarations. A
source that spells `std::memchr` against a shim whose `<cstring>` does not list
`memchr` is a compile error on that target and NOWHERE else — every host-side
probe compiles against the host's complete libstdc++, where the name always
exists. That is exactly how the nightly `threadx_riscv64` lane stopped:

    service.cpp:1388:43: error: 'memchr' is not a member of 'std'

while `just check cpp`'s `-nostdinc++` probe stayed green, because it parses the
nros-cpp HEADERS against the ThreadX shim and the shim also serves the Cyclone
backend's `.cpp` files, which it never reads (issue 0196's shape: a probe whose
reach is narrower than the thing it protects). The same shim had already
failed this way once, on `std::abort` (RFC-0089 W3.b).

So this gate binds the two ends by harvesting rather than by restating: it
reads every `std::<name>` the served sources use, keeps the names that belong
to `<cstring>`/`<cstdlib>`/`<cstdio>`, and requires each to be exported by the
matching header of every shim that serves the file. Adding a use and adding the
export are then one change, and forgetting the second is a red here instead of
a red on a nightly-only cross build.

Discovery uses `git ls-files` (never a tree walk). Comments and string
literals are stripped before harvesting, so prose about `std::memchr` does not
count as a use.
"""
from __future__ import annotations

import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(REPO / "scripts" / "lib"))
from git_hook_env import nros_clear_inherited_git_env  # noqa: E402

# The C-library half of each header, as the standard declares it. A name here
# that a served source uses must be exported by the shim's header of that name.
STD_C_NAMES: dict[str, frozenset[str]] = {
    "cstring": frozenset("""
        memchr memcmp memcpy memmove memset strcat strchr strcmp strcoll strcpy
        strcspn strerror strlen strncat strncmp strncpy strpbrk strrchr strspn
        strstr strtok strxfrm""".split()),
    "cstdlib": frozenset("""
        abort abs aligned_alloc at_quick_exit atexit atof atoi atol atoll
        bsearch calloc div div_t exit free getenv labs ldiv ldiv_t llabs lldiv
        lldiv_t malloc mblen mbstowcs mbtowc qsort quick_exit rand realloc
        srand strtod strtof strtol strtold strtoll strtoul strtoull system
        wcstombs wctomb _Exit""".split()),
    "cstdio": frozenset("""
        FILE clearerr fclose feof ferror fflush fgetc fgetpos fgets fopen
        fprintf fputc fputs fread freopen fscanf fseek fsetpos ftell fwrite
        getc getchar perror printf putc putchar puts remove rename rewind
        scanf setbuf setvbuf snprintf sprintf sscanf tmpfile tmpnam ungetc
        vfprintf vfscanf vprintf vscanf vsnprintf vsprintf vsscanf""".split()),
}
NAME_TO_HEADER = {n: h for h, names in STD_C_NAMES.items() for n in names}

# shim dir -> the tracked source roots it serves. A root is a path prefix.
SHIMS: dict[str, tuple[str, ...]] = {
    "packages/boards/nros-board-threadx-qemu-riscv64/cxx-compat": (
        "packages/api/nros-cpp/include/",
        "packages/rmw/cyclonedds/nros-rmw-cyclonedds/src/",
        "packages/rmw/cyclonedds/nros-rmw-cyclonedds/include/",
        "examples/rv-virt-threadx/",
    ),
    "zephyr/cxx-compat": (
        "packages/api/nros-cpp/include/",
        "packages/rmw/cyclonedds/nros-rmw-cyclonedds/src/",
        "packages/rmw/cyclonedds/nros-rmw-cyclonedds/include/",
        "examples/zephyr/",
    ),
}
SOURCE_SUFFIXES = (".cpp", ".cc", ".cxx", ".hpp", ".hh", ".h")

USING_RE = re.compile(r"\busing\s+::\s*([A-Za-z_]\w*)\s*;")
STD_USE_RE = re.compile(r"(?<![\w:])(?:::)?std\s*::\s*([A-Za-z_]\w*)")
COMMENT_OR_STRING_RE = re.compile(
    r'//[^\n]*|/\*.*?\*/|"(?:\\.|[^"\\\n])*"|\'(?:\\.|[^\'\\\n])*\'', re.S
)


def strip(text: str) -> str:
    """Blank comments and literals, keeping newlines so line numbers survive."""
    return COMMENT_OR_STRING_RE.sub(lambda m: re.sub(r"[^\n]", " ", m.group(0)), text)


def exports(shim_dir: Path) -> dict[str, set[str]]:
    out: dict[str, set[str]] = {}
    for header in STD_C_NAMES:
        p = shim_dir / header
        out[header] = set(USING_RE.findall(strip(p.read_text()))) if p.is_file() else set()
    return out


def uses(text: str) -> list[tuple[int, str]]:
    stripped = strip(text)
    found = []
    for m in STD_USE_RE.finditer(stripped):
        name = m.group(1)
        if name in NAME_TO_HEADER:
            found.append((stripped.count("\n", 0, m.start()) + 1, name))
    return found


def tracked(root: Path) -> list[str]:
    # issues 0986/0988 — this gate is on the fast line, so `pre-push` reaches
    # it, and a push from a linked worktree exports `GIT_DIR`, which overrides
    # `git -C`: `ls-files` would answer for another repository.
    env = nros_clear_inherited_git_env(dict(os.environ))
    r = subprocess.run(["git", "-C", str(root), "ls-files", "-z"],
                       capture_output=True, check=True, env=env)
    return [p for p in r.stdout.decode().split("\0") if p]


def check(root: Path, shims: dict[str, tuple[str, ...]]) -> list[str]:
    files = tracked(root)
    problems = []
    for shim, roots in shims.items():
        shim_dir = root / shim
        if not shim_dir.is_dir():
            problems.append(f"{shim}: shim directory is MISSING — this gate would pass on absence")
            continue
        exp = exports(shim_dir)
        served = [f for f in files if f.startswith(roots) and f.endswith(SOURCE_SUFFIXES)]
        if not served:
            problems.append(f"{shim}: serves NO tracked source under {roots} — the roots are stale")
            continue
        for f in served:
            try:
                text = (root / f).read_text(errors="replace")
            except OSError:
                continue
            for line, name in uses(text):
                header = NAME_TO_HEADER[name]
                if name not in exp[header]:
                    problems.append(
                        f"{f}:{line}: std::{name} — {shim}/{header} does not export it "
                        f"(add `using ::{name};`)"
                    )
    return problems


def selftest() -> int:
    """Negative-control both halves: a missing export fails; comments do not count."""
    env = nros_clear_inherited_git_env(dict(os.environ))
    with tempfile.TemporaryDirectory() as td:
        t = Path(td)
        subprocess.run(["git", "init", "-q", str(t)], check=True, env=env)
        (t / "shim").mkdir()
        (t / "shim/cstring").write_text("namespace std {\nusing ::memcpy;\n}\n")
        (t / "src").mkdir()
        (t / "src/a.cpp").write_text(
            "// std::memchr in a comment is not a use\n"
            'const char* s = "std::strchr in a string is not a use";\n'
            "void f(){ std::memcpy(0,0,0); std::memchr(0,0,0); }\n"
        )
        subprocess.run(["git", "-C", str(t), "add", "."], check=True, env=env)
        got = check(t, {"shim": ("src/",)})
        want = ["src/a.cpp:3: std::memchr"]
        if len(got) != 1 or not got[0].startswith(want[0]):
            print(f"check-cxx-compat-shim-coverage --selftest: FAILED, got {got!r}", file=sys.stderr)
            return 1
        (t / "shim/cstring").write_text("namespace std {\nusing ::memcpy;\nusing ::memchr;\n}\n")
        if check(t, {"shim": ("src/",)}):
            print("check-cxx-compat-shim-coverage --selftest: FAILED, a covered use was reported",
                  file=sys.stderr)
            return 1
    print("check-cxx-compat-shim-coverage --selftest: OK (missing export fails; comments/strings ignored)")
    return 0


def main(argv: list[str]) -> int:
    # The negative control runs on EVERY invocation, not behind a flag: a
    # control nobody runs decays into a comment (`check-gate-selftests`).
    rc = selftest()
    if rc or "--selftest" in argv:
        return rc
    problems = check(REPO, SHIMS)
    if problems:
        print(f"check-cxx-compat-shim-coverage: {len(problems)} std:: name(s) a shim does not export:",
              file=sys.stderr)
        for p in problems:
            print(f"  {p}", file=sys.stderr)
        print("\n  These compile on the host (complete libstdc++) and fail only on the target\n"
              "  whose shim is the whole C++ library. Export the name in the shim header.",
              file=sys.stderr)
        return 1
    print(f"check-cxx-compat-shim-coverage: OK ({len(SHIMS)} shim(s); every std:: C-library "
          "name their served sources use is exported)")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
