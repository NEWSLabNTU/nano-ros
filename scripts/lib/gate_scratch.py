"""A gate's scratch directory at a STABLE path, so sccache can hit (phase-489 W5).

Python twin of `scripts/lib/gate-scratch.sh`; the rationale is written there
and not repeated. `tempfile.TemporaryDirectory()` names a new directory every
run, and sccache's Rust key contains the cargo target dir's path, so a build
under one can never be answered by a restored cache.

The directory is EMPTY when handed out and removed on exit: only the NAME is
stable, never the contents.
"""

import contextlib
import os
import re
import shutil

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))


def scratch_path(name):
    if not re.fullmatch(r"[A-Za-z0-9_-][A-Za-z0-9._-]*", name):
        raise ValueError("gate_scratch: bad name %r" % (name,))
    base = os.environ.get("NROS_GATE_SCRATCH_ROOT") or os.path.join(ROOT, "tmp", "gate-scratch")
    return os.path.join(base, name)


@contextlib.contextmanager
def gate_scratch(name):
    path = scratch_path(name)
    shutil.rmtree(path, ignore_errors=True)
    os.makedirs(path)
    try:
        yield path
    finally:
        shutil.rmtree(path, ignore_errors=True)
