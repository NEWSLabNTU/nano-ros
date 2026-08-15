---
id: 589
title: "Zephyr native_sim: any Rust `println!`/`eprintln!` recurses forever in
  `zvfs_write` and SIGSEGVs the image"
status: open
type: bug
severity: high
area: zephyr, api-cpp
related: [issue-0557, issue-0436, issue-0572, phase-358]
---

## Symptom

A Zephyr `native_sim` image dies with no message — `timeout: the monitored
command dumped core` — the moment any Rust `std` stdio call runs. The backtrace
is one frame repeated until the stack is gone:

```
#6  zvfs_write (fd=1, buf=0x5097d0, sz=17) at zephyr/lib/os/fdtable.c:339
#7  zvfs_write (fd=1, buf=0x5097d0, sz=17) at zephyr/lib/os/fdtable.c:340
#8  zvfs_write (fd=1, buf=0x5097d0, sz=17) at zephyr/lib/os/fdtable.c:340
…                                    (repeats)
```

with the fd's mutex recursively taken by the same thread:

```
p fdtable[1]
  vtable = 0x5448e0 <stdinout_fd_op_vtable>,
  lock = { owner = 0x588980 <z_main_thread>, lock_count = 104756 }
```

`k_mutex` is recursive, so this does not deadlock — it just recurses until the
stack is exhausted.

## Cause — in Zephyr, not in nano-ros

`zephyr/lib/os/fdtable.c`:

```c
ssize_t zvfs_write(int fd, const void *buf, size_t sz)
{
        ...
        res = fdtable[fd].vtable->write_offs(fdtable[fd].obj, buf, sz, fdtable[fd].offset);
```

and the vtable installed for fd 0/1/2:

```c
static ssize_t stdinout_write_vmeth(void *obj, const void *buffer, size_t count)
{
#if defined(CONFIG_BOARD_NATIVE_POSIX)
        return zvfs_write(1, buffer, count);      /* ← straight back in */
```

`zvfs_write(1, …)` → `stdinout_write_vmeth` → `zvfs_write(1, …)` → … There is no
termination condition. Confirmed present in this build:

```
CONFIG_BOARD_NATIVE_POSIX=y
CONFIG_POSIX_DEVICE_IO=y
CONFIG_PICOLIBC=y
```

## Why it has not bitten before

C/C++ `printf` on these images goes through picolibc's console hook, not the
POSIX fdtable, so every existing example prints fine. Only a POSIX `write(1|2,
…)` reaches the recursion — which is exactly the path Rust `std::println!` /
`std::eprintln!` take.

**The config is identical in images that pass** (checked `build-c-service-server-cyclonedds`
against `build-c-action-server-cyclonedds`), so this is latent in EVERY
native_sim image and fires only when a Rust std stdio call is reached. That
makes it a landmine rather than a bug in any one cell: adding a `println!` to a
Rust-side error path can turn a diagnosable failure into a silent core dump.

## How it was found

Issue 0557's fix routed `nros_cpp_action_server_create`'s error through
`node_error_to_cpp_ret`, which carries issue 0436's
`eprintln!("nros: NodeError::{err:?}")`. The 17-byte buffer in the backtrace is
that literal:

```
x/s buf   →   "nros: NodeError::"
```

So the print itself killed the image. Before that change the mapper was never
reached on this path, and the failure surfaced as a quiet `-100`.

## Worked around (not fixed) in nano-ros

`packages/api/nros-cpp/src/lib.rs` now gates that diagnostic
`#[cfg(all(feature = "std", not(feature = "platform-zephyr")))]`. The mapping —
the part that carries the information — still runs everywhere.

`grep -n 'std::eprintln!' packages/api/nros-cpp/src/lib.rs` finds **6** sites in
that file alone; the other five sit on paths not yet exercised on Zephyr. They
are the same landmine.

## What a real fix needs

* either route nano-ros's Rust-side diagnostics through a Zephyr-safe sink
  (`printk` / the Zephyr LOG backend / `nros-log`) on that platform, so a
  diagnostic can never be fatal;
* or carry the Zephyr fix upstream — `stdinout_write_vmeth` should call the
  native write primitive, not re-enter `zvfs_write`;
* plus a gate, so the next `println!` added to a Rust path that Zephyr links
  cannot reintroduce it silently.

## Acceptance

* a Rust `println!`/`eprintln!` on a `native_sim` image prints instead of
  crashing, or is structurally impossible to write;
* the six `std::eprintln!` sites in `nros-cpp` are each safe or unreachable on
  Zephyr, and something enforces that.
