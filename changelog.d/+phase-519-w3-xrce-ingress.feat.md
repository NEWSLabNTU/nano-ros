The XRCE backend can ask the agent to pace a subscription. A subscription that
declares an ingress rate (`rmw_subscription_options_t::ingress_rate_mhz`) gets
`max_bytes_per_second = rate_hz x` its type's bound, unless the rate is below
1 Hz or the product exceeds the agent's 65535 B/s field; those cases log why
and stay unpaced. A paced sample arrives up to one period old (RFC-0074 "On
XRCE"). No language surface fills the field yet, so no image changes behaviour.
