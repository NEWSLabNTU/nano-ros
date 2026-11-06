# Contracts for a bringup with no launch file

<!-- type: how-to · level: 2 -->

**Needs first:** [Project layout](../../tutorial/grow-your-workspace.md) ·
[the contract walkthrough](../../tutorial/tell-the-build.md#write-the-contract)

Write a contract for a bringup that has no `launch/` directory when you want
its image sized from what it creates, or its QoS declared, without adding a
launch file.

A bringup with no `launch/` runs the nodes its `system.toml` lists in
`[[component]]` rows, and the build generates its launch file from those rows.
A contract normally sits beside the launch file it describes, but a generated
launch file lives under `build/`, where you cannot author anything. So you
write the contract beside `system.toml` instead, as `system.contract.yaml`, and
the build carries it to the generated launch file.

## Before you start

- A bringup with `system.toml`, at least one `[[component]]` row, and no
  `launch/` directory, that builds. If the bringup has a `launch/` directory,
  this page does not apply: write `launch/<stem>.contract.yaml` beside the
  launch file instead.
- The name each node registers its endpoints on, written as an absolute name
  at the call site.

## Steps

1. **Find each node's name.** In the generated launch file, a node's name is
   its `[[component]]` row's `name`, so that is the key you use under
   `nodes:`.

<!-- tabs -->

#### Rust

```toml
[[component]]
pkg = "native_listener"
class = "native_rs_listener::Listener"
name = "listener"
```

#### C

```toml
[[component]]
pkg = "native_c_listener"
class = "native_c_listener::Listener"
name = "listener"
```

#### C++

```toml
[[component]]
pkg = "cpp_listener"
class = "cpp_listener::Listener"
name = "listener"
```

<!-- /tabs -->

2. **Write `system.contract.yaml` beside `system.toml`.** Key `nodes:` by the
   `name` from step 1 and each endpoint by its local name; wire it under
   `topics:` to the absolute name the code registers:

   ```yaml
   version: 1

   nodes:
     listener:
       sub:
         chatter:
           qos:
             history: keep_last
             depth: 1

   topics:
     /chatter:
       type: std_msgs/msg/String
       sub: [listener/chatter]
   ```

   For a C or C++ node, state the depth its call site passes: a C or C++
   registration whose depth differs from the contract's is refused
   ([Declare QoS in a contract](declare-qos-in-a-contract.md)).

3. **Build the image.**

   ```bash
   nros build linux
   ```

   The build generates the launch file and copies your contract beside it. In
   a workspace, both are under `build/nros/launch/<bringup>/launch/`:

   ```bash
   ls build/nros/launch/<bringup>/launch/
   ```

   ```text
   system.contract.yaml
   system.launch.xml
   ```

   For a single package that holds its own `system.toml`, the directory is
   `<package>/build/nros/launch/<package>/`. The copy is rewritten only when
   your file changes, and deleted when you delete yours.

4. **Check the resolve line.** It reports the counts the contract produced:

   ```text
   nros build:   resolved → <workspace>/build/<bringup>__linux/resolved.toml (max_cbs <n>, nodes <n>, digest <digest>)
   ```

   `(entity counts not derived; configured defaults kept)` means some
   component is not described: see
   [Fix a contract refusal](fix-a-contract-refusal.md).

## Example

[`examples/linux/rust/listener`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/linux/rust/listener)
is a single-package Rust listener with no `launch/`. Its `system.toml` holds
the `[[component]]` row from the Rust tab above and an `[image.linux]`, and
its `system.contract.yaml` is the file from step 2. The code registers the
subscription on `/chatter`, the absolute name the contract states:

```rust
node.create_subscription_for_callback_name::<StringMsg>("on_chatter", "/chatter")?;
```

The contract's one line of QoS takes the image's static RAM on Linux from
273,802 to 172,298 bytes, because the zenoh backend then sizes its payload
pools for one sample instead of four. The listener still receives every
message a 1 Hz talker publishes.

## If it doesn't work

- **The build ignores `system.contract.yaml`.** The bringup has a `launch/`
  directory, so its own launch files are used and nothing is generated or
  carried. Move the contract to `launch/<stem>.contract.yaml`, beside the
  launch file it describes.
- **The build warns that the bringup has no `launch/` directory and no
  `[[component]]` row.** The warning reads:

  ```text
  nros: warning: <bringup>/system.toml: this bringup has no `launch/` directory and no `[[component]]` row, so nothing says what it runs and no system model is resolved for it. Add a `[[component]]` row (pkg, class, name) to system.toml, or add `launch/system.launch.xml`.
  ```

  With no row, there is no launch file to carry the contract to. Add the row.
- **A node's endpoints are refused, or never matched.** The `nodes:` key is not
  the row's `name`, or the code registers a relative name. Use the row's
  `name`, and an absolute name at the call site. See
  [Fix a contract refusal](fix-a-contract-refusal.md) for the messages.

## See also

- [Size an image from its contract](size-an-image-from-its-contract.md): what
  the counts size, and how to measure the result.
- [Declare QoS in a contract](declare-qos-in-a-contract.md).
- [Contract format reference](https://github.com/NEWSLabNTU/ros-launch-manifest/blob/v0.3.12/docs/format-reference.md):
  every contract key, generated from the grammar this release pins.
