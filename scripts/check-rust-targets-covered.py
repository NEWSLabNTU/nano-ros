#!/usr/bin/env python3
"""Every Rust cross target DECLARED in the tree has a row in config/rust-targets.txt.

Issue 0833. The list itself removes the installer-vs-doctor divergence; this
gate removes the next one up. A board lands a new triple in its
`nros-board.toml`, a toolchain file sets `Rust_CARGO_TARGET`, a leaf pins
`[build] target` — and nothing connects any of those to the provisioning
recipe. `armv8r-none-eabihf` was declared by two boards and two fixture rows
while `just doctor` never looked for it, so the doctor passed on a host where
`just freertos build-fixtures` could not get past cmake configure.

Three declaring producers, because fixing one and leaving the others running is
this repo's most-repeated bug shape (CLAUDE.md, "fix the CLASS"):

  * packages/boards/*/nros-board.toml   `[target.<triple>]`
  * cmake/toolchain/*.cmake             `set(Rust_CARGO_TARGET "<triple>" ...)`
  * **/.cargo/config.toml               `[build] target = "<triple>"`

phase-445 W6 emptied the THIRD producer for `examples/**`: an example tracks no
`.cargo/` file any more, and its triple comes from the board its `system.toml`
names (RFC-0098 D1). That is not a coverage loss here, and the direction is why
— every board descriptor gained its `[build] target` in W2, so the FIRST
producer already declares every triple an example can reach, and this list is
allowed to be a superset anyway. The third producer is kept because the ~23
tracked configs outside `examples/` still pin triples of their own.

The list is allowed to be a SUPERSET — it carries targets no board declares yet
(armv7r for the Orin SPE board). That direction is one-way on purpose:
over-provisioning costs a download, under-provisioning costs a red build whose
error is four layers from its cause.

## The SDK index is a FOURTH place, and it drifted (issue 0944)

`nros-sdk-index.toml` has its own `[rust.target.*]` table, walked by
`nros setup --check`, which computes `rustup target add <triple>` as the remedy.
It was a hand-authored second copy of this list with nothing between them, and
it was already stale: `armv8r-none-eabihf` was absent, so the CLI's own doctor
surface could not report it missing on a host that needed it. That is issue
0833's defect one layer up — the reason to gate it rather than just add the row.

This direction is EXACT, not superset, and in both senses:

  * a `rustup` row MUST have a `[rust.target.*]` entry — otherwise
    `nros setup --check` is blind to it, which is how armv8r got missed;
  * a `build-std` row MUST NOT — those targets have no prebuilt rust-std,
    `rustup target list` never reports them, and `rustup target add` on one
    fails. An entry there would make the CLI print a remedy that cannot work.

Several aliases may share a triple (`thumbv7m` and `thumbv7m-nightly` differ
only by `toolchain`); the check is on the set of triples, not the aliases.
"""

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
LIST = ROOT / "config" / "rust-targets.txt"
INDEX = ROOT / "nros-sdk-index.toml"


def declared_rows():
    """(target, source) for every declaration, via git ls-files (no traversal)."""
    rows = []

    def tracked(*globs):
        out = subprocess.run(
            ["git", "-C", str(ROOT), "ls-files", "-z", *globs],
            capture_output=True, text=True, check=True,
        ).stdout
        return [p for p in out.split("\0") if p]

    for rel in tracked("packages/boards/*/nros-board.toml"):
        text = (ROOT / rel).read_text(encoding="utf-8", errors="replace")
        for m in re.finditer(r"^\[target\.([^\]]+)\]", text, re.M):
            rows.append((m.group(1), rel))

    for rel in tracked("cmake/toolchain/*.cmake"):
        text = (ROOT / rel).read_text(encoding="utf-8", errors="replace")
        for m in re.finditer(r'set\(\s*Rust_CARGO_TARGET\s+"([^"]+)"', text):
            rows.append((m.group(1), rel))

    for rel in tracked("*.cargo/config.toml", ".cargo/config.toml"):
        text = (ROOT / rel).read_text(encoding="utf-8", errors="replace")
        # Only [build] target, not [target.<triple>.<key>] override tables.
        in_build = False
        for line in text.splitlines():
            s = line.strip()
            if s.startswith("["):
                in_build = s == "[build]"
                continue
            if not in_build:
                continue
            m = re.match(r'target\s*=\s*"([^"]+)"', s)
            if m:
                rows.append((triple_of_build_target(m.group(1)), rel))
    return rows


def triple_of_build_target(value):
    """The triple a `[build] target` value names.

    A custom target is declared by its JSON PATH, and the triple is the file
    STEM — what `rustup`/`-Zbuild-std` name and what cargo uses for
    `[target.<triple>]`. Stripping only the extension kept the directory, so
    `../riscv32imac-unknown-nuttx-elf.json` (the nuttx-riscv FFI subcrate, once
    the board owned the one spec, phase-445) read as a target named
    `../riscv32imac-unknown-nuttx-elf`. Same rule as the CLI's
    `triple_of_build_target` and `check-board-cargo-config-shape`.
    """
    if value.endswith(".json"):
        return value[: -len(".json")].replace("\\", "/").rsplit("/", 1)[-1]
    return value


def listed():
    if not LIST.is_file():
        print(f"MISSING: {LIST.relative_to(ROOT)}", file=sys.stderr)
        sys.exit(2)
    out = {}
    for raw in LIST.read_text(encoding="utf-8").splitlines():
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        parts = line.split()
        if len(parts) != 2 or parts[1] not in ("rustup", "build-std"):
            print(
                f"config/rust-targets.txt: bad row {raw!r}\n"
                "  expected: <triple> <rustup|build-std>",
                file=sys.stderr,
            )
            sys.exit(2)
        out[parts[0]] = parts[1]
    return out


def index_triples():
    """Triples named by `[rust.target.*]` in the SDK index.

    Parsed by regex rather than a TOML library so this gate keeps working on a
    host with no `tomllib` (it runs before any provisioning) and so a syntax
    error in the index is reported by `just check sdk-index`, which owns it,
    rather than surfacing here as a stack trace.
    """
    if not INDEX.is_file():
        print(f"MISSING: {INDEX.name}", file=sys.stderr)
        sys.exit(2)
    triples = set()
    in_block = False
    for raw in INDEX.read_text(encoding="utf-8").splitlines():
        line = raw.strip()
        if line.startswith("["):
            in_block = line.startswith("[rust.target.")
            continue
        if in_block:
            m = re.match(r'triple\s*=\s*"([^"]+)"', line)
            if m:
                triples.add(m.group(1))
    return triples


def toolchain_file_triples():
    """The `targets = [...]` array in rust-toolchain.toml."""
    import re as _re
    path = ROOT / "rust-toolchain.toml"
    if not path.is_file():
        return None
    text = path.read_text(encoding="utf-8")
    m = _re.search(r"^targets\s*=\s*\[(.*?)\]", text, _re.S | _re.M)
    if not m:
        return set()
    return set(_re.findall(r'"([^"]+)"', m.group(1)))


def check_toolchain_file(known):
    """`rustup` rows <-> rust-toolchain.toml `targets`, exactly.

    A FIFTH place, and it drifted the same way the SDK index did (issue 0944).
    rustup reads that file automatically, so it is worth keeping — but it was
    hand-authored from a grep of `--target` rather than from this list, which
    made it wrong in BOTH directions at once: it carried three triples no row
    had (aarch64-unknown-none, armv7a-none-eabi, armv7a-none-eabihf) and missed
    one that did (armv8r-none-eabihf). Same shape as the CLI source-dirs list,
    which was blind in both directions for the same reason.

    build-std rows are excluded: rustup cannot install them, and naming one here
    makes every `rustup` invocation in the tree fail.
    """
    have = toolchain_file_triples()
    if have is None:
        return 0
    want = {t for t, kind in known.items() if kind == "rustup"}
    build_std = {t for t, kind in known.items() if kind == "build-std"}

    absent = sorted(want - have)
    forbidden = sorted(have & build_std)
    stray = sorted(have - want - build_std)

    if not (absent or forbidden or stray):
        return 0

    print("rust-toolchain.toml `targets` disagrees with "
          "config/rust-targets.txt:\n", file=sys.stderr)
    for t in absent:
        print(f"  MISSING from rust-toolchain.toml: {t}", file=sys.stderr)
        print("      A fresh clone or a reinstalled toolchain will not have it, "
              "and the\n      failure arrives as `can't find crate for `core`` "
              "in a cross build.", file=sys.stderr)
    for t in forbidden:
        print(f"  build-std target in rust-toolchain.toml: {t}", file=sys.stderr)
        print("      rustup cannot install it; every rustup call in the tree "
              "then fails.", file=sys.stderr)
    for t in stray:
        print(f"  in rust-toolchain.toml but in no row: {t}", file=sys.stderr)
        print("      Add it to config/rust-targets.txt (with its declaring "
              "site) or drop it.", file=sys.stderr)
    print("\n  rust-toolchain.toml MIRRORS the `rustup` rows. Never hand-edit "
          "it\n  against a grep of `--target`.", file=sys.stderr)
    return 1


def check_index(known):
    """`rustup` rows <-> `[rust.target.*]`, exactly. Issue 0944."""
    have = index_triples()
    want = {t for t, kind in known.items() if kind == "rustup"}
    build_std = {t for t, kind in known.items() if kind == "build-std"}

    absent = sorted(want - have)
    forbidden = sorted(have & build_std)
    stray = sorted(have - want - build_std)

    if not (absent or forbidden or stray):
        return 0

    print("nros-sdk-index.toml `[rust.target.*]` disagrees with "
          "config/rust-targets.txt:\n", file=sys.stderr)
    for t in absent:
        print(f"  MISSING from the index: {t}", file=sys.stderr)
        print("      `nros setup --check` cannot report it missing, so a host "
              "without it\n      gets a cmake CONFIGURE error instead of a "
              "remedy (issue 0944).", file=sys.stderr)
    for t in forbidden:
        print(f"  build-std target listed in the index: {t}", file=sys.stderr)
        print("      `rustup target add` cannot install it; the index would "
              "print a remedy\n      that always fails.", file=sys.stderr)
    for t in stray:
        print(f"  in the index but in no row: {t}", file=sys.stderr)
        print("      Add it to config/rust-targets.txt or drop it from the "
              "index.", file=sys.stderr)
    print("\n  [rust.target.<alias>]\n  triple = \"<triple>\"", file=sys.stderr)
    return 1


def self_test():
    """The tree exercises at most one of `check_index`'s three verdicts at a
    time, so the other two would ship unproven. Issue 0942's lesson: a gate that
    has never been shown to fail is not known to work."""
    ok = True

    def case(name, have, known, want_rc):
        nonlocal ok
        global index_triples
        saved = index_triples
        index_triples = lambda: set(have)
        try:
            import io, contextlib
            buf = io.StringIO()
            with contextlib.redirect_stderr(buf):
                rc = check_index(known)
        finally:
            index_triples = saved
        if rc != want_rc:
            ok = False
            print(f"  self-test FAIL {name}: rc={rc} want {want_rc}")

    rustup = {"a": "rustup", "b": "rustup"}
    both = {"a": "rustup", "n": "build-std"}

    case("in sync", ["a", "b"], rustup, 0)
    case("missing from index", ["a"], rustup, 1)
    case("build-std listed in index", ["a", "n"], both, 1)
    case("stray index entry", ["a", "zzz"], {"a": "rustup"}, 1)
    case("build-std absent is fine", ["a"], both, 0)

    # A spec PATH names its file stem — the directory is not part of the triple
    # (phase-445: `../riscv32imac-unknown-nuttx-elf.json` read as a target named
    # `../riscv32imac-unknown-nuttx-elf` when only the extension was stripped).
    for value, want in [
        ("thumbv7m-none-eabi", "thumbv7m-none-eabi"),
        ("riscv32imac-unknown-nuttx-elf.json", "riscv32imac-unknown-nuttx-elf"),
        ("../riscv32imac-unknown-nuttx-elf.json", "riscv32imac-unknown-nuttx-elf"),
        ("${workspace}/packages/boards/b/x-y-z.json", "x-y-z"),
    ]:
        got = triple_of_build_target(value)
        if got != want:
            ok = False
            print(f"  self-test FAIL triple_of_build_target({value!r}) = {got!r}, want {want!r}")

    print("check-rust-targets-covered --self-test: "
          + ("OK" if ok else "FAILED"))
    return 0 if ok else 1


def main():
    if "--self-test" in sys.argv:
        return self_test()
    known = listed()
    missing = {}
    for target, source in declared_rows():
        if target not in known:
            missing.setdefault(target, set()).add(source)

    if missing:
        print("Rust cross targets declared in-tree with no row in "
              "config/rust-targets.txt:\n", file=sys.stderr)
        for target in sorted(missing):
            print(f"  {target}", file=sys.stderr)
            for src in sorted(missing[target]):
                print(f"      declared by {src}", file=sys.stderr)
        print(
            "\nAdd a row to config/rust-targets.txt. Column 2:\n"
            "  rustup     — `rustup target list` knows it (prebuilt rust-std).\n"
            "               `just workspace rust-targets` will install it and\n"
            "               `just doctor` will verify it; both read that file.\n"
            "  build-std  — Tier 3 / custom JSON. Nothing to install; the row\n"
            "               exists so this gate can tell deliberate from forgotten.\n"
            "\nWithout the row nothing provisions the target, and the failure\n"
            "surfaces as a cmake CONFIGURE error from corrosion (Issue 0833).",
            file=sys.stderr,
        )
        return 1

    if check_index(known) != 0:
        return 1
    if check_toolchain_file(known) != 0:
        return 1

    rustup = sum(1 for k in known.values() if k == "rustup")
    print(f"check-rust-targets-covered: OK "
          f"({len(known)} listed, {len(set(t for t, _ in declared_rows()))} declared, "
          f"{rustup} mirrored in the SDK index and in rust-toolchain.toml)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
