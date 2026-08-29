---
id: 877
title: "FreeRTOS pubsub delivers by hand and delivers NOTHING under the test
  harness — and the talker trips a FreeRTOS queue assert"
status: open
type: bug
area: testing, boards
related: [issue-0891, issue-0830, issue-0387]
---

## Symptom

`test_rtos_pubsub_e2e::platform_1_Platform__Freertos`, all three languages,
fails at ~65 s with `0 messages received`. Solo, on an idle host (load 0.23),
and identically on `origin/main` — so it is neither load nor a local change.

The two sides look healthy in isolation:

    Talker output:                     Listener output:
    Network ready                      nros C Listener
    Publishing: 'Hello World: 1'       Locator: tcp/192.0.3.1:7900
    Publishing: 'Hello World: 2'       Subscriber created for topic: /chatter
    …                                  Waiting for messages (Ctrl+C to exit)...

Talker publishes, listener waits, nothing arrives. The router logs no session.

## The same two images DELIVER when run by hand

Started manually — router on `tcp/0.0.0.0:7900`, listener, 22 s, then talker,
with the harness's own QEMU arguments (`-machine mps2-an385`,
`-nic user,model=lan9118,net=192.0.3.0/24,host=192.0.3.1`):

    I heard: [Hello World: 15]
    I heard: [Hello World: 16]
    …

So the images, the board's lwIP plan, the slirp addressing and the router are
all fine. Whatever fails is in how the HARNESS runs them, not in delivery.

That is the useful half of this report: it rules out the transport, and rules
out the whole class of "the guest cannot reach the router" explanations.

## Two theories killed on the way, recorded so nobody re-runs them

* **`br-qemu` missing.** The bridge really is absent on this host, and it is
  irrelevant: `192.0.3.x` here is slirp with a custom net, not a bridge. The
  FreeRTOS *action* cells pass on the same host, which contradicted the theory
  before it cost anything.
* **`6ae0249aa` (recent talker/listener ENTITY_BOUNDS change).** It touched
  only the Rust copies; all three languages fail.

Ports are also not it — the manifest bakes talker and listener both on 7900
(service 7910, action 7920), verified from the built `build.ninja`.

## Second, separate bug found while reproducing

The manually-run talker dies after ~19 publishes:

    FreeRTOS ASSERT FAILED: third-party/freertos/kernel/queue.c:1673

Delivery had already worked by then, so it is not the cause of this issue — but
it is a real fault in the FreeRTOS C talker image and does not appear to be
recorded anywhere. It needs its own diagnosis; noted here so the observation is
not lost with this session.

## Where to look next

The difference is the harness, so compare what it does that a hand-run does
not: `ZenohRouter::start_slirp` calls `kill_listeners_on_port` before binding;
the router is started per `(variant, lang)`; several cells' routers may be alive
at once. None of that is yet ruled in or out.

## Acceptance

* The cell passes under the harness, or the harness difference that breaks it is
  named and fixed.
* The `queue.c:1673` assert is filed separately with its own reproduction.
