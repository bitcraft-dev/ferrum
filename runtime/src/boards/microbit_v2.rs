// BBC micro:bit v2 board support.
//
// This module provides concrete implementations of the Ferrum runtime
// traits for the nRF52833 chip on the micro:bit v2.
//
// HAL CRATE: microbit-v2 = "0.15"
//
// GENERATED CODE COMPATIBILITY
//   The emitter generates code that calls:
//     pin.set_high().unwrap()
//     pin.set_low().unwrap()
//     pin.toggle().unwrap()
//     pin.is_high().unwrap()
//     pin.is_low().unwrap()
//     pin.set_duty(value)
//     pin.read()                  (analog)
//     display.write_str("text")
//
//   Each of these maps to the HAL method below, wrapped in the
//   trait impl. The generated structs use `dyn FerrumOutput` etc.
//   so the compiler knows which vtable to use.
//
// NOTE ON TYPES
//   The micro:bit HAL uses `Pin<Output<PushPull>>` for outputs and
//   `Pin<Input<PullUp>>` for inputs. These are the concrete types
//   that implement the traits defined in traits.rs.

// This file is conditionally compiled only when building for micro:bit v2.
// The cfg gate is set by the generated Cargo.toml features section.
#[cfg(feature = "microbit_v2")]

use crate::traits::{FerrumAnalog, FerrumDelay, FerrumDisplay, FerrumInput, FerrumOutput, FerrumPwm};

// ── Output pin ────────────────────────────────────────────────────

use microbit::hal::gpio::{Output, Pin, PushPull, Input, PullUp, Floating};

impl FerrumOutput for Pin<Output<PushPull>> {
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

impl FerrumInput for Pin<Input<PullUp>> {
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

// The micro:bit HAL provides a Saadc (Successive Approximation ADC).
// We wrap the channel read into the FerrumAnalog trait.

use microbit::hal::saadc::{Saadc, SaadcConfig};

pub struct MicrobitAnalogPin {
    saadc:   Saadc,
    channel: microbit::hal::saadc::SaadcInput,
}

impl MicrobitAnalogPin {
    pub fn new(saadc: Saadc, channel: microbit::hal::saadc::SaadcInput) -> Self {
        MicrobitAnalogPin { saadc, channel }
    }
}

impl FerrumAnalog for MicrobitAnalogPin {
    type Error = ();

    fn read(&mut self) -> Result<u16, Self::Error> {
        // SAADC returns i16 (-32768..32767 for 16-bit mode).
        // Scale to 0..1023 (10-bit equivalent) to match spec.
        let raw: i16 = self.saadc.read(&mut self.channel).map_err(|_| ())?;
        let scaled = ((raw.max(0) as u32 * 1023) / 32767) as u16;
        Ok(scaled)
    }
}

// ── PWM output ────────────────────────────────────────────────────

use microbit::hal::pwm::{Pwm, Instance as PwmInstance};

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

// The micro:bit v2 has a built-in 5×5 LED matrix.
// For text output we use the microbit display driver.

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
        // Scroll the string across the 5×5 matrix.
        // Each character is displayed for 200ms.
        for ch in s.chars() {
            let img = char_to_image(ch);
            self.display.show(&mut self.timer, img, 200);
        }
        Ok(())
    }

    fn write_int(&mut self, n: i32) -> Result<(), Self::Error> {
        // Convert integer to string and display it.
        // Uses a small no_alloc integer formatter.
        let mut buf = [0u8; 12];
        let s = fmt_int(n, &mut buf);
        self.write_str(s)
    }

    fn clear(&mut self) -> Result<(), Self::Error> {
        self.display.clear();
        Ok(())
    }
}

/// Minimal 5×5 LED image for a character.
/// Full character set omitted for brevity — production build
/// would include a complete bitmap font.
fn char_to_image(ch: char) -> microbit::display::image::GreyscaleImage {
    use microbit::display::image::GreyscaleImage;
    match ch {
        ' ' => GreyscaleImage::blank(),
        _   => {
            // Placeholder: show a dot pattern for unknown chars
            GreyscaleImage::new(&[
                [0, 0, 0, 0, 0],
                [0, 9, 9, 9, 0],
                [0, 9, 0, 9, 0],
                [0, 9, 9, 9, 0],
                [0, 0, 0, 0, 0],
            ])
        }
    }
}

// ── Delay ─────────────────────────────────────────────────────────

impl FerrumDelay for microbit::hal::Delay {
    fn delay_ms(&mut self, ms: u32) {
        use embedded_hal::blocking::delay::DelayMs;
        DelayMs::delay_ms(self, ms);
    }
    fn delay_us(&mut self, us: u32) {
        use embedded_hal::blocking::delay::DelayUs;
        DelayUs::delay_us(self, us);
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