"""The exact, order-independent stack walk, as a MEASUREMENT (phase-557 W7f
Checkpoint 2, issue 2550 step 3).

The walk today is the deepest of a fixed set of callee orders: a heuristic
on a cyclic graph, and a lower bound on the worst order. The exact answer is
the deepest SIMPLE path from the root (each function at most once, as a
non-recursive stack never holds a frame twice). Finding it is exponential in
the size of a strongly connected component, so whether it is affordable is a
question about the SCCs, which this module measures:

- `sccs(edges, root)`: the strongly connected components reachable from the
  root (Tarjan, iterative);
- `deepest_simple(edges, frames, root, budget)`: the exact search, with an
  expansion budget. It is a DP over the condensation (the DAG of SCCs) that
  enumerates simple paths exhaustively only inside each SCC, and it reports
  how many node expansions that took, or that the budget ran out.

It ships no floor. Checkpoint 2's bar is in the phase doc.
"""

from __future__ import annotations

import time


def reachable(edges: dict[str, set[str]], root: str) -> set[str]:
    seen, stack = {root}, [root]
    while stack:
        for w in edges.get(stack.pop(), ()):
            if w not in seen:
                seen.add(w)
                stack.append(w)
    return seen


def sccs(edges: dict[str, set[str]], root: str) -> list[list[str]]:
    """Tarjan's SCCs of the subgraph reachable from `root`, iteratively, in
    reverse topological order (a component before anything that calls it)."""
    index: dict[str, int] = {}
    low: dict[str, int] = {}
    on: set[str] = set()
    st: list[str] = []
    out: list[list[str]] = []
    counter = 0
    work = [(root, iter(sorted(edges.get(root, ()))))]
    index[root] = low[root] = counter
    counter += 1
    st.append(root)
    on.add(root)
    while work:
        v, it = work[-1]
        advanced = False
        for w in it:
            if w not in index:
                index[w] = low[w] = counter
                counter += 1
                st.append(w)
                on.add(w)
                work.append((w, iter(sorted(edges.get(w, ())))))
                advanced = True
                break
            if w in on:
                low[v] = min(low[v], index[w])
        if advanced:
            continue
        work.pop()
        if work:
            u = work[-1][0]
            low[u] = min(low[u], low[v])
        if low[v] == index[v]:
            comp = []
            while True:
                w = st.pop()
                on.discard(w)
                comp.append(w)
                if w == v:
                    break
            out.append(comp)
    return out


class Budget(Exception):
    pass


def deepest_simple(edges: dict[str, set[str]], frames: dict[str, int], root: str,
                   budget: int = 10**7, seconds: float = 120.0) -> dict:
    """The deepest simple path from `root`, by frame bytes.

    Returns `{depth, expansions, seconds, finished, largest_scc, sccs}`. With
    `finished` False the budget ran out, and `depth` is the best found so
    far (a lower bound).

    `best_out[v]` is the deepest path STARTING at v that leaves v's SCC
    immediately, computed for components in reverse topological order, so a
    component's exits are known before it is searched. Inside a component,
    every simple path from each node is enumerated, and each node on such a
    path may end the path or step out of the component to an exit."""
    comps = sccs(edges, root)
    comp_of = {v: i for i, c in enumerate(comps) for v in c}
    best_from: dict[str, int] = {}  # deepest path starting at v (exact)
    exp = 0
    t0 = time.monotonic()
    finished = True
    try:
        for ci, comp in enumerate(comps):
            members = set(comp)

            def leave(u: str) -> int:
                """Frame of u plus the deepest exit from u out of the SCC."""
                best = 0
                for w in edges.get(u, ()):
                    if comp_of.get(w) != ci:
                        best = max(best, best_from[w])
                return frames.get(u, 0) + best

            if len(comp) == 1 and comp[0] not in edges.get(comp[0], ()):
                best_from[comp[0]] = leave(comp[0])
                exp += 1
                continue
            for start in comp:
                # iterative DFS over simple paths inside the component
                best = 0
                stack = [(start, frames.get(start, 0) - frames.get(start, 0),
                          iter(sorted(w for w in edges.get(start, ()) if w in members)))]
                on = {start}
                acc = [0]  # sum of frames of the path BEFORE the top node
                while stack:
                    u, _, it = stack[-1]
                    exp += 1
                    if exp > budget or (exp & 0xFFFF) == 0 and time.monotonic() - t0 > seconds:
                        raise Budget
                    best = max(best, acc[-1] + leave(u))
                    nxt = None
                    for w in it:
                        if w not in on:
                            nxt = w
                            break
                    if nxt is None:
                        stack.pop()
                        on.discard(u)
                        acc.pop()
                        if not acc:
                            acc.append(0)
                        continue
                    on.add(nxt)
                    acc.append(acc[-1] + frames.get(u, 0))
                    stack.append((nxt, 0, iter(sorted(w for w in edges.get(nxt, ())
                                                      if w in members))))
                best_from[start] = best
    except Budget:
        finished = False
    sizes = sorted((len(c) for c in comps), reverse=True)
    return {"depth": best_from.get(root), "expansions": exp,
            "seconds": round(time.monotonic() - t0, 2), "finished": finished,
            "largest_scc": sizes[0] if sizes else 0,
            "nontrivial_sccs": sum(1 for c in comps if len(c) > 1)}


def self_test() -> list[str]:
    bad: list[str] = []

    def check(name: str, ok: bool) -> None:
        if not ok:
            bad.append(name)

    # a DAG: deepest path is root->b->c
    e = {"r": {"a", "b"}, "a": set(), "b": {"c"}, "c": set()}
    fr = {"r": 10, "a": 100, "b": 50, "c": 70}
    r = deepest_simple(e, fr, "r")
    check("a DAG's deepest path", r["depth"] == 130 and r["finished"])
    # a cycle a<->b: the simple path takes both once, then exits to the deepest
    e = {"r": {"a"}, "a": {"b", "x"}, "b": {"a", "y"}, "x": set(), "y": set()}
    fr = {"r": 1, "a": 10, "b": 20, "x": 5, "y": 100}
    r = deepest_simple(e, fr, "r")
    check("a cycle is taken once, exits to its deepest successor",
          r["depth"] == 1 + 10 + 20 + 100 and r["largest_scc"] == 2)
    # the heuristic trap: entering the cycle at b first must not hide a->b->y
    e = {"r": {"b", "a"}, "a": {"b"}, "b": {"a", "y"}, "y": set()}
    fr = {"r": 1, "a": 40, "b": 2, "y": 3}
    r = deepest_simple(e, fr, "r")
    check("the exact answer does not depend on which node of a cycle is met first",
          r["depth"] == 1 + 40 + 2 + 3)
    check("SCCs come in reverse topological order",
          [sorted(c) for c in sccs({"r": {"a"}, "a": {"b"}, "b": {"a"}}, "r")]
          == [["a", "b"], ["r"]])
    # the budget is honoured and reported
    n = 9
    e = {f"n{i}": {f"n{j}" for j in range(n) if j != i} for i in range(n)}
    e["r"] = {"n0"}
    r = deepest_simple(e, {k: 1 for k in e}, "r", budget=1000)
    check("a budget that runs out says so", not r["finished"] and r["expansions"] > 1000)
    return bad
