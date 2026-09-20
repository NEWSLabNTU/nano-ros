#!/usr/bin/env python3
"""A build shim's `--target` must name a block some bringup declares.

Issue 1396; the class issue 1312 measured and could not gate.

`nros codegen-system --target <id>` selects the `[image.<id>]` / `[deploy.<id>]`
block of the bringup being baked. Everything that block decides — the
`[tiers.*.<rtos>]` sub-table first among them, then the resolved domain id, RMW,
locator and per-image launch — comes from there. An id that names NO block is
not refused: `tier_resolver::target_board_id` simply finds nothing, and
`derive_target_rtos` answers `BoardFamily::Native.tier_rtos_key()`. So a
synthesised string costs an embedded image the host's scheduling parameters,
silently, and the build is otherwise indistinguishable from a correct one.

Four shims had synthesised one. The Zephyr and ESP-IDF shims were fixed by
issue 1312 (`--for-entry <PKG|dir>`: the image that CLAIMS the entry answers,
which is the question a framework configure can actually ask). The NuttX and
PlatformIO shims were left, and issue 1396 fixed both — NuttX by pinning the
image id in `nros_bringup.mk`, PlatformIO by asking `--for-entry` as well.

WHAT IT CHECKS

For every `codegen-system` invocation in a tracked BUILD file, no `--target`
may be a bare LITERAL. A shim serves whatever bringup it is pointed at, so it
cannot know that bringup's block key; it must pass a VARIABLE (`$(X)`, `${X}`)
and let whatever ASSIGNS that variable check it, or ask `--for-entry <pkg|dir>`
and let the image that claims the entry answer. A variable value is out of
scope by construction — this gate cannot evaluate it, and the assigning
producer is where the check belongs. `scripts/nuttx/stage-external-apps.sh
--image` is that producer's check, and it is what the 1396 fix added.

The one way out is an authored marker on the invocation naming THAT VALUE and
a TRACKED ISSUE ID — the CLAUDE.md rule for a tolerated gap, and the thing
1312's two survivors did not have, which is why the class had no gate:

    # nros-shim-target-gap: platformio (issue 1396) — no PIO lane exists

A nearby `issue NNNN` on its own is NOT enough, and neither is "the literal
names a block somewhere". Both were tried and both are measured wrong; see
`MARKER` and `findings` below.

SCOPE, and why it is drawn by file KIND rather than by directory

A shim is a build file, so the scope is every tracked build file: the four build
file NAMES plus the build-language suffixes. Rust is in scope through `build.rs`
ALONE — a build script is a shim, an ordinary `.rs` is not, and the CLI's own
sources and tests spell whole invocations inside string literals as prose
(`packages/cli/nros-cli-core/src/cmd/codegen_system.rs`,
`packages/testing/nros-tests/tests/cli_bringup_px4.rs`). `Kconfig` is excluded
for the same reason from the other side: it runs nothing, and its `help` blocks
quote invocations (`integrations/px4/module-template/component-skeleton/
Kconfig`). `docs/` and `book/` are prose throughout.

Buildless. `--self-test` drives both verdicts over synthetic files, including
the two real pre-1396 shim bodies.
"""

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

BUILD_FILE_NAMES = ("build.rs", "Makefile", "makefile.defs", "CMakeLists.txt")
BUILD_FILE_SUFFIXES = (
    ".cmake", ".just", ".sh", ".bash", ".py", ".mk", ".in",
    ".template", ".toml", ".yml", ".yaml",
)
PROSE_PREFIXES = ("docs/", "book/")

#: This file. Its `--self-test` fixtures are whole invocations carrying the
#: literals the gate refuses — that is what a positive control IS — and it is a
#: `.py` that names the verb, so it is in scope by every rule above.
#:
#: The exclusion is narrow (one exact path, not `scripts/check-*`) and it is
#: CHECKED, because a self-exclusion is the classic reach hole. It is also the
#: only finding this gate has ever reported on the real tree: while the script
#: was untracked it was invisible to `git ls-files`, so the gate read green, and
#: the first run after the commit reported seven findings in itself. A gate that
#: passes because its subject is not yet tracked is issue 1226's shape.
SELF = "scripts/check-shim-codegen-target.py"

VERB = "codegen-system"

#: `--target <value>`, in the three spellings a build file writes it: separated
#: by whitespace, joined by `=`, or as two adjacent argv strings
#: (`"--target", "platformio"`). The value must START with an alphanumeric, so
#: `$(X)` / `${X}` / `%X%` never match — and neither does `--target-dir`, whose
#: next character is `-`.
TARGET_ARG = re.compile(
    r'--target(?:[=\s]|["\']\s*,\s*["\'])\s*["\']?([A-Za-z0-9][A-Za-z0-9_.\-]*)'
)

#: A whole-line comment in any of the scoped languages.
COMMENT_LINE = re.compile(r'^\s*(#|//|\*|--\s)')

#: The repo's spelling for a tracked issue reference.
ISSUE_REF = re.compile(r'issue[ \-]?(\d{3,4})', re.IGNORECASE)

#: The opt-out, and why it is an AUTHORED MARKER rather than a nearby mention.
#:
#: The first version accepted any `issue NNNN` within six lines of the
#: invocation, and that was measured wrong on this repo's own fix: the NuttX
#: template's comment explains the 1396 fix and cites the issue, so when the
#: defect was re-injected to test the gate, the gate stayed GREEN — a file that
#: DISCUSSES the issue excused the very thing the issue was about. Prose
#: proximity cannot distinguish "this gap is tolerated" from "here is what we
#: fixed".
#:
#: So the marker names the VALUE it tolerates and carries a tracked id on the
#: same line, the shape `check-rmw-api-parity`'s `gap` entries already use:
#:
#:     # nros-shim-target-gap: platformio (issue 1396) — no PIO lane exists
MARKER = re.compile(r'nros-shim-target-gap:\s*([A-Za-z0-9][A-Za-z0-9_.\-]*)')

#: How far an invocation's arguments may run, and how far back a comment may
#: sit and still be read as its reason.
WINDOW_AHEAD = 12
WINDOW_BEHIND = 6


def tracked_files():
    out = subprocess.run(
        ["git", "-C", str(ROOT), "ls-files"],
        capture_output=True, text=True, check=True,
    ).stdout.split("\n")
    return [p for p in out if p]


def is_build_file(path: str) -> bool:
    if path.startswith(PROSE_PREFIXES) or path == SELF:
        return False
    return path.rsplit("/", 1)[-1] in BUILD_FILE_NAMES or path.endswith(BUILD_FILE_SUFFIXES)


def declared_block_ids(read_system_toml) -> set:
    """Every `[image.<id>]` / `[deploy.<id>]` id any bringup declares."""
    ids = set()
    pat = re.compile(r'^\s*\[(?:image|deploy)\.("?)([^\]"]+)\1\]')
    for text in read_system_toml():
        for line in text.splitlines():
            m = pat.match(line)
            if m:
                ids.add(m.group(2))
    return ids


def norm_id(n: str) -> str:
    """`0088` and `88` are the same issue; compare them one way."""
    return n.lstrip("0") or "0"


def findings(paths, read_text, declared, tracked):
    """`[(path, line, value, names_a_block)]` — every unmarked `--target` LITERAL.

    `declared` does NOT decide the verdict; it annotates it. The first version
    of this gate passed a literal that named a block somewhere in the tree, and
    that was measured wrong on the very defect it was written for: re-injecting
    `--target nuttx` into the NuttX template left the gate GREEN, because six
    example-workspace bringups declare `[image.nuttx]`. They are not the bringup
    that shim bakes, and no static check can know which one is — the template's
    bringup is a make variable.

    So the rule is the one a SHIM can actually be held to: it serves whatever
    bringup it is pointed at, so it may not hardcode a block key at all. Pass a
    variable (and check it where it is ASSIGNED — `stage-external-apps.sh
    --image` is that check), or ask `--for-entry` and let the image that claims
    the entry answer.

    `names_a_block` is carried into the message because it is the more
    dangerous case, not the safer one: a literal that happens to name a block
    reads as correct to anyone who greps for it.

    `tracked` is the set of issue ids that EXIST (normalised). A marker naming
    a file nobody wrote is not a tracked gap, it is a comment.
    """
    out = []
    seen = set()
    for path in paths:
        text = read_text(path)
        if text is None or VERB not in text:
            continue
        lines = text.splitlines()
        for i, line in enumerate(lines):
            if VERB not in line or COMMENT_LINE.match(line):
                continue
            # The invocation's argument window: forward from the verb, stopping
            # at the first blank line (every spelling in the tree — backslash
            # continuation, a cmake COMMAND block, a python argv list — keeps
            # its arguments contiguous).
            window = []
            for j in range(i, min(i + WINDOW_AHEAD, len(lines))):
                if j > i and not lines[j].strip():
                    break
                window.append(j)
            # The marker may sit in a comment just above the invocation or
            # anywhere inside it. It excuses ONLY the value it names, and only
            # while the issue it names exists.
            excused = set()
            for k in range(max(0, i - WINDOW_BEHIND), max(window) + 1):
                m = MARKER.search(lines[k])
                if m and ({norm_id(n) for n in ISSUE_REF.findall(lines[k])} & tracked):
                    excused.add(m.group(1))
            for j in window:
                if COMMENT_LINE.match(lines[j]):
                    continue
                for m in TARGET_ARG.finditer(lines[j]):
                    value = m.group(1)
                    key = (path, j + 1, value)
                    if key in seen:
                        continue
                    seen.add(key)
                    if value in excused:
                        continue
                    out.append((path, j + 1, value, value in declared))
    return out


def tracked_issue_ids() -> set:
    ids = set()
    for d in (ROOT / "docs/issues", ROOT / "docs/issues/archived"):
        if d.is_dir():
            for f in d.glob("[0-9]*.md"):
                ids.add(norm_id(f.name.split("-", 1)[0]))
    return ids


def main() -> int:
    if "--self-test" in sys.argv:
        return self_test("-v" in sys.argv or "--verbose" in sys.argv)
    # Always, not only behind the flag: a negative control nobody runs decays
    # into a comment, and this gate is nothing BUT a pattern matcher — its one
    # failure mode is a scanner that quietly stops matching. The first draft
    # missed the argv-list spelling `"--target", "platformio"` entirely.
    if self_test(verbose=False, quiet=True) != 0:
        return 1

    paths = [p for p in tracked_files() if is_build_file(p)]

    def read_system_toml():
        for p in tracked_files():
            if p.rsplit("/", 1)[-1] == "system.toml":
                try:
                    yield (ROOT / p).read_text(encoding="utf-8")
                except (OSError, UnicodeDecodeError):
                    continue

    declared = declared_block_ids(read_system_toml)
    if not declared:
        # Not a verdict input any more — but a finding's note says whether the
        # literal names a block, and an empty harvest would tell every reader
        # the opposite of the truth. A wrong explanation sends the next person
        # somewhere there is nothing to find.
        print(
            "check-shim-codegen-target: FAILED — no `[image.*]` / `[deploy.*]` block "
            "found in any tracked system.toml. Findings are annotated from that set, "
            "so an empty one would mis-describe every one of them.",
            file=sys.stderr,
        )
        return 1

    def read_text(p):
        try:
            return (ROOT / p).read_text(encoding="utf-8")
        except (OSError, UnicodeDecodeError):
            return None

    tracked = tracked_issue_ids()
    if not tracked:
        print(
            "check-shim-codegen-target: FAILED — no issue files found under "
            "docs/issues/. That is the set an excuse is checked against, so an "
            "empty one would reject every legitimate tolerated gap.",
            file=sys.stderr,
        )
        return 1

    bad = findings(paths, read_text, declared, tracked)
    if bad:
        print(
            "check-shim-codegen-target: a build shim hardcodes a `codegen-system "
            "--target`\n",
            file=sys.stderr,
        )
        for path, line, value, names_a_block in bad:
            note = (
                "  <- names a block in SOME bringup, which is worse, not better: "
                "it reads as correct while naming a block this shim's own bringup "
                "may not declare"
                if names_a_block else
                "  <- names no block in any bringup"
            )
            print(f"  {path}:{line}: --target {value}\n  {note}", file=sys.stderr)
        print(
            "\n`--target` selects an `[image.<id>]` / `[deploy.<id>]` block of the "
            "bringup being baked, and an id naming none is taken SILENTLY: the tier "
            "resolver answers with the HOST's sub-table (issue 1312). A shim serves "
            "whatever bringup it is pointed at, so it cannot know the key — pass a "
            "VARIABLE and check it where it is assigned (`stage-external-apps.sh "
            "--image` is that check), or ask `--for-entry <pkg|dir>` and let the "
            "image that claims the entry answer.\n"
            "If neither is possible here, mark the gap on the invocation — the VALUE "
            "and a tracked issue id, on ONE comment line:\n"
            "    # nros-shim-target-gap: <value> (issue NNNN) — <why>\n"
            "A nearby mention of an issue is deliberately NOT enough: a file that "
            "explains a fix would then excuse the defect it fixed.",
            file=sys.stderr,
        )
        return 1

    print(f"check-shim-codegen-target: OK ({len(paths)} build files, "
          f"{len(declared)} declared block ids)")
    return 0


# --------------------------------------------------------------------------
# self-test
# --------------------------------------------------------------------------

#: The real pre-1396 shim bodies, trimmed to the invocation. Both must be
#: findings: they are what this gate exists to have caught.
NUTTX_BEFORE = """\
\tQ) $(NROS_BIN) codegen-system \\
\t\t--bringup $(NROS_BRINGUP_NAME) \\
\t\t--target nuttx \\
\t\t--out $(NROS_BRINGUP_OUT) \\
\t\t--workspace $(NROS_BRINGUP_WORKSPACE)
"""

PIO_BEFORE = """\
    cmd = [nros, "codegen-system", "--ahead-of-vendor",
           "--workspace", workspace, "--bringup", bringup,
           "--target", "platformio", "--framework", _framework(),
           "--out", out_dir]
"""

NUTTX_AFTER = """\
NROS_CODEGEN_TARGET_ARG = $(if $(NROS_BRINGUP_IMAGE),--target $(NROS_BRINGUP_IMAGE))
\t$(Q) $(NROS_BIN) codegen-system \\
\t\t--bringup $(NROS_BRINGUP_NAME) \\
\t\t$(NROS_CODEGEN_TARGET_ARG) \\
\t\t--out $(NROS_BRINGUP_OUT)
"""


def self_test(verbose: bool, quiet: bool = False) -> int:
    ok = fail = 0

    def chk(label, cond):
        nonlocal ok, fail
        if cond:
            ok += 1
            if verbose:
                print(f"  ok   {label}")
        else:
            fail += 1
            print(f"  FAIL {label}", file=sys.stderr)

    declared = {"native", "qemu-armv7a-nuttx", "zephyr"}
    tracked = {"1396", "1312"}

    def run(files):
        return findings(list(files), files.get, declared, tracked)

    def vals(res):
        return sorted(v for _, _, v, _flag in res)

    chk("the real pre-1396 NuttX shim is a finding",
        vals(run({"Makefile": NUTTX_BEFORE})) == ["nuttx"])
    chk("the real pre-1396 PlatformIO shim is a finding — the argv-list spelling",
        vals(run({"x.py": PIO_BEFORE})) == ["platformio"])
    chk("the NuttX fix passes (the value is a make variable)",
        run({"Makefile": NUTTX_AFTER}) == [])
    # The measured correction to this gate's first rule. `nuttx` IS a declared
    # block id (six example-workspace bringups declare `[image.nuttx]`), so
    # "names a block somewhere" left the NuttX template's RE-INJECTED defect
    # green — the gate could not catch the bug it was written for. A shim may
    # not hardcode ANY key; the ones that do name a block get a sharper note,
    # because those are the ones that read as correct.
    chk("a literal that names a declared block is STILL a finding",
        vals(run({"a.sh": 'nros codegen-system --target qemu-armv7a-nuttx --out x\n'}))
        == ["qemu-armv7a-nuttx"])
    chk("and it is reported as naming a block",
        run({"a.sh": 'nros codegen-system --target qemu-armv7a-nuttx\n'})[0][3] is True)
    chk("a literal naming no block is reported as naming none",
        run({"a.sh": 'nros codegen-system --target bogus\n'})[0][3] is False)
    chk("`--for-entry` is not a `--target`",
        run({"a.sh": 'nros codegen-system --for-entry ${DIR} --out x\n'}) == [])
    chk("`--target-dir` is not `--target`",
        run({"a.sh": 'nros codegen-system --target-dir /tmp/t --out x\n'}) == [])
    chk("a `--target=<literal>` spelling is caught too",
        vals(run({"a.sh": 'nros codegen-system --target=bogus --out x\n'})) == ["bogus"])
    chk("a shell variable value is out of scope",
        run({"a.sh": 'nros codegen-system --target "$IMAGE" --out x\n'}) == [])
    chk("a cmake variable value is out of scope",
        run({"a.cmake": 'COMMAND nros codegen-system --target ${_img}\n'}) == [])

    # The opt-out, and its limits.
    #
    # `unfiled` is INTERPOLATED, never written out: `check-prose-issue-refs`
    # reads the literal `issue NNNN` anywhere in the tree and demands the file
    # exist, and the point of that case is an id where no file does.
    unfiled = "9" * 4
    mark = "# nros-shim-target-gap: bogus (issue 1396) — no lane builds this\n"
    chk("the marker above the invocation excuses the value it names",
        run({"a.sh": mark + 'nros codegen-system --target bogus --out x\n'}) == [])
    chk("the marker INSIDE the invocation window works too",
        run({"a.sh": 'nros codegen-system \\\n  ' + mark + '  --target bogus --out x\n'})
        == [])
    chk("it excuses ONLY the value it names",
        vals(run({"a.sh": mark + 'nros codegen-system --target other --out x\n'}))
        == ["other"])
    # The measured regression this shape exists for.
    chk("a nearby mention of the issue is NOT a marker — prose is not an opt-out",
        vals(run({"a.sh": '# issue 1396 fixed the synthesised `--target nuttx`\n'
                          'nros codegen-system --target bogus --out x\n'})) == ["bogus"])
    chk("a marker naming an issue nobody filed does not excuse",
        vals(run({"a.sh": f'# nros-shim-target-gap: bogus (issue {unfiled})\n'
                          'nros codegen-system --target bogus --out x\n'})) == ["bogus"])
    chk("a marker with no issue id at all does not excuse",
        vals(run({"a.sh": '# nros-shim-target-gap: bogus\n'
                          'nros codegen-system --target bogus --out x\n'})) == ["bogus"])
    chk("an unexcused sibling invocation two blocks away is still a finding",
        vals(run({"a.sh": mark + 'nros codegen-system --target bogus\n'
                          '\n\n\n\n\n\n\nnros codegen-system --target other\n'}))
        == ["other"])

    # Mutations: each arm must be load-bearing.
    chk("a file that never invokes the verb is out of scope",
        run({"a.sh": 'cmake --build b --target install\n'}) == [])
    chk("a `cmake --build --target` far from the verb is not attributed to it",
        run({"a.sh": 'nros codegen-system --out x\n\n'
                     'cmake --build b --target install\n'}) == [])
    chk("a commented-out invocation is not an invocation",
        run({"a.sh": '# nros codegen-system --target bogus\n'}) == [])
    chk("a commented `--target` inside a live invocation is prose, not an argument",
        run({"a.sh": 'nros codegen-system \\\n  # was --target bogus\n  --out x\n'}) == [])
    # `declared` annotates; it cannot make a literal pass. Asserted because the
    # first rule let it decide, and that is what this gate got wrong.
    chk("an empty declared set changes only the NOTE, never the verdict",
        vals(findings(["a.sh"], {"a.sh": 'nros codegen-system --target native\n'}.get,
                      set(), tracked)) == ["native"])
    chk("a declared value is still a finding, with the note flipped",
        findings(["a.sh"], {"a.sh": 'nros codegen-system --target native\n'}.get,
                 {"native"}, tracked)[0][3] is True)
    chk("`0088` and `88` name the same issue",
        findings(["a.sh"],
                 {"a.sh": '# nros-shim-target-gap: bogus (issue 0088)\n'
                          'nros codegen-system --target bogus\n'}.get,
                 declared, {"88"}) == [])

    # Scope.
    chk("build.rs is in scope", is_build_file("packages/x/build.rs"))
    chk("an ordinary .rs is not", not is_build_file("packages/x/src/lib.rs"))
    chk("Kconfig is not (it runs nothing, and its help text quotes invocations)",
        not is_build_file("integrations/px4/module-template/Kconfig"))
    # A real path: `check-doc-refs` reads any `docs/**.md` spelled anywhere in
    # the tree and demands the file exist.
    chk("docs are not", not is_build_file("docs/issues/README.md"))
    chk("a book page is not", not is_build_file("book/src/x.yml"))
    chk("the NuttX template Makefile is", is_build_file(
        "integrations/nuttx/apps-external-template/Makefile"))
    chk("the PlatformIO hook is", is_build_file("integrations/platformio/nros_codegen.py"))
    chk("the ESP-IDF shim is", is_build_file("integrations/nano-ros/CMakeLists.txt"))
    chk("the Zephyr module is", is_build_file("zephyr/cmake/nros_system_generate.cmake"))
    # The self-exclusion, and its exact width. A gate that quietly stopped
    # reading `scripts/` would look identical to one that reads all of it.
    chk("this script excludes ITSELF (its fixtures are the positive controls)",
        not is_build_file(SELF))
    chk("this script is otherwise in scope — the exclusion is the ONLY reason",
        SELF.endswith(".py") and not SELF.startswith(PROSE_PREFIXES))
    chk("a sibling gate script is still in scope",
        is_build_file("scripts/check-codegen-tool-reconfigure.py"))
    chk("the exclusion is one exact path — a near-miss name is still in scope",
        is_build_file("scripts/check-shim-codegen-target2.py")
        and is_build_file("scripts/lib/check-shim-codegen-target.py"))

    # The declared-id harvest reads both TOML spellings of a key.
    ids = declared_block_ids(lambda: iter([
        '[image.native]\nboard = "x"\n[deploy."qemu-armv7a-nuttx"]\n'
        '[board_config."not-an-image"]\n'
    ]))
    chk("declared ids harvest bare and quoted keys",
        ids == {"native", "qemu-armv7a-nuttx"})

    # The excuse must name an issue that EXISTS, or it is not a tracked id.
    chk("issue 1396 is a tracked id in this tree", "1396" in tracked_issue_ids())

    if verbose:
        print(f"\n{ok} passed, {fail} failed")
    if fail:
        print("check-shim-codegen-target self-test: FAILED", file=sys.stderr)
        return 1
    if not quiet:
        print(f"check-shim-codegen-target self-test: OK ({ok} checks)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
