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
from pathlib import Path

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

    Skips comments and heredoc bodies.
    """
    out = []
    terminator = None
    for line in (run or "").split("\n"):
        if terminator is not None:
            if line.strip() == terminator:
                terminator = None
            continue
        stripped = line.strip()
        if stripped.startswith("#"):
            continue
        out.append(line)
        m = HEREDOC.search(line)
        if m:
            terminator = m.group(2)
    return out


def load_workflows():
    """Every `.github/workflows/*.yml`, as `(repo-relative path, parsed doc)`."""
    import yaml

    docs = []
    for p in sorted(WORKFLOWS.glob("*.yml")):
        docs.append((p.relative_to(REPO), yaml.safe_load(p.read_text())))
    return docs
