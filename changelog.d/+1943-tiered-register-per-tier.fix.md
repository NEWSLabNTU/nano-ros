A multi-tier `nros::main!` entry no longer registers every component on every
tier. A tier now registers only the nodes whose callback groups it runs, the
rule the C/C++ entries already follow; before, the third tier exhausted each
component's class store and failed with an opaque `NodeRegister`, which on
FreeRTOS stopped every tier spawned after it (issue 1943).
