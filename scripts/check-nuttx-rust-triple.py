#!/usr/bin/env python3
"""The make road's Rust target triple agrees with upstream's table.

phase-550 W4 (issue 2236 Q11 b). `integrations/nuttx/RustTriple.mk` resolves
the ONE triple a make-road build uses: upstream's `RUST_TARGET_TRIPLE`
(nuttx-apps `tools/Rust.mk`) first, then an armv7a fallback for the one arch
upstream has no branch for. This gate evaluates both, with GNU make, over
every arch upstream's macro knows plus the ones it does not, and requires:

  1. where upstream answers, the make road answers the same;
  2. where upstream is silent on armv7a, the fallback answers
     `armv7a-nuttx-<abi>`; once the nuttx-apps pin answers armv7a itself, the
     fallback is dead and this gate says to delete it;
  3. any other arch upstream is silent on gets no triple, and a configured
     tree refuses it while the Makefiles are read, naming LLVM_ARCHTYPE.

It needs the nuttx-apps tree (the source ladder finds it); without one it
exits 78 so the recipe records a skip. `--self-test` runs the same cases
against a fallback that answers for every arch, which must fail.
"""
from __future__ import annotations

import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
TRIPLE_MK = ROOT / "integrations" / "nuttx" / "RustTriple.mk"

# (LLVM_ARCHTYPE, LLVM_ABITYPE, LLVM_CPUTYPE). Upstream's table keys on the
# arch, then the CPU for thumbv8m / riscv; the ABI rides through for arm.
CASES = [
    ("x86_64", "", ""),
    ("x86", "", ""),
    ("thumbv6m", "eabi", "cortex-m0"),
    ("thumbv7m", "eabi", "cortex-m3"),
    ("thumbv7em", "eabihf", "cortex-m4"),
    ("thumbv8m.main", "eabihf", "cortex-m33"),
    ("thumbv8m.main", "eabi", "cortex-m23"),
    ("riscv32", "", "sifive-e20"),
    ("riscv32", "", "sifive-e31"),
    ("riscv32", "", "sifive-e76"),
    ("riscv32", "", ""),
    ("riscv64", "", "sifive-s51"),
    ("riscv64", "", "sifive-u54"),
    ("riscv64", "", ""),
    # Not in upstream's table today.
    ("armv7a", "eabihf", "cortex-a7"),
    ("armv7a", "eabi", "cortex-a9"),
    ("armv7r", "eabihf", "cortex-r5"),
    ("aarch64", "", "cortex-a53"),
    ("xtensa", "", ""),
]

DRIVER = """\
include $(RUST_MK)
include $(TRIPLE_MK)
.PHONY: print
print:
\t@echo 'upstream=$(strip $(call RUST_TARGET_TRIPLE))'
\t@echo 'ours=$(NROS_RUST_TRIPLE)'
"""


def locate_rust_mk() -> Path | None:
    r = subprocess.run(
        [sys.executable, str(ROOT / "scripts/lib/source_locate.py"),
         "nuttx-apps", "tools/Rust.mk"],
        capture_output=True, text=True, check=False)
    if r.returncode != 0 or not r.stdout.strip():
        return None
    return Path(r.stdout.strip()) / "tools" / "Rust.mk"


def run(rust_mk: Path, triple_mk: Path, topdir: Path, case) -> subprocess.CompletedProcess:
    arch, abi, cpu = case
    with tempfile.NamedTemporaryFile("w", suffix=".mk", delete=False) as f:
        f.write(DRIVER)
        driver = f.name
    try:
        return subprocess.run(
            ["make", "-s", "--no-print-directory", "-f", driver, "print",
             f"RUST_MK={rust_mk}", f"TRIPLE_MK={triple_mk}",
             f"TOPDIR={topdir}", "CONFIG_ARCH=x",
             f"LLVM_ARCHTYPE={arch}", f"LLVM_ABITYPE={abi}",
             f"LLVM_CPUTYPE={cpu}"],
            capture_output=True, text=True, check=False)
    finally:
        Path(driver).unlink()


def parse(out: str) -> dict[str, str]:
    vals = {}
    for line in out.splitlines():
        k, _, v = line.partition("=")
        vals[k] = v.strip()
    return vals


def check(rust_mk: Path, triple_mk: Path) -> list[str]:
    errors: list[str] = []
    with tempfile.TemporaryDirectory() as tmp:
        unconfigured = Path(tmp) / "unconfigured"
        configured = Path(tmp) / "configured"
        unconfigured.mkdir()
        (configured / "include" / "nuttx").mkdir(parents=True)
        (configured / "include" / "nuttx" / "config.h").write_text("")
        for case in CASES:
            arch, abi, cpu = case
            label = f"LLVM_ARCHTYPE={arch} ABI={abi or '-'} CPU={cpu or '-'}"
            r = run(rust_mk, triple_mk, unconfigured, case)
            if r.returncode != 0:
                errors.append(f"{label}: make failed on an unconfigured parse:\n{r.stderr.strip()}")
                continue
            v = parse(r.stdout)
            up, ours = v.get("upstream", ""), v.get("ours", "")
            if up:
                if arch == "armv7a":
                    errors.append(
                        f"{label}: upstream's Rust.mk now answers {up!r} for armv7a, so the "
                        "fallback in integrations/nuttx/RustTriple.mk is dead: delete it")
                elif ours != up:
                    errors.append(f"{label}: upstream answers {up!r}, the make road {ours!r}")
                continue
            if arch == "armv7a":
                want = f"armv7a-nuttx-{abi}"
                if ours != want:
                    errors.append(f"{label}: the armv7a fallback answers {ours!r}, want {want!r}")
                continue
            if ours:
                errors.append(
                    f"{label}: upstream has no triple, but the make road answers {ours!r} "
                    "(the fallback is for armv7a alone)")
                continue
            r = run(rust_mk, triple_mk, configured, case)
            if r.returncode == 0 or "LLVM_ARCHTYPE" not in r.stderr:
                errors.append(
                    f"{label}: a configured tree with no triple must be refused naming "
                    f"LLVM_ARCHTYPE; make exited {r.returncode}: {r.stderr.strip()!r}")
    return errors


def self_test(rust_mk: Path) -> int:
    text = TRIPLE_MK.read_text()
    needle = "$(if $(filter armv7a,$(LLVM_ARCHTYPE)),armv7a-nuttx-$(LLVM_ABITYPE))"
    if needle not in text:
        print("check-nuttx-rust-triple --self-test: the fallback line moved; update the mutation",
              file=sys.stderr)
        return 1
    with tempfile.NamedTemporaryFile("w", suffix=".mk", delete=False) as f:
        f.write(text.replace(needle, "armv7a-nuttx-$(LLVM_ABITYPE)"))
        mutant = Path(f.name)
    try:
        errs = check(rust_mk, mutant)
    finally:
        mutant.unlink()
    if not errs:
        print("check-nuttx-rust-triple --self-test: a fallback answering for EVERY arch passed",
              file=sys.stderr)
        return 1
    return 0


def main() -> int:
    rust_mk = locate_rust_mk()
    if rust_mk is None or not rust_mk.is_file():
        print("check-nuttx-rust-triple: nuttx-apps is provisioned nowhere this build looks "
              "(nros setup --source nuttx-apps); upstream's table cannot be read")
        return 78
    if self_test(rust_mk) != 0:
        return 1
    errors = check(rust_mk, TRIPLE_MK)
    if errors:
        print("check-nuttx-rust-triple: the make road's triple disagrees with upstream's table:")
        for e in errors:
            print(f"  {e}")
        return 1
    print(f"check-nuttx-rust-triple: OK — {len(CASES)} cases against {rust_mk} "
          "(self-test: a fallback for every arch fails)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
