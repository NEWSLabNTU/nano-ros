#!/usr/bin/env python3
"""phase-567 W4 -- a configured Zephyr build dir, cut down to what
`check-knob-delivery.py` reads, so the merge queue can run that gate.

The island-shaped workspace (examples/workspaces/island-shaped) states its
Zephyr image's pools above the derived counts in a Kconfig fragment, the shape
issue 2618 refused after link. The merge queue has no west workspace (user
ruling 2026-11-13: Zephyr builds run daily), so it cannot configure that image.
It checks this snapshot instead, and the daily Zephyr lane configures the image
live, checks the live build dir, and fails when the snapshot differs from it.

The snapshot holds only the lines `check-knob-delivery.py` parses:

  CMakeCache.txt                  NROS_RESOLVED_* and NROS_KNOB_SOURCE_*
  nros/entity_inventory.cmake     set(NROS_DERIVED_* ...)
  nros/message_bound_knobs.cmake  set(NROS_DERIVED_* ...)
  build.ninja                     the -D<define>=<n> of the C define knobs,
                                  one line per distinct value

with this checkout's path written as `<nano-ros>`, so the snapshot does not
depend on where it was taken.

Usage:
  knob-delivery-fixture.py extract <build-dir> <fixture-dir>
  knob-delivery-fixture.py diff    <build-dir> <fixture-dir>
  knob-delivery-fixture.py --self-test
"""
import importlib.util
import os
import re
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
REGEN = ("just check island-shaped-zephyr-regen   # needs a Zephyr 3.7 west "
         "workspace (just setup zephyr)")


def _knob_delivery():
    spec = importlib.util.spec_from_file_location(
        "check_knob_delivery", os.path.join(ROOT, "scripts", "check-knob-delivery.py"))
    assert spec is not None and spec.loader is not None
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def _header(name):
    return [
        "# phase-567 W4 -- snapshot of %s from a configured Zephyr build dir" % name,
        "# of examples/workspaces/island-shaped, image island_bringup:zephyr,",
        "# cut to the lines scripts/check-knob-delivery.py reads. GENERATED:",
        "# do not edit. Regenerate with:",
        "#   " + REGEN,
    ]


def _lines(path, pattern):
    if not os.path.exists(path):
        return None
    out = []
    with open(path, encoding="utf8", errors="ignore") as fh:
        for line in fh:
            line = line.rstrip("\n")
            if re.match(pattern, line):
                out.append(line)
    return sorted(set(out))


def snapshot(build_dir):
    """{relative path: text} of the cut-down build dir."""
    files = {}
    cache = _lines(os.path.join(build_dir, "CMakeCache.txt"),
                   r"^(NROS_RESOLVED_|NROS_KNOB_SOURCE_)[A-Z0-9_]+:[A-Z]+=")
    if not cache:
        raise SystemExit("knob-delivery-fixture: no NROS_RESOLVED_* in %s/CMakeCache.txt"
                         " -- not a configured nano-ros Zephyr build dir" % build_dir)
    files["CMakeCache.txt"] = cache
    for name in ("entity_inventory.cmake", "message_bound_knobs.cmake"):
        got = _lines(os.path.join(build_dir, "nros", name), r"^set\(NROS_DERIVED_")
        if got is not None:
            files["nros/" + name] = got
    defines = sorted(set(_knob_delivery().C_DEFINE_KNOBS.values()))
    ninja = os.path.join(build_dir, "build.ninja")
    seen = set()
    if os.path.exists(ninja):
        with open(ninja, encoding="utf8", errors="ignore") as fh:
            for m in re.finditer(r"-D([A-Z0-9_]+)=([0-9]*)", fh.read()):
                if m.group(1) in defines:
                    seen.add("-D%s=%s" % (m.group(1), m.group(2)))
    files["build.ninja"] = sorted(seen)
    # A path names the checkout it was configured in; the snapshot must not.
    root = os.path.realpath(ROOT)
    files = {name: sorted(set(line.replace(root, "<nano-ros>") for line in lines))
             for name, lines in files.items()}
    return {name: "\n".join(_header(name) + lines) + "\n" for name, lines in files.items()}


def read_fixture(fixture_dir):
    out = {}
    for base, _, names in os.walk(fixture_dir):  # walk-ok: a 4-file fixture dir, or a temp dir
        for n in names:
            p = os.path.join(base, n)
            with open(p, encoding="utf8") as fh:
                out[os.path.relpath(p, fixture_dir)] = fh.read()
    return out


def extract(build_dir, fixture_dir):
    snap = snapshot(build_dir)
    for name, text in snap.items():
        path = os.path.join(fixture_dir, name)
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "w", encoding="utf8") as fh:
            fh.write(text)
    for name in set(read_fixture(fixture_dir)) - set(snap):
        os.remove(os.path.join(fixture_dir, name))
    print("knob-delivery-fixture: wrote %d file(s) to %s" % (len(snap), fixture_dir))
    return 0


def diff(build_dir, fixture_dir):
    snap = snapshot(build_dir)
    have = read_fixture(fixture_dir)
    bad = sorted(n for n in set(snap) | set(have) if snap.get(n) != have.get(n))
    if not bad:
        print("knob-delivery-fixture: %s matches the live build dir %s"
              % (fixture_dir, build_dir))
        return 0
    print("knob-delivery-fixture: the committed snapshot %s is STALE against the"
          " live build dir %s:" % (fixture_dir, build_dir))
    for n in bad:
        print("  - %s: %s" % (n, "missing from the snapshot" if n not in have else
                              "not in the build dir" if n not in snap else "differs"))
        live = set(snap.get(n, "").splitlines())
        for line in sorted(live ^ set(have.get(n, "").splitlines())):
            print("      %-8s %s" % ("live" if line in live else "snapshot", line))
    print("\n  The merge queue checks the snapshot, so it must equal what a live"
          " configure writes.\n  Regenerate it and commit the result:\n    " + REGEN)
    return 1


def self_test():
    with tempfile.TemporaryDirectory() as tmp:
        build = os.path.join(tmp, "build")
        os.makedirs(os.path.join(build, "nros"))
        with open(os.path.join(build, "CMakeCache.txt"), "w") as fh:
            fh.write("CMAKE_X:STRING=1\nNROS_RESOLVED_NROS_ENTITY_MAX_PUBLISHERS:STRING=6\n"
                     "NROS_KNOB_SOURCE_NROS_ENTITY_MAX_PUBLISHERS:STRING=kconfig\n")
        with open(os.path.join(build, "nros", "entity_inventory.cmake"), "w") as fh:
            fh.write("# x\nset(NROS_DERIVED_MAX_PUBLISHERS 3)\nset(OTHER 1)\n")
        with open(os.path.join(build, "build.ninja"), "w") as fh:
            fh.write("FLAGS = -DZPICO_MAX_PUBLISHERS=6 -DUNRELATED=4\n"
                     "FLAGS = -DZPICO_MAX_PUBLISHERS=6\n")
        fix = os.path.join(tmp, "fixture")
        failures = 0
        extract(build, fix)
        snap = read_fixture(fix)
        if "set(OTHER" in snap["nros/entity_inventory.cmake"] or \
                "CMAKE_X" in snap["CMakeCache.txt"] or "UNRELATED" in snap["build.ninja"]:
            print("self-test: an unread line reached the snapshot")
            failures += 1
        if snap["build.ninja"].count("-DZPICO_MAX_PUBLISHERS=6") != 1:
            print("self-test: a define value is not kept exactly once")
            failures += 1
        if diff(build, fix) != 0:
            print("self-test: a fresh snapshot does not match its build dir")
            failures += 1
        kd = _knob_delivery()
        if kd.read_cache(fix) != kd.read_cache(build) or \
                kd.read_fragment(fix) != kd.read_fragment(build) or \
                kd.read_defines(fix) != kd.read_defines(build):
            print("self-test: check-knob-delivery reads the snapshot differently")
            failures += 1
        with open(os.path.join(build, "CMakeCache.txt"), "a") as fh:
            fh.write("NROS_RESOLVED_NROS_ENTITY_MAX_SUBSCRIBERS:STRING=6\n")
        if diff(build, fix) == 0:
            print("self-test: a stale snapshot was not reported")
            failures += 1
    print("knob-delivery-fixture self-test: %s" % ("FAILED (%d)" % failures if failures else "ok"))
    return 1 if failures else 0


def main(argv):
    if argv[1:] == ["--self-test"]:
        return self_test()
    if len(argv) == 4 and argv[1] in ("extract", "diff"):
        return (extract if argv[1] == "extract" else diff)(argv[2], argv[3])
    print("\n".join((__doc__ or "").strip().splitlines()[-4:]))
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv))
