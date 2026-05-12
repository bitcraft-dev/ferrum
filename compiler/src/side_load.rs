// ================================================================
// FERRUM RUNTIME
// ferrum_runtime.rs
//
// The runtime crate that generated .fe programs link against.
// Also contains the TURN statement bug fix for rust_emit.rs.
//
// Each module is clearly delimited by:
//
//   ╔══ MODULE: path/to/file ══╗
//   ...
//   ╚══ END MODULE: path/to/file ══╝
//
// Contents:
//   1.  BUGFIX: rust_emit.rs StmtKind::Turn correction
//   2.  runtime/src/lib.rs          — crate root + re-exports
//   3.  runtime/src/traits.rs       — FerrumPin, FerrumPwm, FerrumDisplay
//   4.  runtime/src/scheduler.rs    — EVERY polling scheduler
//   5.  runtime/src/boards/mod.rs   — board selector
//   6.  runtime/src/boards/microbit_v2.rs
//   7.  runtime/src/boards/rp2040.rs
//   8.  runtime/src/debounce.rs     — button debounce helper
//   9.  runtime/Cargo.toml
//  10.  Updated codegen/context.rs  — pin_access fixed per board
// ================================================================


// ╔══ MODULE: ferrum/runtime/src/traits.rs ══╗
//

// ╚══ END MODULE: ferrum/runtime/src/traits.rs ══╝


// ╔══ MODULE: ferrum/runtime/src/scheduler.rs ══╗

// ╚══ END MODULE: ferrum/runtime/src/scheduler.rs ══╝


// ╔══ MODULE: ferrum/runtime/src/debounce.rs ══╗
//

// ╚══ END MODULE: ferrum/runtime/src/debounce.rs ══╝


// ╔══ MODULE: ferrum/runtime/src/boards/microbit_v2.rs ══╗
//

// ╚══ END MODULE: ferrum/runtime/src/boards/microbit_v2.rs ══╝


// ╔══ MODULE: ferrum/runtime/src/boards/rp2040.rs ══╗
//

// ╚══ END MODULE: ferrum/runtime/src/boards/rp2040.rs ══╝


// ╔══ MODULE: ferrum/runtime/src/boards/mod.rs ══╗

// ╚══ END MODULE: ferrum/runtime/src/boards/mod.rs ══╝


// ╔══ MODULE: ferrum/runtime/src/lib.rs ══╗
//

// ╚══ END MODULE: ferrum/runtime/src/lib.rs ══╝


// ╔══ FILE: ferrum/runtime/Cargo.toml ══╗
//

// ╚══ END FILE: ferrum/runtime/Cargo.toml ══╝



// ================================================================
// RUNTIME CRATE STRUCTURE
// ================================================================
//
// ferrum/runtime/
// ├── Cargo.toml
// └── src/
//     ├── lib.rs
//     ├── traits.rs
//     ├── scheduler.rs
//     ├── debounce.rs
//     └── boards/
//         ├── mod.rs
//         ├── microbit_v2.rs
//         └── rp2040.rs
//
// ================================================================
// HOW GENERATED CODE USES THE RUNTIME
// ================================================================
//
// Every generated .rs file will have these additions at the top
// (the emitter's emit_imports() adds them):
//
//   use ferrum_runtime::*;
//   use ferrum_runtime::boards::microbit_v2::*;   // board-specific
//
// Device struct fields will use the concrete HAL types from the
// board support module. Trait method calls on those fields
// (set_high, is_low, set_duty, read, etc.) resolve through
// the trait impls defined here.
//
// ================================================================
// WHAT STILL NEEDS DOING BEFORE A REAL FLASH
// ================================================================
//
//  1. The micro:bit v2 char_to_image() function needs a full
//     5×5 bitmap font for at least A-Z, 0-9, and common symbols.
//
//  2. The TURN bug fix in rust_emit.rs (see top of this file)
//     must be applied before any TURN statement will compile.
//
//  3. The codegen/mod.rs CodegenResult must be updated to include
//     memory_x output (see updated context section above).
//
//  4. The main.rs driver must write memory.x alongside the .rs
//     file so the linker can find it.
//
//  5. Integration test: run ferrum compile soil_moisture.fe and
//     verify the output passes cargo check --target thumbv7em-none-eabihf.
// ================================================================