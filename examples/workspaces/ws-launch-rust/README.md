# ws-launch-rust — advanced launch composition (phase-263 B5)

A minimal product-shaped nano-ros workspace whose topology lives in **launch
XML**, exercising the nano-ros launch v1 surface end-to-end.

## What the launch demonstrates

`src/demo_bringup/launch/system.launch.xml`:

| Feature | Used as |
| --- | --- |
| `<arg name= default=>` | `robot_ns` (default `alpha`), `chatter_topic` (default `chatter`) |
| `$(var …)` substitution | `<group ns="$(var robot_ns)">`, remap target |
| `<group ns=…>` | namespaces the talker under `/alpha` |
| `<node>` + child `<param>` | `rate_hz` on the talker |
| `<remap from= to=>` | remaps the talker's relative `chatter` publisher |
| `<include file=>` + `<arg value=>` | pulls in `sensors.launch.xml`, forwarding `robot_ns` |

`sensors.launch.xml` is the included sub-launch — it wraps the listener in a
`<group ns=…>` under the forwarded `robot_ns`.

The two Node pkgs (`talker_pkg`, `listener_pkg`) use **relative** topic names
(`chatter`, `heard`) so the group namespace + remap have something to act on. The
node logic is intentionally plain — the launch layer is the subject here.

All of the above resolves in the launch **record** (`build/<sys>/nros/record.json`
carries `robot_ns = alpha`, the remap, and the `rate_hz` param), and `nros::main!`
links both resolved nodes. Note: lowering a `<group ns=…>` into the per-node
namespace of the downstream orchestration IR (`nros-plan.json`) is not yet wired —
the IR currently normalizes the node namespace to `/`. So this workspace
demonstrates advanced-launch **parse + resolve + build**; full group-ns runtime
placement is a planner-maturity item.

## Build & run

```sh
nros ws sync
cargo build -p native_entry        # macro resolves the whole launch tree at build time
nros plan demo_bringup             # inspect the fully-resolved, namespaced topology
```

## Not covered (launch v1 limits)

- `if=` / `unless=` conditionals — v1 has no conditional attributes.
- `$(env …)` — supported by the parser, but an unset var is a build-time error
  (the macro resolves substitutions at compile time), so it is omitted from the
  committed launch.
- Nested substitutions (`$(find $(var pkg))`) — not supported in v1.
