# Deploy to production

<!-- type: how-to · level: 3 -->

**Needs first:** [Configure an image](../build/configure-an-image.md)

Use this page to take an image from a workspace to the machine or board it
runs on, verify it from ROS 2, and check it is ready to ship.

The order is the same on every target: build the image, put it where it
runs, then verify ROS 2 communication. On a host or an emulator `nros run`
does the last two; on hardware `nros flash --monitor` does.

## Before you start

- A workspace whose images build, and the boards provisioned
  (`nros setup` at the workspace root).
- For a zenoh image, ROS 2's router; see
  [ROS 2 and the zenoh router](../build/install-options.md#ros-2-and-the-zenoh-router).

## Steps

1. Build every image the workspace declares:

   ```bash
   nros build --all
   ```

   After each image, `nros build` prints the artifact and the board's own
   next command. Where each target family's artifact lands is in
   [Build outputs](../../reference/build-outputs.md).

2. Check how the image will run. `--dry-run --why` prints the run plan, and
   where each fact came from, without running anything:

   ```bash
   nros run host_node_bringup:linux --dry-run --why
   ```

   ```text
   image = "host_node_bringup:linux"
   verb = "run"

   [runner]
   package = "nros_runner_native"
   name = "native"

   [console]
   kind = "stdio"

   [link]
   kind = "tcp"
   locator = "tcp/127.0.0.1:7447"
   rmw = "zenoh"
   ```

3. Run a host or emulator image, with a smoke test. `nros run` builds it if
   it is stale, starts the router its link needs, streams the console, and
   with `--expect` exits successfully when the line appears:

   ```bash
   nros run host_node_bringup:linux --expect 'Publishing:'
   ```

4. Flash a real board, and watch its console on the same port:

   ```bash
   nros flash <bringup>:<image> --monitor --expect 'Publishing:'
   ```

   `nros flash` runs the board's human steps (a jumper, a reset), flashes
   through the board's flash method, and watches the console. `nros monitor`
   attaches to a board that is already running. A board with no flash method
   stays on the vendor's own tools.

5. Verify from ROS 2. With the router running and ROS 2 sourced:

   ```bash
   export RMW_IMPLEMENTATION=rmw_zenoh_cpp
   ros2 topic list
   ros2 topic echo /chatter std_msgs/msg/String --qos-reliability best_effort
   ```

   If discovery works but samples do not arrive, check the domain ID, the
   router endpoint, QoS reliability and the board's network.

6. Check readiness for production, below, on your own board.

### Check readiness for production

The in-tree tests run reference boards, mostly under emulation, where cycle
counters read zero and wall-clock percentiles track the host's load. What you
ship has to be measured on your silicon, at its clock and load. Count the
boxes per group; a group missing two or more is not ready.

**Timing, measured on the board**

- [ ] End-to-end P99 latency, publisher to callback, within your budget.
- [ ] Worst-case stack depth per sched task
      ([Analyze timing](../realtime/analyze-timing.md)), and the platform's
      stack-overflow detection on (FreeRTOS `configCHECK_FOR_STACK_OVERFLOW`,
      Zephyr stack sentinels, NuttX `CONFIG_DEBUG_STACK`).
- [ ] Timer overruns under sustained load
      ([timer overrun policy](../realtime/pick-a-scheduling-model.md)).
- [ ] Heap use over 24 hours at nominal load, and no allocation on a hot
      path.

**Platform and network**

- [ ] Multicast discovery fires on your RTOS and driver, if you use Cyclone
      DDS.
- [ ] The platform clock extends a narrow hardware counter to a monotonic
      64-bit value (a 32-bit millisecond count wraps at 49.7 days).
- [ ] The image recovers from 5% packet loss, a 30-second partition, and a
      restart of the router or agent.
- [ ] Critical sections are short.

**Middleware and interop**

- [ ] The release pinned with `nros pin`, so the backend versions are the
      tested ones (`nros setup --list` prints them).
- [ ] Every QoS policy you use is supported by your backend; an unsupported
      one is reported at entity creation.
- [ ] A stock ROS 2 node receives what the image publishes, with each custom
      message, and QoS profiles match.
- [ ] Every nano-ros RTOS in your fleet sees every other.

**Safety and operations**

- [ ] A timeout on every blocking call; nano-ros has no wait-forever form.
- [ ] The E2E CRC attached, if you use the `safety-e2e` feature
      ([Safety](../../background/safety.md)).
- [ ] A hardware watchdog wired; the executor reports overruns but pets
      none.
- [ ] A log sink chosen ([Log and observe a board](log-and-observe.md)),
      time synchronization, and a bootloader and update strategy, none of
      which nano-ros ships.
- [ ] The licenses accepted: nano-ros is MIT OR Apache-2.0; zenoh-pico,
      Cyclone DDS and Micro-XRCE-DDS-Client carry their own. nano-ros has no
      commercial support entity, so plan for in-house expertise.

## Example

A workspace with a host image and a board image, built, smoke-tested, then
flashed:

```bash
nros build --all
nros run host_node_bringup:linux --expect 'Publishing:'
nros flash <bringup>:<image> --monitor --expect 'Publishing:'
```

## The target model

> This section collects the target-model pages that were folded in here. The
> platform pages and "Flash a real board" will take them over.

Every place an image can run is described the same way: a **launcher or a
flasher** (a host process, an emulator, or a tool that programs a board), a
**console** (where the image's output appears), and a **link** (how its
middleware reaches the rest of the system). `nros` reads these from the
board's descriptor, so the same commands work on every board.

- `nros run` launches an image on the host or in an emulator.
- `nros flash` programs a real board, and `nros monitor` attaches to its
  console.
- `nros bench` records which board on your machine is on which port.

See the [`nros` CLI reference](../../reference/cli.md) for each verb.

### Running an image: `nros run`

`nros run` builds the image if it is stale, starts what its link needs, and
streams the console, for host images and emulators. `--expect '<line>'` exits
successfully when that line appears. On a real board it refuses and names the
flashing step instead.

### Run plans

Before it runs anything, `nros run` resolves a **run plan**: which launcher or
flasher, which console and which link, and where each fact came from. The
facts come from five levels, highest first: the command line, the image, your
machine's bench file, the board's descriptor, and the runner's defaults.
`--dry-run --why` prints the plan without running it, as in step 2.

### Flashing and monitoring a board

`nros flash` programs a real board with a built image, and `nros monitor`
attaches to its console. Both read how to do it from the board's descriptor,
and an image overrides any key in its `[image.<id>.flash]` and
`[image.<id>.console]` tables. Until those verbs work for your board, flash
with the vendor's own tools.

### Your bench: `nros bench`

`nros bench scan` records which attached USB device is which board, in
`$NROS_HOME/bench.toml`, so `nros flash` and `nros monitor` find the port and
probe without being told. `nros bench list` prints the file and what is
attached now.

## If it doesn't work

- **`nros run` refuses and names a flashing step.** The image's board is
  hardware. Use `nros flash --monitor`.
- **The image starts but never prints.** A zenoh image blocks opening its
  session until it reaches the router. Check the router runs, on the
  endpoint in the run plan's `[link]`.
- **`nros flash` cannot find the board.** Run `nros bench scan` with the
  board attached, or pass `--port`.

## See also

- [Troubleshoot](troubleshoot.md): failures by symptom.
- [Build outputs](../../reference/build-outputs.md): artifacts and the
  vendor commands that take them.
- [`nros` CLI](../../reference/cli.md): every flag of `run`, `flash`,
  `monitor` and `bench`.

## Design background

- [RFC-0003: RTOS integration pattern](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0003-rtos-integration-pattern.md)
- [RFC-0065: colcon-like workspace builder](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0065-colcon-like-workspace-builder.md)
- [RFC-0075: zenoh router provenance](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0075-zenoh-router-provenance-and-the-unstable-seam.md)
- [RFC-0118: target execution model](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0118-target-execution-model.md)
