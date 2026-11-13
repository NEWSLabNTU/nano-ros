# Use the starter image

<!-- type: how-to · level: 1 -->

> **Not published yet.** Until the image is made public, `docker pull` below
> fails with `denied`. Install `nros` on your own machine meanwhile, as
> [Install](./install.md) describes.

The starter image is a container with nano-ros already installed: the `nros`
command, ROS 2 Humble with its Zenoh router, the toolchains for the Linux host
and the emulated `mps2-an385` boards (bare-metal and FreeRTOS), Rust, and the
crates the first projects build from. Pulling it is the whole install, and a
project builds in it with no network. It suits a first look, a lab with many
seats, a machine behind a firewall, and a team that wants its own image on
top.

The image is `ghcr.io/newslabntu/nano-ros-starter`. Each release of `nros`
publishes a tag of the same version, and that tag never changes.

## Before you start

- **Docker,** or another engine that runs Docker images, on a Linux x86_64
  host. The image is built for amd64 only.
- **About 1.3 GB to download** and 4.5 GB of disk once unpacked.

## Try it

Pull the image and start a shell in it:

```bash probe=10
docker pull ghcr.io/newslabntu/nano-ros-starter:latest
docker run --rm -it ghcr.io/newslabntu/nano-ros-starter:latest
```

You are now the user `nros` in `/work`, the directory your projects go in.
Scaffold a talker for the Linux host, then build and run it:

```bash probe=20
source /opt/ros/humble/setup.bash
nros new my_ws --board linux --template talker --lang rust --rmw zenoh
cd my_ws
nros run my_node --expect "Publishing: 'Hello World: 1'" --timeout 300
```

```text probe-expect=20
Publishing: 'Hello World: 1'
nros run: matched after
```

`nros run` starts ROS's Zenoh router, builds the talker and runs it. None of
it reaches the network: the crates it needs are already in the image. With
`--expect` it stops at the first line containing that text.

Cargo runs offline in the image (`CARGO_NET_OFFLINE=true`), so a seat with no
network builds without being told to. When your project adds a dependency
from crates.io, start the container with `-e CARGO_NET_OFFLINE=false` so
cargo can fetch it.

The [Tutorial](../tutorial/run-a-talker.md) runs from here unchanged, and so
do the [Linux host](../starters/linux-host.md),
[mps2-an385 bare-metal](../starters/mps2-an385-bare-metal.md) and
[mps2-an385 FreeRTOS](../starters/mps2-an385-freertos.md) starters, minus
their install steps. To keep your work when the container stops, mount a
directory of your own at `/work`:

```bash probe=30
docker run --rm -it -v "$PWD:/work" ghcr.io/newslabntu/nano-ros-starter:latest
```

Use `latest` to try things out. Anything you want to rebuild later should
name a version instead, as the next sections do.

## Run a course on it

Every seat should open the same image for the whole semester. Pin it by
digest, which names exact bytes; a tag is only a name. Read the digest of
the version you chose:

```bash probe=40
docker pull ghcr.io/newslabntu/nano-ros-starter:<version>
docker image inspect --format '{{index .RepoDigests 0}}' \
    ghcr.io/newslabntu/nano-ros-starter:<version>
```

Then commit a `.devcontainer/devcontainer.json` to the course repository that
names it, so an editor with Dev Containers support opens every seat in it:

```json
{
  "name": "nano-ros course",
  "image": "ghcr.io/newslabntu/nano-ros-starter@sha256:<digest>",
  "workspaceMount": "source=${localWorkspaceFolder},target=/work,type=bind",
  "workspaceFolder": "/work",
  "remoteUser": "nros"
}
```

A seat that behaves differently from the others first checks it opened the
same bytes: `nros doctor` prints the image's version on its `container`
line, and the host reads the digest with the `docker image inspect` command
above. If the two disagree, the seat pulled something else; pull the digest
again.

**Pull before the session, not in it.** A lab link is the limit: on a
100 Mbit/s link shared by 30 seats, every seat pulling the 1.3 GB image at
once takes about 50 minutes. Pull the pinned digest on each seat the day
before, or load it from a mirror on the lab network.

## Use it with no network

On a machine with network access, save a version to a file:

```bash probe=50
docker pull ghcr.io/newslabntu/nano-ros-starter:<version>
docker save ghcr.io/newslabntu/nano-ros-starter:<version> \
    | gzip > nano-ros-starter.tar.gz
```

Move the file across, then load and run it:

```bash probe=52
docker load < nano-ros-starter.tar.gz
docker run --rm -it --network none ghcr.io/newslabntu/nano-ros-starter:<version>
```

The image builds the first projects with `--network none`; that is checked
every time it is built. A board or package the image does not carry needs
network access once, in a derived image (below), before it moves across.
When you ask for help, include the `nros doctor` output: its `container` line
names the version you ran.

## Build your team's image on top

When your team needs a board or a ROS 2 package the starter does not carry,
derive an image from it rather than installing into a running container.
`docker/starter-recipe/Dockerfile` in the nano-ros repository is a working
example: it starts `FROM` the starter, adds the `mps3-an536-freertos` board
with `nros setup`, and installs `ros-humble-nav-msgs`. Copy it and change the
two marked lines.

Name a version in `FROM`, never `latest`, and the starter promises four
things your image can rely on:

1. **A version tag never changes.** `FROM` a version gives the same bytes
   every time you build.
2. **The image matches its release.** What it has installed is what `nros
   setup` of that release would install, so `nros setup` in your image only
   adds what you ask for.
3. **The paths stay put.** `NROS_HOME` is `/opt/nros` and the working
   directory is `/work` for the whole of a major version.
4. **The recipe is tested.** Each release builds the example recipe on the
   new image.
