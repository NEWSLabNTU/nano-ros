# Find what nano-ros offers

<!-- type: how-to · level: 1 -->

**Needs first:** [Install](../../start/install.md)

The `nros` command lists what you can use and shows one item in full: the
RMW backends, the boards, the project templates, the settings an image
takes, the run and flash runners, and the diagnostic codes. You do not need
to read the source or a list in this book to know what exists, and what the
command lists is what the installed release has.

Every list works the same way:

- `nros <noun> list` prints one row per item;
- `nros <noun> show <name>` prints one item in full, with a link to its page
  in this book;
- `--json` on either prints the same answer for a script;
- a name it does not know is refused with the nearest names it does.

## Steps

### See the RMW backends

```bash probe=3110 track=installed
nros rmw list
```

```text
NAME        HOST PEER     SUMMARY
cyclonedds  none          The Cyclone DDS RMW backend for nano-ros — full DDS/RTPS, …
uorb        none          The uORB RMW backend for nano-ros — PX4's in-process message bus.
xrce        xrce-agent    The Micro XRCE-DDS RMW backend for nano-ros — client/agent …
zenoh       zenoh-router  The zenoh RMW backend for nano-ros (zenoh-pico on embedded targets).
```

Each row names what the backend needs running on the host (its peer) and,
on a second line, the boards a test runs it on. Show one in full:

```bash probe=3120 track=installed
nros rmw show zenoh
```

```text
zenoh
  The zenoh RMW backend for nano-ros (zenoh-pico on embedded targets).

  also named        rmw-zenoh
  selected by       rmw = "zenoh"
  host peer         zenoh-router
…
```

`selected by` is the line you write in an image's `system.toml` to use it.

### See the boards

```bash probe=3130 track=installed
nros board list
```

One row per board key, with its platform and its support status. `nros
board show <board>` prints everything the board's definition states: how it
runs, how it flashes, its console and its network.

### See the project templates

```bash probe=3140 track=installed
nros template list
```

```text
TEMPLATE       INTERFACES          WHAT IT IS
talker         std_msgs            publishes std_msgs/String on /chatter at 1 Hz
listener       std_msgs            subscribes to std_msgs/String on /chatter
server         example_interfaces  serves example_interfaces/srv/AddTwoInts on /add_two_ints
client         example_interfaces  calls example_interfaces/srv/AddTwoInts on /add_two_ints
action-server  example_interfaces  serves example_interfaces/action/Fibonacci on /fibonacci
action-client  example_interfaces  sends a Fibonacci goal to /fibonacci
```

Each one is a `--template` for `nros new`.

### See the settings an image takes

The settings an image can change are called knobs. List them:

```bash probe=3150 track=installed
nros knob list
```

Each row gives the knob's name, its scope (an image, or a board), its
default and what it is. Show one to see where to set it:

```bash probe=3160 track=installed
nros knob show net.max_sockets
```

```text
net.max_sockets
…
  set it in:  system.toml, [image.<id>.knobs.net] max_sockets
  or for a board:  nros-board.toml, [knobs] net.max_sockets
  scope:      board
…
```

A default that depends on the board is not a number in the list. Add
`--platform <platform>` to `nros knob list` to see each value resolved for
that platform, with where it came from.

### See the runners

```bash probe=3170 track=installed
nros runner list
```

A runner is what `nros run` and `nros flash` hand an image to: QEMU, Renode,
an Arm FVP, `west`, a serial console. `nros runner show <name>` tells how one
is used.

### Look up a diagnostic code

Errors from `nros` carry a code such as `NROS-0003`. List them, or show one
with its explanation and fix:

```bash probe=3180 track=installed
nros code list
nros code show NROS-0003
```

```text
NROS-0003: the workspace declares no image
…
## How to fix it
…
```

### See the images of a workspace

In a workspace, `nros image` lists what it builds. Make a small one:

<!-- tabs -->

#### Rust

```bash probe=3190 track=installed
nros new tour_ws --board linux --template talker --lang rust
cd tour_ws
```

#### C

```bash probe=3190 track=installed
nros new tour_ws --board linux --template talker --lang c
cd tour_ws
```

#### C++

```bash probe=3190 track=installed
nros new tour_ws --board linux --template talker --lang cpp
cd tour_ws
```

<!-- /tabs -->

```bash probe=3200 track=installed
nros image list
nros image show linux
```

```text
IMAGE  BRINGUP            BOARD  RMW    LAUNCH
linux  tour_node_bringup  linux  zenoh  -
```

`nros image show linux` prints the image's board, platform, RMW and
capabilities. After `nros configure`, `nros image show linux --derived`
also prints every value the build derived for it.

### Let a script read the answer

Every `list` and `show` takes `--json`:

```bash probe=3210 track=installed
nros rmw list --json > rmws.json
```

Each document names its schema (`nros.rmw.list.v1` here), and the schema's
keys do not change within a version.

### Complete names with Tab

```bash probe=3220 track=installed
nros completions bash
```

```text
# Turn on `nros` completion: add this line to ~/.bashrc
source <(COMPLETE=bash nros)
```

It prints the line that turns on completion, and the file it goes in. Name
`zsh` or `fish` instead of `bash` for those shells, or name none to use the
shell `$SHELL` names. With it, `nros rmw show <Tab>` offers the backends, `nros board
show <Tab>` the board keys and `nros knob show <Tab>` the knobs.

## If a name is refused

A misspelled name is refused with the nearest one:

```text
Error: no RMW named `zenhoh`; did you mean `zenoh`?
  `nros rmw list` names every one
```

## See also

- [CLI reference](../../reference/cli.md): every command and option.
- [Configure an image](configure-an-image.md): where the settings `nros knob`
  lists are written.
