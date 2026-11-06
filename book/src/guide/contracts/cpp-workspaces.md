# C / C++ multi-node workspaces

<!-- type: how-to · level: 2 -->

This page now holds only the contract sections of the C and C++ workspace
walkthrough. The workspace layout, the node and bringup packages, the image
and the entry it generates moved to
[Organize a larger workspace](../build/organize-a-larger-workspace.md) and
[Mix languages](../build/mix-languages.md).

The reference C++ workspace is
[`examples/workspaces/cpp/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/workspaces/cpp).

## Contracts for a C or C++ workspace

### Telling the build what the component creates

`NROS_EXECUTOR_MAX_CBS`, the subscriber slots and the arena are `const` sizes
compiled into nros-node before a component TU is compiled, while a typed
component wires itself in its CONSTRUCTOR, at runtime. Nothing the macros emit
can supply those sizes — it exists only after linking. So somebody has to state
what the image creates, ahead of the build.

That statement is a **contract sidecar** beside the launch file, not something
in this CMakeLists. `<stem>.contract.yaml` next to `<stem>.launch.xml` in the
bringup package:

```yaml
# src/demo_bringup/launch/system.contract.yaml
version: 1

nodes:
  talker:
    paths:
      on_timer:                                  # a path with a timer trigger
        trigger: { timer: { rate_hz: 1 } }       # IS the periodic callback
        output: [chatter]
    pub:
      chatter: {}
  listener:
    sub:
      chatter: {}

topics:
  /chatter:
    type: std_msgs/msg/Int32
    pub: [talker/chatter]
    sub: [listener/chatter]
```

`nodes:` keys are LOCAL endpoint names — an absolute path there is concatenated
onto the node's FQN and matches no topic. `topics:`, `services:` and `actions:`
wire those endpoints to absolute names and carry the type. `paths:` is where
timers live: the model records a path with no `input` as the periodic callback.

**A bringup with no `launch/` writes the same file at `<bringup>/system.contract.yaml`**,
beside its `system.toml`. Its launch file is generated from the `[[component]]`
rows, so there is no `launch/` directory to put a sidecar in; the build carries
this one to where the resolver looks. That is how a one-node bringup states its
QoS — and stating it is worth real memory: one `sub: { chatter: { qos: { history:
keep_last, depth: 1 } } }` row takes a one-node Rust listener on Linux from 273,802
to 172,298 bytes of static RAM, because the zenoh backend then sizes its payload
pools for the one sample the image keeps instead of the four it might have.

The name you write must be the name the code registers, ABSOLUTELY. A contract
states RESOLVED names; a call site writing `"chatter"` or a node the launch file
remaps is one the build cannot match with certainty, so it refuses that endpoint
by name and the image keeps its configured pools rather than being sized from a
guess.

The build folds it into the resolved SystemModel, and the entry passes that
model to the sizing automatically — nothing here takes an argument for it.

The derivation is all-or-nothing per image: **a component the contract does not
describe makes that image derive nothing** and keep its configured
`NROS_EXECUTOR_MAX_CBS`. A count taken over only the components that answered
is smaller than the image needs, and a short `MAX_CBS` fails entity creation at
boot — so "some of them" is refused rather than guessed at.

Note that a publisher is inventoried but claims no callback slot, so the entity
count and the derived `MAX_CBS` are legitimately different numbers.

> Earlier versions stated this per component, as
> `nano_ros_node_register(ENTITIES ...)`. That keyword is retired
> and now fails with a pointer here. It was a list beside the code with nothing
> comparing the two, and on the safety island one of them declared six
> subscriptions for a node that creates seven: every pool derived from it was
> short by one, and the eleventh subscription failed at boot with a transport
> error that named nothing.

#### The census: the contract checked against the code

Since the contract is still a statement beside the code, a CROSS image (any
image whose board is not `linux`) checks it before it configures: the host
image generated from the same launch file is run once in census mode — it
constructs every component against a recording backend, writes what they
created, and exits without spinning or dialling a router — and the configure
compares that census with the contract row by row. An endpoint the code creates
and the contract omits refuses as `missing-in-contract` (not waivable: it is
the short pool); one the contract declares and the code never creates refuses
as `phantom` (waivable per row under `[census.waive]`).

The configure never builds the native image itself. `nros build` does: before
it plans a cross image, it takes that image's census. Taking it does nothing
when there is nothing to do — a model with no contract, or a census that still
matches the code (freshness is by content: touching a file changes nothing,
adding a subscription does). A build driven some other way (a hand-run
`west build` or `cmake`) with no census, or a stale one, REFUSES at configure
and prints the command that takes it; running `nros build` once for the image
takes it too. A bringup that wants the old
behaviour writes `[census] on_missing = "warn"` / `on_stale = "warn"` in its
`system.toml`, and gets a CMake WARNING instead.

## Design background

- [RFC-0122](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0122-behaviour-bringups-road-witnesses-cadence-backed-ci.md): behaviour bringups, single-node witnesses, and cadence-backed CI.
  The paired examples as image rows of behaviour bringups in the language
  workspaces.
