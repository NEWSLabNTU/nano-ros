# Choose your path

<!-- type: how-to · level: 1 -->

Pick the card that matches your work and follow its route in order. Each
step needs only the steps above it, so you never read a page that assumes
one you skipped. Each card also states its budget: the time or the property
the route is written and tested to meet.

Every route starts with [Install](install.md). If you are not sure which card
fits, start with Education: its first chapters are the common start of every
other route.

<style>
.nros-path-card { border: 1px solid var(--table-border-color); border-left: 4px solid var(--links);
  border-radius: 6px; padding: 0.1em 1.2em 0.4em; margin: 1.4em 0; }
.nros-path-card h2 { margin-top: 0.6em; }
</style>

<div class="nros-path-card">

## Education

**For:** instructors and students in a course or lab on robotics or
embedded systems.

**Budget:** a 15-minute install on each seat, labs that fit a two-hour
session, and the same result on every seat.

**Route:**

1. [Install](install.md) on each seat.
2. [Run a talker on an emulated board](../tutorial/run-a-talker.md). The
   output it prints is the result to check on every seat.
3. [What you just built](../tutorial/what-you-just-built.md): the node
   package and the workspace around it.
4. [Add a second node](../tutorial/add-a-second-node.md), in Rust,
   [C](../tutorial/add-a-second-node-c.md) or
   [C++](../tutorial/add-a-second-node-cpp.md).
5. [Pick and configure the RMW](../tutorial/pick-the-rmw.md). Cyclone DDS
   needs no router, which keeps a lab machine simple.
6. [Your own message](../tutorial/your-own-message.md).
7. Services and actions: the `server`, `client` and action templates in
   [the C node chapter](../tutorial/add-a-second-node-c.md#the-node-package).
8. [Parameters and launch files](../tutorial/grow-your-workspace.md).
9. The lab's board: its starter in the
   [list of starters](../starters/pick-your-board.md), such as
   [Nucleo-F767ZI](../starters/nucleo-f767zi.md), or its entry in
   [Supported Boards](../reference/supported-boards.md).

</div>

<div class="nros-path-card">

## Field solutions

**For:** engineers and technicians who build a system for each customer,
often on a board they were handed.

**Budget:** 30 minutes from an unopened box to `ros2 topic echo` showing the
board's data.

**Route:**

1. [Install](install.md).
2. Your board's starter, from the
   [list of starters](../starters/pick-your-board.md). The boards with an
   emulated twin come first, such as the
   [Nucleo-F767ZI](../starters/nucleo-f767zi.md) and the
   [Raspberry Pi Pico W](../starters/rpi-pico-w.md): the starter runs the
   twin first, then flashes the board itself. A board with no starter has a
   page in [Supported Boards](../reference/supported-boards.md) that says
   what nano-ros can show for it.
3. [Talk to ROS 2](../tutorial/talk-to-ros2.md): the board's topics in your
   ROS 2 graph, in both directions.

</div>

<div class="nros-path-card">

## Industrial

**For:** teams who build products where safety and robustness matter.

**Budget:** reproducible, auditable and offline. The same inputs give the
same image, every fact about a board is on a page, and the build runs with no
network.

**Route:**

1. [Install](install.md).
2. Background: [the execution model](../background/execution-model.md),
   [static memory and pools](../background/static-memory-and-pools.md) and
   [safety](../background/safety.md).
3. User guide: [pin a release and work offline](../guide/build/install-options.md#pinning-a-version),
   [build inside a vendor tree](../guide/build/existing-cmake-or-vendor-tree.md)
   for air-gapped CI, and [deploy to production](../guide/deploy/deploy-to-production.md).
4. Reference: [board support levels](../reference/board-support-levels.md)
   and [the configuration keys](../reference/configuration-surface.md).
   `nros doctor --report` prints the support level of every board your
   images name, as a report you can file with a review.

</div>

<div class="nros-path-card">

## ROS 2 developer porting nodes

**For:** developers with existing `rclcpp` nodes who want them on a
microcontroller.

**Budget:** Install's 15 minutes, then the first chapter's run. The port
itself depends on your node, so this route sets no time for it.

**Route:**

1. [Install](install.md).
2. [Run a talker on an emulated board](../tutorial/run-a-talker.md), to see
   the toolchain work before your own code is in it.
3. [How nano-ros differs from ROS 2](../background/differences-from-ros2.md).
4. [Port a C++ node](../guide/port/port-a-cpp-node.md).
5. [Move a ROS 2 project over](../guide/port/move-a-ros2-project.md).

</div>

<div class="nros-path-card">

## Board porter

**For:** developers who bring nano-ros to a board, a platform or a
transport it does not support yet.

**Budget:** Install's 15 minutes, then the first chapter's run. Porting is
development work, so this route sets no time for it.

**Route:**

1. [Install](install.md).
2. [Run a talker on an emulated board](../tutorial/run-a-talker.md) and
   [What you just built](../tutorial/what-you-just-built.md).
3. [How a build becomes an image](../background/how-a-build-becomes-an-image.md).
4. Extending nano-ros: [Add a board](../extending/add-a-board.md). A board of yours
   lives in your own tree.
5. [nano-ros Internals](../internals/orientation/start-here.html), when the
   port needs a change inside nano-ros itself, such as a new platform.

</div>

## If none of these fit

Read [Welcome](welcome.md) for what nano-ros is and what it is not, then
follow the Education route as far as you need. Its first two chapters are the
shortest way to see nano-ros run. If a step fails,
[Troubleshooting](../reference/troubleshooting-index.md) lists the problems
readers meet in their first ten minutes.

## Design background

The three purposes and their budgets are RFC-0116 D1. The five cards are
RFC-0124 D10.
