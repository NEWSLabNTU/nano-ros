A subscription whose contract declares `ingress: { rate_hz }` now asks the XRCE
agent for that pace on its own: the rate travels through the sizing descriptor
to every Rust, C and C++ subscription registration. Measured on a native XRCE
image, a 10 Hz declaration against a 100 Hz publisher received 10.34 Hz. A
declared rate the backend cannot pace because the registration states no type
bound is now logged.
