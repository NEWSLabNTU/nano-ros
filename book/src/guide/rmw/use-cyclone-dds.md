# Use Cyclone DDS

<!-- type: how-to · level: 2 -->

**Needs first:** [Pick and configure the RMW](../../tutorial/pick-the-rmw.md)

Use Cyclone DDS when an image should talk to ROS 2 nodes directly, with no
router or agent running beside it.

The Cyclone DDS backend is Eclipse Cyclone DDS, the same DDS implementation
ROS 2 uses through `rmw_cyclonedds_cpp`, pinned to the version ROS 2 ships.
An image on it speaks RTPS and discovers its peers itself, so a stock
`rmw_cyclonedds_cpp` node sees it with nothing in between.

## Where it runs

Which boards build and run a Cyclone DDS image is generated from the board
registry, not written here:

- [Supported boards](../../reference/supported-boards.md): each board's
  evidence page lists, under **Breadth**, the RMWs its tests run.
- [Per-RMW feature matrix](../../reference/rmw-feature-matrix.md): what the
  backend implements (services, status events, QoS policies) next to zenoh
  and XRCE-DDS.
- [Support status](../../reference/support-status.md#rmw-backends--versions-and-interop-pairing):
  the pinned Cyclone version and the ROS 2 release it pairs with.

## Before you start

- A workspace, such as the host one from
  [Run a talker](../../tutorial/run-a-talker.md#no-qemu-no-router-run-it-on-this-machine).
- ROS 2 Humble with `ros-humble-rmw-cyclonedds-cpp` on the same network,
  to see the image from ROS 2.

## Steps

1. Provision the backend for your board. `--rmw cyclonedds` adds Cyclone's
   sources and the message tools its type support needs; `nros build` then
   builds Cyclone itself, with no separate step:

   ```bash
   nros setup linux --rmw cyclonedds
   ```

2. Select it. Set `rmw` in the bringup's `system.toml`, for every image:

   ```toml
   [system]
   rmw = "cyclonedds"
   ```

   Check what the bringup resolved:

   ```bash
   nros config show
   ```

   ```text
   [system]
   rmw                = cyclonedds                   # system.toml [system]
   ```

   To change one image only, put `rmw = "cyclonedds"` in its
   `[image.<id>]` block instead; see
   [Switch the RMW per image](switch-the-rmw-per-image.md).

3. Run it. No router starts, because Cyclone DDS needs none:

   ```bash
   nros run host_node --expect 'Publishing:' --timeout 120
   ```

   ```text
   Publishing: 'Hello World: 1'
   nros run: matched after
   ```

4. See it from ROS 2, in a second terminal:

   ```bash
   source /opt/ros/humble/setup.bash
   export RMW_IMPLEMENTATION=rmw_cyclonedds_cpp
   ros2 topic echo /chatter std_msgs/msg/String --qos-reliability best_effort
   ```

   ```text
   data: 'Hello World: 4'
   ```

5. Pick the DDS domain. The image runs on `[system] domain_id`, and Cyclone's
   own domain follows it. A ROS 2 peer must use the same number:

   ```toml
   [system]
   domain_id = 5
   ```

   ```bash
   ROS_DOMAIN_ID=5 ros2 topic echo /chatter std_msgs/msg/String --qos-reliability best_effort
   ```

6. Configure Cyclone itself, if the defaults do not fit your network: which
   interface to use, whether to multicast, tracing. Write Cyclone's own XML,
   the document `CYCLONEDDS_URI` names on a ROS 2 host, as
   `rmw/cyclonedds.xml` beside `system.toml`:

   ```xml
   <CycloneDDS xmlns="https://cdds.io/config">
     <Domain id="any">
       <General>
         <Interfaces>
           <NetworkInterface autodetermine="true" priority="default" multicast="default" />
         </Interfaces>
         <AllowMulticast>default</AllowMulticast>
       </General>
     </Domain>
   </CycloneDDS>
   ```

   The build copies the file into the image and hands it to Cyclone's own
   parser, so it reaches a target that has no file system. Use
   `<Domain id="any">`, or the image's `domain_id`: `nros build` refuses any
   other id. On a host image, `CYCLONEDDS_URI` still replaces the file at run
   time.

7. On an RTOS image, give the network stack what RTPS discovery needs:
   multicast (IGMP), enough network buffers for the burst of discovery
   traffic, and a heap, since Cyclone allocates with `malloc`. On Zephyr,
   start from the fragment the shipped examples use and name it in the
   image:

   ```toml
   [image.zephyr_cyclonedds]
   board = "zephyr"
   rmw = "cyclonedds"
   os_config = ["prj-cyclonedds.conf"]
   ```

   Copy
   [`examples/zephyr/rust/talker/prj-cyclonedds.conf`](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/zephyr/rust/talker/prj-cyclonedds.conf)
   beside `system.toml`. Its lines that matter for Cyclone are
   `CONFIG_NET_IPV4_IGMP=y`, the `CONFIG_NET_PKT_*` and `CONFIG_NET_BUF_*`
   counts, and `CONFIG_HEAP_MEM_POOL_SIZE`. The settings for the other
   platforms are listed in
   [Pick and configure the RMW](../../tutorial/pick-the-rmw.md#cyclone-dds-network-configuration-per-platform).

## Example

A host talker on DDS domain 5 with its own Cyclone configuration. This is
the bringup `nros new host_ws --board linux --template talker --rmw cyclonedds`
writes, with `domain_id` changed:

```toml
# src/host_node_bringup/system.toml
[system]
name = "host_node"
rmw = "cyclonedds"
ros_edition = "humble"
domain_id = 5

[[component]]
pkg = "host_node"
class = "host_node::Talker"
name = "host_node"

[image.posix]
board = "linux"
```

with `src/host_node_bringup/rmw/cyclonedds.xml` as in step 6. Run the image
and the listener:

```bash
nros run host_node
```

```bash
source /opt/ros/humble/setup.bash
export RMW_IMPLEMENTATION=rmw_cyclonedds_cpp ROS_DOMAIN_ID=5
ros2 topic echo /chatter std_msgs/msg/String --qos-reliability best_effort
```

## If it doesn't work

- **The build stops on `<Domain id>`.** The XML names a domain other than
  `any` or the image's `domain_id`. Change it to `any`.
- **`ros2 topic echo` shows nothing.** Check that both sides use the same
  `ROS_DOMAIN_ID` and that the ROS 2 side runs `rmw_cyclonedds_cpp`. On a
  network without multicast, list the peers in the XML instead.
- **Two Zephyr `native_sim` images never discover each other.** Its
  offloaded sockets do not pass multicast to the host; see
  [Troubleshoot](../deploy/troubleshoot.md#cyclone-dds-two-native_sim-nodes-never-discover-each-other).
- **An RTOS image stalls during discovery.** The network buffer counts or
  the heap are too small for the discovery burst. Raise them in the image's
  fragment.

## See also

- [Configure an image](../build/configure-an-image.md#the-middlewares-own-file):
  the middleware's own file, and one file per image.
- [Configuration surface](../../reference/configuration-surface.md): where
  `rmw` and `domain_id` are set and what they become on Zephyr.
- [Bridge between backends](bridge-between-backends.md): Cyclone DDS beside
  zenoh or XRCE-DDS in one image.
- [How nano-ros differs from ROS 2](../../background/differences-from-ros2.md).

## Design background

- [RFC-0031: RMW selection and lowering](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0031-rmw-selection-and-lowering.md)
- [RFC-0109: network config layers](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0109-network-config-layers.md)
