#!/usr/bin/env python3
"""A consumer may not spell a fixture group KEY the manifest disagrees with.

Issue 1025.

WHAT BROKE, AND WHY THE EXISTING HELPER DID NOT STOP IT

The shared cargo target dir is keyed on a row's VARIANT: `nros_fixture_group_slug`
hashes (platform, cargo args, env), so a row with no variant lands in
`build/cargo-fixtures/<platform>` and a row with one lands in
`build/cargo-fixtures/<platform>-<cksum>`.

phase-340 item 7 already fixed one half of this. `just esp32 build-qemu` had
hand-written `examples/qemu-esp32-baremetal/rust/$ex/target/...` to find the ELF
it packs into a flash image, the platform migrated, and the path stopped
existing. The fix was `nros_fixture_row_artifact_dir` — one FORMULA, shared by
the build and the consumer.

The formula was single; the INPUTS were still derived twice. The packer called
the helper as::

    nros_fixture_row_artifact_dir "examples/…/$ex" qemu-esp32-baremetal "" ""

— platform from the call site, args and env spelled as two empty literals, while
the producer (`fixtures-build.sh`) passed the ROW's real ones. Two of three
constants supply a different answer. The two agreed for as long as the esp32
rows carried no variant, and diverged the moment commit 41a7d8de7 added
`env = { ZPICO_MAX_QUERYABLES = "2" }` to all three of them: the build wrote
`cargo-fixtures/qemu-esp32-baremetal-4118800323/` and the packer kept asking for
`cargo-fixtures/qemu-esp32-baremetal/`, so no ESP32 flash image could be
produced at all — for four days, across five tier-2 tests, with every gate green.

WHAT THIS GATE CHECKS, AND WHAT IT DELIBERATELY DOES NOT

Rule 1 — an UNPAIRED `nros_fixture_row_artifact_dir` call whose key is spelled in
literals must name a group some manifest row actually builds into.

"Unpaired" is the whole discrimination, and it is the mechanism of 1025 stated
as a predicate. A recipe that calls `nros_fixture_target_dir_flag` with the SAME
literal triple in the same body BUILDS what it then reads: its two derivations
are one derivation, it cannot look where it did not write, and the interactive
`_run-qemu` recipes on freertos/nuttx/threadx are exactly that shape. A call
with no such partner is reading an artifact SOMEBODY ELSE wrote — and that
somebody is the manifest-driven fixture build, whose key comes from the row. So
the consumer must either match the manifest or stop hand-spelling the key
(`nros_fixture_row_artifact_dir_by_id`, which reads all three fields from the
row).

Rule 2 — a literal `cargo-fixtures/<slug>` path must name a slug the manifest
produces. CLAUDE.md's phase-340 entry says "never a literal"; four such literals
survive in `just/qemu-baremetal.just` and `scripts/check-weak-symbols-image.sh`,
and they are correct today only because `qemu-arm-baremetal` and `freertos` have
one group each. The day either platform's rows gain an `env`, they become 1025
again, in a script whose failure is a missing symbol table.

NOT checked, and why:

* `nros_fixture_target_dir_flag` on its own. A build-side flag that disagrees
  with the manifest costs a duplicate compile, not a wrong read, and some are
  deliberate: `just native build-examples` builds the DEFAULT configuration of
  every leaf including five `examples/templates/**` crates with no row at all.
  A rule here would be red on correct code.
* A call whose args or env come from a shell variable. That call site is already
  deriving its key rather than asserting one; whether the derivation is right is
  a runtime question this gate cannot answer, and guessing would be worse than
  declining.
* Whether a self-consistent `_run-qemu` recipe builds into the group its
  manifest rows use. `just nuttx talker` does not (its rows carry
  `NROS_LOCATOR=…`, the recipe passes `"" ""`), so it compiles a third
  population of the same crates. That is a phase-340 P2 duplicate-build defect,
  not a broken read, and fixing it means also delivering the row's env to the
  build — a behaviour change on an interactive path this gate's author could not
  test. Left as it is, said out loud rather than quietly folded in.

Self-tests its own classifier on every run (phase-395): a synthetic broken call
site must be reported and its corrected form must not, so "this gate can fail"
is re-established on every invocation instead of asserted once in a commit.
"""

from __future__ import annotations

import importlib.util
import re
import subprocess
import sys
from fnmatch import fnmatch
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
MANIFEST_PY = ROOT / "scripts" / "build" / "fixtures-manifest.py"

READ_HELPER = "nros_fixture_row_artifact_dir"
BUILD_HELPER = "nros_fixture_target_dir_flag"
BY_ID_HELPER = "nros_fixture_row_artifact_dir_by_id"

# One shell word: a double-quoted run, a single-quoted run, or bare
# non-whitespace. Deliberately not `shlex`, which cannot be told to keep `$(`
# and `{{ }}` intact and would swallow the call's own closing `)"`.
TOKEN = re.compile(r'"([^"]*)"|\'([^\']*)\'|(\S+)')

# A slug spelled as a literal path component.
LITERAL_GROUP = re.compile(r"cargo-fixtures/([A-Za-z0-9_.-]+)")

# A shell/just word that is not a constant.
DYNAMIC = re.compile(r"\$|\{\{")


def _load_manifest_module():
    spec = importlib.util.spec_from_file_location("nros_fixtures_manifest", MANIFEST_PY)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def manifest_rows(fm):
    """`[(dir, platform, slug)]` for every buildable cargo `[[fixture]]` row.

    The slug is NOT computed here: `shell_group_batch` shells into
    `nros_fixture_group_slug`, which is the one derivation of the key
    (RFC-0070 R3). A gate that reimplemented the `cksum` would pass against its
    own copy of the rule, which is the failure `check-fixture-groups.py`'s header
    already records for this same key.
    """
    rows = [
        e
        for e in fm.load(fm.DEFAULT_MANIFEST)
        if not e.get("skip_build") and fm.is_cargo_row(e)
    ]
    derived = fm.shell_group_batch(
        (e.get("platform", ""), fm.cargo_args(e), fm.env_str(e)) for e in rows
    )
    return [
        ((e.get("dir") or "").rstrip("/"), e.get("platform", ""), slug)
        for e, (slug, _eligible) in zip(rows, derived)
    ]


def hand_slug(fm, platform: str, args: str, envstr: str) -> str:
    (slug, _eligible), = fm.shell_group_batch([(platform, args, envstr)])
    return slug


def logical_lines(text: str):
    """`(first_line_number, joined_text)` per backslash-continued logical line."""
    out = []
    buf, start = None, None
    for lineno, raw in enumerate(text.splitlines(), 1):
        stripped = raw.rstrip()
        if buf is None:
            buf, start = "", lineno
        buf += stripped[:-1] + " " if stripped.endswith("\\") else stripped
        if not stripped.endswith("\\"):
            out.append((start, buf))
            buf, start = None, None
    if buf is not None:
        out.append((start, buf))
    return out


def is_comment(line: str) -> bool:
    return line.lstrip().startswith("#")


def call_args(logical: str, helper: str, *, count: int):
    """The first `count` shell words after `helper` in `logical`, or None."""
    # `\b` alone would match the by-id helper when asked for the read helper.
    m = re.search(re.escape(helper) + r"(?![A-Za-z0-9_])", logical)
    if not m:
        return None
    toks = []
    for t in TOKEN.finditer(logical[m.end() :]):
        toks.append(next(g for g in t.groups() if g is not None))
        if len(toks) == count:
            break
    return toks if len(toks) == count else None


def bodies(text: str):
    """`(start_line, body_text)` per top-level block.

    A `.just` recipe body and a `.sh` function body are both "the indented run
    after a column-0 header", which is all the pairing test needs: two calls in
    ONE recipe are one derivation, two calls in two recipes are two.
    """
    lines = text.splitlines()
    out, start, buf = [], 1, []
    for lineno, raw in enumerate(lines, 1):
        at_col0 = raw and not raw[0].isspace()
        if at_col0 and buf:
            out.append((start, "\n".join(buf)))
            buf, start = [], lineno
        elif not buf:
            start = lineno
        buf.append(raw)
    if buf:
        out.append((start, "\n".join(buf)))
    return out


def leaf_glob(leaf: str) -> str:
    """The leaf argument as a path pattern, `*` for each non-constant segment."""
    leaf = leaf.strip("\"'")
    if leaf.startswith("$PWD/"):
        leaf = leaf[len("$PWD/") :]
    parts = [("*" if DYNAMIC.search(p) else p) for p in leaf.split("/")]
    return "/".join(parts).rstrip("/")


def check_source(fm, rel: str, text: str, rows) -> list[str]:
    problems = []
    produced = {slug for _dir, _plat, slug in rows}

    for start, body in bodies(text):
        pairs = set()
        for _ln, logical in logical_lines(body):
            if is_comment(logical):
                continue
            got = call_args(logical, BUILD_HELPER, count=3)
            if got:
                pairs.add(tuple(got))

        for offset, logical in logical_lines(body):
            lineno = start + offset - 1
            if is_comment(logical):
                continue
            got = call_args(logical, READ_HELPER, count=4)
            if not got:
                continue
            leaf, platform, args, envstr = got
            if DYNAMIC.search(platform):
                continue  # the platform itself is derived — nothing to assert
            if DYNAMIC.search(args) or DYNAMIC.search(envstr):
                continue  # the key is derived, not asserted
            if (platform, args, envstr) in pairs:
                continue  # builds and reads with ONE key — self-consistent
            want = hand_slug(fm, platform, args, envstr)
            pattern = leaf_glob(leaf)
            matched = [
                (d, s) for (d, p, s) in rows if p == platform and fnmatch(d, pattern)
            ]
            if not matched:
                continue  # no manifest row here — nothing to disagree with
            if any(s == want for _d, s in matched):
                continue
            got_slugs = sorted({s for _d, s in matched})
            problems.append(
                f"{rel}:{lineno}: `{READ_HELPER} … {platform} \"{args}\" \"{envstr}\"` "
                f"resolves the group `{want}`, which no manifest row for "
                f"`{pattern}` builds into (they build {', '.join(got_slugs)}). "
                f"This call does not build what it reads, so its key must come "
                f"from the row: use `{BY_ID_HELPER} <row-id>` (issue 1025)."
            )

    for lineno, logical in logical_lines(text):
        if is_comment(logical):
            continue
        for m in LITERAL_GROUP.finditer(logical):
            slug = m.group(1)
            if slug in produced:
                continue
            problems.append(
                f"{rel}:{lineno}: the literal `cargo-fixtures/{slug}` names a group "
                f"no `[[fixture]]` row builds into. Derive it from "
                f"`{BUILD_HELPER}` / `{BY_ID_HELPER}` instead of spelling it "
                f"(issue 1025)."
            )
    return problems


# ── the negative control, run on the normal path ────────────────────────────

_SELFTEST_BROKEN = """
build-qemu:
    #!/usr/bin/env bash
    source scripts/build/fixtures-target-dir.sh
    bash scripts/build/fixtures-build.sh %(platform)s rust --id %(rowid)s
    artifact_dir="$(nros_fixture_row_artifact_dir \\
        "%(leaf)s" %(platform)s "" "")"
"""

_SELFTEST_FIXED = """
build-qemu:
    #!/usr/bin/env bash
    source scripts/build/fixtures-target-dir.sh
    bash scripts/build/fixtures-build.sh %(platform)s rust --id %(rowid)s
    artifact_dir="$(nros_fixture_row_artifact_dir_by_id %(rowid)s)"
"""

_SELFTEST_PAIRED = """
_run-qemu:
    #!/usr/bin/env bash
    tdir_flag="$(nros_fixture_target_dir_flag %(platform)s "" "")"
    artifact_dir="$(nros_fixture_row_artifact_dir "%(leaf)s" %(platform)s "" "")"
    cargo build $tdir_flag
"""


def selftest(fm) -> None:
    """A synthetic call site of each shape, against a synthetic row table.

    The row table is synthetic ON PURPOSE. Driving the control off a real
    variant row makes it hostage to the manifest: `examples/native/rust/talker`
    carries a plain `linux` row AND eight variants, so a hand-spelled
    `linux "" ""` there is CORRECT (one row does build into `linux`) and the
    control silently stops being one. A control that can be disarmed by an
    unrelated manifest edit is the decay this gate is supposed to prevent.

    The slug DERIVATION is still real: `hand_slug` shells into
    `nros_fixture_group_slug` for the call site's key. Only the manifest side is
    stood in for.
    """
    platform = "selftest-platform"
    leaf = "examples/selftest-platform/rust/talker"
    rows = [(leaf, platform, f"{platform}-4118800323")]
    subst = {"platform": platform, "leaf": leaf, "rowid": "selftest-row"}

    broken = check_source(fm, "<selftest>", _SELFTEST_BROKEN % subst, rows)
    if not broken:
        sys.stderr.write(
            "check-fixture-artifact-dir-keys: SELFTEST FAILED — a hand-spelled "
            f'`… {platform} "" ""` against variant leaf {leaf} was NOT reported. '
            "The gate cannot fail, so its green means nothing.\n"
        )
        raise SystemExit(2)
    for shape, src in (("fixed", _SELFTEST_FIXED), ("paired", _SELFTEST_PAIRED)):
        noise = check_source(fm, "<selftest>", src % subst, rows)
        if noise:
            sys.stderr.write(
                f"check-fixture-artifact-dir-keys: SELFTEST FAILED — the {shape} "
                f"call site was reported: {noise}\n"
            )
            raise SystemExit(2)


def sources():
    """The tracked just/shell sources, from the git index — never a walk.

    `check-no-tracked-file-find` measured 7m36s -> 0.8s for the same 232 paths,
    and it is right about this one too: an `rglob` here would also read a
    worktree's build output.
    """
    out = subprocess.run(
        ["git", "ls-files", "-z", "justfile", "just/*.just", "scripts/*.sh"],
        cwd=ROOT,
        capture_output=True,
        text=True,
        check=True,
    ).stdout
    for rel in out.split("\0"):
        if rel:
            yield ROOT / rel


def main() -> int:
    fm = _load_manifest_module()
    rows = manifest_rows(fm)
    selftest(fm)

    problems = []
    checked = 0
    for path in sources():
        if not path.is_file():
            continue
        text = path.read_text(encoding="utf-8", errors="replace")
        if READ_HELPER not in text and "cargo-fixtures/" not in text:
            continue
        checked += 1
        problems += check_source(fm, str(path.relative_to(ROOT)), text, rows)

    if problems:
        sys.stderr.write(
            "check-fixture-artifact-dir-keys: a consumer names a cargo fixture "
            "group the manifest does not build into.\n\n"
        )
        for p in problems:
            sys.stderr.write(f"  {p}\n")
        sys.stderr.write("\n")
        return 1
    print(
        f"check-fixture-artifact-dir-keys: OK "
        f"({checked} source(s), {len(rows)} cargo fixture row(s), selftest red)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
