#![no_std]
#![no_main]

use defmt::info;
use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};

use defmt_rtt as _;
use panic_probe as _;

use coworkos_firmware::clock;

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let _p = embassy_stm32::init(clock::config());

    info!("CoworkOS firmware up: STM32U585 / Cortex-M33");

    let mut tick: u32 = 0;
    loop {
        info!("tick {}", tick);
        tick = tick.wrapping_add(1);
        Timer::after(Duration::from_secs(1)).await;
    }
}
