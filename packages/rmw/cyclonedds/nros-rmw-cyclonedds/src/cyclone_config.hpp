#ifndef NROS_RMW_CYCLONEDDS_CYCLONE_CONFIG_HPP
#define NROS_RMW_CYCLONEDDS_CYCLONE_CONFIG_HPP

// phase-206 W1 — the Cyclone domain config for the platforms that build it
// themselves, and the rule for COMBINING the three places it can come from.
//
// Three sources exist: the baked baseline below, the compile-time Kconfig blob
// (`CONFIG_NROS_CYCLONE_CONFIG_XML`), and the caller's `CYCLONEDDS_URI`. Until
// this phase `session.cpp` SELECTED one of them with a three-way ternary, so a
// user who set either override silently lost the entire baseline — the
// `<Threads>` stack sizes, the `<Sizing>` receive buffers, the
// `<Internal><MultipleReceiveThreads>` choice and the platform's
// `AllowMulticast` policy — while asking only for, say, a different peer list.
// On FreeRTOS and ThreadX those stack sizes are load-bearing (a 1 KiB default
// thread stack overflows on the first real ROS payload), so "override the peer
// list" silently meant "reintroduce a stack overflow".
//
// CycloneDDS composes natively: `ddsi_config_init` (third-party/dds/cyclonedds,
// pin 67ff7518 / 0.10.5, `src/core/ddsi/src/ddsi_config.c:2505-2593`) splits the
// config string on commas and feeds each token into the SAME `cfgst` — a token
// starting with `<` as inline XML, anything else as a file path or `file://`.
// So all three sources can be handed over at once, and the composition below is
// the whole fix.
//
// PRECEDENCE — verified, not assumed. Opening a `<CycloneDDS>` root shifts
// `cfgst->source` left by one bit (ddsi_config.c:2019-2029), and `do_update`
// (ddsi_config.c:1614-1631) resets an already-set scalar
// (`free_configured_element` clears `n->count`) when `source > n->sources`.
// LATER TOKENS WIN. Measured against the pinned tree by dumping the resolved
// config (`<Tracing><Category>config`):
//
//   baseline THEN user:  AllowMulticast=false {0,1}   (the user's value)
//                        Threads/…/StackSize=64 KiB {0}  (the baseline's, kept)
//   user THEN baseline:  AllowMulticast=spdp  {0,1}   (the baseline's — order
//                                                      is what decides)
//
// Hence the order below: baseline, then Kconfig, then the user. The user still
// wins on everything they state, and stops losing what they did not.
//
// The one exception, and it is a property of Cyclone rather than of this code:
// `<Thread>` is a LIST element (multiplicity 0), so a later token does not
// replace an earlier entry of the same `Name` — it APPENDS one, and
// `lookup_thread_properties` (`src/core/ddsi/src/q_thread.c:280-288`) returns
// the FIRST match. A user who re-states a thread this baseline already names
// therefore does not override its stack size. That is the safe direction for
// the setting whose loss this phase is about, but it is a real limit; a user
// who needs a different stack for a baked thread has to change the baseline.

#include <cstddef>

// The baseline is compiled only where `session.cpp` actually creates the domain
// (the platforms with no Cyclone config loader of their own); hosted POSIX and
// plain Zephyr let Cyclone read `CYCLONEDDS_URI` itself, so compiling ~1.7 KiB
// of rodata into those images would buy nothing. `cyclone_config_test.cpp`
// defines the fourth macro so the host test can compose against the REAL string
// rather than a copy of it that could drift.
#if defined(NROS_PLATFORM_FREERTOS) || defined(NROS_PLATFORM_THREADX) ||                           \
    defined(CONFIG_BOARD_NATIVE_SIM) || defined(NROS_RMW_CYCLONEDDS_TEST_BASELINE)
#define NROS_RMW_CYCLONEDDS_HAVE_BASELINE 1
#endif

namespace nros_rmw_cyclonedds {

#ifdef NROS_RMW_CYCLONEDDS_HAVE_BASELINE
constexpr const char* kEmbeddedCycloneConfig =
    "<CycloneDDS>"
    "<Domain Id=\"any\">"
    "<General>"
#if defined(NROS_PLATFORM_FREERTOS)
    // Issue 0888 — say it, rather than inheriting Cyclone default.
    //
    // FreeRTOS was the one platform arm with no AllowMulticast at all, so
    // it fell through to the default (multicast for data as well as
    // discovery) while its two siblings each state a policy. That is a
    // silent difference, not a considered one: an image built here
    // advertises multicast data locators, and whether that is what anyone
    // wanted depended on a default nobody wrote down.
    //
    // The platform is fully capable of multicast — LWIP_IGMP is on, the
    // netif carries NETIF_FLAG_IGMP, the LAN9118 driver enables MCPAS, and
    // SPDP discovery over 239.255.0.1 demonstrably works. So this is a
    // choice, not a limitation: discovery multicast, data unicast, which
    // is what the ThreadX arm below settled on for the same reasons and
    // what a ROS 2 peer configured for an embedded island typically sets
    // on its own side.
    "<AllowMulticast>spdp</AllowMulticast>"
#elif defined(NROS_PLATFORM_THREADX)
    // Phase 177.26 — SPDP multicast discovery over NetX Duo. NetX enables
    // IGMPv2 (`nx_igmp_enable`) and virtio-net accepts all multicast on RX;
    // peers discover via the default DDSI multicast group, data unicast.
    "<AllowMulticast>spdp</AllowMulticast>"
#elif defined(CONFIG_BOARD_NATIVE_SIM)
    // Phase 180 — native_sim (NSOS). Multicast breaks cyclone's select-based
    // socket waitset here (the multicast RX fd select()s as failed), so
    // disable it and discover via unicast SPDP to 127.0.0.1 (Peers, below).
    "<AllowMulticast>false</AllowMulticast>"
#endif
    "</General>"
#if defined(CONFIG_BOARD_NATIVE_SIM)
    // Unicast SPDP to localhost (numeric IP — NSOS getaddrinfo can't resolve
    // the name). Widen the participant-index scan so the talker reaches the
    // listener even when host-port collisions bump it to a higher index.
    "<Discovery>"
    "<ParticipantIndex>auto</ParticipantIndex>"
    "<MaxAutoParticipantIndex>20</MaxAutoParticipantIndex>"
    "<Peers><Peer Address=\"127.0.0.1\"/></Peers>"
    "</Discovery>"
#endif
    "<Sizing>"
    "<ReceiveBufferSize>64 KiB</ReceiveBufferSize>"
    "<ReceiveBufferChunkSize>16 KiB</ReceiveBufferChunkSize>"
    "</Sizing>"
    // One receive thread, not per-socket ones. The split exists to shave
    // latency on a host with cores to spare; here it costs two threads that
    // cannot be given a stack (see the Threads block below), on a platform
    // whose default thread stack is 1 KiB.
    "<Internal>"
    "<MultipleReceiveThreads>false</MultipleReceiveThreads>"
    "</Internal>"
    // Thread stacks. ddsrt's FreeRTOS port defaults a thread to
    // configMINIMAL_STACK_SIZE (256 words = 1 KiB here), which is not a stack
    // any Cyclone worker can run in: `recvUC` overflowed on the first real
    // ROS payload (a 13 KiB Autoware trajectory) with
    // `*** STACK OVERFLOW: recvUC ***`, and — because the overflow lands in
    // the adjacent heap — the SAME image also failed at create_subscription
    // with a bad-free heap_4 assert when it booted into an already-populated
    // graph. Small fixed-size samples never reached the depth, which is why
    // the Int32 examples pass and only a real ROS peer surfaces it.
    // Naming a thread here is the ONLY way to size it: Cyclone has no
    // global default stack setting.
    "<Threads>"
    "<Thread Name=\"dq.builtins\">"
    "<StackSize>64 KiB</StackSize>"
    "</Thread>"
    // Receive path: reads a fragment, reassembles, deserializes.
    //
    // Only "recv" is nameable. With MultipleReceiveThreads enabled Cyclone
    // splits reception into per-socket "recvUC"/"recvMC" threads, and those
    // names are NOT configurable — `check_thread_properties` validates against
    // a fixed list and rejects them ("unknown thread"), which fails the whole
    // config and takes the participant with it. So the split is disabled just
    // below, leaving one receive thread that this entry actually sizes.
    "<Thread Name=\"recv\">"
    "<StackSize>64 KiB</StackSize>"
    "</Thread>"
    // User-data delivery — where the application's own types are built.
    "<Thread Name=\"dq.user\">"
    "<StackSize>64 KiB</StackSize>"
    "</Thread>"
    // Timed events and GC do less, but the 1 KiB default is below what any
    // of them can safely use.
    "<Thread Name=\"tev\">"
    "<StackSize>16 KiB</StackSize>"
    "</Thread>"
    "<Thread Name=\"gc\">"
    "<StackSize>16 KiB</StackSize>"
    "</Thread>"
    "</Threads>"
    "</Domain>"
    "</CycloneDDS>";
#endif // NROS_RMW_CYCLONEDDS_HAVE_BASELINE

/// Upper bound on the composed config string.
///
/// A fixed buffer, because this runs on targets where `session_create` cannot
/// assume a heap. The baseline is ~1.7 KiB; the rest is headroom for the
/// Kconfig blob and the caller's URI. Overflow is a hard failure, never a
/// truncation: a silently clipped config is unterminated XML, which Cyclone
/// rejects with a parse error naming a line number in a string nobody wrote.
constexpr size_t kCycloneConfigMax = 4096;

/// Join the non-empty `frags` with `,` into `out`, in order.
///
/// Returns false — and leaves `out` unusable — if the result would not fit,
/// including its terminator. Empty and null fragments are skipped, so an unset
/// override contributes nothing rather than an empty token (Cyclone's parser
/// skips those too, but a `,,` in a config string is noise in its error
/// messages).
inline bool compose_cyclone_config(char* out, size_t cap, const char* const* frags, size_t nfrags) {
    if (out == nullptr || cap == 0 || (frags == nullptr && nfrags != 0)) {
        return false;
    }
    size_t len = 0;
    for (size_t i = 0; i < nfrags; ++i) {
        const char* frag = frags[i];
        if (frag == nullptr || frag[0] == '\0') {
            continue;
        }
        if (len != 0) {
            if (len + 1 >= cap) {
                return false;
            }
            out[len++] = ',';
        }
        for (const char* p = frag; *p != '\0'; ++p) {
            if (len + 1 >= cap) {
                return false;
            }
            out[len++] = *p;
        }
    }
    out[len] = '\0';
    return true;
}

} // namespace nros_rmw_cyclonedds

#endif // NROS_RMW_CYCLONEDDS_CYCLONE_CONFIG_HPP
