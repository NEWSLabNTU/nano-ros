//! Network poll callback and global state for STM32F4.

use stm32_eth::dma::EthernetDMA;

nros_smoltcp::define_network_state!(
    NETWORK_STATE: EthernetDMA<'static, 'static>,
    poll_via_ref = smoltcp_network_poll,
    device_arg = EthernetDMA<'static, 'static>,
    before_poll = {},
    after_poll = {
        nros_platform_stm32f4::clock::update_from_dwt();
    }
);
