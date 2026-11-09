# nano-ros compared with micro-ROS and mros2

<!-- type: explanation · level: 1 -->

Three open-source projects put ROS 2 nodes on microcontrollers: micro-ROS,
mros2 and nano-ros. They make different trade-offs, and each is the right
choice for some projects. This page compares them so you can pick one.

Every claim about nano-ros links to the page or file in this book or its
repository that shows it. Every claim about another project cites that
project's own documentation or release, read on 2026-11-10. Where a project
publishes nothing on a point, the table says "not published" rather than
guessing. If you find a claim here that is out of date, open an issue: the
other projects move too.

## At a glance

| | nano-ros | micro-ROS | mros2 |
| --- | --- | --- | --- |
| Node API | Rust; C with rclc's names; C++ with rclcpp's names | C: rcl and rclc[^ur-cl] | C++, a subset "compatible with rclcpp"[^mr] |
| Middleware | zenoh, XRCE-DDS or Cyclone DDS, chosen per image | XRCE-DDS (`rmw_microxrcedds`)[^ur-feat] | RTPS through embeddedRTPS on lwIP[^mr] |
| Process beside the device | a zenoh router, an XRCE-DDS agent, or none with Cyclone DDS | always the Micro XRCE-DDS Agent[^ur-feat] | none[^mr] |
| Topics, services, actions, parameters | all four | all four; actions through rclc, no parameter client[^ur-cmp] | topics only[^mr] |
| Operating systems | see [Supported boards](../reference/supported-boards.md) | FreeRTOS, Zephyr and NuttX as the main ones, plus modules for ESP-IDF, Mbed, ThreadX, Arduino and more[^ur-setup] | Mbed OS, ESP-IDF; TOPPERS/ASP3 up to v0.3.2; POSIX partly[^mr] |
| Formal checks | Kani and Verus proofs in the source tree ([Safety](safety.md)) | none found in its docs | none found in its docs |
| License | MIT or Apache-2.0 ([`LICENSE-MIT`, `LICENSE-APACHE`](https://github.com/NEWSLabNTU/nano-ros)) | Apache-2.0[^ur-lic] | Apache-2.0[^mr] |
| Latest release | see [Install](../start/install.md) | `micro_ros_setup` 6.1.0, 2026-08-11[^ur-rel] | v0.5.4, 2023-10-05[^mr-rel] |

## Where micro-ROS is stronger

micro-ROS is the established choice, and for many projects it is the better
one.

- **Backing and support.** It is backed by the ROS 2 Embedded Working Group,
  a formal ROS 2 working group, and eProsima sells commercial support for
  Micro XRCE-DDS[^ur-feat]. nano-ros is maintained by one research lab and has
  no commercial support.
- **Boards and vendor tools.** micro-ROS lists officially supported boards
  with long-term support, among them ESP32, Raspberry Pi Pico, Teensy,
  Arduino Portenta H7 and Renesas EK RA6M5[^ur-hw]. It has modules for vendor
  tools such as STM32CubeMX, Renesas e2studio and PlatformIO[^ur-setup].
  nano-ros supports fewer boards; [Supported boards](../reference/supported-boards.md)
  lists them, with what has been tested on each.
- **Distributions.** micro-ROS tracks Humble, Jazzy, Kilted, Lyrical and
  Rolling[^ur-setup]. nano-ros targets Humble and Jazzy.
- **Published measurements.** micro-ROS publishes a memory study for its
  client on an ESP32: about 400 B of RAM for each extra publisher, about
  500 B for each subscriber and about 300 B for each service or
  client[^ur-mem].

Choose micro-ROS when you already have rclc code and an agent in your
deployment, when you need one of its supported boards or vendor tools, or
when you need commercial support.

## Where mros2 is stronger

mros2 talks RTPS straight from the board, with no agent and no
router[^mr]: a board on the same network as ROS 2 nodes is simply another DDS
participant. Its C++ API follows rclcpp. Its scope is narrower: topics only,
no services, actions or parameters, and no arrays in messages[^mr]. Its last
release was in 2023[^mr-rel].

nano-ros reaches the same agentless setup with its Cyclone DDS backend
([Use Cyclone DDS](../guide/rmw/use-cyclone-dds.md)), on the boards that
backend supports.

## Where nano-ros is different

- **Three languages, one node model.** A node is written in Rust, C or C++,
  and the book's starters show each language on each board
  ([Welcome](../start/welcome.md) has the table, generated from the probe
  results). The C API keeps rclc's names and the C++ API keeps rclcpp's, so
  code from micro-ROS or ROS 2 ports with few changes
  ([How nano-ros differs from ROS 2 and micro-ROS](differences-from-ros2.md)).
- **The middleware is a build choice.** The same node builds for zenoh,
  XRCE-DDS or Cyclone DDS, chosen per image
  ([Switch the RMW per image](../guide/rmw/switch-the-rmw-per-image.md)). One
  image can bridge two of them
  ([Bridge between backends](../guide/rmw/bridge-between-backends.md)). The
  zenoh backend talks to upstream `rmw_zenoh` with no bridge
  ([Talk to ROS 2](../tutorial/talk-to-ros2.md)).
- **Memory is sized when you build.** Node, topic and message pools are
  fixed at build time from what the workspace declares
  ([Static memory and pools](static-memory-and-pools.md)).
- **Scheduling beyond priorities.** Tasks get EDF and sporadic classes and
  budgets ([The execution model](execution-model.md)).
- **End-to-end integrity and proofs.** An opt-in CRC and sequence check
  guards each message, and the CRC and its validator carry Verus proofs
  ([Safety](safety.md)).
- **The starters and the Tutorial are replayed.** Their shell steps run in
  CI from a clean install, and each starter's status line says how its board
  was last checked.

## What one image costs

One measured image, so you can judge the size for yourself. A Rust talker
over zenoh on the MPS2-AN385 bare-metal board (the
[bare-metal starter](../starters/mps2-an385-bare-metal.md)), built with
`nros build -- --release` at commit `a9dc35a5e1` on 2026-11-09 and read with
`arm-none-eabi-size`:

| | Bytes |
| --- | --- |
| Code and constants | 397,852 |
| RAM outside the heap | 40,128 |
| Heap, sized by the board | 132,816 |

This is not comparable with micro-ROS's per-entity figures above: theirs are
increments on an ESP32 over XRCE-DDS, this is a whole image over zenoh with
its network stack. A like-for-like comparison would need both projects built
for the same board, middleware and message set; nobody has published one.

[^ur-cl]: micro-ROS, *Client library*, <https://micro.ros.org/docs/concepts/client_library/introduction/>, read 2026-11-10.
[^ur-feat]: micro-ROS, *Features and architecture*, <https://micro.ros.org/docs/overview/features/>, read 2026-11-10.
[^ur-cmp]: micro-ROS, *ROS 2 feature comparison*, <https://micro.ros.org/docs/overview/ROS_2_feature_comparison/>, read 2026-11-10.
[^ur-setup]: micro-ROS, `micro_ros_setup` README, <https://github.com/micro-ROS/micro_ros_setup>, read 2026-11-10.
[^ur-hw]: micro-ROS, *Supported hardware*, <https://micro.ros.org/docs/overview/hardware/>, read 2026-11-10.
[^ur-lic]: micro-ROS, *License*, <https://micro.ros.org/docs/overview/license/>, read 2026-11-10.
[^ur-rel]: micro-ROS, `micro_ros_setup` releases, <https://github.com/micro-ROS/micro_ros_setup/releases>, 6.1.0 released 2026-08-11.
[^ur-mem]: micro-ROS, *Memory profiling*, <https://micro.ros.org/docs/concepts/benchmarking/memo_prof/>, read 2026-11-10: ESP32 with FreeRTOS, UDP over Wi-Fi, MTU 512 B, XRCE history 4, RMW history 8.
[^mr]: mros2 README, <https://github.com/mROS-base/mros2>, read 2026-11-10.
[^mr-rel]: mros2 releases, <https://github.com/mROS-base/mros2/releases>, v0.5.4 released 2023-10-05.
