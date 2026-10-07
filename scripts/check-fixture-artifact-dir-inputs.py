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
sys.path.insert(0, str(ROOT / "scripts" / "lib"))
from check_just_sources import just_sources  # noqa: E402 — phase-472 W2
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

    Modelled on the real defect (it was the esp32 packer; ESP32 is dormant now,
    issue 1525, so the fixture uses a platform still on the shared list): a
    packer that reads a lane-built artifact and
    supplies the row's args and env itself. `_flag` is the exempting sibling, so
    the pair also pins the NARROWING — without it this gate flagged five
    self-consistent run-example recipes, which is a gate wider than its rule.
    """
    import tempfile

    bug = ('  artifact_dir="$(nros_fixture_row_artifact_dir '
           '"examples/mps2-an385-freertos/rust/$ex" freertos "" "")"\n')
    ok = ('  artifact_dir="$(nros_fixture_row_artifact_dir_by_id '
          '"freertos-rust-$ex" freertos)"\n')
    self_consistent_body = ('  flag="$(nros_fixture_target_dir_flag nuttx "" "")"\n'
                            '  d="$(nros_fixture_row_artifact_dir "$L" nuttx "" "")"\n')

    # phase-472 W8 — the exemption's NEIGHBOURS: the flag in ANOTHER recipe,
    # and the flag for ANOTHER platform, do not make this packer self-consistent.
    other_recipe = ('  flag="$(nros_fixture_target_dir_flag freertos "" "")"\n'
                    "other:\n" + bug)
    other_plat = ('  flag="$(nros_fixture_target_dir_flag nuttx "" "")"\n' + bug)
    dep_builder = ('  flag="$(nros_fixture_target_dir_flag freertos "" "")"\n'
                   "consumer: recipe\n" + bug)
    for name, body, want in (("the 1025 defect", bug, 1),
                             ("the by-id fix", ok, 0),
                             ("a recipe that builds it itself", self_consistent_body, 0),
                             ("the flag in a neighbouring recipe", other_recipe, 1),
                             ("the flag for another platform", other_plat, 1),
                             ("the flag in the builder this recipe DEPENDS on", dep_builder, 0)):
        with tempfile.TemporaryDirectory() as td:
            # A packer behind `mod check` + `import` — the population is the
            # justfile GRAPH (phase-472 W2), so the fixture exercises the reach.
            repo = Path(td)
            (repo / "just" / "check").mkdir(parents=True)
            (repo / "justfile").write_text("mod check 'just/check.just'\n")
            (repo / "just" / "check.just").write_text("import 'check/probe.just'\n")
            (repo / "just" / "check" / "probe.just").write_text("recipe:\n" + body)
            got = len(_scan(repo))
            if (got > 0) != (want > 0):
                sys.exit(f"check-fixture-artifact-dir-inputs SELFTEST FAILED: "
                         f"{name} -> {got} finding(s), expected {'>=1' if want else '0'}")


FLAG_CALL = re.compile(r"nros_fixture_target_dir_flag\s+([^)\n]*)")


def recipe_body(lines, line):
    """The text of the just recipe containing 1-based `line`: back to its
    column-0 header, forward to the next column-0 line."""
    start = line - 1
    while start > 0 and (not lines[start] or lines[start][:1].isspace()):
        start -= 1
    end = line
    while end < len(lines) and (not lines[end] or lines[end][:1].isspace()):
        end += 1
    return "\n".join(lines[start:end])


def dep_bodies(lines, body):
    """Bodies of the recipes this recipe's header DEPENDS on, in the same file —
    `test-x: build-x` is the builder running first, so its flag is this
    recipe's too. Parameters (`verbose=""`) and `_private` helpers alike."""
    header = body.split("\n", 1)[0]
    if ":" not in header:
        return []
    deps = [d for d in header.split(":", 1)[1].split() if "=" not in d and d]
    out = []
    for i, l in enumerate(lines):
        name = re.match(r"^@?([A-Za-z0-9_-]+)\b[^:\n]*:(?!=)", l)
        if name and name.group(1) in deps:
            out.append(recipe_body(lines, i + 1))
    return out


def self_consistent(body, plat, call):
    """Does `body` build this artifact with the same (platform, empties)?"""
    want = len(re.findall(r'""(?=\s|$)', call))
    for m in FLAG_CALL.finditer(body):
        args = " ".join(m.group(1).split())
        if plat in args.split() and len(re.findall(r'""(?=\s|$)', args)) == want:
            return True
    return False


def _scan(repo: Path):
    """The finder, over a repo's justfile graph — shared by main and the selftest.

    phase-472 W2: the graph (`check_just_sources.just_sources`), not
    `just/*.just` — the flat glob read neither the root `justfile` nor the 13
    files of `mod check`, so a packer there inventing a row's variant passed.
    """
    shared = shared_platforms()
    bad = []
    for path in (Path(p) for p in just_sources(str(repo))):
        text = path.read_text()
        lines = text.splitlines()
        for m in CALL.finditer(text):
            call = " ".join(m.group(1).split())
            words = call.split()
            plat = next((w for w in words if w in shared), None)
            if plat is None or not re.findall(r'""(?=\s|$)', call):
                continue
            line = text[: m.start()].count("\n") + 1
            # phase-472 W8 — the exemption is "this recipe BUILDS the artifact
            # itself, with the same (platform, args, env)", so it is keyed on
            # exactly that: a `nros_fixture_target_dir_flag` call in the SAME
            # recipe body, for the SAME platform, with the SAME empties. It was
            # "the flag name within 25 lines", which a neighbouring recipe's
            # build — for any platform — satisfied.
            body = recipe_body(lines, line)
            if any(self_consistent(b, plat, call) for b in [body] + dep_bodies(lines, body)):
                continue
            bad.append((path.relative_to(repo), line, plat,
                        len(re.findall(r'""(?=\s|$)', call)), call[:90]))
    return bad


def main() -> int:
    selftest()
    shared = shared_platforms()
    bad = _scan(ROOT)

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
