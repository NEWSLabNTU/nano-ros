"""Which lines of a workflow `run:` body are COMMANDS, and which are prose.

Phase-466 W2. Extracted verbatim from `check-workflow-repo-env.py` (issue 0933)
when a second gate needed the same answer, because a second spelling of this
predicate is how the two would drift apart — and they would drift in the
direction that reads green: a detector that misses an invocation reports OK.

Two false positives make a grep-based gate over `.github/workflows/` worse than
nothing, and both were live in the tree when 0933 was written:

  * **Prose.** `nightly.yml`'s CLI-build step has a COMMENT about `nros sync`
    and does not run it.
  * **Command text inside a heredoc.** `queue-notify.yml` builds a pull-request
    comment whose TEXT tells the author to run `just queue-triage` and
    `just ci l1`. Those are strings being posted to GitHub.

The scan is deliberately line-based rather than a shell parse: the rule has to
be explainable in the failure message, and a half-correct parser produces
verdicts nobody can check by eye.

A line counts when the tool stands in command position: at the start, after
`&&`, `||`, `;` or `then`, optionally behind `KEY=value` prefixes.
"""

import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import comments  # noqa: E402  phase-472 W3 — the one shell comment stripper

REPO = Path(__file__).resolve().parent.parent.parent
WORKFLOWS = REPO / ".github" / "workflows"

# The repo tools whose invocation 0933 watches for. `just` is singled out by
# `check-workflow-just-provisioning` for a reason that does not apply to the
# other two — see that gate's header.
TOOLS = ("just", "nros", "west")

# `<<EOF`, `<<-EOF`, `<<'EOF'`, `<<"EOF"` — the body is text, not commands.
HEREDOC = re.compile(r"<<-?\s*(['\"]?)([A-Za-z_][A-Za-z0-9_]*)\1")


def invoke_re(tools=TOOLS):
    """A tool at a command position, optionally behind `KEY=value` prefixes."""
    return re.compile(
        r"(?:^|&&\s*|\|\|\s*|;\s*|\bthen\s+)\s*"
        r"(?:[A-Za-z_][A-Za-z0-9_]*=\S*\s+)*"
        rf"(?:{'|'.join(tools)})\s",
    )


INVOKE = invoke_re()


def command_lines(run: str):
    """The lines of a `run:` body that are actually commands.

    Skips comments and heredoc bodies. Comments go through the shared shell
    stripper (phase-472 W3): the old test skipped only a line STARTING with `#`,
    so `true # && source ./activate.sh` read as an activation.
    """
    out = []
    terminator = None
    for line in comments.strip_comments(run or "", "sh").split("\n"):
        if terminator is not None:
            if line.strip() == terminator:
                terminator = None
            continue
        stripped = line.strip()
        if not stripped:
            continue
        out.append(line)
        m = HEREDOC.search(line)
        if m:
            terminator = m.group(2)
    return out


def logical_lines(run: str):
    """`command_lines`, with shell continuations JOINED into one command.

    A line ending in `\\` continues; so does one ending in `||`, `&&` or `|`.
    phase-472 W6 / issue 1615: `just a \\` + `|| just b` is ONE fallback
    command split over two lines, and a per-LINE matcher never saw both halves.
    """
    out, cur = [], ""
    for line in command_lines(run):
        s = line.rstrip()
        if s.endswith("\\"):
            cur += s[:-1] + " "
            continue
        if re.search(r"(\|\||&&|\|)\s*$", s):
            cur += s + " "
            continue
        out.append(cur + s)
        cur = ""
    if cur:
        out.append(cur)
    return out


ACTIONS = REPO / ".github" / "actions"

# phase-472 W1 — the CI SHELL population, for every gate that reads it: the
# workflows AND the local actions. A composite action's `run:` steps are CI
# steps (`nightly.yml` calls `setup-nros-cli` its SOLE CLI acquisition path),
# and nine gates read `.github/workflows/` only, so a rule broken inside an
# action was never asked. One definition, so the next gate cannot pick a
# smaller one. Pathspecs for `git ls-files` / `git grep` callers (git's `*`
# crosses `/`, so `.github/actions/*/action.yml` is spelled exactly).
WORKFLOW_PATHSPECS = (".github/workflows/*.yml", ".github/workflows/*.yaml")
ACTION_PATHSPECS = (".github/actions/*/action.yml", ".github/actions/*/action.yaml")


def ci_pathspecs(include_actions=True):
    return WORKFLOW_PATHSPECS + (ACTION_PATHSPECS if include_actions else ())


def ci_files(include_actions=True, repo=None):
    """Every workflow and (by default) every local action file, sorted, as Paths.

    For TEXT readers. A YAML reader that walks `jobs.*.steps` wants
    `load_workflows(include_actions=True)`, which shapes an action like a job.
    """
    repo = Path(repo) if repo else REPO
    wf, ac = repo / ".github" / "workflows", repo / ".github" / "actions"
    out = list(wf.glob("*.yml")) + list(wf.glob("*.yaml"))
    if include_actions:
        out += list(ac.glob("*/action.yml")) + list(ac.glob("*/action.yaml"))
    return sorted(out)


def ci_files_self_test():
    """The reach, as a negative control: an action file IS in the population."""
    import tempfile

    with tempfile.TemporaryDirectory() as tmp:
        t = Path(tmp)
        (t / ".github" / "workflows").mkdir(parents=True)
        (t / ".github" / "actions" / "x").mkdir(parents=True)
        (t / ".github" / "workflows" / "a.yml").write_text("on: push\n")
        (t / ".github" / "actions" / "x" / "action.yml").write_text("runs: {}\n")
        got = [str(p.relative_to(t)) for p in ci_files(repo=t)]
        assert got == [".github/actions/x/action.yml", ".github/workflows/a.yml"], got
        assert [str(p.relative_to(t)) for p in ci_files(False, repo=t)] == [".github/workflows/a.yml"]

# The pseudo-job a composite action's steps are filed under, so a gate that
# walks `doc["jobs"][*]["steps"]` reads an action with no second code path.
COMPOSITE_JOB = "(composite action)"


def load_composite_actions():
    """Every local composite action, shaped like a one-job workflow.

    Issue 1548. A `run:` step in `.github/actions/*/action.yml` is a CI shell
    exactly like one in a workflow, and `setup-qemu-patched` sat outside both
    `check-workflow-repo-env` and `check-workflow-indexed-apt` because each read
    `.github/workflows/` only — while `check-workflow-just-provisioning`, a
    sibling over the same class, already read actions. The reach of a gate is
    the rule's, not the directory the first offender happened to live in.

    A non-composite action (docker / node) has no `run:` steps and is skipped.
    """
    import yaml

    docs = []
    for p in (f for f in ci_files() if ACTIONS in f.parents):
        doc = yaml.safe_load(p.read_text()) or {}
        runs = doc.get("runs") or {}
        if runs.get("using") != "composite":
            continue
        docs.append((p.relative_to(REPO), {"jobs": {COMPOSITE_JOB: {"steps": runs.get("steps") or []}}}))
    return docs


def load_workflows(include_actions=False):
    """Every `.github/workflows/*.yml`, as `(repo-relative path, parsed doc)`.

    `include_actions=True` appends every local composite action (see
    `load_composite_actions`). A PER-STEP gate wants that. A per-JOB gate that
    already expands `uses: ./.github/actions/<x>` in place
    (`check-workflow-just-provisioning`) must NOT take it: it would audit an
    action out of the context of the job that provides its prerequisites.
    """
    import yaml

    docs = []
    for p in ci_files(include_actions=False):
        docs.append((p.relative_to(REPO), yaml.safe_load(p.read_text())))
    if include_actions:
        docs.extend(load_composite_actions())
    return docs
