#!/usr/bin/env python3
"""Phase 365 W4 — the SDK store is CONSTRUCTED from the pin, never enumerated.

nano-ros decides where a provisioned tool goes: `nros setup` writes
`<store>/<tool>/<version>` because `nros-sdk-index.toml` named that version. A
consumer therefore builds the path from those same two inputs — via
`nros sdk-path <tool>`, `sdk_store::tool_dir()` inside the CLI, or the pin
readers `nros_sdk_pin()` (cmake, `cmake/NanoRosSdkPin.cmake`), `nros_sdk_pinned_version` (shell,
`scripts/lib/sdk-pin.sh`) and `nros_build_paths::sdk_pinned_version()` (Rust
build scripts) — and does not go looking.

WHAT THIS BANS, AND WHY IT IS ENUMERATION AND NOT MENTION

The first draft of this gate was "one spelling of `.nros/sdk` per language". A
survey killed that: the 18 sites in the tree are three populations, and only one
is wrong.

  * INSTALLERS legitimately write the store (`scripts/zenohd/build.sh`,
    `scripts/xrce-agent/build.sh`) — they are the producer.
  * `.nros/sdks/arm-fvp` is a DIFFERENT tree (`sdks`, not `sdk`).
  * the defect is a consumer ENUMERATING versions and picking one — a glob or
    `ls` over `<store>/<tool>/*` followed by "newest", "last", or "sorted".

So the rule is about the SHAPE, not the string. A per-project pin cannot be
answered by scanning a store that is shared between projects, which is what
made this worth a gate:

  * measured 2026-08-16, one `lane=all` configure in a tree pinning
    `corrosion 0.6.1-nros1`: **155** resolutions of 0.5.1 against 28 of 0.6.1;
  * two independent causes, both enumeration — cmake APPENDED its newest-first
    candidates after the environment's, and `cmake-prefix.sh` globbed
    `$store/corrosion/*/`, which matched the legacy unversioned install's
    `lib/` and `share/` subdirectories (not versions at all, and under
    `sort -Vr` a pure-alpha name sorts BEFORE the numeric ones);
  * `scripts/dev/zenohd.sh` did `ls …/sdk/zenohd/*/bin/zenohd | sort -V | tail`,
    and `cmake/toolchain/riscv64-threadx.cmake` globbed
    `riscv-none-elf-gcc/*/` and took `list(GET … -1)`. Same class, other tools,
    both found by this rule rather than by a failure.

TWO SHAPES, BECAUSE THE FIRST ONE WAS NARROWER THAN THE RULE (issue 1546)

  1. LITERAL — a store path with a wildcard where the version belongs
     (`/sdk/<tool>/*`). This was the whole gate until issue 1546.
  2. WINDOWED — an enumeration primitive (cmake `file(GLOB`, shell `ls` /
     `find` / a `for … in "$x"/*` glob, Rust `read_dir(`) on a line with a
     STORE reference at or up to WINDOW lines above it, AND a sort/pick
     (`list(SORT`, `sort`, `.sort(`, `.reverse()`, `list(GET … -1)`, `| tail`)
     at or up to WINDOW lines below it.

  Shape 1 alone missed every real site that 1546 found: the shared
  cross-toolchain helper spells the store `"${_store}/*"`, the riscv64 shell
  helper `ls -1 "$store" | sort -Vr`, and two Rust resolvers `read_dir(&dir)`
  then `.sort(); .reverse()`. The fix for issue 0625 had moved the defect into
  a shared helper — out of reach of the regex that was supposed to keep it out.
  Shape 2 needs the SORT because listing what the store holds, to NAME it in a
  diagnostic, is legitimate: nothing is chosen from an unsorted list.

WHAT IS NOT BANNED (phase-431 W3)

`sdk_store::installed_versions` reads `<store>/<tool>/` and takes the newest.
That is enumeration, and it is correct, because it answers a DIFFERENT question:
not "where is the version I pinned" (constructible from two inputs) but "what is
the newest thing installed here" (nothing but the store knows). It backs the
`front` link — `$NROS_HOME/bin/nros` points at the newest installed CLI, which is
the "one command" promise. It carries 0625's defence in the filter rather than in
the sort: a candidate must begin with a digit AND carry a `.nros-provenance`
marker, so `lib/` under a legacy flat prefix cannot win by sorting. It is
exempted BY FUNCTION NAME in EXEMPT_FUNCTIONS, not by file, so a second
enumeration added to the same file is still caught.

The rule is unchanged for consumers: a PIN is constructed, never searched.

The self-test runs on EVERY invocation, before the tree scan, through the same
`scan_text` the scan uses — a gate whose regex matches nothing says OK too.

Issue 0625; issue 1546; phase-365; phase-431 W3; phase-472 W7/W9.
"""

import re
import subprocess
import sys

# Shape 1 — a store path with a WILDCARD where the VERSION belongs.
#
# Anchored on `/sdk/<tool>/*` and nothing to its left. The first version keyed
# on `.nros` or `NROS_HOME` immediately preceding `/sdk/`, and matched nothing:
# the real spelling is `"${NROS_HOME:-$HOME/.nros}"/sdk/zenohd/*`, where a `}"`
# sits between. That draft passed its own self-test — the gate was vacuous and
# said OK. Hence the self-test below is the point, not a formality.
LITERAL = re.compile(r"""/sdks?/[A-Za-z0-9_.-]+/\*""")
# Shape 3 (issue 1736) — a HARD-CODED version where the pin belongs:
# `~/.nros/sdk/arm-none-eabi-gcc/13.2-nros1/…`. Not enumeration, but the same
# rule ("constructed from the pin") broken the other way: the literal is a
# snapshot of a pin that has since moved (`13.2-nros1` while the index pinned
# `13.2-nros5`, in `check-cpp-freestanding-mechanisms.py`). A path segment
# after `/sdk/<tool>/` that starts with a digit is a version — on a path rooted
# at the REAL store (`.nros`, `$NROS_HOME`, `$NROS_SDK_STORE`, `$NROS_STORE`).
# A selftest building a SYNTHETIC store under its own temp dir
# (`"$d/store/sdk/zephyr-sdk/0.16.8"`) is a producer of a fixture, not a
# consumer of a pin, and is not this.
VERSIONED = re.compile(
    r"""(?:\.nros|\bNROS_HOME\}?|\bNROS_STORE\}?)/sdk/[A-Za-z0-9_.-]+/\d"""
    r"""|\bNROS_SDK_STORE\}?(?::-[^}]*\})?/[A-Za-z0-9_.-]+/\d"""
)

# Shape 2 — the three parts of "enumerate the store, then pick".
ENUMERATE = re.compile(
    r"""file\s*\(\s*GLOB"""  # cmake
    r"""|(^|[\s;|(`$])(ls|find)\s"""  # shell
    r"""|\bfor\s+\w+\s+in\s+[^;]*/\*"""  # shell glob loop
    r"""|\bread_dir\s*\("""  # rust
)
STORE = re.compile(
    r"""NROS_SDK_STORE|NROS_STORE|\.nros/sdk|/sdk\b|\bsdk_store\b|\bstore_root\b"""
    r"""|\$\{?_?store\b|\b_?store\b"""
)
PICK = re.compile(
    r"""list\s*\(\s*SORT"""
    r"""|list\s*\(\s*GET\s+\S+\s+-1"""
    r"""|\bsort\b(?!ed)"""
    r"""|\.sort(_by|_by_key|_unstable|_unstable_by)?\s*\("""
    r"""|\.reverse\s*\(\)|\.rev\s*\(\)|\.max(_by|_by_key)?\s*\(|\.last\s*\(\)"""
    r"""|\|\s*(tail|head)\b"""
)
WINDOW = 8

# Shape 3 (issue 1563) — a VERSION-LEVEL enumeration of the whole store: a glob
# two levels under a store variable (`$_nros_sdk/*/*/bin`), or a store root
# handed to a directory walker at version depth (`_nros_bin_dirs "$_nros_sdk" 3`).
# Nothing is SORTED there, so shape 2 cannot see it — but a PATH is an ORDERED
# list, so building one from that walk picks a version by readdir order.
# `activate.sh` / `activate.fish` did exactly this: `13.2-nros4` beat the pin.
VERSION_LEVEL = re.compile(
    r"""(?:sdk|store)\w*\}?"?/\*/\*"""
    r"""|(?:sdk|store)\w*\}?"?\s+[23]\s*;?\s*\)?\s*$"""
)

# Where consumers live. Rust sources are scanned too: two of the four sites
# issue 1546 found were `read_dir` + `.sort(); .reverse()` in Rust resolvers.
SCAN_PATHSPECS = (
    "cmake/",
    "scripts/",
    "just/",
    "justfile",
    "zephyr/",
    "packages/**/cmake/**",
    "packages/**/*.cmake",
    "packages/**/*.rs",
    "packages/**/*.sh",
    "*.sh",
    # issue 1563 — the fish activation builds a PATH from the store too.
    "*.fish",
)

# The installers write the store; they are the producer, not a consumer.
ALLOWED_PATHS = (
    "scripts/zenohd/build.sh",
    "scripts/xrce-agent/build.sh",
    "scripts/installers/",
    # This gate's own documentation and cases.
    "scripts/check-sdk-store-not-enumerated.py",
)

# (path suffix, enclosing function) -> why. Keyed on the FUNCTION, so the
# exemption covers the one answer it was reasoned about and nothing beside it.
EXEMPT_FUNCTIONS = {
    (
        "orchestration/sdk_store.rs",
        "installed_versions",
    ): "answers 'newest installed', not 'where is my pin' (phase-431 W3)",
}

COMMENT = re.compile(r"""^\s*(#|//|/\*|\*|--)""")
RUST_FN = re.compile(r"""\bfn\s+([A-Za-z_]\w*)""")


def _is_code(line: str) -> bool:
    return bool(line.strip()) and not COMMENT.match(line)


def _enclosing_fn(lines, idx):
    for j in range(idx, -1, -1):
        m = RUST_FN.search(lines[j])
        if m and _is_code(lines[j]):
            return m.group(1)
    return None


def _exempt(path: str, lines, idx) -> bool:
    if not path.endswith(".rs"):
        return False
    fn = _enclosing_fn(lines, idx)
    return any(path.endswith(suffix) and fn == name for (suffix, name) in EXEMPT_FUNCTIONS)


def scan_text(path: str, text: str):
    """Every (lineno, line, shape) in `text` that enumerates the store."""
    hits = []
    lines = text.splitlines()
    for i, line in enumerate(lines):
        if not _is_code(line):
            continue
        if LITERAL.search(line):
            if not _exempt(path, lines, i):
                hits.append((i + 1, line.strip(), "literal"))
            continue
        if VERSIONED.search(line):
            if not _exempt(path, lines, i):
                hits.append((i + 1, line.strip(), "hard-coded pin"))
            continue
        if VERSION_LEVEL.search(line):
            hits.append((i + 1, line.strip(), "version-level walk"))
            continue
        if not ENUMERATE.search(line):
            continue
        above = [ln for ln in lines[max(0, i - WINDOW) : i + 1] if _is_code(ln)]
        below = [ln for ln in lines[i : i + WINDOW + 1] if _is_code(ln)]
        if not any(STORE.search(ln) for ln in above):
            continue
        if not any(PICK.search(ln) for ln in below):
            continue
        if _exempt(path, lines, i):
            continue
        hits.append((i + 1, line.strip(), "enumerate+pick"))
    return hits


# ---------------------------------------------------------------------------
# Self-test. Every BAD case is a shape that shipped; every GOOD case is a
# neighbour the gate must NOT flag (the fix itself among them).

BAD = {
    # issue 1736 — a hard-coded versioned store path, the pin restated.
    "scripts/check-x.py": """\
ARM_GXX = os.path.expanduser("~/.nros/sdk/arm-none-eabi-gcc/13.2-nros1/bin/arm-none-eabi-g++")
""",
    "scripts/build/n.sh": """\
    ninja="$HOME/.nros/sdk/ninja/1.11.1-nros1/bin/ninja"
""",
    "scripts/build/m.sh": """\
    make="${NROS_SDK_STORE}/make/4.4-nros2/bin/make"
""",
    # issue 1546 — the shared cross-toolchain helper, as it was.
    "cmake/x.cmake": """\
    set(_store "${_store_root}/${_A_TOOL}")
    if(IS_DIRECTORY "${_store}")
        file(GLOB _vers RELATIVE "${_store}" "${_store}/*")
        list(SORT _vers COMPARE NATURAL ORDER DESCENDING)
""",
    # Shape 1 ALONE — a store glob with no enumerate+pick in reach, so ONLY
    # `LITERAL` can see it. Without this case every BAD case was also caught
    # by shape 2, and blanking `LITERAL` left the self-test green (phase-472 W9).
    "scripts/build/lit.sh": """\
    cp "$NROS_HOME"/sdk/zenohd/*/bin/zenohd "$out"
""",
    # issue 1546 — the riscv64 shell helper, as it was.
    "scripts/build/x.sh": """\
    local store="${NROS_SDK_STORE:-$HOME/.nros/sdk}/riscv-none-elf-gcc"
    if [ -d "$store" ]; then
        local ver
        for ver in $(ls -1 "$store" 2>/dev/null | sort -Vr); do
""",
    # issue 1546 — the two Rust resolvers, as they were.
    "packages/tooling/x/src/lib.rs": """\
    fn store_bin() -> Option<PathBuf> {
        let dir = sdk_store().join("riscv-none-elf-gcc");
        let mut versions: Vec<_> = std::fs::read_dir(&dir)
            .ok()?
            .collect();
        versions.sort();
        versions.reverse();
""",
    # issue 0625 — zenohd.sh and riscv64-threadx.cmake, as they were.
    "scripts/dev/zenohd.sh": 'z="$(ls "${NROS_HOME:-$HOME/.nros}"/sdk/zenohd/*/bin/zenohd | sort -V | tail -1)"\n',
    "cmake/toolchain/y.cmake": """\
    file(GLOB _cands "$ENV{HOME}/.nros/sdk/riscv-none-elf-gcc/*/")
    list(GET _cands -1 _pick)
""",
    # A shell glob loop over the store, then a pick.
    "just/x.just": """\
    store="$NROS_SDK_STORE/corrosion"
    for d in "$store"/*; do echo "$d"; done | sort -V | tail -1
""",
    # An exempted file is exempt only in the exempted FUNCTION.
    "packages/cli/nros-cli-core/src/orchestration/sdk_store.rs": """\
fn some_new_resolver() -> Option<PathBuf> {
    let dir = store_root().join(tool);
    let mut v: Vec<_> = std::fs::read_dir(&dir).ok()?.collect();
    v.sort();
""",
}

BAD.update({
    # issue 1563 — activate.sh, as it was: every version's bin dir on PATH.
    "activate.sh": """\
    done <<EOF
$(_nros_bin_dirs "$_nros_sdk" 3; _nros_bin_dirs "$_nros_sdk" 2)
EOF
""",
    # ...and activate.fish.
    "activate.fish": """\
    for _nros_tcbin in $_nros_sdk/*/*/bin $_nros_sdk/*/bin
""",
})

GOOD = {
    # issue 1736 — a selftest's SYNTHETIC store is a fixture, not the pin.
    "scripts/ci/doc.sh": """\
    mkdir -p "$d/store/sdk/zephyr-sdk/0.16.8/zephyr-sdk-0.16.8"
""",
    # The fix: constructed from the pin; an UNSORTED listing for a message.
    "cmake/x.cmake": """\
    set(_store "${_store_root}/${_A_TOOL}")
    nros_sdk_pin("${_A_TOOL}" _pin_ver _pin_upstream)
    if(EXISTS "${_store}/${_pin_ver}/bin/${_p}-gcc")
        set(${_A_OUT_PREFIX} "${_store}/${_pin_ver}/bin/${_p}" PARENT_SCOPE)
    endif()
    if(IS_DIRECTORY "${_store}")
        file(GLOB _others RELATIVE "${_store}" LIST_DIRECTORIES true "${_store}/*")
        list(FILTER _others INCLUDE REGEX "^[0-9]")
        set_property(GLOBAL PROPERTY "_NROS_CT_STRANDED_${_A_TOOL}" "${_others}")
    endif()
""",
    "scripts/build/x.sh": """\
    local dir="${NROS_SDK_STORE:-$HOME/.nros/sdk}/riscv-none-elf-gcc"
    for d in "$dir"/[0-9]*/; do
        [ -d "$d" ] || continue
        found="${found:+$found, }${d##*/}"
    done
""",
    # A read_dir + sort that has nothing to do with the store.
    "packages/rmw/zenoh/x/src/lib.rs": """\
fn add_c_sources_recursive(build: &mut cc::Build, dir: &Path) -> usize {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .collect();
    entries.sort_by_key(|e| e.file_name());
""",
    # The sanctioned enumeration, in its own function.
    "packages/cli/nros-cli-core/src/orchestration/sdk_store.rs": """\
pub fn installed_versions(tool: &str) -> Vec<String> {
    let dir = store_root().join(tool);
    let mut v: Vec<_> = std::fs::read_dir(&dir).into_iter().flatten().collect();
    v.sort();
    v.reverse();
""",
    # A comment describing the removed shape is not a reintroduction.
    "scripts/y.sh": """\
# This used to do `ls "$store" | sort -Vr` over /sdk/riscv-none-elf-gcc/* .
store="$(nros sdk-path corrosion)"
""",
}


def self_test() -> list:
    failures = []
    for path, text in BAD.items():
        if not scan_text(path, text):
            failures.append(f"missed a BAD case: {path}")
    for path, text in GOOD.items():
        hits = scan_text(path, text)
        if hits:
            failures.append(f"flagged a GOOD case: {path}: {hits}")
    return failures


def tracked_files():
    out = subprocess.run(
        ["git", "ls-files", "-z", "--", *SCAN_PATHSPECS],
        capture_output=True,
        text=True,
        check=True,
    ).stdout
    return sorted({p for p in out.split("\0") if p})


def main() -> int:
    failures = self_test()
    if failures:
        print("ERROR: check-sdk-store-not-enumerated's self-test failed:", file=sys.stderr)
        for f in failures:
            print(f"  {f}", file=sys.stderr)
        return 2
    if "--self-test" in sys.argv[1:]:
        print(f"sdk store not enumerated: self-test OK ({len(BAD)} bad, {len(GOOD)} good)")
        return 0

    bad = []
    files = tracked_files()
    for path in files:
        if any(path.startswith(a) or a in path for a in ALLOWED_PATHS):
            continue
        try:
            with open(path, encoding="utf-8", errors="replace") as fh:
                text = fh.read()
        except (IsADirectoryError, FileNotFoundError):
            continue
        for lineno, line, shape in scan_text(path, text):
            bad.append(f"{path}:{lineno}: [{shape}] {line}")

    if bad:
        print(
            "ERROR: the SDK store is ENUMERATED instead of constructed from the pin:",
            file=sys.stderr,
        )
        for b in bad:
            print(f"  {b}", file=sys.stderr)
        print(
            "\n  Listing the store and picking a version answers a PER-PROJECT pin\n"
            "  by searching a store SHARED between projects. Measured cost: 155\n"
            "  wrong resolutions against 28 right ones in one configure (issue\n"
            "  0625); a sibling checkout's newer install shadowing the pin (1546).\n"
            "\n"
            "  Construct it instead — <store>/<tool>/<pinned version>:\n"
            "      shell / cmake / just :  nros sdk-path <tool>\n"
            "      cmake, no CLI        :  nros_sdk_pin()  (cmake/NanoRosSdkPin.cmake)\n"
            "      shell, no CLI        :  nros_sdk_pinned_version  (scripts/lib/sdk-pin.sh)\n"
            "      rust build script    :  nros_build_paths::sdk_pinned_version()\n"
            "      inside the CLI       :  sdk_store::tool_dir(&index, tool)",
            file=sys.stderr,
        )
        return 1

    print(
        f"sdk store not enumerated: OK ({len(files)} files, self-test "
        f"{len(BAD)} bad / {len(GOOD)} good)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
