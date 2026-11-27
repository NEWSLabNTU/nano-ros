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
  inputs.txt                      issue 2957: the tracked files this checkout
                                  holds that the configure READ -- the inputs
                                  of build.ninja's RERUN_CMAKE edge, i.e.
                                  every CMake/Kconfig file whose edit re-runs
                                  the configure. `check island-snapshot-inputs`
                                  asks a branch that edits one of them for a
                                  regenerated snapshot or a live check. Derived
                                  from the build, so it cannot fall behind the
                                  resolver the way an authored list would.

with this checkout's path written as `<nano-ros>`, so the snapshot does not
depend on where it was taken.

Usage:
  knob-delivery-fixture.py extract <build-dir> <fixture-dir>
  knob-delivery-fixture.py diff    <build-dir> <fixture-dir>
  knob-delivery-fixture.py diff-derived <build-dir> <fixture-dir>
  knob-delivery-fixture.py digest  [<rev>]
  knob-delivery-fixture.py --self-test

`diff-derived` compares only the NROS_DERIVED_* files (the census half), so a
lane with no west workspace can check them against any configured nano-ros
build dir of the same workspace (issue 2957). `digest` prints the hash of the
snapshot's inputs at <rev> (default: the working tree), which the live check
prints as an `Island-Snapshot: checked <digest>` trailer.
"""
import hashlib
import importlib.util
import os
import re
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
REGEN = ("just check island-shaped-zephyr-regen   # needs a Zephyr 3.7 west "
         "workspace (just setup zephyr)")
FIXTURE = "tests/fixtures/knob-delivery/island-shaped-zephyr"
INPUTS = "inputs.txt"
DERIVED = ("nros/entity_inventory.cmake", "nros/message_bound_knobs.cmake")
TRAILER = "Island-Snapshot"


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
        ("# the tracked files on its configure's RERUN_CMAKE edge (issue 2957). GENERATED:"
         if name == INPUTS else
         "# cut to the lines scripts/check-knob-delivery.py reads. GENERATED:"),
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


def _derived_lines(build_dir):
    """{nros/<file>: NROS_DERIVED_* lines} of the files the build dir has."""
    out = {}
    for name in DERIVED:
        got = _lines(os.path.join(build_dir, name), r"^set\(NROS_DERIVED_")
        if got is not None:
            out[name] = got
    return out


def snapshot(build_dir):
    """{relative path: text} of the cut-down build dir."""
    files = {}
    cache = _lines(os.path.join(build_dir, "CMakeCache.txt"),
                   r"^(NROS_RESOLVED_|NROS_KNOB_SOURCE_)[A-Z0-9_]+:[A-Z]+=")
    if not cache:
        raise SystemExit("knob-delivery-fixture: no NROS_RESOLVED_* in %s/CMakeCache.txt"
                         " -- not a configured nano-ros Zephyr build dir" % build_dir)
    files["CMakeCache.txt"] = cache
    files.update(_derived_lines(build_dir))
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
    inputs = configure_inputs(build_dir, root)
    if inputs is not None:
        files[INPUTS] = inputs
    return {name: "\n".join(_header(name) + lines) + "\n" for name, lines in files.items()}


def _tracked(root, paths):
    """The subset of `paths` (checkout-relative) git tracks in `root`."""
    if not paths:
        return set()
    p = subprocess.run(["git", "-C", root, "ls-files", "-z", "--"] + sorted(paths),
                       capture_output=True, text=True)
    if p.returncode != 0:
        raise SystemExit("knob-delivery-fixture: git ls-files failed in %s: %s"
                         % (root, p.stderr.strip()))
    return set(x for x in p.stdout.split("\0") if x)


def configure_inputs(build_dir, root):
    """Checkout-relative tracked files on build.ninja's RERUN_CMAKE edge, or
    None when the build dir has no such edge (not a CMake/ninja build dir).

    Build output (a generated file, the CLI binary) is not tracked and so is
    dropped: only a file a branch can edit belongs on the list."""
    ninja = os.path.join(build_dir, "build.ninja")
    if not os.path.exists(ninja):
        return None
    with open(ninja, encoding="utf8", errors="ignore") as fh:
        text = fh.read().replace("$\n", " ")
    found = set()
    for m in re.finditer(r"^build [^\n]*?: RERUN_CMAKE ([^\n]*)", text, re.M):
        for tok in m.group(1).replace("|", " ").split():
            path = os.path.normpath(tok.replace("$ ", " "))
            if path.startswith(root + os.sep):
                found.add(os.path.relpath(path, root))
    if not found:
        return None
    return sorted(_tracked(root, found))


def fixture_inputs(text):
    """The paths an inputs.txt text lists (comment lines dropped)."""
    return [l.strip() for l in text.splitlines() if l.strip() and not l.startswith("#")]


def digest(root, paths, rev=None):
    """sha256 over `path <blob id>` for each listed path, at `rev` (a commit)
    or, with rev None, in the working tree. A path absent there hashes as
    `path -`, so deleting an input moves the digest too."""
    blobs = {}
    if rev is None:
        for path in paths:
            full = os.path.join(root, path)
            if os.path.isfile(full):
                p = subprocess.run(["git", "-C", root, "hash-object", "--", path],
                                   capture_output=True, text=True, check=True)
                blobs[path] = p.stdout.strip()
    else:
        p = subprocess.run(["git", "-C", root, "ls-tree", "-r", "-z", rev, "--"] + list(paths),
                           capture_output=True, text=True, check=True)
        for entry in p.stdout.split("\0"):
            if entry:
                meta, path = entry.split("\t", 1)
                blobs[path] = meta.split()[2]
    h = hashlib.sha256()
    for path in sorted(paths):
        h.update(("%s %s\n" % (path, blobs.get(path, "-"))).encode())
    return h.hexdigest()[:16]


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


def diff_derived(build_dir, fixture_dir):
    """issue 2957 -- the census half alone. The NROS_DERIVED_* counts come from
    the workspace's census, which a lane with no west workspace builds too, so
    any configured nano-ros build dir of this workspace states them."""
    snap = {name: "\n".join(_header(name) + lines) + "\n"
            for name, lines in _derived_lines(build_dir).items()}
    if not snap:
        raise SystemExit("knob-delivery-fixture: no %s under %s -- not a configured"
                         " nano-ros build dir" % (" or ".join(DERIVED), build_dir))
    have = read_fixture(fixture_dir)
    bad = [n for n in DERIVED if snap.get(n) != have.get(n)]
    if not bad:
        print("knob-delivery-fixture: the derived counts in %s match %s" % (fixture_dir, build_dir))
        return 0
    print("knob-delivery-fixture: the derived counts in the committed snapshot %s"
          " differ from the census of this tree (%s):" % (fixture_dir, build_dir))
    for n in bad:
        live = set(snap.get(n, "").splitlines())
        print("  - %s" % n)
        for line in sorted(live ^ set(have.get(n, "").splitlines())):
            print("      %-8s %s" % ("census" if line in live else "snapshot", line))
    print("\n  The Zephyr image's configure takes these counts from the same census,"
          "\n  so its snapshot is stale too. Regenerate it and commit the result:\n    " + REGEN)
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
        census = os.path.join(tmp, "census")
        os.makedirs(os.path.join(census, "nros"))
        with open(os.path.join(census, "nros", "entity_inventory.cmake"), "w") as fh:
            fh.write("set(NROS_DERIVED_MAX_PUBLISHERS 3)\n")
        if diff_derived(census, fix) != 0:
            print("self-test: a census with the snapshot's counts was reported stale")
            failures += 1
        with open(os.path.join(census, "nros", "entity_inventory.cmake"), "w") as fh:
            fh.write("set(NROS_DERIVED_MAX_PUBLISHERS 4)\n")
        if diff_derived(census, fix) == 0:
            print("self-test: a moved derived count was not reported")
            failures += 1
        with open(os.path.join(build, "CMakeCache.txt"), "a") as fh:
            fh.write("NROS_RESOLVED_NROS_ENTITY_MAX_SUBSCRIBERS:STRING=6\n")
        if diff(build, fix) == 0:
            print("self-test: a stale snapshot was not reported")
            failures += 1
        failures += _self_test_inputs(tmp)
    print("knob-delivery-fixture self-test: %s" % ("FAILED (%d)" % failures if failures else "ok"))
    return 1 if failures else 0


def _self_test_inputs(tmp):
    """inputs.txt keeps the tracked inputs of the RERUN_CMAKE edge only, and
    the digest moves with an input's content (issue 2957)."""
    failures = 0
    repo = os.path.realpath(os.path.join(tmp, "repo"))
    os.makedirs(os.path.join(repo, "cmake"))
    env = dict(os.environ)
    sys.path.insert(0, os.path.join(ROOT, "scripts", "lib"))
    from git_hook_env import nros_clear_inherited_git_env  # noqa: E402
    nros_clear_inherited_git_env(env)

    def git(*args):
        subprocess.run(["git", "-C", repo] + list(args), check=True, capture_output=True, env=env)
    for name, text in (("cmake/A.cmake", "a\n"), ("Kconfig", "k\n")):
        with open(os.path.join(repo, name), "w") as fh:
            fh.write(text)
    git("init", "-q")
    git("add", "cmake/A.cmake", "Kconfig")
    git("-c", "user.name=t", "-c", "user.email=t@t", "commit", "-qm", "x")
    build = os.path.join(repo, "build")
    os.makedirs(build)
    with open(os.path.join(repo, "untracked.cmake"), "w") as fh:
        fh.write("u\n")
    with open(os.path.join(build, "build.ninja"), "w") as fh:
        fh.write("build build.ninja: RERUN_CMAKE | %s/cmake/A.cmake $\n    %s/Kconfig "
                 "%s/untracked.cmake /usr/share/cmake/x.cmake\nbuild other: CC %s/cmake/B.c\n"
                 % (repo, repo, repo, repo))
    old = os.environ.copy()
    os.environ.clear()
    os.environ.update(env)
    try:
        got = configure_inputs(build, repo)
        if got != ["Kconfig", "cmake/A.cmake"]:
            print("self-test: configure_inputs read %r, want the two tracked edge inputs" % got)
            failures += 1
        before = digest(repo, got)
        if before != digest(repo, got, "HEAD"):
            print("self-test: the working-tree and HEAD digests of a clean tree differ")
            failures += 1
        with open(os.path.join(repo, "Kconfig"), "w") as fh:
            fh.write("k2\n")
        if digest(repo, got) == before:
            print("self-test: an edited input did not move the digest")
            failures += 1
    finally:
        os.environ.clear()
        os.environ.update(old)
    return failures


def main(argv):
    if argv[1:] == ["--self-test"]:
        return self_test()
    if len(argv) == 4 and argv[1] in ("extract", "diff", "diff-derived"):
        return {"extract": extract, "diff": diff, "diff-derived": diff_derived}[argv[1]](
            argv[2], argv[3])
    if argv[1:2] == ["digest"] and len(argv) <= 3:
        with open(os.path.join(ROOT, FIXTURE, INPUTS), encoding="utf8") as fh:
            paths = fixture_inputs(fh.read())
        print(digest(ROOT, paths, argv[2] if len(argv) == 3 else None))
        return 0
    print("\n".join((__doc__ or "").strip().splitlines()[-14:]))
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv))
