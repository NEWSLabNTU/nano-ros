#!/usr/bin/env python3
"""check-gated-store-not-uploaded: no workflow step may cache or upload a
licence-gated SDK (phase-575 W3).

A `[gated.*]` package in nros-sdk-index.toml (TI Arm Clang the first) is
installed on a self-hosted runner under its tool store, `$NROS_HOME/gated/`,
by an operator who accepted its licence for that host. The licence does not
allow redistribution, and this repository's caches and artifacts are public.
So no `actions/cache` (save or restore) and no `actions/upload-artifact` step
may name a path that contains the gated store:

  - a path naming `gated` at all;
  - the tool store or anything above it: `.nros`, `$NROS_HOME` /
    `env.NROS_HOME`, a bare `~` / `$HOME` / `/`, or the runner's whole
    home or work tree (`runner.workspace`, `github.workspace`).

A path this gate cannot resolve (an `${{ env.X }}` set by a step) is judged
by its NAME: one naming `NROS_HOME`, `GATED` or `TI_ARM_CLANG` is refused,
any other is the step's own scratch and passes.

Hosted jobs also cache store prefixes through `scripts/ci/sdk-store-cache.sh
paths <kind>`, whose output no workflow text shows; this gate runs it for
every kind and refuses a path under `<store>/gated/` or the store root itself.

Selftest on the normal path, every run.
"""
import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
WORKFLOWS = os.path.join(ROOT, ".github", "workflows")

ACTION = re.compile(r"uses:\s*['\"]?actions/(cache(?:/save|/restore)?|upload-artifact)@")

BANNED = [
    (re.compile(r"(?i)gated"), "names the gated store"),
    (re.compile(r"(^|[/\s~])\.nros(/|$|\s)"), "contains the tool store (.nros), and with it gated/"),
    (re.compile(r"NROS_HOME"), "is the tool store root, which holds gated/"),
    (re.compile(r"TI_ARM_CLANG"), "names a gated install"),
    (re.compile(r"^\s*(~|\$HOME|\$\{\{\s*env\.HOME\s*\}\}|/)\s*/?\s*$"), "is a whole home or root, which holds the tool store"),
    (re.compile(r"(runner\.workspace|github\.workspace)\s*\}\}\s*/?\s*$"), "is a whole workspace tree"),
]


def step_paths(text):
    """(line, path) for every `path:` entry of a cache / upload step."""
    lines = text.splitlines()
    out = []
    i = 0
    while i < len(lines):
        m = ACTION.search(lines[i])
        if not m:
            i += 1
            continue
        # The step is the lines after `uses:` indented deeper than the step's
        # `- ` marker; its `with:` block holds `path:`.
        step_indent = len(lines[i]) - len(lines[i].lstrip(" -"))
        j = i + 1
        while j < len(lines):
            ln = lines[j]
            stripped = ln.strip()
            if stripped and (len(ln) - len(ln.lstrip(" "))) < step_indent:
                break
            if stripped.startswith("- ") and (len(ln) - len(ln.lstrip(" "))) <= step_indent - 2:
                break
            pm = re.match(r"\s*path:\s*(.*)$", ln)
            if pm:
                val = pm.group(1).strip()
                if val in ("|", ">", "|-", ">-", ""):
                    base = len(ln) - len(ln.lstrip(" "))
                    k = j + 1
                    while k < len(lines) and (not lines[k].strip() or
                                              len(lines[k]) - len(lines[k].lstrip(" ")) > base):
                        if lines[k].strip():
                            out.append((k + 1, lines[k].strip()))
                        k += 1
                    j = k
                    continue
                out.append((j + 1, val.strip("'\"")))
            j += 1
        i = j
    return out


CACHE_SCRIPT = os.path.join(ROOT, "scripts", "ci", "sdk-store-cache.sh")
KIND = re.compile(r"^\s*([a-z0-9-]+)\)\s*tools=", re.M)


def store_cache_offenders(store):
    """Paths `sdk-store-cache.sh paths <kind>` would cache, judged against the
    gated store under `store`."""
    with open(CACHE_SCRIPT, encoding="utf8") as fh:
        kinds = KIND.findall(fh.read())
    if not kinds:
        return ["scripts/ci/sdk-store-cache.sh: parsed no cache kinds; nothing was checked"]
    bad = []
    env = dict(os.environ, NROS_HOME=store)
    gated = os.path.join(store, "gated")
    for kind in kinds:
        r = subprocess.run(["bash", CACHE_SCRIPT, "paths", kind], capture_output=True,
                           text=True, env=env)
        if r.returncode != 0:
            bad.append(f"sdk-store-cache.sh paths {kind}: exit {r.returncode}: {r.stderr.strip()}")
            continue
        for path in r.stdout.split():
            norm = os.path.normpath(path)
            if norm == os.path.normpath(store) or norm == gated or norm.startswith(gated + os.sep) \
                    or gated.startswith(norm + os.sep):
                bad.append(f"sdk-store-cache.sh paths {kind}: `{path}` holds or is the gated store")
    return bad


def offenders(files):
    bad = []
    for name, text in files:
        for lineno, path in step_paths(text):
            for rx, why in BANNED:
                if rx.search(path):
                    bad.append(f"{name}:{lineno}: `{path}` {why}")
                    break
    return bad


GOOD = """
jobs:
  a:
    steps:
      - uses: actions/cache@v4
        with:
          path: |
            ~/.cargo/registry
            ~/.cargo/git
          key: k
      - uses: actions/upload-artifact@v4
        with:
          path: ${{ runner.temp }}/nros-run-reports/
"""


def selftest():
    fails = []

    def chk(what, cond):
        if not cond:
            fails.append(what)

    chk("the workflows' shapes pass", offenders([("t.yml", GOOD)]) == [])
    chk("a block path naming gated/ is refused",
        offenders([("t.yml", GOOD.replace("~/.cargo/git", "~/.nros/gated/ti-arm-clang"))]) != [])
    chk("the whole store is refused",
        offenders([("t.yml", GOOD.replace("~/.cargo/git", "~/.nros"))]) != [])
    chk("NROS_HOME is refused",
        offenders([("t.yml", GOOD.replace("${{ runner.temp }}/nros-run-reports/",
                                          "${{ env.NROS_HOME }}/sdk"))]) != [])
    chk("a bare home is refused",
        offenders([("t.yml", GOOD.replace("~/.cargo/git", "~"))]) != [])
    chk("a whole workspace is refused",
        offenders([("t.yml", GOOD.replace("${{ runner.temp }}/nros-run-reports/",
                                          "${{ github.workspace }}"))]) != [])
    chk("cache/save is read too",
        offenders([("t.yml", GOOD.replace("actions/cache@v4", "actions/cache/save@v4")
                    .replace("~/.cargo/git", "~/.nros/gated"))]) != [])
    chk("a TI root variable is refused",
        offenders([("t.yml", GOOD.replace("${{ runner.temp }}/nros-run-reports/",
                                          "${{ env.TI_ARM_CLANG_ROOT }}"))]) != [])
    return fails


def main():
    fails = selftest()
    if fails:
        for f in fails:
            print(f"check-gated-store-not-uploaded: SELFTEST FAILED: {f}", file=sys.stderr)
        return 1
    files = []
    for n in sorted(os.listdir(WORKFLOWS)):
        if n.endswith((".yml", ".yaml")):
            with open(os.path.join(WORKFLOWS, n), encoding="utf8") as fh:
                files.append((f".github/workflows/{n}", fh.read()))
    if not files:
        print("check-gated-store-not-uploaded: no workflows found; nothing was checked", file=sys.stderr)
        return 1
    steps = sum(len(step_paths(t)) for _, t in files)
    if steps == 0:
        print("check-gated-store-not-uploaded: parsed ZERO cache/upload paths; the parser is broken",
              file=sys.stderr)
        return 1
    bad = offenders(files) + store_cache_offenders("/nros-gated-check-store")
    if bad:
        print("check-gated-store-not-uploaded: a cache or upload step could publish a "
              "licence-gated SDK (nros-sdk-index.toml [gated.*]; phase-575 W3):", file=sys.stderr)
        for b in bad:
            print(f"  {b}", file=sys.stderr)
        print("Upload the file you need, never the tool store or a tree that holds it.",
              file=sys.stderr)
        return 1
    print(f"check-gated-store-not-uploaded: OK ({steps} cache/upload path(s) in {len(files)} "
          f"workflow(s), and every sdk-store-cache.sh kind; selftest 8 cases)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
