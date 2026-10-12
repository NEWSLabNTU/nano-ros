"""Parse an EVENT-ROUTED `runs-on:` / `container.image:` pair. phase-489 W2.

A job that runs self-hosted on some events and hosted on others is written as
one GitHub expression, and the gates that reason about runners have to read it
instead of treating every `${{` as unknowable:

    runs-on: ${{ COND && fromJSON('["self-hosted", ...]') || 'ubuntu-22.04' }}
    container:
      image: ${{ !(COND) && 'ghcr.io/...' || '' }}

`a && b || c` is GitHub's ternary: `b` when `a` is truthy (a non-empty array
always is), `c` otherwise. So the self-hosted labels are reachable exactly on
the events COND admits, and the gate's question — can a fork's pull request
reach a self-hosted runner? — reduces to which events COND can be true on.

COND is accepted only as a conjunction (`&&`) of terms this module
understands, and every unknown term makes the whole thing unparseable, which
callers treat as "could be anything" (fail closed):

    vars.NAME == 'true'                                      a repo switch
    github.event_name == 'EVENT'                             one event
    contains(fromJSON('["E1","E2"]'), github.event_name)     a set of events

At least one event term is required, and the reachable set is the
INTERSECTION of the event terms. A `vars.` term can only narrow it. No `||`,
no `!`, no parentheses inside COND — a disjunction is where "only on these
events" quietly stops being true, so it is refused rather than evaluated.

Used by check-workflow-runner-isolation (security) and
check-workflow-just-provisioning (the hosted arm still names a CI image).
"""
from __future__ import annotations

import json
import re
from dataclasses import dataclass

_RUNS_ON = re.compile(
    r"^\$\{\{\s*(?P<cond>.+?)\s*&&\s*fromJSON\('(?P<labels>\[[^']*\])'\)\s*"
    r"\|\|\s*'(?P<hosted>[A-Za-z0-9._-]+)'\s*\}\}$",
    re.S,
)
_IMAGE = re.compile(
    r"^\$\{\{\s*!\((?P<cond>.+)\)\s*&&\s*'(?P<image>[^']+)'\s*\|\|\s*''\s*\}\}$",
    re.S,
)
_VAR = re.compile(r"^vars\.[A-Za-z_][A-Za-z0-9_]*\s*==\s*'[^']*'$")
_EQ = re.compile(r"^github\.event_name\s*==\s*'(?P<e>[a-z_]+)'$")
_IN = re.compile(
    r"^contains\(\s*fromJSON\('(?P<set>\[[^']*\])'\)\s*,\s*github\.event_name\s*\)$"
)


@dataclass(frozen=True)
class Route:
    cond: str               # normalised (whitespace-collapsed) condition
    events: frozenset       # events on which the self-hosted arm is reachable
    labels: tuple           # the self-hosted arm's labels
    hosted: str             # the other arm


def _norm(s: str) -> str:
    return " ".join(s.split())


def parse_cond(cond: str):
    """frozenset of events COND can be true on, or None if not understood."""
    if "||" in cond or "!" in cond:
        return None
    # Split on top-level `&&`. Parentheses only occur inside `contains(...)`.
    terms, depth, cur = [], 0, ""
    i = 0
    while i < len(cond):
        c = cond[i]
        if c == "(":
            depth += 1
        elif c == ")":
            depth -= 1
        if depth == 0 and cond.startswith("&&", i):
            terms.append(cur.strip())
            cur = ""
            i += 2
            continue
        cur += c
        i += 1
    terms.append(cur.strip())
    events = None
    for term in terms:
        if _VAR.match(term):
            continue
        m = _EQ.match(term)
        if m:
            got = {m.group("e")}
        else:
            m = _IN.match(term)
            if not m:
                return None
            try:
                got = set(json.loads(m.group("set")))
            except ValueError:
                return None
            if not all(isinstance(e, str) for e in got):
                return None
        events = got if events is None else events & got
    if events is None:
        return None  # no event term: reachable on EVERY event
    return frozenset(events)


def parse_runs_on(runs_on) -> Route | None:
    """The route a `runs-on:` expression encodes, or None if it is not one."""
    if not isinstance(runs_on, str):
        return None
    m = _RUNS_ON.match(runs_on.strip())
    if not m:
        return None
    try:
        labels = json.loads(m.group("labels"))
    except ValueError:
        return None
    if not isinstance(labels, list) or not all(isinstance(x, str) for x in labels):
        return None
    events = parse_cond(m.group("cond"))
    if events is None:
        return None
    return Route(_norm(m.group("cond")), events, tuple(labels), m.group("hosted"))


def parse_image(image):
    """(normalised cond, hosted image) for a routed `container.image:`, or None."""
    if not isinstance(image, str):
        return None
    m = _IMAGE.match(image.strip())
    if not m:
        return None
    if parse_cond(m.group("cond")) is None:
        return None
    return _norm(m.group("cond")), m.group("image")


def _self_test() -> None:
    r = parse_runs_on(
        "${{ vars.X == 'true' && contains(fromJSON('[\"merge_group\",\"workflow_dispatch\"]'),"
        " github.event_name) && fromJSON('[\"self-hosted\",\"linux\"]') || 'ubuntu-22.04' }}")
    assert r and r.events == {"merge_group", "workflow_dispatch"}, r
    assert r.labels == ("self-hosted", "linux") and r.hosted == "ubuntu-22.04", r
    # Intersection of event terms.
    r = parse_runs_on(
        "${{ contains(fromJSON('[\"merge_group\",\"pull_request\"]'), github.event_name)"
        " && github.event_name == 'merge_group' && fromJSON('[\"self-hosted\"]') || 'x' }}")
    assert r and r.events == {"merge_group"}, r
    # Refused: no event term, a disjunction, a negation, an unknown term.
    for bad in (
        "${{ vars.X == 'true' && fromJSON('[\"self-hosted\"]') || 'x' }}",
        "${{ github.event_name == 'push' || github.event_name == 'pull_request'"
        " && fromJSON('[\"self-hosted\"]') || 'x' }}",
        "${{ !(github.event_name == 'pull_request') && fromJSON('[\"self-hosted\"]') || 'x' }}",
        "${{ inputs.big && github.event_name == 'push' && fromJSON('[\"self-hosted\"]') || 'x' }}",
        "${{ inputs.runner }}",
    ):
        assert parse_runs_on(bad) is None, bad
    c, img = parse_image("${{ !(github.event_name == 'merge_group') && 'ghcr.io/a/b:c' || '' }}")
    assert c == "github.event_name == 'merge_group'" and img == "ghcr.io/a/b:c"
    assert parse_image("${{ github.event_name == 'merge_group' && 'img' || '' }}") is None


if __name__ == "__main__":
    _self_test()
    print("runner_route self-test: OK")
