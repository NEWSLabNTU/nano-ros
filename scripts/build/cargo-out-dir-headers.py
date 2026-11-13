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

LINK SCRIPTS (phase-553 W4, RFC-0130 D2)

A C-rooted bare-metal image is linked by CMake, against a staticlib whose
startup crates (cortex-m-rt, the board, the PAC) publish their linker scripts
from their build scripts' `$OUT_DIR`s. Corrosion does not forward a
staticlib's `rustc-link-search`, so CMake cannot find them. `--link-scripts
<pkg>` copies the top-level `*.x` of each such package's `$OUT_DIR` into
`<dest>/link/`, one directory CMake names at configure time and hands the
linker with `-L`. Same stream, same reason as the headers.

STACK FLOORS (issue 2400 W3)

A placer is the one step that has both the archive and the header a consumer
pairs with it, so it is where the stack walk of a LIBRARY runs: with
`--stack-floor <image>` the cargo command is armed for the walk (bitcode,
`.su`), and after it `scripts/nros-stack-floor.py` walks the archive cargo
reported for `--stack-floor-package` and writes `NROS_STACK_FLOOR_<ROLE>` and
`NROS_STACK_FLOOR_<ROLE>_MEASURED` into that package's placed config header.
The consumer that sizes the threads (PX4's `nros_px4_add_module`) reads them
from there, so they can never describe another build's archive.

Run: cargo-out-dir-headers.py --package nros-c [--package …]
         [--link-scripts <pkg> …] --dest <dir> -- cargo build …
     cargo-out-dir-headers.py … --stack-floor <image> --stack-floor-role OPEN=<root>… \
         --stack-floor-rmw <rmw> -- cargo …
     cargo-out-dir-headers.py --self-test
"""

import argparse
import os
import shutil
import subprocess
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "lib"))
# phase-484 W6 (RFC-0103 D8) — the one reader of cargo's JSON stream.
from cargo_out_dir import (  # noqa: E402,F401
    artifact_files as _artifact_files, out_dirs as _out_dirs, package_matches,
    self_test as _parser_self_test)
from artifact_layout import (  # noqa: E402  issue 1852
    C_GENERATED_DIR, C_HEADER_SUBDIR, CPP_GENERATED_DIR, CPP_HEADER_SUBDIR, GEN_DIRS)


def with_defines(text: str, defines: str) -> str:
    """`text` with `defines` placed before its include guard's closing
    `#endif`, or appended when it has none."""
    if not defines:
        return text
    lines = text.splitlines(keepends=True)
    for i in range(len(lines) - 1, -1, -1):
        if lines[i].lstrip().startswith("#endif"):
            return "".join(lines[:i]) + defines + "\n" + "".join(lines[i:])
        if lines[i].strip():
            break
    return text + ("" if text.endswith("\n") else "\n") + defines


def copy_headers(out_dir: str, dest: str, extra: dict | None = None) -> list:
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
        if head not in GEN_DIRS:
            continue
        for name in sorted(files):
            if name.endswith(".h"):
                pairs.append((os.path.join(root, name), os.path.join(rel_root, name)))
    copied = []
    for s, rel in sorted(pairs, key=lambda x: x[1]):
        d = os.path.join(dest, rel)
        os.makedirs(os.path.dirname(d), exist_ok=True)
        with open(s, "rb") as fs:
            want = fs.read()
        # `extra`: lines this step adds to one header (the stack floors), part
        # of the content compared below, so an unchanged walk keeps the mtime.
        add = (extra or {}).get(os.path.basename(rel))
        if add:
            want = with_defines(want.decode(), add).encode()
        # Unchanged content must not restamp the mtime: these headers are
        # inputs to every consuming TU, and a fresh mtime on an identical file
        # rebuilds the world for nothing (the same reason `write_atomic` in
        # nros-build-helpers short-circuits).
        try:
            with open(d, "rb") as fd:
                if fd.read() == want:
                    copied.append(rel)
                    continue
        except OSError:
            pass
        with open(d, "wb") as fd:
            fd.write(want)
        copied.append(rel)
    return copied


LINK_SCRIPT_SUBDIR = "link"


def copy_if_changed(src: str, dst: str) -> None:
    """Copy `src` to `dst` unless `dst` already holds the same bytes.

    An unchanged file keeps its mtime, so nothing keyed on it rebuilds.
    """
    try:
        with open(src, "rb") as fs, open(dst, "rb") as fd:
            if fs.read() == fd.read():
                return
    except OSError:
        pass
    os.makedirs(os.path.dirname(dst), exist_ok=True)
    shutil.copyfile(src, dst)


def copy_link_scripts(out_dir: str, dest: str) -> list:
    """Copy the top-level `*.x` of `out_dir` into `<dest>/link/`.

    Top level only: `link_script!` and cortex-m-rt both write there, and a
    deeper `.x` is nobody's linker script.
    """
    copied = []
    try:
        names = sorted(os.listdir(out_dir))
    except OSError:
        return copied
    for name in names:
        src = os.path.join(out_dir, name)
        if name.endswith(".x") and os.path.isfile(src):
            copy_if_changed(src, os.path.join(dest, LINK_SCRIPT_SUBDIR, name))
            copied.append(name)
    return copied


def named_target(cargo_cmd: list) -> str | None:
    """The `--target` the cargo command names, or None."""
    for i, a in enumerate(cargo_cmd):
        if a == "--":
            break
        if a == "--target" and i + 1 < len(cargo_cmd):
            return cargo_cmd[i + 1]
        if a.startswith("--target="):
            return a.split("=", 1)[1]
    return None


def builds_std(cargo_cmd: list) -> bool:
    """True when the command builds the standard library itself
    (`-Zbuild-std`): then core and alloc are units of this build, walked from
    its own intermediates, and the toolchain's sysroot has no rlibs for the
    target (a tier-3 target such as `thumbv7em-nuttx-eabihf`)."""
    args = cargo_cmd[:cargo_cmd.index("--")] if "--" in cargo_cmd else cargo_cmd
    return any(a.startswith("-Zbuild-std") or a.startswith("build-std")
               for a in args)


def cargo_target(cargo_cmd: list) -> str:
    """The `--target` the cargo command names, else the host's triple."""
    named = named_target(cargo_cmd)
    if named:
        return named
    out = subprocess.run(["rustc", "-vV"], capture_output=True, text=True, check=True).stdout
    return next(ln.split(":", 1)[1].strip() for ln in out.splitlines() if ln.startswith("host:"))


def armed_env(env: dict, triple: str) -> dict:
    """`env` with what the walk reads added to the cargo command: target-scoped
    `-Csave-temps` (the bitcode, which the walk compiles for frames), and
    `-fstack-usage` for cc-rs's C parts, each JOINED with a value already set,
    and no `RUSTC_WRAPPER` (a cache hands back no bitcode). The archive's code
    is unchanged: neither flag changes codegen."""
    env = dict(env)
    t = triple.upper().replace("-", "_").replace(".", "_")
    rf = f"CARGO_TARGET_{t}_RUSTFLAGS"
    env[rf] = (env.get(rf, "") + " -Csave-temps").strip()
    cf = "CFLAGS_" + triple.replace("-", "_").replace(".", "_")
    env[cf] = (env.get(cf) or env.get(f"CFLAGS_{triple}") or env.get("TARGET_CFLAGS")
               or env.get("CFLAGS") or "")
    env[cf] = (env[cf] + " -fstack-usage -fcallgraph-info=su").strip()
    env["RUSTC_WRAPPER"] = ""
    return env


def walk(sf: argparse.Namespace, triple: str, lines: list, work: str) -> str:
    """Walk the archive cargo reported for `sf.package` and return the
    `#define` lines for its config header; exit on a refused walk."""
    archives = _artifact_files(lines, sf.package, ".a")
    if not archives:
        raise SystemExit(f"nros: --stack-floor: cargo built no archive for '{sf.package}' "
                         "(a staticlib), so there is nothing to walk")
    root = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", ".."))
    os.makedirs(work, exist_ok=True)
    msgs = os.path.join(work, "cargo-messages.json")
    with open(msgs, "w") as f:
        f.writelines(lines)
    cmd = [sys.executable, os.path.join(root, "scripts", "nros-stack-floor.py"),
           "--table", os.path.join(root, "config", "stack-floors.json"),
           "--target", triple, "--image", sf.image,
           "--cargo-messages", msgs, "--intermediates-from-build",
           *([] if sf.targeted else ["--untargeted"]),
           "--llc-cache", os.path.join(work, "llc"), "--emit-defines",
           *([] if sf.builds_std else ["--sysroot"])]
    if sf.rmw:
        cmd += ["--rmw", sf.rmw]
    for r in sf.roles:
        cmd += ["--role", r]
    cmd.append(archives[-1])
    p = subprocess.run(cmd, stdout=subprocess.PIPE, text=True)
    if p.returncode != 0:
        raise SystemExit(f"nros: the stack walk of '{sf.package}' failed (see above)")
    return p.stdout.strip()


def asks_for_stream(cargo_cmd: list) -> bool:
    """True when the command asks cargo for the JSON stream this script reads.

    Issue 1824 — under Corrosion this script IS the import's cargo
    (`INTERFACE_CORROSION_CARGO`), so it also receives commands that build
    nothing, such as the `cargo-clean_<crate>` target's `cargo clean`. The
    `--message-format` flag is added to the BUILD command only
    (`corrosion_set_cargo_flags`), so its absence means there is nothing to
    place and cargo runs untouched, stdout included.
    """
    return any(a.startswith("--message-format") for a in cargo_cmd)


def run(packages: list, dest: str, cargo_cmd: list, link_packages: list = (),
        sf: argparse.Namespace | None = None) -> int:
    """Run cargo and place the headers of every package in `packages`.

    Several, because one cargo build can hold more than one header writer:
    the nros-cpp staticlib links nros-c, whose OWN build script writes the C
    header the C++ image also mirrors (issue 1824). Each package's headers come
    from that package's `$OUT_DIR`; `copy_headers` keeps each under its own
    `nros-*-generated/` segment, so they cannot collide.
    """
    if not asks_for_stream(cargo_cmd):
        return subprocess.call(cargo_cmd)
    env = None
    triple = None
    if sf is not None:
        triple = cargo_target(cargo_cmd)
        sf.targeted = named_target(cargo_cmd) is not None
        sf.builds_std = builds_std(cargo_cmd)
        env = armed_env(dict(os.environ), triple)
    proc = subprocess.Popen(cargo_cmd, stdout=subprocess.PIPE, text=True, env=env)
    assert proc.stdout is not None
    lines = list(proc.stdout)
    rc = proc.wait()
    if rc != 0:
        return rc
    extra = None
    if sf is not None:
        extra = {sf.header: walk(sf, triple, lines, os.path.join(dest, "nros-stack-floor"))}

    for package in packages:
        out_dirs = _out_dirs(lines, package)
        if not out_dirs:
            # Not fatal, and deliberately so: a package whose build script emits
            # no headers is a normal configuration (nros-cpp with no C API, a
            # probe-only build). Failing here would turn "nothing to copy" into a
            # build break. The consumer that needs a header is what fails — the
            # cmake mirror refuses a placed header that is missing (issue 1824).
            print(
                f"nros: no build-script-executed for '{package}' — no headers copied",
                file=sys.stderr,
            )
            continue

        # LAST wins. Several units of one package can run their build script in
        # a single invocation (measured: nine `nros-c` build-script units in one
        # Zephyr tree, one per feature set). The one cmake wants is the one this
        # build asked for, which is the last cargo reports for the package.
        copied = copy_headers(out_dirs[-1], dest, extra if package == (sf and sf.package) else None)
        if copied:
            print(
                f"nros: {package} headers -> {dest} ({', '.join(copied)})",
                file=sys.stderr,
            )

    # Unlike a header, a missing link script IS fatal: the image's link names
    # it, and a link that cannot find `memory.x` fails far from the cause.
    missing = []
    for package in link_packages:
        out_dirs = _out_dirs(lines, package)
        copied = copy_link_scripts(out_dirs[-1], dest) if out_dirs else []
        if not copied:
            missing.append(package)
    if missing:
        print(
            f"nros: no linker script published by {', '.join(missing)}; "
            "the image cannot be linked",
            file=sys.stderr,
        )
        return 1
    return 0


def self_test() -> int:
    _parser_self_test()  # the shared parser's own negative controls
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

    # Issue 1824 — a command with no JSON stream (Corrosion's `cargo clean`)
    # passes through; the build command, which carries the flag, is read.
    stream_cases = [
        (["cargo", "clean", "-p", "nros-c"], False),
        (["cargo", "rustc", "--message-format=json-render-diagnostics", "--", "-Cx"], True),
        (["cargo", "build", "--message-format", "json"], True),
    ]
    for cmd, want in stream_cases:
        if asks_for_stream(cmd) != want:
            print(f"  FAIL asks_for_stream({cmd}) != {want}", file=sys.stderr)
            fails += 1
    print(f"  ok   asks_for_stream: {len(stream_cases)} command(s)")

    with tempfile.TemporaryDirectory() as tmp:
        out = os.path.join(tmp, "out")
        os.makedirs(os.path.join(out, C_GENERATED_DIR, C_HEADER_SUBDIR))
        hdr = os.path.join(out, C_GENERATED_DIR, C_HEADER_SUBDIR, "nros_config_generated.h")
        with open(hdr, "w") as f:
            f.write("#define A 1\n")
        dest = os.path.join(tmp, "dest")
        copied = copy_headers(out, dest)
        placed = os.path.join(dest, C_GENERATED_DIR, C_HEADER_SUBDIR, "nros_config_generated.h")
        want = [os.path.join(C_GENERATED_DIR, C_HEADER_SUBDIR, "nros_config_generated.h")]
        if copied != want or not os.path.isfile(placed):
            print(f"  FAIL copy_headers did not place the header: {copied}", file=sys.stderr)
            fails += 1
        else:
            print("  ok   copy_headers preserves the nros-*-generated/ segment")

        # nros-cpp emits BOTH its own header and the c-format companion. They
        # must land in DIFFERENT directories; flattening is issue 0360.
        os.makedirs(os.path.join(out, CPP_GENERATED_DIR, CPP_HEADER_SUBDIR), exist_ok=True)
        with open(os.path.join(out, CPP_GENERATED_DIR, CPP_HEADER_SUBDIR,
                               "nros_cpp_config_generated.h"), "w") as f:
            f.write("#define CPP 1\n")
        copy_headers(out, dest)
        c_h = os.path.join(dest, C_GENERATED_DIR, C_HEADER_SUBDIR, "nros_config_generated.h")
        cpp_h = os.path.join(dest, CPP_GENERATED_DIR, CPP_HEADER_SUBDIR, "nros_cpp_config_generated.h")
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

        # Link scripts: the top-level `*.x` only, into `<dest>/link/`.
        lout = os.path.join(tmp, "lout")
        os.makedirs(os.path.join(lout, "deeper"))
        for name in ("memory.x", "c-runtime.x", "deeper/other.x", "build.rs.d"):
            with open(os.path.join(lout, name), "w") as f:
                f.write(name)
        got = copy_link_scripts(lout, dest)
        if got != ["c-runtime.x", "memory.x"] or not os.path.isfile(
                os.path.join(dest, LINK_SCRIPT_SUBDIR, "memory.x")):
            print(f"  FAIL copy_link_scripts copied {got}", file=sys.stderr)
            fails += 1
        else:
            print("  ok   copy_link_scripts takes the top-level *.x into link/")
        if copy_link_scripts(os.path.join(tmp, "nothing"), dest) != []:
            print("  FAIL a missing out_dir should copy no link script", file=sys.stderr)
            fails += 1
        else:
            print("  ok   a missing out_dir copies no link script")

        # No `nros/` under OUT_DIR is normal, not an error.
        if copy_headers(os.path.join(tmp, "nothing"), dest) != []:
            print("  FAIL a missing out_dir/nros should copy nothing", file=sys.stderr)
            fails += 1
        else:
            print("  ok   a build script that emits no headers is not an error")

    # Issue 2400 W3 — the stack floors a placer adds to a header.
    hdr_text = "#ifndef G\n#define G\n#define A 1\n\n#endif /* G */\n"
    got = with_defines(hdr_text, "#define NROS_STACK_FLOOR_OPEN 8\n#define NROS_STACK_FLOOR_OPEN_MEASURED 0")
    if not got.endswith("#define NROS_STACK_FLOOR_OPEN_MEASURED 0\n#endif /* G */\n") \
            or "#define A 1" not in got:
        print(f"  FAIL with_defines did not place the floors inside the guard: {got!r}", file=sys.stderr)
        fails += 1
    elif with_defines("#define A 1\n", "#define B 2") != "#define A 1\n#define B 2":
        print("  FAIL with_defines did not append to a header with no guard", file=sys.stderr)
        fails += 1
    else:
        print("  ok   with_defines places the floors inside the include guard")
    with tempfile.TemporaryDirectory() as tmp:
        out = os.path.join(tmp, "out", CPP_GENERATED_DIR, CPP_HEADER_SUBDIR)
        os.makedirs(out)
        with open(os.path.join(out, "nros_cpp_config_generated.h"), "w") as f:
            f.write(hdr_text)
        dest = os.path.join(tmp, "dest")
        extra = {"nros_cpp_config_generated.h": "#define NROS_STACK_FLOOR_OPEN 8"}
        copy_headers(os.path.join(tmp, "out"), dest, extra)
        placed = os.path.join(dest, CPP_GENERATED_DIR, CPP_HEADER_SUBDIR, "nros_cpp_config_generated.h")
        os.utime(placed, (1000000, 1000000))
        copy_headers(os.path.join(tmp, "out"), dest, extra)
        with open(placed) as f:
            body = f.read()
        if "NROS_STACK_FLOOR_OPEN 8" not in body or int(os.path.getmtime(placed)) != 1000000:
            print("  FAIL a re-placed header with the same floors was rewritten, or lost them",
                  file=sys.stderr)
            fails += 1
        else:
            print("  ok   the same floors keep the placed header's mtime")
    env = armed_env({"CARGO_TARGET_THUMBV7EM_NUTTX_EABIHF_RUSTFLAGS": "-Ca", "CFLAGS": "-Os",
                     "RUSTC_WRAPPER": "sccache"}, "thumbv7em-nuttx-eabihf")
    if (env["CARGO_TARGET_THUMBV7EM_NUTTX_EABIHF_RUSTFLAGS"] != "-Ca -Csave-temps"
            or env["CFLAGS_thumbv7em_nuttx_eabihf"] != "-Os -fstack-usage -fcallgraph-info=su"
            or env["RUSTC_WRAPPER"] != ""):
        print(f"  FAIL armed_env: {env}", file=sys.stderr)
        fails += 1
    else:
        print("  ok   armed_env joins the build's own flags and drops the wrapper")
    if not builds_std(["cargo", "build", "-Zbuild-std=core,alloc"]) \
            or not builds_std(["cargo", "build", "-Z", "build-std=core"]) \
            or builds_std(["cargo", "rustc", "--", "-Zbuild-std"]):
        print("  FAIL builds_std misread a command", file=sys.stderr)
        fails += 1
    else:
        print("  ok   builds_std reads -Zbuild-std, both spellings, before `--` only")
    if cargo_target(["cargo", "rustc", "--target", "x", "--", "--target", "y"]) != "x" \
            or cargo_target(["cargo", "build", "--target=z"]) != "z":
        print("  FAIL cargo_target did not read the command's --target", file=sys.stderr)
        fails += 1
    else:
        print("  ok   cargo_target reads the command's own --target")

    if fails:
        print(f"cargo-out-dir-headers --self-test: {fails} case(s) FAILED", file=sys.stderr)
        return 1
    print("cargo-out-dir-headers --self-test: OK (14 checks)")
    return 0


def main(argv) -> int:
    if "--self-test" in argv:
        return self_test()
    ap = argparse.ArgumentParser()
    ap.add_argument("--package", required=True, action="append",
                    help="a package whose build script writes headers; repeatable")
    ap.add_argument("--link-scripts", action="append", default=[],
                    help="a package whose $OUT_DIR holds linker scripts; repeatable")
    ap.add_argument("--dest", required=True)
    ap.add_argument("--stack-floor", metavar="IMAGE",
                    help="walk the archive after cargo; IMAGE keys its allowance rows in "
                    "config/stack-floors.toml, and a preset IMAGE "
                    "(STACK_FLOOR_PRESETS) also names its roles and RMW")
    ap.add_argument("--stack-floor-role", action="append", default=[], metavar="ROLE=ROOT[,ROOT]",
                    help="a thread role and the function(s) it starts in; repeatable")
    ap.add_argument("--stack-floor-rmw", help="the RMW backend the walk's policy is for")
    ap.add_argument("--stack-floor-package",
                    help="the package whose archive is walked (default: the first --package)")
    ap.add_argument("--stack-floor-header",
                    help="the placed header the floors go into, by file name (default: "
                    "<package>'s own config header, e.g. nros_cpp_config_generated.h)")
    ap.add_argument("cargo", nargs=argparse.REMAINDER)
    args = ap.parse_args(argv)
    cargo_cmd = args.cargo[1:] if args.cargo and args.cargo[0] == "--" else args.cargo
    if not cargo_cmd:
        print("no cargo command given after `--`", file=sys.stderr)
        return 2
    sf = None
    if args.stack_floor:
        preset = STACK_FLOOR_PRESETS.get(args.stack_floor, {})
        roles = args.stack_floor_role or preset.get("roles", [])
        if not roles:
            print(f"--stack-floor {args.stack_floor} needs --stack-floor-role (it is not one of "
                  f"{', '.join(STACK_FLOOR_PRESETS)})", file=sys.stderr)
            return 2
        package = args.stack_floor_package or args.package[0]
        sf = argparse.Namespace(
            image=args.stack_floor, roles=roles,
            rmw=args.stack_floor_rmw or preset.get("rmw"),
            package=package,
            header=args.stack_floor_header or own_config_header(package))
    return run(args.package, args.dest, cargo_cmd, args.link_scripts, sf)


# Images whose walk every placer of them spells the same way: `--stack-floor
# <image>` alone takes the roles and the RMW from here, so the module's remedy
# text, the just recipes and the release staging cannot drift apart.
STACK_FLOOR_PRESETS = {
    # A PX4 uORB module's archive (issue 2400 W3). OPEN: the module's main task
    # runs `start`, which opens the session (`rclcpp::init_in` ->
    # `nros_cpp_init`) and borrows it for every other sched task
    # (`nros_cpp_executor_open_over_session`). SPIN: a PX4 work queue's
    # `Run()`, one `nros_cpp_spin_once` per pass.
    "px4-uorb": {
        "roles": ["OPEN=nros_cpp_init,nros_cpp_executor_open_over_session",
                  "SPIN=nros_cpp_spin_once"],
        "rmw": "uorb",
    },
}


def own_config_header(package: str) -> str:
    """A package's own config header: `nros-c` writes `nros_config_generated.h`
    and `nros-cpp` `nros_cpp_config_generated.h` (and the C companion)."""
    return {"nros-c": "nros_config_generated.h",
            "nros-cpp": "nros_cpp_config_generated.h"}.get(
        package, package.replace("-", "_") + "_config_generated.h")


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
