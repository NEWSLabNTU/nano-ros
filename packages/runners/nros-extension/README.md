# nros-extension

Types and helpers for [nano-ros](https://github.com/NEWSLabNTU/nano-ros)
runner and command packages.

A **runner** package drives a launcher, flasher or console for `nros run`,
`nros flash` and `nros monitor`. A **command** package adds a verb to `nros`.
Both are ordinary packages: a `package.xml` announcing
`<nano_ros_provides kind="runner" name="…"/>` (or `kind="command"`), a small
descriptor, and an executable that `nros build` compiles for the host. nros
starts the executable through a small, versioned, process-based contract
(RFC-0119), and this crate is the Rust side of that contract. nano-ros's own
runners use it too, so a runner you write takes the same path ours do.

| Module | Holds |
| --- | --- |
| `protocol` | the protocol versions this crate speaks, and the negotiation rule |
| `plan` | the RunPlan (RFC-0118 D3), its JSON form, and command-template placeholders |
| `describe` | the `describe` reply and its builder |
| `invocation` | parsing a runner's command line |
| `console` | the console pump and the `nros-runner: link <endpoint>` line |
| `usb` | USB VID:PID parsing and serial-port lookup that never guesses |
| `exit` | the exit codes |

## The runner contract, protocol 1

```text
<bin> describe
    stdout: {"protocol": 1, "verbs": [...], "needs": [...]}

<bin> run|flash|console --plan <plan.json>
    stdout  the target's console stream, unmodified
    stderr  diagnostics; once known, one line `nros-runner: link <endpoint>`
    stdin   console input, forwarded
    exit    0 success · 1 the target failed · 2 the runner failed
```

nros owns everything that must behave the same for every runner: `--expect`
and `--timeout` grading, router start from the reported link, prepare steps,
console capture, the process group, `--dry-run` and `--why`. A runner is asked
one thing: start this, or flash this, and stream what it says.

A runner in another language does not need this crate. It reads the plan JSON
and writes the `describe` JSON.

## Versioning

The crate version follows the **protocol**, not the nano-ros release:

- While the crate is `0.x`, the minor number moves exactly when the set of
  protocol versions it speaks (`protocol::SUPPORTED`) gains or drops one.
  `0.1.x` speaks protocol 1.
- A patch release never changes what goes over the wire. It may add helpers,
  fix bugs, or add a plan field that a protocol-1 reader ignores (plan readers
  ignore unknown fields by design).
- A change a runner must understand is a new protocol version, and so a new
  minor release.

nros refuses a runner it shares no protocol version with, and names both
sides.

## Status

Prepared for crates.io, not yet published. Publication follows once protocol
1 has carried the first-party runners (nano-ros phase-528 and phase-529).
Until then, depend on it by path or git from the nano-ros repository.

## License

MIT OR Apache-2.0.
