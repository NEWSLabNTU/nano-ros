# Phase 212.H.6 — PlatformIO pre-build hook (HOOKLESS vendor path).
#
# Runs BEFORE PIO's library resolver so the baked tree from
# `nros codegen-system --ahead-of-vendor` is visible to the framework
# (zephyr / espidf / arduino) as ordinary sources + include dirs.
#
# Required env / build_flags:
#   build_flags = -DNROS_BRINGUP_NAME=<bringup-pkg-name>
# Optional env:
#   NROS_BIN          — explicit path to `nros` binary
#   NROS_WORKSPACE    — explicit path to workspace root (default = PROJECT_DIR)
import os
import shutil
import subprocess
import sys

Import("env")  # noqa: F821 — PlatformIO injects `env`

def _bringup_name():
    for f in env.get("BUILD_FLAGS", []):
        if isinstance(f, str) and "NROS_BRINGUP_NAME=" in f:
            return f.split("NROS_BRINGUP_NAME=", 1)[1].strip().strip('"')
    return os.environ.get("NROS_BRINGUP_NAME", "")

def _nros_bin():
    return (os.environ.get("NROS_BIN")
            or shutil.which("nros")
            or os.path.expanduser("~/.nros/bin/nros"))

def _framework():
    fws = env.get("PIOFRAMEWORK") or []
    return fws[0] if fws else "native"

def _run_codegen():
    bringup = _bringup_name()
    if not bringup:
        sys.stderr.write("[nros] NROS_BRINGUP_NAME unset; skipping codegen\n")
        return None
    nros = _nros_bin()
    if not nros or not os.path.isfile(nros):
        sys.stderr.write("[nros] nros CLI not found; run scripts/install-nros.sh\n")
        sys.exit(1)
    workspace = os.environ.get("NROS_WORKSPACE", env["PROJECT_DIR"])
    out_dir = os.path.join(env["PROJECT_BUILD_DIR"], env["PIOENV"], "nros-system")
    os.makedirs(out_dir, exist_ok=True)
    cmd = [nros, "codegen-system", "--ahead-of-vendor",
           "--workspace", workspace, "--bringup", bringup,
           "--target", "platformio", "--framework", _framework(),
           "--out", out_dir]
    sys.stderr.write("[nros] %s\n" % " ".join(cmd))
    try:
        subprocess.check_call(cmd)
    except (FileNotFoundError, subprocess.CalledProcessError) as e:
        sys.stderr.write("[nros] codegen-system failed: %s (continuing — verb may not yet exist)\n" % e)
        return None
    return out_dir

_out = _run_codegen()
if _out:
    env.Append(CPPPATH=[os.path.join(_out, "include")])
    src_dir = os.path.join(_out, "src")
    if os.path.isdir(src_dir):
        env.Append(SRC_FILTER=["+<%s/*>" % src_dir])
