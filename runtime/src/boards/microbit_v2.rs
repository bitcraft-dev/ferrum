// BBC micro:bit v2 board support.
//
// This module provides concrete implementations of the Ferrum runtime
// traits for the nRF52833 chip on the micro:bit v2.
//
// HAL CRATE: microbit-v2 = "0.15" (pulls nrf52833-hal 0.18 and
// embedded-hal 1.0).
//
// EMBEDDED-HAL VERSION
//   This board targets embedded-hal 1.0. The digital pin traits
//   (`OutputPin`, `InputPin`, `StatefulOutputPin`) and the delay trait
//   (`DelayNs`) all live in `embedded_hal::` (no `::v2::` / `::blocking::`
//   modules — those were the 0.2 layout). One consequence rippling up
//   into `traits.rs`: reading a pin's state is `&mut self` in 1.0, so
//   `FerrumInput`/`FerrumOutput`'s read methods are `&mut self` too.
//
// GENERATED CODE COMPATIBILITY
//   The emitter generates code that calls the Ferrum trait methods
//   (set_high / set_low / toggle / is_high / is_low / set_duty / read /
//   write_str). Each maps to the HAL method below via the trait impl,
//   so no board-specific detail leaks into the generated program.
//
// This module is compiled only when the `microbit_v2` feature is on —
// the gate lives on `pub mod microbit_v2;` in `boards/mod.rs`, so no
// per-item `#[cfg]` is needed (an earlier stray one gated only the
// import beneath it, which was a latent bug).

use crate::traits::{FerrumAnalog, FerrumDelay, FerrumDisplay, FerrumInput, FerrumOutput, FerrumPulse, FerrumPwm};

use microbit::hal::gpio::{Floating, Input, Output, Pin, PullUp, PushPull};

// ── Output pin ────────────────────────────────────────────────────

impl FerrumOutput for Pin<Output<PushPull>> {
    // embedded-hal 1.0's nRF pin impls are infallible.
    type Error = core::convert::Infallible;

    fn set_high(&mut self) -> Result<(), Self::Error> {
        use embedded_hal::digital::OutputPin;
        OutputPin::set_high(self)
    }

    fn set_low(&mut self) -> Result<(), Self::Error> {
        use embedded_hal::digital::OutputPin;
        OutputPin::set_low(self)
    }

    fn toggle(&mut self) -> Result<(), Self::Error> {
        // 1.0 folded the old `ToggleableOutputPin` into
        // `StatefulOutputPin::toggle`.
        use embedded_hal::digital::StatefulOutputPin;
        StatefulOutputPin::toggle(self)
    }

    fn is_set_high(&mut self) -> Result<bool, Self::Error> {
        use embedded_hal::digital::StatefulOutputPin;
        StatefulOutputPin::is_set_high(self)
    }
}

// ── Input pin ─────────────────────────────────────────────────────

impl FerrumInput for Pin<Input<PullUp>> {
    type Error = core::convert::Infallible;

    fn is_high(&mut self) -> Result<bool, Self::Error> {
        use embedded_hal::digital::InputPin;
        InputPin::is_high(self)
    }

    fn is_low(&mut self) -> Result<bool, Self::Error> {
        use embedded_hal::digital::InputPin;
        InputPin::is_low(self)
    }
}

// ── Analog input ──────────────────────────────────────────────────

// The micro:bit HAL provides a Saadc (Successive Approximation ADC).
// nrf-hal 0.18 replaced the old `SaadcInput` marker type + `OneShot`
// trait with an inherent `Saadc::read_channel(&mut self, &mut PIN)`
// over any pin implementing `saadc::Channel`, so this wrapper is now
// generic over the channel pin type.

use microbit::hal::saadc::{Channel, Saadc};

pub struct MicrobitAnalogPin<PIN: Channel> {
    saadc:   Saadc,
    channel: PIN,
}

impl<PIN: Channel> MicrobitAnalogPin<PIN> {
    pub fn new(saadc: Saadc, channel: PIN) -> Self {
        MicrobitAnalogPin { saadc, channel }
    }
}

impl<PIN: Channel> FerrumAnalog for MicrobitAnalogPin<PIN> {
    type Error = ();

    fn read(&mut self) -> Result<u16, Self::Error> {
        // SAADC returns i16. Scale to 0..1023 (10-bit equivalent) to
        // match the spec's analog range.
        let raw: i16 = self.saadc.read_channel(&mut self.channel).map_err(|_| ())?;
        let scaled = ((raw.max(0) as u32 * 1023) / 32767) as u16;
        Ok(scaled)
    }
}

// ── PWM output ────────────────────────────────────────────────────

use microbit::hal::pwm::{Instance as PwmInstance, Pwm};

pub struct MicrobitPwmChannel<T: PwmInstance> {
    pwm:      Pwm<T>,
    duty:     f32,
    max_duty: u16,
}

impl<T: PwmInstance> MicrobitPwmChannel<T> {
    pub fn new(pwm: Pwm<T>) -> Self {
        let max_duty = pwm.max_duty();
        MicrobitPwmChannel { pwm, duty: 0.0, max_duty }
    }
}

impl<T: PwmInstance> FerrumPwm for MicrobitPwmChannel<T> {
    type Error = ();

    fn set_duty(&mut self, value: f32) -> Result<(), Self::Error> {
        let clamped   = value.max(0.0).min(1.0);
        self.duty     = clamped;
        let raw_duty  = (clamped * self.max_duty as f32) as u16;
        self.pwm.set_duty_on_common(raw_duty);
        Ok(())
    }

    fn duty(&self) -> f32 { self.duty }
}

// ── Display ───────────────────────────────────────────────────────

// The micro:bit v2 has a built-in 5×5 LED matrix. The blocking display
// driver's `show` now takes a raw `[[u8; 5]; 5]` greyscale frame
// directly (0–9 per LED) and drives its timing off any
// `embedded_hal::delay::DelayNs` — which `Timer<TIMER1>` implements —
// so no `GreyscaleImage` wrapper type is involved anymore.

use microbit::display::blocking::Display;
use microbit::hal::Timer;
use microbit::pac::TIMER1;

pub struct MicrobitDisplay {
    display: Display,
    timer:   Timer<TIMER1>,
}

impl MicrobitDisplay {
    pub fn new(display: Display, timer: Timer<TIMER1>) -> Self {
        MicrobitDisplay { display, timer }
    }
}

impl FerrumDisplay for MicrobitDisplay {
    type Error = ();

    fn write_str(&mut self, s: &str) -> Result<(), Self::Error> {
        // Scroll the string across the 5×5 matrix, 200ms per character.
        for ch in s.chars() {
            let frame = char_to_frame(ch);
            self.display.show(&mut self.timer, frame, 200);
        }
        Ok(())
    }

    fn write_int(&mut self, n: i32) -> Result<(), Self::Error> {
        let mut buf = [0u8; 12];
        let s = fmt_int(n, &mut buf);
        self.write_str(s)
    }

    fn clear(&mut self) -> Result<(), Self::Error> {
        self.display.clear();
        Ok(())
    }
}

/// Minimal 5×5 greyscale frame for a character (0–9 per LED).
/// Full character set omitted for brevity — a production build would
/// include a complete bitmap font (see ROADMAP: micro:bit bitmap font).
fn char_to_frame(ch: char) -> [[u8; 5]; 5] {
    match ch {
        ' ' => [[0; 5]; 5],
        // Placeholder: a hollow box for any non-space char.
        _ => [
            [0, 0, 0, 0, 0],
            [0, 9, 9, 9, 0],
            [0, 9, 0, 9, 0],
            [0, 9, 9, 9, 0],
            [0, 0, 0, 0, 0],
        ],
    }
}

// ── Delay ─────────────────────────────────────────────────────────

impl FerrumDelay for microbit::hal::Delay {
    fn delay_ms(&mut self, ms: u32) {
        use embedded_hal::delay::DelayNs;
        DelayNs::delay_ms(self, ms);
    }
    fn delay_us(&mut self, us: u32) {
        use embedded_hal::delay::DelayNs;
        DelayNs::delay_us(self, us);
    }
}

// ── Pulse (TRIGGER + ECHO) ──────────────────────────────────────────
//
// Used by a PULSE composite device (e.g. an HC-SR04 ultrasonic
// distance sensor): one output pin sends a brief trigger pulse, one
// input pin's HIGH duration afterward is the echo's time-of-flight.
// This struct is the ONLY board-specific code for PULSE support —
// generated code (and the rest of the compiler) only ever calls the
// `FerrumPulse` trait's two methods on it. A different board adds
// PULSE support by writing its own equivalent struct against its own
// timing primitives and pointing `BoardProfile::pulse_pin_type` /
// `pulse_pin_new` at it; nothing else changes.
//
// TIMING: the nRF52833 is a Cortex-M4, which has the DWT unit's
// free-running cycle counter (CYCCNT). We read it before/after the
// echo pulse and convert the cycle delta to microseconds using the
// core clock frequency. A board without DWT (e.g. RP2040's Cortex-M0+)
// would need a different mechanism internally — see rp2040.rs — but
// that's invisible from here, which is the point of the trait boundary.
pub struct MicrobitPulsePin {
    trigger: Pin<Output<PushPull>>,
    echo:    Pin<Input<Floating>>,
}

impl MicrobitPulsePin {
    pub fn new(trigger: Pin<Output<PushPull>>, echo: Pin<Input<Floating>>) -> Self {
        MicrobitPulsePin { trigger, echo }
    }
}

/// nRF52833 default core clock (HFCLK) in Hz.
const CPU_HZ: u32 = 64_000_000;
/// Give up waiting for / measuring an echo after this long — an
/// HC-SR04's own datasheet caps a valid round trip well under this,
/// so a disconnected sensor or one facing open air can't hang the
/// caller forever.
const TIMEOUT_US: u32 = 30_000;

fn cycles_to_us(cycles: u32) -> u32 {
    cycles / (CPU_HZ / 1_000_000)
}

fn us_to_cycles(us: u32) -> u32 {
    us.saturating_mul(CPU_HZ / 1_000_000)
}

fn cycle_count() -> u32 {
    cortex_m::peripheral::DWT::cycle_count()
}

impl FerrumPulse for MicrobitPulsePin {
    type Error = core::convert::Infallible;

    fn trigger_pulse(&mut self, duration_us: u32) -> Result<(), Self::Error> {
        use embedded_hal::digital::OutputPin;
        OutputPin::set_high(&mut self.trigger)?;
        let start = cycle_count();
        while cycle_count().wrapping_sub(start) < us_to_cycles(duration_us) {}
        OutputPin::set_low(&mut self.trigger)?;
        Ok(())
    }

    fn read_echo_us(&mut self) -> Result<u32, Self::Error> {
        use embedded_hal::digital::InputPin;

        // Wait for the sensor to start its echo pulse.
        let wait_start = cycle_count();
        while InputPin::is_low(&mut self.echo)? {
            if cycles_to_us(cycle_count().wrapping_sub(wait_start)) > TIMEOUT_US {
                return Ok(0); // no echo — sensor likely not connected
            }
        }

        // Measure how long it stays high.
        let echo_start = cycle_count();
        while InputPin::is_high(&mut self.echo)? {
            if cycles_to_us(cycle_count().wrapping_sub(echo_start)) > TIMEOUT_US {
                break; // out of range
            }
        }
        let echo_end = cycle_count();

        Ok(cycles_to_us(echo_end.wrapping_sub(echo_start)))
    }
}

// ── Integer formatter (no_alloc) ──────────────────────────────────

fn fmt_int(n: i32, buf: &mut [u8; 12]) -> &str {
    if n == 0 {
        buf[0] = b'0';
        return core::str::from_utf8(&buf[..1]).unwrap();
    }
    let negative = n < 0;
    let mut val  = if negative { -(n as i64) } else { n as i64 };
    let mut pos  = 11usize;
    while val > 0 {
        buf[pos] = b'0' + (val % 10) as u8;
        val /= 10;
        pos -= 1;
    }
    if negative {
        buf[pos] = b'-';
        pos -= 1;
    }
    core::str::from_utf8(&buf[pos + 1..]).unwrap()
}
