#!/usr/bin/env python3
"""Issue 2932 -- a host build never exports a libc name.

A bare-metal platform crate supplies the C library its image lacks:
`#[unsafe(no_mangle)] extern "C" fn malloc`, `memcpy`, `strtol`, ... That is
right on `target_os = "none"`, where nothing else defines them. The same crate
is also built for the HOST: its unit tests (`just check bare-metal-wall-clock`)
and host clippy. There a `no_mangle` libc name is not a stand-in. It REPLACES
glibc's, for the whole process. `nros-platform-mps2-an385` did exactly that
with `malloc`/`free`/`realloc`/`calloc`: every allocation in its test binary,
libtest's threads included, went to a `FreeListHeap` whose `Sync` impl assumes
one context, and a race aborted a full gate with an rlsf "bitmap
contradiction" (2026-11-24). It passed 20 of 20 runs solo, so only a check
that looks at the binary catches the class.

WHAT IT READS, all derived:

  * the NAMES: every `extern "C" fn` that a `no_mangle` attribute exports from
    a `libc_stubs.rs` under `packages/`, plus the four allocator names;
  * the CRATES: every crate under `packages/` whose sources export one of
    those names with `no_mangle` (the stub crates themselves);
  * the ARTIFACTS: each crate's host `--lib` build (its rlib), built with every
    `libc-*` feature it declares except `c-libc` (the switch that turns them
    off), so the check sees the most the crate can export.

It fails if `nm --defined-only` finds any of the names defined in one. An
rlib holds the crate's objects, so a definition there reaches every host
program that links the crate: its unit tests, host tooling, a clippy run.
The self-test (run on every invocation) compiles a C object that defines
`malloc` and one that does not, and requires the first to be flagged and the
second not, so a parser that stopped reading `nm` cannot pass silently.

Usage:
    check-host-libc-symbols.py            # build + check (build tier)
    check-host-libc-symbols.py --selftest # the negative control alone
"""

from __future__ import annotations

import json
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
ALLOCATOR = {"malloc", "free", "realloc", "calloc"}

# `#[unsafe(no_mangle)]` (or the 2021 spelling), any attributes/doc between,
# then `pub unsafe extern "C" fn <name>`.
EXPORT = re.compile(
    r"#\[(?:unsafe\()?no_mangle\)?\]\s*(?:#\[[^\]]*\]\s*|///[^\n]*\n\s*)*"
    r"(?:pub\s+)?(?:unsafe\s+)?extern\s+\"C\"\s+fn\s+([A-Za-z_][A-Za-z0-9_]*)"
)
NM_LINE = re.compile(r"^[0-9a-fA-F]*\s+([A-Za-z])\s+(\S+)$")


def exported(text: str) -> set[str]:
    return set(EXPORT.findall(text))


def libc_names(root: Path = REPO) -> set[str]:
    names = set(ALLOCATOR)
    tracked = subprocess.run(
        ["git", "-C", str(root), "ls-files", "packages/*libc_stubs.rs"],
        capture_output=True, text=True, check=True,
    ).stdout.split()
    for rel in tracked:
        names |= exported((root / rel).read_text(encoding="utf-8"))
    return names


def crate_dir(src: Path) -> Path | None:
    for d in src.parents:
        if (d / "Cargo.toml").is_file():
            return d
    return None


def stub_crates(names: set[str], root: Path = REPO) -> list[Path]:
    out = set()
    tracked = subprocess.run(
        ["git", "-C", str(root), "ls-files", "packages/*.rs"],
        capture_output=True, text=True, check=True,
    ).stdout.split()
    for rel in tracked:
        p = root / rel
        if exported(p.read_text(encoding="utf-8", errors="replace")) & names:
            d = crate_dir(p)
            if d:
                out.add(d)
    return sorted(out)


def defined(nm_text: str, names: set[str]) -> set[str]:
    """Names from `nm --defined-only` output that are in `names` (text or weak)."""
    hits = set()
    for line in nm_text.splitlines():
        m = NM_LINE.match(line.strip())
        if m and m.group(1) in "TtWwVv" and m.group(2) in names:
            hits.add(m.group(2))
    return hits


def nm(path: str) -> str:
    return subprocess.run(["nm", "--defined-only", path],
                          capture_output=True, text=True, check=True).stdout


def libc_features(manifest: Path) -> list[str]:
    feats, in_feats = [], False
    for line in manifest.read_text(encoding="utf-8").splitlines():
        s = line.strip()
        if s.startswith("["):
            in_feats = s == "[features]"
            continue
        m = re.match(r"([A-Za-z0-9_-]+)\s*=", s)
        if in_feats and m and m.group(1).startswith("libc-"):
            feats.append(m.group(1))
    return feats


def cargo_artifacts(crate: Path, cmd: list[str]) -> list[str] | None:
    """Run `cmd` (a cargo build with `--message-format=json`) and return the
    artifact files THIS crate produced, or None if it does not build here."""
    out = subprocess.run(cmd, capture_output=True, text=True, cwd=REPO)
    if out.returncode != 0:
        return None
    files = []
    want = crate.resolve()
    for line in out.stdout.splitlines():
        try:
            msg = json.loads(line)
        except json.JSONDecodeError:
            continue
        if msg.get("reason") != "compiler-artifact":
            continue
        if Path(msg.get("manifest_path", "")).resolve().parent != want:
            continue
        if msg.get("executable"):
            files.append(msg["executable"])
        else:
            files += [f for f in msg.get("filenames", [])
                      if f.endswith((".rlib", ".a", ".so"))]
    return files


def host_builds(crate: Path) -> tuple[str, list[str]]:
    """(kind, files) for the crate's host builds: its `--lib` artifacts, or,
    when the library does not build for the host on its own (bare-metal
    `asm!` outside `cfg(test)`), its host `--lib` TEST binary, which is the
    build the bare-metal gates run. ("none", []) if neither builds here."""
    manifest = crate / "Cargo.toml"
    feats = libc_features(manifest)
    extra = ["--features", ",".join(feats)] if feats else []
    base = ["--message-format=json", "--manifest-path", str(manifest), *extra]
    libs = cargo_artifacts(crate, ["cargo", "build", "--lib", *base])
    if libs:
        return "lib", libs
    tests = cargo_artifacts(crate, ["cargo", "test", "--lib", "--no-run", *base])
    if tests:
        return "test", tests
    return "none", []


def selftest(quiet: bool = False) -> None:
    cc = shutil.which("cc") or shutil.which("gcc") or shutil.which("clang")
    assert exported('#[unsafe(no_mangle)]\npub unsafe extern "C" fn malloc(n: usize) {}') == {"malloc"}
    assert exported('#[no_mangle]\n/// doc\npub extern "C" fn strlen() {}') == {"strlen"}
    assert exported('pub extern "C" fn malloc() {}') == set(), "an unexported fn is not an export"
    assert defined("0000000000015724 T calloc\n00000000000157ac T malloc\n"
                   "0000000000001000 T main\n", ALLOCATOR) == {"calloc", "malloc"}
    assert defined("                 U malloc\n", ALLOCATOR) == set(), "a reference is not a definition"
    if cc is None:
        raise SystemExit("check-host-libc-symbols: no C compiler for the negative control")
    with tempfile.TemporaryDirectory() as d:
        bad, good = Path(d, "bad.c"), Path(d, "good.c")
        bad.write_text("#include <stddef.h>\nvoid *malloc(size_t n) { (void)n; return 0; }\n")
        good.write_text("int not_libc(void) { return 0; }\n")
        for src in (bad, good):
            subprocess.run([cc, "-c", str(src), "-o", str(src.with_suffix(".o"))], check=True)
        assert defined(nm(str(bad.with_suffix(".o"))), ALLOCATOR) == {"malloc"}, \
            "negative control: an object defining malloc was not flagged"
        assert defined(nm(str(good.with_suffix(".o"))), ALLOCATOR) == set()
    names = libc_names()
    assert ALLOCATOR <= names and "memcpy" in names, sorted(names)
    if not quiet:
        print("check-host-libc-symbols: self-test OK")


def main(argv: list[str]) -> int:
    if argv[1:] == ["--selftest"]:
        selftest()
        return 0
    selftest(quiet=True)
    names = libc_names()
    crates = stub_crates(names)
    if not crates:
        print("check-host-libc-symbols: found no crate exporting a libc name -- "
              "the derivation is broken, not the tree", file=sys.stderr)
        return 2
    bad = []
    for crate in crates:
        kind, exes = host_builds(crate)
        if kind == "none":
            # Neither its library nor its unit tests build for the host, so no
            # host program can link these names from it. Said, not skipped.
            print(f"  {crate.relative_to(REPO)}: no host build (library and unit tests "
                  "both refuse the host target); nothing a host process could link")
            continue
        for exe in exes:
            hits = defined(nm(exe), names)
            if hits:
                bad.append((crate.relative_to(REPO), exe, sorted(hits)))
    if bad:
        print("check-host-libc-symbols: a HOST build exports libc names (issue 2932).")
        for crate, exe, hits in bad:
            print(f"  {crate}: {', '.join(hits)}\n    in {exe}")
        print("On the host these replace glibc's for the whole process. Export them only\n"
              "on bare metal: `#[cfg(... target_os = \"none\")]` on the stub module.")
        return 1
    print(f"check-host-libc-symbols: {len(crates)} stub crate(s) "
          f"({', '.join(str(c.relative_to(REPO)) for c in crates)}): no host build "
          f"defines any of {len(names)} libc name(s).")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
