// ================================================================
// STILL TO COME: codegen/rust_emit.rs
// ================================================================
//
// The codegen pass (rust_emit.rs) walks the annotated AST and
// emits a valid Rust source file that compiles against the target
// board's HAL crate. It is the final pass before `cargo build`.
//
// Key mappings from the spec's §20 Rust transpilation layer:
//
//   DEFINE Led AS { OUTPUT, PWM BRIGHTNESS }
//   → pub struct Led { output: OutputPin, brightness: PwmPin }
//
//   CREATE Led status ON { PIN 3, PIN 4 }
//   → let status = Led::new(pins.p3, pins.p4);
//
//   TURN status HIGH
//   → status.output.set_high().unwrap();
//
//   SET status BRIGHTNESS 0.75
//   → status.brightness.set_duty(0.75);
//
//   FUNCTION pulse BORROW Led: led { ... }
//   → fn pulse(led: &mut Led) { ... }
//
//   GIVE   → pass by value (move)
//   LEND   → &T (shared reference)
//   BORROW → &mut T (mutable reference)
//
//   EVERY 1000ms { ... }
//   → Timer interrupt or cooperative scheduler tick
//
//   LOOP { ... }
//   → loop { ... }