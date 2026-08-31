#!/usr/bin/env python3
"""Run cargo, then place a build script's generated headers where cmake expects.

phase-400 W5.c — WHY THIS EXISTS

`nros-{c,cpp}`'s build scripts generate per-build config headers. They have
always been written to `$CARGO_TARGET_DIR/nros-{c,cpp}-generated/nros/*.h` — a
path INSIDE cargo's tree that cargo does not manage. That works only because
nothing cleans it, and it is what blocks sharing the target dir across images:
image B takes a cargo cache hit, the build script never re-runs, and the
directory the consumers were pointed at is never written.

`$OUT_DIR` is cargo's own answer. It is per-unit, hashed BY CARGO (so two
feature sets or two knob sets cannot collide without anyone keying anything),
and its path is reported on the stable JSON stream:

    {"reason":"build-script-executed", "package_id": …, "out_dir": …}

Crucially cargo emits that event even when nothing rebuilds — measured at 13
events on a fully cached run. That is the property the side channel lacks and
the reason this approach removes the blocker instead of routing around it.

cmake still needs the header at a path it can name at CONFIGURE time, so this
script bridges the two: cargo chooses the source, cmake chooses the destination,
and the copy is part of the build step that produced it.

WHY IT WRAPS CARGO RATHER THAN PIPING

`add_custom_target(COMMAND …)` has no shell, so there is no pipe to put a filter
on. Wrapping keeps one process tree, one exit code, and lets stderr through
untouched — `--message-format=json-render-diagnostics` renders diagnostics to
stderr as usual and leaves stdout pure JSON for us, so nothing a developer reads
is swallowed.

Run: cargo-out-dir-headers.py --package nros-c --dest <dir> -- cargo build …
     cargo-out-dir-headers.py --self-test
"""

import argparse
import json
import os
import shutil
import subprocess
import sys


def package_matches(package_id: str, name: str) -> bool:
    """True when a cargo `package_id` names `name`.

    Cargo spells these at least two ways and has changed the spelling before:

        path+file:///…/packages/api/nros-c#0.5.0
        registry+https://…#heapless@0.8.0

    So match the NAME rather than parse a format: either the `#name@version`
    tail, or the last path segment before `#`. Guessing one spelling is how this
    silently stops finding the package after a cargo upgrade — and a miss here is
    a missing header, which surfaces as a compile error far away.
    """
    if "#" not in package_id:
        return package_id == name
    head, tail = package_id.rsplit("#", 1)
    if "@" in tail:
        return tail.rsplit("@", 1)[0] == name
    return head.rstrip("/").rsplit("/", 1)[-1] == name


def copy_headers(out_dir: str, dest: str) -> list:
    """Copy `<out_dir>/nros-*-generated/**/*.h` under `<dest>/`, structure kept.

    The `nros-{c,cpp}-generated/` segment is preserved deliberately. `nros-cpp`'s
    build script emits BOTH its own header and the c-format companion, so
    flattening would put two different headers in one directory and let include
    ORDER decide which a TU sees — issue 0360's variant collision. Copying the
    tree verbatim lands each header in the directory its consumers already
    include, whichever package produced it.
    """
    pairs = []
    # walk-ok: `out_dir` is cargo's OUT_DIR — a build directory whose generated
    # headers are UNTRACKED by construction, so `git ls-files` cannot see them.
    # This is the case the gate's own text carves out ("scanning for UNTRACKED
    # artifacts is fine — scope it to a build dir"), and the scope here is one
    # crate's OUT_DIR, not `examples/` or `packages/`.
    for root, _dirs, files in os.walk(out_dir):
        rel_root = os.path.relpath(root, out_dir)
        head = rel_root.split(os.sep)[0]
        if not head.startswith("nros-") or not head.endswith("-generated"):
            continue
        for name in sorted(files):
            if name.endswith(".h"):
                pairs.append((os.path.join(root, name), os.path.join(rel_root, name)))
    copied = []
    for s, rel in sorted(pairs, key=lambda x: x[1]):
        d = os.path.join(dest, rel)
        os.makedirs(os.path.dirname(d), exist_ok=True)
        # Unchanged content must not restamp the mtime: these headers are
        # inputs to every consuming TU, and a fresh mtime on an identical file
        # rebuilds the world for nothing (the same reason `write_atomic` in
        # nros-build-helpers short-circuits).
        try:
            with open(s, "rb") as fs, open(d, "rb") as fd:
                if fs.read() == fd.read():
                    copied.append(rel)
                    continue
        except OSError:
            pass
        shutil.copyfile(s, d)
        copied.append(rel)
    return copied


def run(package: str, dest: str, cargo_cmd: list) -> int:
    proc = subprocess.Popen(cargo_cmd, stdout=subprocess.PIPE, text=True)
    out_dirs = []
    assert proc.stdout is not None
    for line in proc.stdout:
        line = line.strip()
        if not line or not line.startswith("{"):
            continue
        try:
            msg = json.loads(line)
        except ValueError:
            continue
        if msg.get("reason") != "build-script-executed":
            continue
        if package_matches(msg.get("package_id", ""), package):
            od = msg.get("out_dir")
            if od:
                out_dirs.append(od)
    rc = proc.wait()
    if rc != 0:
        return rc

    if not out_dirs:
        # Not fatal, and deliberately so: a package whose build script emits no
        # headers is a normal configuration (nros-cpp with no C API, a probe-only
        # build). Failing here would turn "nothing to copy" into a build break.
        # The BYPRODUCT declaration on the cmake side is what makes a genuinely
        # missing header fail, at the consumer that needs it.
        print(
            f"nros: no build-script-executed for '{package}' — no headers copied",
            file=sys.stderr,
        )
        return 0

    # LAST wins. Several units of one package can run their build script in a
    # single invocation (measured: nine `nros-c` build-script units in one
    # Zephyr tree, one per feature set). The one cmake wants is the one this
    # build asked for, which is the last cargo reports for the package.
    copied = copy_headers(out_dirs[-1], dest)
    if copied:
        print(
            f"nros: {package} headers -> {dest} ({', '.join(copied)})",
            file=sys.stderr,
        )
    return 0


def self_test() -> int:
    import tempfile

    fails = 0
    cases = [
        ("path+file:///x/packages/api/nros-c#0.5.0", "nros-c", True),
        ("path+file:///x/packages/api/nros-cpp#0.5.0", "nros-c", False),
        ("registry+https://github.com/rust-lang/crates.io-index#heapless@0.8.0", "heapless", True),
        ("registry+https://github.com/rust-lang/crates.io-index#heapless@0.8.0", "heap", False),
        # the bare spelling, in case cargo ever reports one
        ("nros-c", "nros-c", True),
    ]
    for pid, name, want in cases:
        got = package_matches(pid, name)
        if got != want:
            print(f"  FAIL package_matches({pid!r}, {name!r}) = {got}, want {want}", file=sys.stderr)
            fails += 1
    print(f"  ok   package_matches: {len(cases)} spelling(s)")

    with tempfile.TemporaryDirectory() as tmp:
        out = os.path.join(tmp, "out")
        os.makedirs(os.path.join(out, "nros-c-generated", "nros"))
        hdr = os.path.join(out, "nros-c-generated", "nros", "nros_config_generated.h")
        with open(hdr, "w") as f:
            f.write("#define A 1\n")
        dest = os.path.join(tmp, "dest")
        copied = copy_headers(out, dest)
        placed = os.path.join(dest, "nros-c-generated", "nros", "nros_config_generated.h")
        want = [os.path.join("nros-c-generated", "nros", "nros_config_generated.h")]
        if copied != want or not os.path.isfile(placed):
            print(f"  FAIL copy_headers did not place the header: {copied}", file=sys.stderr)
            fails += 1
        else:
            print("  ok   copy_headers preserves the nros-*-generated/ segment")

        # nros-cpp emits BOTH its own header and the c-format companion. They
        # must land in DIFFERENT directories; flattening is issue 0360.
        os.makedirs(os.path.join(out, "nros-cpp-generated", "nros"), exist_ok=True)
        with open(os.path.join(out, "nros-cpp-generated", "nros",
                               "nros_cpp_config_generated.h"), "w") as f:
            f.write("#define CPP 1\n")
        copy_headers(out, dest)
        c_h = os.path.join(dest, "nros-c-generated", "nros", "nros_config_generated.h")
        cpp_h = os.path.join(dest, "nros-cpp-generated", "nros", "nros_cpp_config_generated.h")
        if not (os.path.isfile(c_h) and os.path.isfile(cpp_h)):
            print("  FAIL the two packages' headers did not stay separate", file=sys.stderr)
            fails += 1
        else:
            print("  ok   c and cpp headers land in their own directories")

        # An UNCHANGED header must not be restamped: consumers rebuild on mtime.
        os.utime(placed, (1000000, 1000000))
        copy_headers(out, dest)
        if int(os.path.getmtime(placed)) != 1000000:
            print("  FAIL an unchanged header was rewritten (mtime moved)", file=sys.stderr)
            fails += 1
        else:
            print("  ok   an unchanged header keeps its mtime")

        # A CHANGED header must be copied.
        with open(hdr, "w") as f:
            f.write("#define A 2\n")
        copy_headers(out, dest)
        with open(placed) as f:
            if f.read() != "#define A 2\n":
                print("  FAIL a changed header was not copied", file=sys.stderr)
                fails += 1
            else:
                print("  ok   a changed header is copied")

        # No `nros/` under OUT_DIR is normal, not an error.
        if copy_headers(os.path.join(tmp, "nothing"), dest) != []:
            print("  FAIL a missing out_dir/nros should copy nothing", file=sys.stderr)
            fails += 1
        else:
            print("  ok   a build script that emits no headers is not an error")

    if fails:
        print(f"cargo-out-dir-headers --self-test: {fails} case(s) FAILED", file=sys.stderr)
        return 1
    print("cargo-out-dir-headers --self-test: OK (6 checks)")
    return 0


def main(argv) -> int:
    if "--self-test" in argv:
        return self_test()
    ap = argparse.ArgumentParser()
    ap.add_argument("--package", required=True)
    ap.add_argument("--dest", required=True)
    ap.add_argument("cargo", nargs=argparse.REMAINDER)
    args = ap.parse_args(argv)
    cargo_cmd = args.cargo[1:] if args.cargo and args.cargo[0] == "--" else args.cargo
    if not cargo_cmd:
        print("no cargo command given after `--`", file=sys.stderr)
        return 2
    return run(args.package, args.dest, cargo_cmd)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
