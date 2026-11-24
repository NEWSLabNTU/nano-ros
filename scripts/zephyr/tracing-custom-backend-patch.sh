#!/usr/bin/env bash
# scripts/zephyr/tracing-custom-backend-patch.sh <workspace-dir>
#
# Issue 2878 -- an image selects a tracing backend it registers itself.
#
# `TRACING_BACKEND_DEFINE()` is public, so an application can register a
# backend outside the Zephyr tree, but stock Zephyr gives it no way to SELECT
# it: the backend is a Kconfig choice of the in-tree ones, and its name is
# fixed by that choice.
#
#   3.7  `subsys/tracing/tracing_core.c` names the backend with an `#elif`
#        chain over the in-tree `CONFIG_TRACING_BACKEND_*` symbols.
#   4.4  the chain is gone: `tracing_core.c` reads `CONFIG_TRACING_BACKEND_NAME`,
#        but that string has no prompt and only in-tree defaults, and the
#        choice still has no member an out-of-tree backend can take.
#
# The Autoware safety island carried this as
# `patches/zephyr/0003-tracing-allow-an-out-of-tree-backend.patch` (its
# overwrite-oldest RAM ring); this is that patch, now applied by nano-ros's own
# patch set for every consumer, with the same two symbols on both lines:
#
#   CONFIG_TRACING_BACKEND_CUSTOM=y              a member of the backend choice
#   CONFIG_TRACING_BACKEND_CUSTOM_NAME="<name>"  the first argument the image
#                                                gave TRACING_BACKEND_DEFINE()
#
# What it edits, in `subsys/tracing/`:
#   Kconfig         both symbols (both lines); on 4.4 also one `default` of
#                   TRACING_BACKEND_NAME
#   tracing_core.c  one `#elif` (3.7 only)
#
# Selecting it compiles no in-tree backend; the image's own backend is the only
# one. Idempotent (a tree carrying it is reported as applied). Inert unless an
# image selects CONFIG_TRACING_BACKEND_CUSTOM. Check mode (issue 2831): under
# NROS_PATCH_CHECK=1 it exits 1 naming the edit when the tree lacks it.
set -euo pipefail

# shellcheck source=scripts/lib/zephyr-patchset.sh
source "$(cd "$(dirname "${BASH_SOURCE[0]}")/../lib" && pwd)/zephyr-patchset.sh"

WORKSPACE="${1:?usage: tracing-custom-backend-patch.sh <workspace-dir>}"
TRACING="$WORKSPACE/zephyr/subsys/tracing"
for f in "$TRACING/Kconfig" "$TRACING/tracing_core.c"; do
    if [ ! -f "$f" ]; then
        echo "ERROR: $f missing" >&2
        exit 1
    fi
done

_py_rc=0
python3 - "$TRACING" <<'PYEOF' || _py_rc=$?
import os
import re
import sys

tracing = sys.argv[1]
MARK = "nano-ros: out-of-tree tracing backend"
kc_p, core_p = f"{tracing}/Kconfig", f"{tracing}/tracing_core.c"
kc, core = open(kc_p).read(), open(core_p).read()

# 4.4 names the backend in Kconfig; 3.7 in tracing_core.c's #elif chain.
kconfig_named = "\nconfig TRACING_BACKEND_NAME\n" in kc
carried = [MARK in kc] + ([] if kconfig_named else [MARK in core])
if all(carried):
    print("[tracing-custom-backend-patch] already applied")
    sys.exit(0)
if any(carried):
    sys.stderr.write(
        "ERROR: the out-of-tree tracing backend is in some of subsys/tracing's "
        "Kconfig and tracing_core.c but not all; restore both and re-run\n")
    sys.exit(1)


def die(msg):
    sys.stderr.write(f"ERROR: {msg}\n")
    sys.exit(1)


# The choice member goes last in the backend choice: before the `endchoice`
# that closes the block holding TRACING_BACKEND_POSIX (present on both lines).
m = re.search(r"\nconfig TRACING_BACKEND_POSIX\n", kc)
if not m:
    die(f"anchor not found in {kc_p}: config TRACING_BACKEND_POSIX")
end = kc.find("\nendchoice\n", m.end())
if end < 0:
    die(f"no endchoice after TRACING_BACKEND_POSIX in {kc_p}")
member = (
    "\n"
    f"# {MARK} (issue 2878).\n"
    "config TRACING_BACKEND_CUSTOM\n"
    "\tbool \"Out-of-tree backend\"\n"
    "\thelp\n"
    "\t  A backend the image registers itself with TRACING_BACKEND_DEFINE(),\n"
    "\t  named by TRACING_BACKEND_CUSTOM_NAME. No in-tree backend is built.\n"
)
name_sym = (
    "\n"
    f"# {MARK} (issue 2878).\n"
    "config TRACING_BACKEND_CUSTOM_NAME\n"
    "\tstring \"Name of the out-of-tree backend\"\n"
    "\tdepends on TRACING_BACKEND_CUSTOM\n"
    "\thelp\n"
    "\t  The first argument the image gave TRACING_BACKEND_DEFINE().\n"
)
after_end = end + len("\nendchoice\n")
kc = kc[:end] + member + kc[end:after_end] + name_sym + kc[after_end:]

if kconfig_named:
    # 4.4: TRACING_BACKEND_NAME is a promptless string with one default per
    # in-tree member; add the custom one after the last of them.
    m = re.search(r"\nconfig TRACING_BACKEND_NAME\n\tstring\n((?:\tdefault [^\n]*\n)+)", kc)
    if not m:
        die(f"TRACING_BACKEND_NAME is not a string with defaults in {kc_p}")
    kc = (kc[:m.end()]
          + f"\t# {MARK} (issue 2878).\n"
          + "\tdefault TRACING_BACKEND_CUSTOM_NAME if TRACING_BACKEND_CUSTOM\n"
          + kc[m.end():])
else:
    anchor = "#else\n#define TRACING_BACKEND_NAME \"\"\n"
    if core.count(anchor) != 1:
        die(f"anchor not found once in {core_p}: the #else of the backend chain")
    core = core.replace(
        anchor,
        f"/* {MARK} (issue 2878). */\n"
        "#elif defined CONFIG_TRACING_BACKEND_CUSTOM\n"
        "#define TRACING_BACKEND_NAME CONFIG_TRACING_BACKEND_CUSTOM_NAME\n"
        + anchor,
        1,
    )

if os.environ.get("NROS_PATCH_CHECK") == "1":
    sys.exit(10)
open(kc_p, "w").write(kc)
if not kconfig_named:
    open(core_p, "w").write(core)
line = "Kconfig name" if kconfig_named else "tracing_core.c #elif"
print(f"[tracing-custom-backend-patch] patched {tracing} (TRACING_BACKEND_CUSTOM, {line})")
PYEOF
if [ "$_py_rc" -eq 10 ]; then
    nros_patch_would_apply tracing-custom-backend-patch "$TRACING: TRACING_BACKEND_CUSTOM (Kconfig, tracing_core.c)"
fi
exit "$_py_rc"
