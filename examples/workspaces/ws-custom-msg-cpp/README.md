# ws-custom-msg-cpp — in-workspace custom message (C++)

## What it shows

The C++ projection of [`ws-custom-msg-c`](../ws-custom-msg-c/): the same
`src/custom_msgs/msg/Reading.msg` (`temperature`/`humidity`/`sequence`)
published on `/reading`. `ReadingTalker` binds a 1 s `bind_timer` member
callback; `ReadingListener` uses `bind_subscription_raw` and decodes the CDR
by hand. Two entries, cross-process (issue 0096).

## Run

```sh
source ./activate.sh && nros ws sync
nros codegen-system --bringup demo_bringup
cmake -S . -B build && cmake --build build
zenohd --listen tcp/127.0.0.1:7447 &
./build/src/native_listener_entry/native_listener_entry &
./build/src/native_talker_entry/native_talker_entry
```

## Expected output

```
[reading_talker] sent seq=1 temp=21.5
reading seq=1 temp=21.5          # listener
```

## Copy-out notes

Standard workspace copy-out (`nros ws sync` + `-DNANO_ROS_ROOT`/`NROS_REPO_DIR`).
Fixture ids `workspace-cpp-native-custom-msg-{talker,listener}`.
