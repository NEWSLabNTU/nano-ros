#!/usr/bin/env python3
"""An example leaf must not overrule the platform its own build passes.

WHY THIS EXISTS (issue 0835). Six leaves --
`examples/qemu-riscv64-threadx/rust/*/CMakeLists.txt` -- each carried

    set(NANO_ROS_PLATFORM threadx)

on line 11. A plain `set()` creates a NORMAL variable, which SHADOWS the cache
entry that `-DNANO_ROS_PLATFORM=threadx_riscv64` wrote -- and that `-D` is what
`just/threadx-riscv64.just` passes to these very leaves, on both its Rust and
its C/C++ path. So each leaf reported one platform while its own
`CMakeCache.txt` recorded another, both written in the same second.

The visible cost was 2.8 GB of duplicate build output: the shared Corrosion
cargo directory keyed on the label, so `build/corrosion-cargo/threadx-riscv64/`
held FOUR groups where the configuration space has two --
`{threadx, threadx_riscv64} x {zenoh, cyclonedds}` -- split exactly by family,
six rust leaves in one and thirteen c/cpp leaves in the other.

The quieter cost is the one this gate is for. Two `stdc++` link decisions in the
root `CMakeLists.txt` were written as `NANO_ROS_PLATFORM STREQUAL "threadx"`,
which means they DEPENDED on the leaves misreporting. Making the leaves honest
would have stopped those matching -- silently, surfacing as a link error on a
platform nobody was editing. A label that is wrong is worse than a label that is
missing, because things start keying on it.

WHAT IT CHECKS. For every example leaf with a `[[fixture]]` row that carries a
`platform` coordinate:

  1. an unconditional `set(NANO_ROS_PLATFORM ...)` is an ERROR -- it shadows
     whatever the build passes, even when the two happen to agree today;
  2. the value it defaults to must MATCH the row's platform coordinate.

Guarded (`if(NOT DEFINED NANO_ROS_PLATFORM)`) is the accepted form: a copied-out
project still configures with no `-D`, and the repo's own build still wins.

Leaves with NO platform coordinate -- `examples/templates/*`, which are
compile-check rows -- are out of scope. For a template the platform IS the thing
the reader is being shown how to choose, and no build contradicts it.

Exit 0 when every leaf agrees with its own build, 1 otherwise.
"""

import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
MANIFEST = os.path.join(ROOT, "examples", "fixtures.toml")

SET_RE = re.compile(r"^([ \t]*)set\(\s*NANO_ROS_PLATFORM\s+([^)\s]+)\s*\)", re.M)
GUARD_RE = re.compile(r"if\s*\(\s*NOT\s+DEFINED\s+NANO_ROS_PLATFORM\s*\)", re.I)


def norm(p):
    """`threadx-riscv64` (manifest) and `threadx_riscv64` (cmake) are one name."""
    return p.strip().strip('"').replace("-", "_")


def rows(manifest_text):
    """{leaf dir -> platform coordinate} for rows that declare one."""
    out = {}
    for blk in re.split(r"\n(?=\[\[)", manifest_text):
        d = re.search(r'^\s*dir\s*=\s*"([^"]+)"', blk, re.M)
        p = re.search(r'^\s*platform\s*=\s*"([^"]+)"', blk, re.M)
        if d and p:
            out.setdefault(d.group(1), norm(p.group(1)))
    return out


def guarded(text, at):
    """Is the `set()` at offset `at` inside an `if(NOT DEFINED …)` guard?

    Deliberately crude: the guard has to be the nearest preceding `if(` on the
    same nesting level, and these files put it immediately above. A leaf that
    needs something cleverer should say so in a comment and the gate can grow a
    case; guessing at arbitrary CMake control flow with a regex would make this
    gate lie in the direction that matters.
    """
    before = text[:at]
    last_if = before.rfind("if(")
    last_endif = before.rfind("endif()")
    if last_if == -1 or last_if < last_endif:
        return False
    return bool(GUARD_RE.search(text[last_if:at]))


def scan(root, manifest_text):
    problems = []
    checked = 0
    for leaf, platform in sorted(rows(manifest_text).items()):
        path = os.path.join(root, leaf, "CMakeLists.txt")
        if not os.path.isfile(path):
            continue
        with open(path, encoding="utf8") as fh:
            text = fh.read()
        for m in SET_RE.finditer(text):
            checked += 1
            value = norm(m.group(2))
            line = text[:m.start()].count("\n") + 1
            if not guarded(text, m.start()):
                problems.append(
                    (leaf, line, f"unconditional `set(NANO_ROS_PLATFORM {m.group(2)})` "
                                 f"shadows the -D its own build passes")
                )
            elif value != platform:
                problems.append(
                    (leaf, line, f"defaults to `{m.group(2)}` but its fixture row's "
                                 f"platform is `{platform}`")
                )
    return problems, checked


def self_test(quiet=False):
    """Negative control: the rule must FIRE on the shape it exists for.

    Runs on the NORMAL path, not behind a flag -- `check-gate-selftests` holds
    this file to that, and a control nobody runs decays into a comment.
    """
    import tempfile

    manifest = (
        '[[fixture]]\ndir = "examples/leaf"\nplatform = "threadx-riscv64"\n'
    )
    cases = [
        # (leaf CMakeLists body, expected number of problems, what it proves)
        ("set(NANO_ROS_PLATFORM threadx)\n", 1,
         "the exact 0835 shape: unconditional, and the wrong value"),
        ("set(NANO_ROS_PLATFORM threadx_riscv64)\n", 1,
         "unconditional is a problem even when the value AGREES -- that is the "
         "state this gate exists to stop, because the next label-keyed thing "
         "silently depends on which one wins"),
        ("if(NOT DEFINED NANO_ROS_PLATFORM)\n"
         "    set(NANO_ROS_PLATFORM threadx)\nendif()\n", 1,
         "guarded but disagreeing with the row's coordinate"),
        ("if(NOT DEFINED NANO_ROS_PLATFORM)\n"
         "    set(NANO_ROS_PLATFORM threadx_riscv64)\nendif()\n", 0,
         "the accepted form: guarded, and matching"),
        ("# threadx_riscv64 is the platform here\n", 0,
         "prose naming a platform is not a set()"),
    ]
    with tempfile.TemporaryDirectory() as tmp:
        os.makedirs(os.path.join(tmp, "examples", "leaf"))
        for body, want, why in cases:
            with open(os.path.join(tmp, "examples", "leaf", "CMakeLists.txt"),
                      "w", encoding="utf8") as fh:
                fh.write(body)
            got, _ = scan(tmp, manifest)
            assert len(got) == want, (
                f"self-test: expected {want} problem(s), got {len(got)} — {why}\n"
                f"  body: {body!r}\n  got: {got}"
            )
    if not quiet:
        print("check-example-platform-not-shadowed self-test: OK "
              f"({len(cases)} case(s))")
    return 0


def main(argv):
    if "--self-test" in argv:
        return self_test()
    # Always, not only behind the flag. See `scripts/check-board-tiers.py`.
    rc = self_test(quiet=True)
    if rc:
        return rc

    with open(MANIFEST, encoding="utf8") as fh:
        manifest_text = fh.read()
    problems, checked = scan(ROOT, manifest_text)
    if problems:
        print("check-example-platform-not-shadowed: "
              f"{len(problems)} leaf/leaves overrule their own build:")
        for leaf, line, why in problems:
            print(f"  {leaf}/CMakeLists.txt:{line}")
            print(f"      {why}")
        print(
            "\n  A plain `set()` is a NORMAL variable and shadows the cache entry a\n"
            "  `-DNANO_ROS_PLATFORM=…` wrote, so the leaf and its own CMakeCache\n"
            "  disagree. Write it as the copy-out DEFAULT instead:\n"
            "\n"
            "      if(NOT DEFINED NANO_ROS_PLATFORM)\n"
            "          set(NANO_ROS_PLATFORM <the row's platform>)\n"
            "      endif()\n"
            "\n  Issue 0835 — this cost 2.8 GB of duplicate corrosion cargo groups,\n"
            "  and two `stdc++` link decisions had come to depend on the wrong label."
        )
        return 1
    print(f"check-example-platform-not-shadowed: OK ({checked} declaration(s) "
          f"across {len(rows(manifest_text))} leaf/leaves with a platform coordinate)")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
