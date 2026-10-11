#![no_std]
#![no_main]
// The panic handler is the board's crate-root item (`use panic_semihosting as
// _;`), carried by the generated selection package (issue 1305); `own` says
// the image brings it rather than routing panics to the platform.
nros::main!(panic = "own");
