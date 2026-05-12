// Core hardware abstraction traits.
//
// Every generated device struct field implements one of these traits.
// The traits are the contract between:
//   - Generated code (calls trait methods)
//   - Board support implementations (implements trait methods)
//
// WHY TRAITS NOT DIRECT HAL CALLS
//   Generated code names traits, not HAL types, in its method calls.
//   This means the same generated code compiles against any board
//   that provides an implementation — no board-specific logic leaks
//   into the user's program.
//
// TRAIT SURFACE
//   The exact method names here must match what rust_emit.rs emits:
//     set_high()  / set_low()  / is_high()  / is_low()  / toggle()
//     set_duty()  / duty()
//     write_str() / clear()
//     read()       (analog)

#![no_std]

/// Digital output pin — drives HIGH or LOW.
/// Implemented by board support for OUTPUT interfaces.
pub trait FerrumOutput {
    type Error;
    fn set_high(&mut self)  -> Result<(), Self::Error>;
    fn set_low(&mut self)   -> Result<(), Self::Error>;
    fn toggle(&mut self)    -> Result<(), Self::Error>;
    fn is_set_high(&self)   -> Result<bool, Self::Error>;
}

/// Digital input pin — reads HIGH or LOW.
/// Implemented by board support for INPUT interfaces.
pub trait FerrumInput {
    type Error;
    fn is_high(&self) -> Result<bool, Self::Error>;
    fn is_low(&self)  -> Result<bool, Self::Error>;
}

/// Analog input — reads a raw 0–1023 value.
/// Implemented by board support for ANALOG_INPUT interfaces.
pub trait FerrumAnalog {
    type Error;
    /// Returns a value in the range 0–1023.
    fn read(&mut self) -> Result<u16, Self::Error>;
    /// Returns a percentage in the range 0.0–100.0.
    fn read_percent(&mut self) -> Result<f32, Self::Error> {
        self.read().map(|v| v as f32 / 1023.0 * 100.0)
    }
}

/// PWM output — sets a duty cycle as a fraction 0.0–1.0.
/// Implemented by board support for PWM interfaces.
pub trait FerrumPwm {
    type Error;
    /// Set duty cycle. `value` must be in range 0.0–1.0.
    fn set_duty(&mut self, value: f32) -> Result<(), Self::Error>;
    fn duty(&self) -> f32;
}

/// Text display output.
/// Implemented by board support for DISPLAY interfaces.
pub trait FerrumDisplay {
    type Error;
    fn write_str(&mut self, s: &str)  -> Result<(), Self::Error>;
    fn write_int(&mut self, n: i32)   -> Result<(), Self::Error>;
    fn clear(&mut self)               -> Result<(), Self::Error>;
}

/// Blocking delay — used by DELAY statements.
pub trait FerrumDelay {
    fn delay_ms(&mut self, ms: u32);
    fn delay_us(&mut self, us: u32);
}

/// Pulse input/output pair — used for distance sensors.
/// TRIGGER = output pulse, ECHO = input timing.
pub trait FerrumPulse {
    type Error;
    fn trigger_pulse(&mut self, duration_us: u32) -> Result<(), Self::Error>;
    /// Returns pulse width in microseconds.
    fn read_echo_us(&mut self) -> Result<u32, Self::Error>;
}

#[cfg(test)]
mod trait_tests {
    use super::*;

    // ── Mock implementations for testing ─────────────────────────

    struct MockOutput { state: bool }
    impl FerrumOutput for MockOutput {
        type Error = ();
        fn set_high(&mut self)  -> Result<(), ()> { self.state = true;  Ok(()) }
        fn set_low(&mut self)   -> Result<(), ()> { self.state = false; Ok(()) }
        fn toggle(&mut self)    -> Result<(), ()> { self.state = !self.state; Ok(()) }
        fn is_set_high(&self)   -> Result<bool, ()> { Ok(self.state) }
    }

    struct MockInput  { state: bool }
    impl FerrumInput for MockInput {
        type Error = ();
        fn is_high(&self) -> Result<bool, ()> { Ok(self.state)  }
        fn is_low(&self)  -> Result<bool, ()> { Ok(!self.state) }
    }

    struct MockAnalog { value: u16 }
    impl FerrumAnalog for MockAnalog {
        type Error = ();
        fn read(&mut self) -> Result<u16, ()> { Ok(self.value) }
    }

    struct MockPwm { duty: f32 }
    impl FerrumPwm for MockPwm {
        type Error = ();
        fn set_duty(&mut self, v: f32) -> Result<(), ()> { self.duty = v; Ok(()) }
        fn duty(&self) -> f32 { self.duty }
    }

    struct MockDelay { total_ms: u32 }
    impl FerrumDelay for MockDelay {
        fn delay_ms(&mut self, ms: u32) { self.total_ms += ms; }
        fn delay_us(&mut self, us: u32) { self.total_ms += us / 1000; }
    }

    #[test]
    fn output_toggle() {
        let mut pin = MockOutput { state: false };
        pin.set_high().unwrap();
        assert!(pin.is_set_high().unwrap());
        pin.toggle().unwrap();
        assert!(!pin.is_set_high().unwrap());
    }

    #[test]
    fn analog_read_percent() {
        let mut sensor = MockAnalog { value: 512 };
        let pct = sensor.read_percent().unwrap();
        // 512 / 1023 * 100 ≈ 50.05
        assert!((pct - 50.05).abs() < 0.1);
    }

    #[test]
    fn analog_min_max() {
        let mut min_sensor = MockAnalog { value: 0 };
        let mut max_sensor = MockAnalog { value: 1023 };
        assert!((min_sensor.read_percent().unwrap() - 0.0).abs() < 0.01);
        assert!((max_sensor.read_percent().unwrap() - 100.0).abs() < 0.01);
    }

    #[test]
    fn pwm_duty_stored() {
        let mut pwm = MockPwm { duty: 0.0 };
        pwm.set_duty(0.75).unwrap();
        assert!((pwm.duty() - 0.75).abs() < f32::EPSILON);
    }

    #[test]
    fn delay_accumulates() {
        let mut d = MockDelay { total_ms: 0 };
        d.delay_ms(100);
        d.delay_ms(200);
        assert_eq!(d.total_ms, 300);
    }
}