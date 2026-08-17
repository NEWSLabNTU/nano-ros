#!/usr/bin/env python3
"""Issue 0582 — a place that means "the host" spelled as a literal triple.

Nine sites wore this bug and five of them failed SILENTLY: an empty
`find_program` result skipped a block and the build degraded to GNU ld, which
then failed on the picolibc TLS-`errno` mix the block existed to avoid, naming
neither the missing tool nor the toolchain file. On x86 every one of them is
invisible, which is why the defect survived a year after being written down in
`docs/development/audit-findings-2026-07-28.md`.

The issue asks for a gate on the two signatures that are mechanical:

  M2  a `find_program` with `NO_DEFAULT_PATH` whose `PATHS` carries a LITERAL
      target triple. `NO_DEFAULT_PATH` is what makes it silent: without it the
      tool is still found on PATH and the hardcoding is merely redundant.
      Resolve the directory instead — `nros_host_rustlib_bin()` is the helper
      that exists for it.

  M3  a TRACKED `.cargo/config.toml` whose `[build] target` equals the host
      triple. Such a pin is always either a no-op (on that host) or a bug (on
      any other), so it is never the right thing to commit.

Both currently have zero instances. That is the point: this gate is for the
fourth site, not the first three, and it fails loudly the moment one appears.

`--self-test` checks both directions, because a checker that stopped checking
passes silently — the failure shape this whole issue is about.
"""

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

# A rust target triple as a literal. Deliberately anchored on the ARCH so a
# path fragment like `lib/cmake` cannot match, and deliberately NOT limited to
# x86_64: hardcoding `aarch64` is the same bug on an x86 host, and writing the
# check as "x86_64 only" would reproduce the asymmetry that hid this for a year.
TRIPLE = re.compile(
    r"\b(?:x86_64|aarch64|i686|armv7|armv7a|riscv64|riscv32|thumbv[0-9]\w*|powerpc64)"
    r"-[a-z0-9_]+-[a-z0-9_]+(?:-[a-z0-9_]+)?\b"
)

# `find_program(...)` including newlines; cmake calls routinely span lines.
FIND_PROGRAM = re.compile(r"find_program\s*\((.*?)\)", re.S | re.I)


def strip_comments(text):
    """Drop `#` comments, keeping newlines so reported line numbers stay true.

    Comments explaining a triple are documentation, and a gate that flags its
    own prose gets bypassed (the lesson issue 0555's checker records)."""
    out = []
    for line in text.splitlines(True):
        idx = line.find("#")
        out.append(line if idx < 0 else line[:idx] + "\n")
    return "".join(out)


def tracked(*globs):
    args = ["git", "-C", str(ROOT), "ls-files", "--"]
    args.extend(globs)
    return subprocess.run(args, capture_output=True, text=True, check=True).stdout.split()


def m2_offenders(files):
    hits = []
    for rel in files:
        try:
            raw = (ROOT / rel).read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        code = strip_comments(raw)
        for m in FIND_PROGRAM.finditer(code):
            body = m.group(1)
            if "NO_DEFAULT_PATH" not in body.upper():
                continue
            found = TRIPLE.search(body)
            if found:
                line = code[: m.start()].count("\n") + 1
                hits.append((rel, line, found.group(0)))
    return hits


def host_triple():
    try:
        out = subprocess.run(["rustc", "-vV"], capture_output=True, text=True, check=True).stdout
    except (OSError, subprocess.CalledProcessError):
        return None
    for line in out.splitlines():
        if line.startswith("host:"):
            return line.split(":", 1)[1].strip()
    return None


def m3_offenders(files, host):
    if not host:
        return []
    hits = []
    for rel in files:
        try:
            raw = (ROOT / rel).read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        in_build = False
        for n, line in enumerate(strip_comments(raw).splitlines(), 1):
            s = line.strip()
            if s.startswith("["):
                in_build = s == "[build]"
                continue
            if in_build and s.startswith("target") and host in s:
                hits.append((rel, n, host))
    return hits


def self_test():
    import tempfile

    tmp_root = ROOT / "tmp"
    tmp_root.mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory(dir=tmp_root) as d:
        d = Path(d)
        probe = d / "probe.cmake"
        rel = str(probe.relative_to(ROOT))

        # 1 — the signature IS reported.
        probe.write_text(
            'find_program(RUST_LLD rust-lld\n'
            '  PATHS "$ENV{HOME}/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/lib"\n'
            '  NO_DEFAULT_PATH)\n'
        )
        if not m2_offenders([rel]):
            sys.stderr.write("self-test: a literal triple under NO_DEFAULT_PATH was NOT reported\n")
            sys.exit(2)

        # 2 — without NO_DEFAULT_PATH it is redundant, not silent: PATH still
        #     finds the tool. Not this gate's business.
        probe.write_text(
            'find_program(RUST_LLD rust-lld\n'
            '  PATHS "/opt/x86_64-unknown-linux-gnu/bin")\n'
        )
        if m2_offenders([rel]):
            sys.stderr.write("self-test: a find_program WITHOUT NO_DEFAULT_PATH was reported\n")
            sys.exit(2)

        # 3 — a resolved variable is the fix, and must not be flagged.
        probe.write_text('find_program(RUST_LLD rust-lld PATHS "${_lld_dir}" NO_DEFAULT_PATH)\n')
        if m2_offenders([rel]):
            sys.stderr.write("self-test: a resolved directory WAS reported\n")
            sys.exit(2)

        # 4 — prose mentioning a triple is documentation.
        probe.write_text(
            '# x86_64-unknown-linux-gnu was hardcoded here once; see issue 0582.\n'
            'find_program(RUST_LLD rust-lld PATHS "${_lld_dir}" NO_DEFAULT_PATH)\n'
        )
        if m2_offenders([rel]):
            sys.stderr.write("self-test: a COMMENT was reported\n")
            sys.exit(2)

        # 5 — M3 both ways.
        cfg = d / "config.toml"
        crel = str(cfg.relative_to(ROOT))
        cfg.write_text('[build]\ntarget = "HOSTTRIPLE"\n'.replace("HOSTTRIPLE", "x86_64-unknown-linux-gnu"))
        if not m3_offenders([crel], "x86_64-unknown-linux-gnu"):
            sys.stderr.write("self-test: a host-triple [build] target was NOT reported\n")
            sys.exit(2)
        cfg.write_text('[build]\ntarget = "thumbv7em-none-eabihf"\n')
        if m3_offenders([crel], "x86_64-unknown-linux-gnu"):
            sys.stderr.write("self-test: a CROSS [build] target was reported\n")
            sys.exit(2)


def main():
    self_test()

    cmake_files = [f for f in tracked("*.cmake", "CMakeLists.txt")
                   if not f.startswith("third-party/")
                   and "/zenoh-pico/" not in f
                   and "/scripts/zephyr/sdk/" not in f]
    configs = tracked("*/.cargo/config.toml", ".cargo/config.toml")
    if not cmake_files:
        sys.stderr.write("[FAIL] no cmake files scanned — this gate would pass vacuously.\n")
        return 1

    host = host_triple()
    rc = 0

    m2 = m2_offenders(cmake_files)
    if m2:
        sys.stderr.write("[FAIL] literal target triple under NO_DEFAULT_PATH (issue 0582):\n")
        for rel, line, tri in m2:
            sys.stderr.write(f"         {rel}:{line}: {tri}\n")
        sys.stderr.write(
            "\n       `NO_DEFAULT_PATH` is what makes this SILENT: the lookup returns\n"
            "       empty off that architecture, the guarded block is skipped, and the\n"
            "       build degrades — naming neither the missing tool nor this file.\n"
            "       Resolve the directory instead: `nros_host_rustlib_bin()`.\n"
        )
        rc = 1

    m3 = m3_offenders(configs, host)
    if m3:
        sys.stderr.write("[FAIL] tracked `[build] target` pinned to the host triple (issue 0582):\n")
        for rel, line, tri in m3:
            sys.stderr.write(f"         {rel}:{line}: {tri}\n")
        sys.stderr.write(
            "\n       Such a pin is a no-op on this host and a forced cross-compile on\n"
            "       any other. Drop the key; cargo already builds for the host.\n"
        )
        rc = 1

    if rc == 0:
        print(
            f"host-triple literals: OK ({len(cmake_files)} cmake file(s), "
            f"{len(configs)} cargo config(s), host {host or 'unknown'})"
        )
    return rc


if __name__ == "__main__":
    sys.exit(main())
