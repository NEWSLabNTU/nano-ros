A launch contract's hazards are now checked against the image at build time.
For every hazard with an `ftti_ms`, the bake adds up how long the image takes
to detect the fault (the monitor it bakes on the guard topic: a subscriber's
`max_age_ms`, or two 5 s rate windows), to run the reaction path
(`max_latency_ms`) and to release it (its timer period, or its tier's period).
If that exceeds the FTTI, both `nros build` and `nros::main!` refuse with every
term named. Fault classes the image cannot observe are listed, not counted.
