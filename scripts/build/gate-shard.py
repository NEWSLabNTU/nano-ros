#!/usr/bin/env python3
"""Split a derived gate list into N shards of similar measured cost (issue 2141).

`just check build` outgrew one 4-vCPU hosted runner's 120-minute cap: its
two slowest gates, `linux-check` (54 min) and `template-copy-out` (37 min),
ran on the same runner, and the lane's busy time was ~3.8 h over a 69 min
wall on 2026-10-28. Running the lane as shards on separate runners splits
that, and each shard is its own job with its own cap.

The shard assignment is a greedy longest-first fill over the weights file:
take the gates heaviest first, and put each on the shard with the least
weight so far. Ties go to the lower shard index, so the split is the same on
every runner given the same list and weights.

The weights only decide BALANCE, never COVERAGE. Every gate in the input list
lands in exactly one shard whatever the weights say. A gate missing from the
weights file gets the median weight, and a weights row naming no listed gate
is ignored, so a stale weights file costs balance and nothing else.

Usage:
    gate-shard.py <list-file> <weights-file> <k>/<n>   # prints shard k's gates
    gate-shard.py --self-test
"""

import statistics
import sys


def read_list(path):
    with open(path) as f:
        return [
            line.strip()
            for line in f
            if line.strip() and not line.lstrip().startswith("#")
        ]


def read_weights(path):
    weights = {}
    try:
        with open(path) as f:
            for line in f:
                line = line.split("#", 1)[0].strip()
                if not line:
                    continue
                gate, ms = line.split()
                weights[gate] = int(ms)
    except FileNotFoundError:
        pass
    return weights


def assign(gates, weights, n):
    """Return a list of n gate lists; together they hold every gate exactly once."""
    known = [weights[g] for g in gates if g in weights]
    default = int(statistics.median(known)) if known else 1
    order = sorted(
        enumerate(gates), key=lambda ig: (-weights.get(ig[1], default), ig[0])
    )
    shards = [[] for _ in range(n)]
    load = [0] * n
    for _, gate in order:
        k = min(range(n), key=lambda i: (load[i], i))
        shards[k].append(gate)
        load[k] += weights.get(gate, default)
    # Keep each shard in list order, so its output reads like the unsharded lane.
    pos = {g: i for i, g in enumerate(gates)}
    return [sorted(s, key=pos.__getitem__) for s in shards]


def parse_spec(spec):
    k, n = (int(x) for x in spec.split("/"))
    if not 1 <= k <= n:
        raise ValueError(f"shard {spec}: need 1 <= k <= n")
    return k, n


def self_test():
    gates = ["a", "b", "c", "d", "e", "_end"]
    weights = {"a": 100, "b": 90, "c": 10, "d": 10, "gone": 999}
    shards = assign(gates, weights, 2)
    # Coverage: every gate exactly once, whatever the weights.
    flat = sorted(g for s in shards for g in s)
    assert flat == sorted(gates), shards
    # Balance: the two heaviest are split.
    assert ("a" in shards[0]) != ("b" in shards[0]), shards
    # One shard is the whole list.
    assert assign(gates, weights, 1) == [gates]
    # No weights at all still covers everything.
    flat = sorted(g for s in assign(gates, {}, 3) for g in s)
    assert flat == sorted(gates)
    print("gate-shard: self-test OK")


def main(argv):
    if argv[1:] == ["--self-test"]:
        self_test()
        return 0
    if len(argv) != 4:
        print(__doc__.strip().splitlines()[-3], file=sys.stderr)
        return 2
    gates = read_list(argv[1])
    k, n = parse_spec(argv[3])
    shard = assign(gates, read_weights(argv[2]), n)[k - 1]
    sys.stdout.write("".join(g + "\n" for g in shard))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
