# lan9118-smoltcp

LAN9118/SMSC911x Ethernet driver for [smoltcp](https://github.com/smoltcp-rs/smoltcp).

This crate provides a `no_std` compatible driver for the LAN9118 Ethernet controller
(and compatible SMSC911x variants), implementing the `smoltcp::phy::Device` trait.

## Supported Hardware

- SMSC LAN9118
- SMSC LAN9220 (as emulated by QEMU mps2-an385)

## Usage

```rust,ignore
use lan9118_smoltcp::{Lan9118, Config};

// Create driver with default config (for MPS2-AN385)
let config = Config {
    base_addr: 0x4020_0000,
    mac_addr: [0x02, 0x00, 0x00, 0x00, 0x00, 0x01],
};

let mut eth = unsafe { Lan9118::new(config) }?;
eth.init()?;

// Use with smoltcp Interface
let mut iface = smoltcp::iface::Interface::new(iface_config, &mut eth, instant);
```

## Features

- **Polling mode** - No interrupt handling required
- **no_std compatible** - Suitable for bare-metal embedded systems
- **smoltcp integration** - Implements `smoltcp::phy::Device` trait

## QEMU Testing

The driver can be tested with QEMU's mps2-an385 machine:

```bash
qemu-system-arm -M mps2-an385 \
    -netdev tap,id=net0,ifname=tap0,script=no,downscript=no \
    -device lan9118,netdev=net0 \
    -kernel your-binary.elf \
    -semihosting-config enable=on,target=native
```

## References

- [LAN9118 Datasheet](https://www.microchip.com/en-us/product/LAN9118)
- [QEMU MPS2 Documentation](https://www.qemu.org/docs/master/system/arm/mps2.html)
- [smoltcp](https://github.com/smoltcp-rs/smoltcp)
