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

## Results (2026-07-07, native_sim, 100 ms socket timeout, ~33 s publish window)

| variant | delivered (valid/valid) | msgs/s | vs off |
| --- | --- | --- | --- |
| knob off | 298/298 | ~8.9 | 1× (≈ the recv-window rate — each put pays a full window) |
| `TX_BATCH` (+flush thread) | 752/752 | ~22.5 | 2.5× |
| `TX_BATCH` + `TX_SPLIT_LOCK` | 756/756 | ~22.6 | 2.5× |

Integrity clean in all variants (zero invalid payloads through batching,
overflow flushes, and the split-lock steal path).

**Finding:** streaming caps at ~2.5× because the write-buffer OVERFLOW flush
runs on the publisher's own thread — `_z_transport_tx_batch_overflow` sends
under the caller's tx mutex, so a tight-loop publisher that fills the batch
between flush-thread cycles pays the recv-window wait itself on every overflow.
The phase-282 W1 split-lock steal covers only the flush-thread cadence path
(`_z_transport_tx_send_n_batch`). Next lever: extend the steal to the
overflow path (swap-and-send-outside-tx there too).
