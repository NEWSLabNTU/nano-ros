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
    # Phase 218: in-tree CLI at `packages/cli/target/release/nros` is the
    # canonical path. `~/.nros/bin/nros` remains as a transitional fallback.
    nano_ros_root = os.environ.get("NANO_ROS_ROOT")
    in_tree = (os.path.join(nano_ros_root, "packages", "cli", "target",
                            "release", "nros") if nano_ros_root else None)
    return (os.environ.get("NROS_BIN")
            or shutil.which("nros")
            or (in_tree if in_tree and os.path.isfile(in_tree) else None)
            or os.path.expanduser("~/.nros/bin/nros"))

def _run_codegen():
    bringup = _bringup_name()
    if not bringup:
        sys.stderr.write("[nros] NROS_BRINGUP_NAME unset; skipping codegen\n")
        return None
    nros = _nros_bin()
    if not nros or not os.path.isfile(nros):
        sys.stderr.write("[nros] nros CLI not found; run `just setup-cli` + `source ./activate.sh` (Phase 218)\n")
        sys.exit(1)
    workspace = os.environ.get("NROS_WORKSPACE", env["PROJECT_DIR"])
    out_dir = os.path.join(env["PROJECT_BUILD_DIR"], env["PIOENV"], "nros-system")
    os.makedirs(out_dir, exist_ok=True)
    # Issue 1396 — three defects in one command line, none of which this script
    # could ever have reported:
    #
    #   * `--ahead-of-vendor` is a value_enum (`pio` | `px4`) and REQUIRES its
    #     value. Passed bare it swallowed the next token and clap rejected the
    #     whole invocation, so this hook has never once run to completion.
    #   * `--framework` is not a flag `codegen-system` defines. The bake reads
    #     the framework from the workspace, not from PIO.
    #   * `--target platformio` named an `[image.*]` / `[deploy.*]` block no
    #     bringup declares, and an unknown block is taken SILENTLY: the tier
    #     resolver answers with the host's table (issue 1312). The question a
    #     framework hook can actually answer is which image claims the
    #     application directory — `--for-entry`, the same spelling the ESP-IDF
    #     shim uses for the IDF project dir. An entry no image claims degrades
    #     to exactly these defaults, with a note printed by the CLI.
    cmd = [nros, "codegen-system", "--ahead-of-vendor", "pio",
           "--workspace", workspace, "--bringup", bringup,
           "--for-entry", env["PROJECT_DIR"],
           "--out", out_dir]
    sys.stderr.write("[nros] %s\n" % " ".join(cmd))
    # No `except` here. The swallow this replaced said "continuing — verb may
    # not yet exist", which stopped being true when Phase 212.E shipped
    # `codegen-system`; what it hid afterwards was this script's own broken
    # argv. A bake that does not run leaves the image with no baked config, so
    # failing the PIO build is the honest outcome.
    subprocess.check_call(cmd)
    return out_dir

_out = _run_codegen()
if _out:
    env.Append(CPPPATH=[os.path.join(_out, "include")])
    src_dir = os.path.join(_out, "src")
    if os.path.isdir(src_dir):
        env.Append(SRC_FILTER=["+<%s/*>" % src_dir])
