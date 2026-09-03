#!/usr/bin/env python3
"""Issue 1025 — a consumer must not invent a row's cargo args or env.

`nros_fixture_row_artifact_dir <leaf> <platform> <args> <env>` derives the
shared cargo group dir, and the key is a function of ALL THREE of platform,
cargo args and env. A packer that passes empty literals for the last two asks a
different question with the same function, and gets an answer that is wrong
exactly when the row has a variant.

That is not hypothetical: it stranded every ESP32 QEMU flash image the moment
`41a7d8de7` gave those rows an `env`, and nothing noticed because no CI lane
builds fixtures. The formula was already single (phase-340 item 7 made it so);
the INPUTS were still derived twice.

So on a SHARED platform, a call site must use `nros_fixture_row_artifact_dir_by_id
<row-id> <platform>`, which reads the row's args and env from the manifest.

An UNSHARED platform is exempt and stays that way on purpose: it resolves to the
leaf's own `target/` whatever the variant says, so the empty literals are
harmless there — and flagging them would be a gate wider than the rule it
enforces, which this repo has audited itself for before (issue 0196).
"""
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CALL = re.compile(r'nros_fixture_row_artifact_dir\s+(.+?)\)"', re.S)


def shared_platforms():
    """The list, read from the shell that owns it — never a second copy here."""
    src = (ROOT / "scripts/build/fixtures-target-dir.sh").read_text()
    m = re.search(r'NROS_FIXTURE_SHARED_PLATFORMS="\$\{NROS_FIXTURE_SHARED_PLATFORMS:-([^}]*)\}"', src)
    if not m:
        sys.exit("check-fixture-artifact-dir-inputs: cannot find NROS_FIXTURE_SHARED_PLATFORMS")
    return set(m.group(1).split())


def selftest() -> None:
    """The negative control, on the NORMAL path — a gate that cannot fail is a comment.

    Modelled on the real defect: a packer that reads a lane-built artifact and
    supplies the row's args and env itself. `_flag` is the exempting sibling, so
    the pair also pins the NARROWING — without it this gate flagged five
    self-consistent run-example recipes, which is a gate wider than its rule.
    """
    import tempfile

    bug = ('  artifact_dir="$(nros_fixture_row_artifact_dir '
           '"examples/qemu-esp32-baremetal/rust/$ex" qemu-esp32-baremetal "" "")"\n')
    ok = ('  artifact_dir="$(nros_fixture_row_artifact_dir_by_id '
          '"qemu-esp32-baremetal-$ex" qemu-esp32-baremetal)"\n')
    self_consistent = ('  flag="$(nros_fixture_target_dir_flag nuttx "" "")"\n'
                       '  d="$(nros_fixture_row_artifact_dir "$L" nuttx "" "")"\n')

    for name, body, want in (("the 1025 defect", bug, 1),
                             ("the by-id fix", ok, 0),
                             ("a recipe that builds it itself", self_consistent, 0)):
        with tempfile.TemporaryDirectory() as td:
            d = Path(td) / "just"
            d.mkdir()
            (d / "probe.just").write_text("recipe:\n" + body)
            got = len(_scan(d))
            if (got > 0) != (want > 0):
                sys.exit(f"check-fixture-artifact-dir-inputs SELFTEST FAILED: "
                         f"{name} -> {got} finding(s), expected {'>=1' if want else '0'}")


def _scan(just_dir: Path):
    """The finder, over an arbitrary directory — shared by main and the selftest."""
    shared = shared_platforms()
    bad = []
    for path in sorted(just_dir.glob("*.just")):
        text = path.read_text()
        lines = text.splitlines()
        for m in CALL.finditer(text):
            call = " ".join(m.group(1).split())
            words = call.split()
            plat = next((w for w in words if w in shared), None)
            if plat is None or not re.findall(r'""(?=\s|$)', call):
                continue
            line = text[: m.start()].count("\n") + 1
            window = "\n".join(lines[max(0, line - 25) : line + 5])
            if "nros_fixture_target_dir_flag" in window:
                continue
            bad.append((path.name, line, plat,
                        len(re.findall(r'""(?=\s|$)', call)), call[:90]))
    return bad


def main() -> int:
    selftest()
    shared = shared_platforms()
    bad = [(ROOT.joinpath("just", n).relative_to(ROOT), l, p_, e, c)
           for n, l, p_, e, c in _scan(ROOT / "just")]

    if not bad:
        print(f"check-fixture-artifact-dir-inputs: OK "
              f"({len(shared)} shared platform(s); every packer of a lane-built "
              f"artifact derives its row's args and env from the manifest)")
        return 0

    print("check-fixture-artifact-dir-inputs: a consumer is inventing a row's variant.\n")
    for path, line, plat, empties, call in bad:
        print(f"  {path}:{line}  platform={plat}  {empties} empty argument(s)")
        print(f"      {call}")
    print("""
  The group key is (platform, cargo args, env). Passing "" for args or env asks a
  DIFFERENT question with the same function, and the answer diverges the moment
  that row gains a variant — which is issue 1025, where every ESP32 QEMU flash
  image stopped being packable and no lane noticed.

  Fix: use `nros_fixture_row_artifact_dir_by_id <row-id> <platform>`, which reads
  the row's args and env from the manifest. The row id is the one the build loop
  above already passes to `fixtures-build.sh --id`.""")
    return 1


if __name__ == "__main__":
    sys.exit(main())
