The contract's `deadline`, `lifespan`, `lease_duration` and `liveliness` are now
checked against `qos_overrides.*` launch parameters, as reliability, durability,
history and depth already were. A launch parameter that states a different
value from the contract, or states a liveness policy the contract does not,
stops the build and names both sites, together with the line to put in the
contract. Durations compare in milliseconds, so a contract `deadline: 100ms`
agrees with an override of 100 ms written in nanoseconds. `nros codegen-system` (the
Zephyr build) now makes this check, and the hazard FTTI check, before it bakes
anything.
