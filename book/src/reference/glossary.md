# Glossary

<!-- type: reference · level: 3 -->

The nano-ros terms this book uses, each in a sentence or two taken from the
page that explains it, with a link to that page. A term ROS 2 uses the same
way (node, topic, QoS, launch file) is not listed; see the
[ROS 2 documentation](https://docs.ros.org/en/rolling/Concepts.html).

| Term | Meaning | Explained in |
| --- | --- | --- |
| board descriptor | The board package's `nros-board.toml`: the facts true of every copy of a board, such as its name, platform, target triple and how it boots, and its `[knobs]`. | [How a build becomes an image](../background/how-a-build-becomes-an-image.md) |
| board package | The layer that joins a platform to real hardware: it brings up the clock, the network interface and its drivers, and provides the entry point that starts the scheduler or the main loop. | [How a build becomes an image](../background/how-a-build-becomes-an-image.md#the-layers-in-an-image) |
| board registry | The one place a board's facts and support evidence are recorded. The board pages, the support levels and their status words are generated from it. | [Board Support Levels](board-support-levels.md) |
| bringup package | Says what runs and where. It holds a `package.xml` and a `system.toml`, and optionally `launch/` and `config/`; it has no `Cargo.toml`, no `CMakeLists.txt` and no code. | [Organize a larger workspace](../guide/build/organize-a-larger-workspace.md) |
| census | The host image generated from a launch file, run once in census mode: it constructs every component against a recording backend and writes what they created. The configure compares it with the contract row by row. | [Contracts for a C or C++ workspace](../guide/contracts/cpp-workspaces.md#the-census-the-contract-checked-against-the-code) |
| command package | An ordinary package in your workspace, with a `package.xml` and a small descriptor, that adds a verb to `nros`. `nros help --all` lists each extension verb with the package that provides it. | [Runner and command packages](../extending/runner-and-command-packages.md#command-packages) |
| component | A `[[component]]` row in `system.toml`: which node package runs in the bringup. | [Add a second node](../tutorial/add-a-second-node.md) |
| contract | The `<stem>.contract.yaml` beside a launch file (or `system.contract.yaml` in a bringup with no `launch/`) that states what the image creates, ahead of the build. The build sizes the image's pools from it. | [Contract format reference](contract-format.md) |
| generated entry | The program's entry (its `main()`), which `nros build` and `nros run` generate under `build/` for each `[image.*]` block, so there is no package to write for it. | [What you just built](../tutorial/what-you-just-built.md) |
| host | A `[host.<name>]` block in `system.toml`: which machine a node runs on, with `launch` and `nodes` keys. An image needs no host, and a host needs no image. | [Organize a larger workspace](../guide/build/organize-a-larger-workspace.md) |
| image | One program: the bringup's nodes, built for one board with one RMW. It is an `[image.<id>]` block in `system.toml`, not a package. | [What you just built](../tutorial/what-you-just-built.md) |
| knob | A build-time setting, such as a pool size, set on the image (`[image.<id>.knobs.<group>]`), by the board descriptor's `[knobs]`, or by an exported variable. `nros config explain` prints each knob's value and which rung set it. | [Configure an image](../guide/build/configure-an-image.md#per-image-build-knobs) |
| level | A rank the build derives from the contract: each node's rank comes from its trigger rate, and each distinct rank is one level, up to a cap that depends on the platform. | [The execution model](../background/execution-model.md#sched-tasks-and-levels-preemption-from-the-kernel) |
| link | How an image's middleware reaches the rest of the system. `nros` reads it, with the launcher or flasher and the console, from the board's descriptor. | [Deploy to production](../guide/deploy/deploy-to-production.md#the-target-model) |
| locator | The address an image dials. On zenoh it is the only thing a board needs to find the router. | [Configure zenoh](../guide/rmw/configure-zenoh.md) |
| materialize | `nros materialize` copies an image's generated entry into `src/<image>_entry/`; from then on `nros build` uses your copy and does not regenerate it. The last resort. | [Organize a larger workspace](../guide/build/organize-a-larger-workspace.md#when-the-generated-entry-is-not-enough) |
| node package | A library that says what a node does: its publishers, subscriptions, timers and callbacks. It has no `main()`, no board and no deployment settings, so one node package runs on every board. | [Organize a larger workspace](../guide/build/organize-a-larger-workspace.md) |
| runner package | An ordinary package in your workspace that teaches `nros run`, `nros flash` and `nros monitor` to drive a launcher, flasher or console nano-ros does not ship a runner for. | [Runner and command packages](../extending/runner-and-command-packages.md#runner-packages) |
| sched task | One executor on one RTOS thread, with its own priority and spin period. All of an image's sched tasks share one middleware session; parallelism is a sched task, not a callback group. | [Pick a scheduling model](../guide/realtime/pick-a-scheduling-model.md) |
| static pool | An array of slots, carved at build time, that holds the runtime's entities with no allocator. Every slot costs its bytes whether an entity uses it or not, so each pool is sized from what the image declares. | [Static memory and pools](../background/static-memory-and-pools.md) |
| status word | A board's support level, derived from evidence and never typed: where the board's image runs, how often that run happens, and which board's test matrix stands for its breadth. The words and what each means are listed on the generated support-levels page. | [Board Support Levels](board-support-levels.md) |
| twin | An emulated model of a real board. A board with a hardware twin runs the same firmware in an emulator first and then on the board. | [Welcome](../start/welcome.md) |
| witness | A board that runs its platform family's test matrix. A board that inherits shares the witness's code path and adds its own smoke evidence. | [Board Support Levels](board-support-levels.md) |
