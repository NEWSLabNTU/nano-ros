A Zephyr image can re-query its SNTP server on an interval: set
`[image.<id>.knobs.clock] sntp_resync_interval_ms` above 0 and its message
stamps follow a server whose time steps, instead of keeping the offset
taken once at boot. The default, 0, keeps the single boot-time query.
