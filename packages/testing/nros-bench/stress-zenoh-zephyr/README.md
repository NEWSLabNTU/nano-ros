# stress-zenoh-zephyr — Zephyr streaming tx benchmark (phase-282 W2, #145)

A tight-loop zpico publisher on `native_sim`, measuring the Zephyr zsock tx
ceiling across the `ZPICO_TX_BATCH` / `Z_FEATURE_TX_SPLIT_LOCK` knobs. Publishes
`STRESS_COUNT` (default 5000) payloads of `STRESS_SIZE` (default 64) bytes as
fast as the loop allows, over the exact shared tx path every nano-ros language
front-end uses (zpico shim → zenoh-pico), bypassing the executor so the number
isolates the transport. The native `nros-bench/stress-zenoh` listener is the
counting + integrity-validating sink (same payload layout).

## Build (three knob variants)

```bash
source ./activate.sh
cd zephyr-workspace
APP=/abs/path/to/packages/testing/nros-bench/stress-zenoh-zephyr
CONF='prj.conf;/abs/path/to/zephyr/native-sim-nsos.conf'
LOC='-DCONFIG_NROS_ZENOH_LOCATOR="tcp/127.0.0.1:17866"'

west build -b native_sim/native/64 -d build-stress-zenoh       -p always $APP -- -DCONF_FILE="$CONF" $LOC
west build -b native_sim/native/64 -d build-stress-zenoh-batch -p always $APP -- -DCONF_FILE="$CONF" $LOC -DCONFIG_NROS_ZENOH_TX_BATCH=y
west build -b native_sim/native/64 -d build-stress-zenoh-split -p always $APP -- -DCONF_FILE="$CONF" $LOC -DCONFIG_NROS_ZENOH_TX_BATCH=y -DCONFIG_NROS_ZENOH_TX_SPLIT_LOCK=y
```

## Measure

```bash
# IMPORTANT: build the LISTENER with a deep rx ring. The per-subscriber
# SPSC ring depth is compile-time (ZPICO_SUBSCRIBER_RING_DEPTH, default 4);
# a batched publisher delivers a whole wire batch as one callback burst and
# a 4-slot ring silently drops the tail (drop-newest), undercounting the
# measurement by >10x.
(cd packages/testing/nros-bench/stress-zenoh && \
  ZPICO_SUBSCRIBER_RING_DEPTH=1024 cargo build --release)

./build/zenohd/zenohd -l tcp/0.0.0.0:17866 --no-multicast-scouting &
MODE=listener PAYLOAD_SIZE=64 EXPECTED_COUNT=5000 TIMEOUT_SECS=40 \
  NROS_LOCATOR=tcp/127.0.0.1:17866 \
  packages/testing/nros-bench/stress-zenoh/target/release/zenoh-stress-test &
timeout 35 zephyr-workspace/build-stress-zenoh<-variant>/zephyr/zephyr.exe
# listener prints: RECV_DONE: received=N valid=N ... — msgs/s = N / publish window
```

The talker prints `PUBLISH_DONE: sent=N elapsed_ms=T` if the loop completes
inside the window; when the tx path is window-bound it will not (that IS the
measurement — use the listener count over the fixed window instead).

## Results (2026-07-07, native_sim, 100 ms socket timeout, deep-ring listener)

| variant | talker (5000 msgs) | delivered | msgs/s | vs off |
| --- | --- | --- | --- | --- |
| knob off | did not finish in ~33 s | 298/298 valid | ~8.9 | 1× (each put pays a full recv window) |
| `TX_BATCH` (+flush thread) | did not finish in ~33 s | 4499/4499 valid | ~136 | ~15× |
| `TX_BATCH` + `TX_SPLIT_LOCK` | **finished in 27.7 s** | **5000/5000 valid** | **~181** | **~20×** |

Integrity clean in all variants (zero invalid payloads through batching,
overflow parking, and the split-lock steal path); the split variant is the
only one that both completes the 5000-message publish and delivers 100%.

**W2.c (overflow steal):** the batch-only cap came from the write-buffer
OVERFLOW flush running on the publisher's own thread under the tx mutex —
a tight-loop publisher filling the batch between flush-thread cycles paid
the recv-window wait itself on every overflow. With `TX_SPLIT_LOCK` the
overflow now parks the finalized batch in the spare buffer (at most one
parked; a second overflow drains it first = natural backpressure) and the
next flush ships it, so the publisher never blocks on the socket.
