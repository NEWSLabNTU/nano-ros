# Fix a contract refusal

<!-- type: how-to · level: 2 -->

**Needs first:** [the contract walkthrough](../../tutorial/tell-the-build.md#write-the-contract) ·
[Size an image from its contract](size-an-image-from-its-contract.md)

Use this page when `nros build` refuses a contract, or builds the image without
sizing it from the contract, and you need to know which statement is wrong.

The contract is a statement about the code, made beside it. The build checks it
at several points, and each check refuses rather than guesses, because a pool
sized from a wrong statement fails at boot on a board that may have no
console. Find the message you have below; each section names the fix. In the
messages, angle brackets stand for your own names.

## Before you start

- The full output of the `nros build <image>` that failed.
- The contract it read: `<bringup>/launch/<stem>.contract.yaml`, or
  `<bringup>/system.contract.yaml` for a bringup with no `launch/`.

## Steps

1. **Rebuild the image and keep the output.**

   ```text
   nros build <image>
   ```

2. **Find the first refusal in the output** and match it to a section below.
   Fix the contract or the code, then rebuild.

### The contract does not parse

The contract grammar refuses any key it does not list:

```text
at '<path>': unknown key in `qos` — did you mean `depth`? An unrecognised key used to be discarded in silence, which deleted whatever it was meant to declare
```

Correct the spelling; the [contract format reference](https://github.com/NEWSLabNTU/ros-launch-manifest/blob/v0.3.12/docs/format-reference.md)
lists every key. An endpoint key holding a `/` is refused the same way:

```text
endpoint key `/chatter` contains `/`: an endpoint key is a local name, and the node's FQN is prefixed to it (`/chatter` would become `<node>//chatter`). Use the endpoint's local name; remap the topic in the launch file or wire it under `topics:`
```

Key the endpoint by its local name (`chatter`) and put the absolute name under
`topics:`. Once a contract has been refused, later commands refuse the model it
left behind until a resolve succeeds:

```text
SystemModel `<model>` was refused by its producer and cannot be trusted (marker `<marker>`):
  check: <check>
  input: <input>
  Fix the input and run `nros configure`; the next successful resolve removes the marker.
```

Fix the contract and run `nros configure` (or `nros build`) again.

### The contract and a `qos_overrides` parameter disagree

```text
<n> QoS statement(s) disagree between the launch contract and the `qos_overrides.*` parameters.
```

Each disagreement that follows names the topic, the parameter and its node,
and the contract row, or says the row `states no` such policy. Make the
contract state the value the parameter sets. The parameter configures the
runtime and the contract sizes the buffers, so the two must agree.

### The image was built with its configured defaults

The resolve line says the contract sized nothing:

```text
nros build:   resolved → <workspace>/build/<bringup>__<image>/resolved.toml (entity counts not derived; configured defaults kept)
```

The reason is the `refused` line under `[provenance]` in that `resolved.toml`.
The most common one:

```text
<n> of <m> components in this image declare no entities:
    <pkg>::<component> (<class>)
Deriving over only the components that did would publish a slot count smaller than the image needs, and a short NROS_EXECUTOR_MAX_CBS fails entity creation at boot.
```

Sizing is all or nothing per image. Declare every listed component under
`nodes:`, keyed by its node name. If the reason begins `the launch tree
resolved, and it declares neither wiring nor parameters`, the build found no
contract at all: check its file name and where it sits.

### An endpoint is not matched

On a Rust image the build prints one line per endpoint it could not match to
the contract, for example:

```text
nros build:   subscription `chatter`: this subscription registers on `chatter`, which is not an absolute name. A contract states RESOLVED names, and resolving `chatter` needs the node's namespace AND its remappings -- neither of which reaches this reader per endpoint. Write the name absolutely at the call site, or declare the endpoint the contract already names
```

Write the name absolutely at the call site (`"/chatter"`). A node the launch
file remaps is refused too (`the launch description applies name REMAPPINGS to
node`): drop the remap, or accept that the endpoint keeps its worst-case
sizes. A line beginning `no subscription in this image's
contract carries type` names what the contract does describe; correct the type
or the topic.

### The census check refuses a cross image

Before it builds an image for a board other than the host, `nros build` runs
the host image generated from the same launch file once, records what every
component creates, and compares that census with the contract, row by row:

```text
error missing-in-contract <node> sub /chatter
    the code creates this subscription on `/chatter`; no `sub:` row of `nodes.<node>` is wired to it, so no `topics:` entry names it either
    in <contract>, add under `nodes.<node>.sub` (the alias is the author's; `chatter` is this name's last segment):
        chatter: { qos: { depth: <depth> } }
    and wire it under `topics`:
        /chatter:
          type: <package>/msg/<Name>
          sub: [<node>/chatter]
    not waivable: every pool derives one short; the first catch is ExecutorFull on a board with no console
census check: <n> confirmed, <n> error(s), <n> warning(s), <n> waived
```

Each row gives the edit. The verdicts:

| Verdict | Means | Fix |
| --- | --- | --- |
| `missing-in-contract` | the code creates it, the contract does not declare it | add the row the message prints |
| `phantom` | the contract declares and wires it, the code never creates it | remove the row, or waive it |
| `unwired` | declared under `nodes:` but named by no `topics:`, `services:` or `actions:` entry | wire it or remove it |
| `depth-mismatch`, `qos-mismatch`, `type-mismatch` | both sides name it and disagree | change one side |
| `period-mismatch` | a timer's period differs from its path's `rate_hz` | change one side |
| `param-missing-in-contract`, `param-phantom`, `param-type-mismatch` | the same, for a node's `params:` | as above |

Only `phantom` and `param-phantom` rows are waivable, one row at a time, in the
bringup's `system.toml`. The message prints the key to copy:

```toml
[census.waive."<node>:sub:/chatter"]
reason = "..."
```

A refusal that begins `census missing:` or `census stale:` means the census is
absent or older than the code. Run `nros build <image>` for the cross image;
it takes the census first. To take the census without building the cross
image, run `nros configure --census <bringup>:<image>` in the workspace, with
the same cross image. Both build and run the host image, so they take longer
than a plain `nros configure`, which only reports that the census is stale.
If either reports that the bringup `declares no HOST
image resolved from the same launch and arguments`, add one, such as
`[image.linux] board = "linux"` with the same `launch` and `args`.

## Example

The C++ workspace in
[`examples/workspaces/cpp`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/workspaces/cpp)
declares both its nodes in `src/demo_bringup/launch/system.contract.yaml`.
Delete the listener from it:

```yaml
version: 1

nodes:
  talker:
    paths:
      on_timer:
        trigger: { timer: { rate_hz: 1 } }
        output: [chatter]
    pub:
      chatter: { qos: { depth: 1 } }

topics:
  /chatter:
    type: std_msgs/msg/String
    pub: [talker/chatter]
```

The listener's code still subscribes to `/chatter`, so building the
`freertos` image (board `mps2-an385-freertos`) is refused by the census: one
`missing-in-contract` row for `listener sub /chatter`, whose remedy is the
`listener:` entry with `chatter: { qos: { depth: 1 } }` and the
`sub: [listener/chatter]` wiring. Put them back and the census confirms every
row.

## If it doesn't work

- **A configure stops with `ENTITIES was retired`.** A `CMakeLists.txt` still
  passes `ENTITIES` to `nano_ros_node_register`. Delete the argument and state
  the component's entities in the contract.
- **`[census] on_missing = "warn"` or `on_stale = "warn"` is set.** The bringup
  has opted out of the refusal, and the build prints a `census missing
  (WARNING` line instead. Remove the key to make it a refusal again.

## See also

- [Declare QoS in a contract](declare-qos-in-a-contract.md): the compile-time
  and boot-time QoS checks for each language.
- [Contract format reference](https://github.com/NEWSLabNTU/ros-launch-manifest/blob/v0.3.12/docs/format-reference.md):
  every contract key, generated from the grammar this release pins.
