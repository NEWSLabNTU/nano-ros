#!/usr/bin/env python3
"""phase-463 W7 — does a host PROFILE agree with the CENSUS and the CONTRACT?

A profiled run (`$NROS_PROFILE_OUT`, `nros_node::executor::profile`) writes one
`nros.wcet.measurements/1` row per executor callback, with the topics each
invocation published. The census (`$NROS_CENSUS_OUT`) says what the code
CREATED. The contract (`<stem>.contract.yaml`) says what each path OUTPUTS.
This joins the three and refuses on any disagreement:

  P1  ONE-TO-ONE. Every callback-owning census row (a node's timers,
      subscribers, services, actions) has exactly one profile row, and every
      node-attributed profile row has exactly one census row. The key is
      (node, kind, ordinal within that node and kind) for timers -- whose ids
      are synthetic and whose creation order across concurrently set-up tiers
      is not a fact -- and (node, kind, topic) for everything that has one.
  P2  OUTPUTS. For every node whose contract declares timer-triggered paths, the
      union of the topics its timers OBSERVABLY published equals the union of
      those paths' `output:` endpoints, resolved to topics through the
      contract's `topics:` table. This is the check the layer map called
      "trusted": until now nothing compared it to the code.
  P3  EVIDENCE, NOT A BOUND (RFC-0078 D1b). The file states its unit, carries
      `clock_hz = 0` and `convertible_to_time = false`, and has no
      `bound_cycles` anywhere; a callback that never ran carries nulls, never 0.
  P4  COVERAGE. Every row the run was meant to exercise ran: a timer row with
      zero invocations means the run measured nothing for it.

A self-test (planted disagreements, each of which must be caught) runs on
every invocation, so a check that can no longer fail says so.

Usage:
  check-profile-against-census.py --measurements M.json --census C.json \\
      --contract system.contract.yaml
"""

import argparse
import json
import sys

try:
    import yaml
except ImportError:  # pragma: no cover
    sys.stderr.write("check-profile-against-census: needs PyYAML (python3-yaml)\n")
    sys.exit(2)

CALLBACK_FIELDS = {
    "timers": "timer",
    "subscribers": "subscription",
    "services": "service",
    "actions": "action_server",
}


def fqn(node):
    ns = node.get("namespace") or "/"
    ns = ns.rstrip("/")
    return f"{ns}/{node['id']}" if ns else f"/{node['id']}"


def census_rows(census):
    """{(node, kind, key)} where key is an ordinal for timers, else the topic."""
    rows = {}
    for node in census.get("nodes", []):
        n = fqn(node)
        for field, kind in CALLBACK_FIELDS.items():
            for i, ent in enumerate(node.get(field, [])):
                if kind == "timer":
                    key = i
                else:
                    key = (ent.get("id") or "").split("#")[0]
                rows[(n, kind, key)] = ent.get("id")
    return rows


def profile_rows(meas):
    rows = {}
    ordinal = {}
    for m in meas.get("measurements", []):
        if m.get("retired"):
            continue
        n, kind = m["node"], m["kind"]
        if not n:
            # Registered before any node existed: runtime infrastructure the
            # census excludes by design (issue 1693). Not a row to join.
            continue
        if kind == "timer":
            k = ordinal.get((n, kind), 0)
            ordinal[(n, kind)] = k + 1
            key = k
        else:
            key = m["executor_name"]
        rows[(n, kind, key)] = m
    return rows


def contract_outputs(contract):
    """{node fqn: set(topics)} over timer-triggered paths."""
    pub_topic = {}
    for topic, t in (contract.get("topics") or {}).items():
        for ref in t.get("pub") or []:
            pub_topic[ref] = topic
    out = {}
    for node, nd in (contract.get("nodes") or {}).items():
        for _p, path in (nd.get("paths") or {}).items():
            trig = path.get("trigger") or {}
            if "timer" not in trig:
                continue
            topics = out.setdefault(f"/{node}", set())
            for ep in path.get("output") or []:
                topics.add(pub_topic.get(f"{node}/{ep}", f"<unresolved {node}/{ep}>"))
    return out


def check(meas, census, contract):
    errs = []
    # P3
    if meas.get("schema") != "nros.wcet.measurements/1":
        errs.append(f"P3: schema is {meas.get('schema')!r}")
    if meas.get("unit") != "ns":
        errs.append(f"P3: unit is {meas.get('unit')!r}, not 'ns'")
    if (meas.get("conditions") or {}).get("clock_hz") != 0 or meas.get("convertible_to_time"):
        errs.append("P3: a host profile must state clock_hz = 0 and convertible_to_time = false")
    if "bound_cycles" in json.dumps(meas):
        errs.append("P3: the file carries `bound_cycles` -- a host maximum is never a bound (RFC-0078 D1b)")
    for m in meas.get("measurements", []):
        if m.get("iterations", 0) == 0 and m.get("max_observed") is not None:
            errs.append(f"P3: {m['node']} {m['id']} never ran yet states max_observed")
    # P1
    c, p = census_rows(census), profile_rows(meas)
    for key in sorted(set(c) - set(p), key=str):
        errs.append(f"P1: census row {key} (`{c[key]}`) has no profile row")
    for key in sorted(set(p) - set(c), key=str):
        errs.append(f"P1: profile row {key} (`{p[key]['id']}`) has no census row")
    # P4
    for key, m in p.items():
        if key in c and m.get("iterations", 0) == 0 and key[1] == "timer":
            errs.append(f"P4: timer {key} never ran -- the run measured nothing for it")
    # P2
    observed = {}
    for (n, kind, _k), m in p.items():
        if kind == "timer":
            observed.setdefault(n, set()).update(m.get("observed_outputs") or [])
    for n, want in sorted(contract_outputs(contract).items()):
        got = observed.get(n, set())
        if got != want:
            errs.append(
                f"P2: {n}: timers published {sorted(got)}, the contract's path outputs are "
                f"{sorted(want)}"
            )
    return errs, len(c), len(p)


def self_test():
    contract = {
        "nodes": {"talker": {"paths": {"on_timer": {"trigger": {"timer": {"rate_hz": 1}},
                                                    "output": ["chatter"]}}}},
        "topics": {"/chatter": {"pub": ["talker/chatter"]}},
    }
    census = {"nodes": [{"id": "talker", "namespace": None, "timers": [{"id": "timer0#2"}],
                         "subscribers": []}]}
    good = {
        "schema": "nros.wcet.measurements/1", "unit": "ns",
        "conditions": {"clock_hz": 0}, "convertible_to_time": False,
        "measurements": [{"node": "/talker", "id": "timer0", "kind": "timer",
                          "executor_name": "timer@1000000us", "iterations": 3,
                          "max_observed": 10, "observed_outputs": ["/chatter"]}],
    }
    errs, _, _ = check(good, census, contract)
    assert not errs, f"self-test: a consistent triple was refused: {errs}"
    cases = {
        "a missing output": lambda m: m["measurements"][0].update(observed_outputs=[]),
        "an extra output": lambda m: m["measurements"][0].update(
            observed_outputs=["/chatter", "/rogue"]),
        "a missing profile row": lambda m: m.update(measurements=[]),
        "an extra profile row": lambda m: m["measurements"].append(
            dict(m["measurements"][0], id="timer1")),
        "a bound": lambda m: m["measurements"][0].update(bound_cycles=1),
        "a timer that never ran": lambda m: m["measurements"][0].update(
            iterations=0, max_observed=None),
        "a convertible host file": lambda m: m.update(convertible_to_time=True),
    }
    for name, plant in cases.items():
        m = json.loads(json.dumps(good))
        plant(m)
        errs, _, _ = check(m, census, contract)
        if not errs:
            sys.stderr.write(f"check-profile-against-census SELF-TEST FAILED: {name} was not caught\n")
            return False
    return True


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--measurements", required=True)
    ap.add_argument("--census", required=True)
    ap.add_argument("--contract", required=True)
    a = ap.parse_args()
    if not self_test():
        return 1
    with open(a.measurements) as f:
        meas = json.load(f)
    with open(a.census) as f:
        census = json.load(f)
    with open(a.contract) as f:
        contract = yaml.safe_load(f)
    errs, nc, np_ = check(meas, census, contract)
    if errs:
        print(f"check-profile-against-census: {len(errs)} disagreement(s)", file=sys.stderr)
        for e in errs:
            print(f"  - {e}", file=sys.stderr)
        return 1
    print(
        f"check-profile-against-census: OK -- {nc} census callback row(s) <-> {np_} profile "
        f"row(s) one-to-one; observed timer outputs equal the contract's path outputs; "
        f"evidence only (ns, clock_hz 0, no bound)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
