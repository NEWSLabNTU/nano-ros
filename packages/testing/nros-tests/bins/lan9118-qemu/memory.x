/* Memory layout for MPS2-AN385 (QEMU Cortex-M3 with LAN9118 Ethernet) */
MEMORY
{
    /* Flash: 4MB */
    FLASH : ORIGIN = 0x00000000, LENGTH = 4M
    /* SRAM: 4MB */
    RAM : ORIGIN = 0x20000000, LENGTH = 4M
}
