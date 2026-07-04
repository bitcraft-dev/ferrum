// Raspberry Pi Pico (RP2040) board support.
//
// HAL CRATE: rp-pico = "0.9"
//
// The RP2040 uses a different GPIO type system from the nRF52.
// All pin types are generic over DynPinId and a function type.
// We implement the traits on the concrete bound types the emitter
// generates (matching the type strings in BoardProfile).

#[cfg(feature = "rp2040")]

use crate::traits::{FerrumAnalog, FerrumDelay, FerrumDisplay, FerrumInput, FerrumOutput, FerrumPulse, FerrumPwm};

use rp_pico::hal::gpio::{
    DynPinId, FunctionSio, SioOutput, SioInput, Pin, PullUp, PullNone,
};

// ── Output pin ────────────────────────────────────────────────────

impl FerrumOutput for Pin<DynPinId, FunctionSio<SioOutput>, PullNone> {
    type Error = core::convert::Infallible;

    fn set_high(&mut self) -> Result<(), Self::Error> {
        use embedded_hal::digital::v2::OutputPin;
        OutputPin::set_high(self)
    }

    fn set_low(&mut self) -> Result<(), Self::Error> {
        use embedded_hal::digital::v2::OutputPin;
        OutputPin::set_low(self)
    }

    fn toggle(&mut self) -> Result<(), Self::Error> {
        use embedded_hal::digital::v2::ToggleableOutputPin;
        ToggleableOutputPin::toggle(self)
    }

    fn is_set_high(&self) -> Result<bool, Self::Error> {
        use embedded_hal::digital::v2::StatefulOutputPin;
        StatefulOutputPin::is_set_high(self)
    }
}

// ── Input pin ─────────────────────────────────────────────────────

impl FerrumInput for Pin<DynPinId, FunctionSio<SioInput>, PullUp> {
    type Error = core::convert::Infallible;

    fn is_high(&self) -> Result<bool, Self::Error> {
        use embedded_hal::digital::v2::InputPin;
        InputPin::is_high(self)
    }

    fn is_low(&self) -> Result<bool, Self::Error> {
        use embedded_hal::digital::v2::InputPin;
        InputPin::is_low(self)
    }
}

// ── Analog input ──────────────────────────────────────────────────

use rp_pico::hal::adc::{Adc, AdcPin};
use rp_pico::hal::gpio::FunctionNull;

pub struct Rp2040AnalogPin {
    adc: Adc,
    pin: AdcPin<Pin<DynPinId, FunctionNull, PullNone>>,
}

impl Rp2040AnalogPin {
    pub fn new(adc: Adc, pin: AdcPin<Pin<DynPinId, FunctionNull, PullNone>>) -> Self {
        Rp2040AnalogPin { adc, pin }
    }
}

impl FerrumAnalog for Rp2040AnalogPin {
    type Error = ();

    fn read(&mut self) -> Result<u16, Self::Error> {
        use embedded_hal::adc::OneShot;
        // RP2040 ADC is 12-bit (0..4095). Scale to 0..1023.
        let raw: u16 = self.adc.read(&mut self.pin).map_err(|_| ())?;
        Ok((raw as u32 * 1023 / 4095) as u16)
    }
}

// ── PWM output ────────────────────────────────────────────────────

use rp_pico::hal::pwm::{Channel, FreeRunning, Slice, A};

pub struct Rp2040PwmChannel {
    channel:  Channel<Slice<FreeRunning>, A>,
    duty:     f32,
    max_duty: u16,
}

impl Rp2040PwmChannel {
    pub fn new(channel: Channel<Slice<FreeRunning>, A>, max_duty: u16) -> Self {
        Rp2040PwmChannel { channel, duty: 0.0, max_duty }
    }
}

impl FerrumPwm for Rp2040PwmChannel {
    type Error = ();

    fn set_duty(&mut self, value: f32) -> Result<(), Self::Error> {
        let clamped  = value.max(0.0).min(1.0);
        self.duty    = clamped;
        let raw      = (clamped * self.max_duty as f32) as u16;
        self.channel.set_duty(raw);
        Ok(())
    }

    fn duty(&self) -> f32 { self.duty }
}

// ── Display ───────────────────────────────────────────────────────
// The RP2040 has no built-in display. A generated program with a
// DISPLAY interface on RP2040 must attach an external display
// (e.g. SSD1306 OLED over I2C). This stub allows compilation;
// a real display driver would replace it.

pub struct Rp2040Display;

impl FerrumDisplay for Rp2040Display {
    type Error = ();
    fn write_str(&mut self, _s: &str) -> Result<(), ()> { Ok(()) }
    fn write_int(&mut self, _n: i32)  -> Result<(), ()> { Ok(()) }
    fn clear(&mut self)               -> Result<(), ()> { Ok(()) }
}

// ── Pulse (TRIGGER + ECHO) ──────────────────────────────────────────
//
// Used by a PULSE composite device (e.g. an HC-SR04 ultrasonic
// distance sensor). See microbit_v2.rs's `MicrobitPulsePin` for the
// full design note — the `FerrumPulse` trait is the entire contract
// generated code relies on, so each board is free to time the echo
// pulse however suits its hardware.
//
// TIMING: the RP2040 is a Cortex-M0+, which has no DWT cycle counter
// (that's an M3/M4/M7 feature) — unlike the microbit_v2 driver, this
// uses the RP2040's own free-running hardware TIMER peripheral
// (`rp_pico::hal::Timer`), which already counts in microseconds
// directly, no cycles-to-time conversion needed.
pub struct Rp2040PulsePin {
    trigger: Pin<DynPinId, FunctionSio<SioOutput>, PullNone>,
    echo:    Pin<DynPinId, FunctionSio<SioInput>, PullUp>,
    timer:   rp_pico::hal::Timer,
}

impl Rp2040PulsePin {
    pub fn new(
        trigger: Pin<DynPinId, FunctionSio<SioOutput>, PullNone>,
        echo:    Pin<DynPinId, FunctionSio<SioInput>, PullUp>,
        timer:   rp_pico::hal::Timer,
    ) -> Self {
        Rp2040PulsePin { trigger, echo, timer }
    }
}

/// Give up waiting for / measuring an echo after this long — see
/// microbit_v2.rs's `TIMEOUT_US` for why.
const TIMEOUT_US: u64 = 30_000;

impl FerrumPulse for Rp2040PulsePin {
    type Error = core::convert::Infallible;

    fn trigger_pulse(&mut self, duration_us: u32) -> Result<(), Self::Error> {
        use embedded_hal::digital::v2::OutputPin;
        OutputPin::set_high(&mut self.trigger)?;
        let start = self.timer.get_counter().ticks();
        while self.timer.get_counter().ticks().wrapping_sub(start) < duration_us as u64 {}
        OutputPin::set_low(&mut self.trigger)?;
        Ok(())
    }

    fn read_echo_us(&mut self) -> Result<u32, Self::Error> {
        use embedded_hal::digital::v2::InputPin;

        let wait_start = self.timer.get_counter().ticks();
        while InputPin::is_low(&self.echo)? {
            if self.timer.get_counter().ticks().wrapping_sub(wait_start) > TIMEOUT_US {
                return Ok(0); // no echo — sensor likely not connected
            }
        }

        let echo_start = self.timer.get_counter().ticks();
        while InputPin::is_high(&self.echo)? {
            if self.timer.get_counter().ticks().wrapping_sub(echo_start) > TIMEOUT_US {
                break; // out of range
            }
        }
        let echo_end = self.timer.get_counter().ticks();

        Ok(echo_end.wrapping_sub(echo_start) as u32)
    }
}

// ── Delay ─────────────────────────────────────────────────────────

impl FerrumDelay for cortex_m::delay::Delay {
    fn delay_ms(&mut self, ms: u32) {
        cortex_m::delay::Delay::delay_ms(self, ms);
    }
    fn delay_us(&mut self, us: u32) {
        cortex_m::delay::Delay::delay_us(self, us);
    }
}