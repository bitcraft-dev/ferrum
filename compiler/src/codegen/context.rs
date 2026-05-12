// Board context — maps TARGET strings to HAL crate details.
//
// RESPONSIBILITIES
//   The emitter needs to know:
//   - Which HAL crate to use (rp-hal, microbit, etc.)
//   - How to express a pin type for a given interface + pin number
//   - How to initialize the board peripherals in main()
//   - The scheduler model (EVERY → timer or polling loop)
//
// SUPPORTED BOARDS
//   "microbit_v2"  → microbit-v2 crate  (nRF52833)
//   "rp2040"       → rp-hal crate       (Raspberry Pi Pico)
//   "rp_pico"      → rp-hal crate       (alias for rp2040)
//
// EXTENSION
//   Adding a new board = adding one BoardProfile entry.
//   The emitter only calls context methods; it never names a board.

use crate::ast::{ConfigSection, ConfigKey, ConfigValue, InterfaceType, Qualifier};

// ----------------------------------------------------------------
// BoardProfile
// ----------------------------------------------------------------

/// Everything the emitter needs to know about a specific board.
#[derive(Debug, Clone)]
pub struct BoardProfile {
    /// Rust crate name for Cargo.toml dependency.
    pub crate_name:       &'static str,
    /// Rust crate version string.
    pub crate_version:    &'static str,
    /// The `use` path prefix for HAL types.
    pub hal_path:         &'static str,
    /// The type expression for a digital output pin.
    pub output_pin_type:  &'static str,
    /// The type expression for a digital input pin.
    pub input_pin_type:   &'static str,
    /// The type expression for an analog input pin.
    pub analog_pin_type:  &'static str,
    /// The type expression for a PWM channel.
    pub pwm_pin_type:     &'static str,
    /// The type expression for a display handle.
    pub display_type:     &'static str,
    /// Board init code — pasted into the top of generated main().
    pub board_init:       &'static str,
    /// How to get a pin by number: format string with {N} = pin number.
    pub pin_access:       &'static str,
    /// Scheduler model for EVERY blocks.
    pub scheduler_model:  SchedulerModel,
    /// Serial/UART write macro or function name.
    pub serial_write:     &'static str,
    /// Delay function expression. {MS} is replaced with milliseconds.
    pub delay_ms:         &'static str,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SchedulerModel {
    /// EVERY blocks use a polling counter in the main loop.
    /// Simple, no interrupt required, slight timing drift.
    PollingTick,
    /// EVERY blocks use hardware timer interrupts.
    /// More accurate, requires interrupt setup boilerplate.
    TimerInterrupt,
}

// ----------------------------------------------------------------
// Known board profiles
// ----------------------------------------------------------------

pub fn profile_for(target: &str) -> Option<&'static BoardProfile> {
    match target.to_lowercase().as_str() {
        "microbit_v2" | "microbit-v2" => Some(&MICROBIT_V2),
        "rp2040" | "rp_pico" | "rp-pico" => Some(&RP2040),
        _ => None,
    }
}

static MICROBIT_V2: BoardProfile = BoardProfile {
    crate_name:      "microbit-v2",
    crate_version:   "0.15",
    hal_path:        "microbit::hal",
    output_pin_type: "microbit::hal::gpio::Pin<microbit::hal::gpio::Output<microbit::hal::gpio::PushPull>>",
    input_pin_type:  "microbit::hal::gpio::Pin<microbit::hal::gpio::Input<microbit::hal::gpio::PullUp>>",
    analog_pin_type: "microbit::hal::gpio::Pin<microbit::hal::gpio::Input<microbit::hal::gpio::Floating>>",
    pwm_pin_type:    "microbit::hal::pwm::Channel",
    display_type:    "microbit::display::nonblocking::Display",
    board_init: "\
    let board = microbit::Board::take().unwrap();\n    \
    let mut delay = microbit::hal::Delay::new(core.SYST, clocks.system_clock.freq().to_Hz());\n",
    pin_access:      "board.pins.p{N}.into_push_pull_output()",
    scheduler_model: SchedulerModel::PollingTick,
    serial_write:    "defmt::println!",
    delay_ms:        "delay.delay_ms({MS}u32)",
};

static RP2040: BoardProfile = BoardProfile {
    crate_name:      "rp-pico",
    crate_version:   "0.9",
    hal_path:        "rp_pico::hal",
    output_pin_type: "rp_pico::hal::gpio::Pin<rp_pico::hal::gpio::DynPinId, rp_pico::hal::gpio::FunctionSio<rp_pico::hal::gpio::SioOutput>, rp_pico::hal::gpio::PullNone>",
    input_pin_type:  "rp_pico::hal::gpio::Pin<rp_pico::hal::gpio::DynPinId, rp_pico::hal::gpio::FunctionSio<rp_pico::hal::gpio::SioInput>, rp_pico::hal::gpio::PullUp>",
    analog_pin_type: "rp_pico::hal::gpio::Pin<rp_pico::hal::gpio::DynPinId, rp_pico::hal::gpio::FunctionNull, rp_pico::hal::gpio::PullNone>",
    pwm_pin_type:    "rp_pico::hal::pwm::Channel<rp_pico::hal::pwm::Slice<rp_pico::hal::pwm::FreeRunning>, rp_pico::hal::pwm::A>",
    display_type:    "/* DISPLAY not available on bare RP2040 — attach external display */",
    board_init: "\
    let mut pac = rp_pico::pac::Peripherals::take().unwrap();\n    \
    let core  = rp_pico::pac::CorePeripherals::take().unwrap();\n    \
    let mut watchdog = rp_pico::hal::Watchdog::new(pac.WATCHDOG);\n    \
    let clocks = rp_pico::hal::clocks::init_clocks_and_plls(\n        \
        rp_pico::XOSC_CRYSTAL_FREQ, pac.XOSC, pac.CLOCKS,\n        \
        pac.PLL_SYS, pac.PLL_USB, &mut pac.RESETS, &mut watchdog,\n    \
    ).ok().unwrap();\n    \
    let mut delay = cortex_m::delay::Delay::new(core.SYST, clocks.system_clock.freq().to_Hz());\n    \
    let sio   = rp_pico::hal::Sio::new(pac.SIO);\n    \
    let pins  = rp_pico::Pins::new(pac.IO_BANK0, pac.PADS_BANK0, sio.gpio_bank0, &mut pac.RESETS);\n",
    pin_access:      "pins.gpio{N}.into_push_pull_output()",
    scheduler_model: SchedulerModel::PollingTick,
    serial_write:    "defmt::println!",
    delay_ms:        "delay.delay_ms({MS})",
};

// ----------------------------------------------------------------
// EmitContext — runtime state threaded through the emitter
// ----------------------------------------------------------------

/// State collected from CONFIG and carried through emission.
#[derive(Debug)]
pub struct EmitContext {
    pub profile:      &'static BoardProfile,
    pub debug:        bool,
    pub serial_baud:  Option<u32>,
    pub optimize:     OptimizeLevel,
    pub debounce_ms:  Option<u32>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum OptimizeLevel {
    Speed,
    Size,
    None,
}

impl EmitContext {
    /// Build from a parsed CONFIG section.
    /// Returns an error string if TARGET is missing or unrecognised.
    pub fn from_config(config: &Option<ConfigSection>) -> Result<Self, String> {
        let mut target:       Option<String>  = None;
        let mut debug                         = false;
        let mut serial_baud:  Option<u32>     = None;
        let mut optimize                      = OptimizeLevel::None;
        let mut debounce_ms:  Option<u32>     = None;

        if let Some(cfg) = config {
            for item in &cfg.items {
                match (&item.key, &item.value) {
                    (ConfigKey::Target,       ConfigValue::Str(s))  => target = Some(s.clone()),
                    (ConfigKey::Debug,        ConfigValue::Bool(b)) => debug = *b,
                    (ConfigKey::Serial,       ConfigValue::Int(n))  => serial_baud = Some(*n as u32),
                    (ConfigKey::DebounceMs,   ConfigValue::Int(n))  => debounce_ms = Some(*n as u32),
                    (ConfigKey::Optimize,     ConfigValue::Str(s))  => {
                        optimize = match s.as_str() {
                            "speed" => OptimizeLevel::Speed,
                            "size"  => OptimizeLevel::Size,
                            _       => OptimizeLevel::None,
                        };
                    }
                    _ => {}
                }
            }
        }

        let target_str = target.as_deref().unwrap_or("microbit_v2");
        let profile    = profile_for(target_str)
            .ok_or_else(|| format!(
                "Unknown TARGET '{}'. Supported boards: microbit_v2, rp2040, rp_pico",
                target_str
            ))?;

        Ok(EmitContext { profile, debug, serial_baud, optimize, debounce_ms })
    }
}

#[cfg(test)]
mod context_tests {
    use super::*;

    #[test]
    fn microbit_profile_resolves() {
        let p = profile_for("microbit_v2");
        assert!(p.is_some());
        assert_eq!(p.unwrap().crate_name, "microbit-v2");
    }

    #[test]
    fn rp_pico_aliases_resolve() {
        assert!(profile_for("rp2040").is_some());
        assert!(profile_for("rp_pico").is_some());
        assert!(profile_for("rp-pico").is_some());
    }

    #[test]
    fn unknown_board_returns_none() {
        assert!(profile_for("arduino_uno").is_none());
    }

    #[test]
    fn emit_context_defaults_to_microbit() {
        let ctx = EmitContext::from_config(&None).unwrap();
        assert_eq!(ctx.profile.crate_name, "microbit-v2");
        assert!(!ctx.debug);
    }
}