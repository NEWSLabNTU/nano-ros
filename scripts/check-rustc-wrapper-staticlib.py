#!/usr/bin/env python3
"""A `staticlib` never comes out of the sccache cache — issue 1646.

sccache keys a rustc call on its sources, its direct `--extern` rlibs and its
`-l static=` libs. A staticlib bundles its TRANSITIVE closure, including the
native objects a dependency's build script compiled, and rustc reaches those
through `-L dependency=`, which sccache does not hash. So after a C edit in
`zpico-sys`'s build script, `libnros_cpp.a` came back from the cache holding
the OLD object while cargo reported the unit rebuilt.

`scripts/bin/rustc-wrapper/sccache` (the shim) runs a staticlib-emitting rustc
call directly and hands everything else to the real sccache. This gate holds
the two halves of that:

1. THE SHIM ROUTES CORRECTLY — driven, not read: a fake `sccache` and a fake
   `rustc` record which one ran, for every `--crate-type` spelling, a C compile
   (cc-rs prefixes the wrapper to the C compiler), and a host with no sccache.
2. EVERY PRODUCER POINTS AT THE SHIM — each `RUSTC_WRAPPER` assignment in a
   just file, a workflow / composite action, a shell script or a CMake file
   that names sccache must name the shim. A bare `RUSTC_WRAPPER=sccache` is the
   defect, re-armed.

Run: python3 scripts/check-rustc-wrapper-staticlib.py
"""

from __future__ import annotations

import os
import re
import stat
import subprocess
import sys
import tempfile
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(REPO / "scripts" / "lib"))
from file_kinds import files_of_kind  # noqa: E402  phase-472 W5
from population import require_population  # noqa: E402  phase-472 W4

SHIM = REPO / "scripts/bin/rustc-wrapper/sccache"
SHIM_REL = "scripts/bin/rustc-wrapper/sccache"

# `RUSTC_WRAPPER := …`, `RUSTC_WRAPPER=…`, `RUSTC_WRAPPER: …` (YAML env), and
# `set(ENV{RUSTC_WRAPPER} …)`.
ASSIGN = re.compile(r"RUSTC_WRAPPER(?:\}\s+|\s*(?::=|=|:(?!:))\s*)(.+)$")


def _exe(path: Path, body: str) -> None:
    path.write_text(body)
    path.chmod(path.stat().st_mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)


def route(args, *, with_sccache=True) -> str:
    """Run the shim on `args` and return which of the fakes ran."""
    with tempfile.TemporaryDirectory() as tmp:
        t = Path(tmp)
        log = t / "ran"
        bindir = t / "bin"
        bindir.mkdir()
        if with_sccache:
            _exe(bindir / "sccache", f'#!/bin/sh\necho sccache >> "{log}"\n')
        _exe(t / "rustc", f'#!/bin/sh\necho direct >> "{log}"\n')
        env = {k: v for k, v in os.environ.items() if k != "NROS_SCCACHE_REAL"}
        # The shim's own dir first: it must skip ITSELF and find the fake.
        env["PATH"] = os.pathsep.join([str(SHIM.parent), str(bindir), "/usr/bin", "/bin"])
        argv = [str(t / "rustc") if a == "RUSTC" else a for a in args]
        subprocess.run([str(SHIM), *argv], env=env, check=True, cwd=tmp)
        return log.read_text().strip() if log.exists() else ""


def self_test() -> list[str]:
    cases = [
        (["RUSTC", "--crate-type", "staticlib", "--crate-type", "lib"], True, "direct"),
        (["RUSTC", "--crate-type=staticlib"], True, "direct"),
        (["RUSTC", "--crate-type", "lib,staticlib"], True, "direct"),
        (["RUSTC", "--crate-type", "lib"], True, "sccache"),
        (["RUSTC", "--crate-type", "rlib", "--crate-name", "staticlib"], True, "sccache"),
        (["RUSTC", "-vV"], True, "sccache"),
        # cc-rs: the wrapper prefixed to the C compiler.
        (["cc", "-c", "x.c"], True, "sccache"),
        # No sccache anywhere: uncached, never a failure (issue 0874).
        (["RUSTC", "--crate-type", "lib"], False, "direct"),
    ]
    bad = []
    for args, with_sccache, want in cases:
        try:
            got = route(args, with_sccache=with_sccache)
        except subprocess.CalledProcessError as e:
            got = f"rc={e.returncode}"
        if got != want:
            bad.append(f"shim routed {args!r} (sccache={'yes' if with_sccache else 'no'}) "
                       f"to {got or 'nothing'!r}, want {want!r}")
    # The producer matcher, both ways.
    assert ASSIGN.search('export RUSTC_WRAPPER := `command -v sccache`')
    assert ASSIGN.search('echo "RUSTC_WRAPPER=sccache"')
    assert not producer_problem('echo "RUSTC_WRAPPER=$GITHUB_WORKSPACE/' + SHIM_REL + '"')
    assert producer_problem('echo "RUSTC_WRAPPER=sccache"')
    assert producer_problem("RUSTC_WRAPPER: sccache")
    assert producer_problem("set(ENV{RUSTC_WRAPPER} sccache)")
    assert not ASSIGN.search('echo "sccache not found (RUSTC_WRAPPER empty)"')
    assert not producer_problem('printf "  RUSTC_WRAPPER=%s\\n" "${RUSTC_WRAPPER:-<unset>}"')
    return bad


def producer_problem(line: str) -> bool:
    """True when `line` assigns RUSTC_WRAPPER a bare sccache."""
    m = ASSIGN.search(line)
    if not m:
        return False
    value = m.group(1)
    return "sccache" in value and SHIM_REL not in value and "rustc-wrapper/sccache" not in value


def main() -> int:
    if not os.access(SHIM, os.X_OK):
        print(f"check-rustc-wrapper-staticlib: {SHIM_REL} is missing or not executable",
              file=sys.stderr)
        return 1
    bad = self_test()

    files = files_of_kind("just", "ci", "shell", "cmake", "toml")
    assignments = 0
    for rel in files:
        try:
            text = (REPO / rel).read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        for n, line in enumerate(text.splitlines(), 1):
            stripped = line.strip()
            if stripped.startswith("#") or not ASSIGN.search(line):
                continue
            if "sccache" not in line:
                continue
            assignments += 1
            if producer_problem(line):
                bad.append(f"{rel}:{n}: sets RUSTC_WRAPPER to a bare sccache — point it at "
                           f"{SHIM_REL}, which runs a staticlib uncached (issue 1646)")
    if not require_population(assignments, "sccache RUSTC_WRAPPER assignment(s)",
                              gate="check-rustc-wrapper-staticlib"):
        return 1
    if bad:
        print("check-rustc-wrapper-staticlib: FAIL", file=sys.stderr)
        for b in bad:
            print(f"  - {b}", file=sys.stderr)
        return 1
    print("check-rustc-wrapper-staticlib: OK — the shim runs every staticlib uncached and "
          "every producer names it")
    return 0


if __name__ == "__main__":
    sys.exit(main())
