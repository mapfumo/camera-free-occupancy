//! Shared clock tree configuration.
//!
//! Kept in its own module so every binary in this crate — the application and
//! the bring-up tools under `src/bin/` — runs the core at the same frequency.
//! A scan that passes at one SYSCLK and fails at another is a debugging trap
//! worth designing out.

use embassy_stm32::Config;
use embassy_stm32::rcc::{
    AHBPrescaler, APBPrescaler, Pll, PllDiv, PllMul, PllPreDiv, PllSource, Sysclk, VoltageScale,
};

/// 160 MHz from the internal HSI16, so the clock tree carries no dependency on
/// which crystal the board populates: 16 MHz / 1 = 16 MHz PLL input, x20 = 320 MHz
/// VCO, / 2 = 160 MHz on PLL1_R.
///
/// RANGE1 is required above 110 MHz. The EPOD booster, PLL1MBOOST, and the flash
/// wait states are all derived by embassy-stm32 from the resulting SYSCLK.
pub fn config() -> Config {
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
