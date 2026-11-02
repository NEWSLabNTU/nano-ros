A session whose backend states no readiness form (it neither calls the
executor's wake callback on an arrival nor reports a handle to park on) is now
refused when it opens, with `TransportError::Unsupported` and a log line naming
RFC-0113 D5. It used to be served by blocking inside the transport's receive,
which no deadline or other session could end. A custom XRCE transport must
register a receive notification (`nros_rmw_xrce_set_custom_transport_rx_notify`).
The native level runners also stop waking each idle level once per period: a
level now parks until its next timer or an arrival.
