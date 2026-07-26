#![no_std]
#![no_main]

use defmt::info;
use embassy_executor::Spawner;
use embassy_stm32::Config;
use embassy_stm32::rcc::{
    AHBPrescaler, APBPrescaler, Pll, PllDiv, PllMul, PllPreDiv, PllSource, Sysclk, VoltageScale,
};
use embassy_time::{Duration, Timer};

use defmt_rtt as _;
use panic_probe as _;

/// 160 MHz from the internal HSI16, so the clock tree carries no dependency on
/// which crystal the board populates: 16 MHz / 1 = 16 MHz PLL input, x20 = 320 MHz
/// VCO, / 2 = 160 MHz on PLL1_R.
///
/// RANGE1 is required above 110 MHz. The EPOD booster, PLL1MBOOST, and the flash
/// wait states are all derived by embassy-stm32 from the resulting SYSCLK.
fn clock_config() -> Config {
    let mut config = Config::default();

    config.rcc.hsi = true;
    config.rcc.pll1 = Some(Pll {
        source: PllSource::HSI,
        prediv: PllPreDiv::DIV1,
        mul: PllMul::MUL20,
        divp: None,
        divq: None,
        divr: Some(PllDiv::DIV2),
    });
    config.rcc.sys = Sysclk::PLL1_R;
    config.rcc.voltage_range = VoltageScale::RANGE1;
    config.rcc.ahb_pre = AHBPrescaler::DIV1;
    config.rcc.apb1_pre = APBPrescaler::DIV1;
    config.rcc.apb2_pre = APBPrescaler::DIV1;
    config.rcc.apb3_pre = APBPrescaler::DIV1;

    config
}

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let _p = embassy_stm32::init(clock_config());

    info!("CoworkOS firmware up: STM32U585 / Cortex-M33");

    let mut tick: u32 = 0;
    loop {
        info!("tick {}", tick);
        tick = tick.wrapping_add(1);
        Timer::after(Duration::from_secs(1)).await;
    }
}
