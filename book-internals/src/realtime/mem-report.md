# The memory report

> What the report shows and how to read it is
> [Measuring Static Memory](../../realtime/static-memory.html) in
> the nano-ros Book. This page is how to run it: the tool is
> `scripts/nros-mem-report.py` in this repository and has no `nros` verb
> yet, so a user of an installed release cannot run it.

Measure a linked image:

```sh
just mem-report path/to/your/binary
# for example
just mem-report examples/linux/rust/talker/target-zenoh/nros-fast-release/talker
```

`just mem-report` is a thin wrapper over
`python3 scripts/nros-mem-report.py <args>`.

## Showing a saving

Take a baseline, change something, and compare:

```sh
just mem-report my-binary --json > before.json
# ... tune a knob, rebuild ...
just mem-report my-binary --baseline before.json
```

## In CI

The check suite runs the tool's self-test (`nros-mem-report.py --selftest`),
and the L3 CI lane runs `nros-mem-report.py <elf> --check` over the cross
images it built.
