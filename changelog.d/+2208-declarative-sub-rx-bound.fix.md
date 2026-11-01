A declarative Rust subscription (`Component`, `create_subscription_for_callback_*`)
now hands the backend its message type's bound as `rx_buffer_hint`, as the
typed executor path always did. On XRCE a declared ingress can now pace it
(measured: 10.34 Hz against a 100 Hz publisher at `rate_hz: 10`); on zenoh a
bounded type now takes the receive class its bound selects instead of the
default. Unbounded types are unchanged.
