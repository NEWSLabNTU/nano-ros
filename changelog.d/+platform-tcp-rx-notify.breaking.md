A platform port must now export `nros_platform_tcp_set_rx_notify(sock, notify, ctx)`,
the TCP twin of `nros_platform_udp_set_rx_notify`. Return -1 when the stack
cannot notify; a Rust port gets that default from `PlatformTcp::set_rx_notify`.
ThreadX implements it over NetX Duo's receive notification, so zenoh in CALLER
mode on ThreadX now parks until an arrival or its next deadline instead of
sleeping blindly.
