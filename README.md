# Ferrum: Embedded Systems DSL

**Ferrum** is a **domain-specific language (DSL) for embedded systems programming**, designed to guide developers from foundational concepts to advanced embedded design. It teaches hardware interaction through capability-based interfaces and an ownership and borrowing model inspired by Rust, making it suitable for `early learners`, `academic environments`, and `professional exploration`, while *compiling to Rust* as an intermediate target.

## Language Philosophy

Ferrum is built on five core principles:

1. **Hardware is Explicit** - Every pin, interface, and device must be declared before use
2. **Types Carry Meaning** - Types encode real-world constraints (e.g., `Percentage` ≠ generic number)
3. **Errors are Teachers** - Error messages explain issues, point to locations, and suggest fixes
4. **Syntax Maps to Hardware Thinking** - Keywords reflect hardware behavior, not abstract patterns
5. **Ownership Reflects Physical Reality** - Devices belong to one place at a time (physical exclusivity of hardware pins)

## Key Features

### Interface Types
- **INPUT** - Digital input (HIGH/LOW)
- **OUTPUT** - Digital output
- **ANALOG_INPUT** - Voltage reading
- **PWM** - Pulse-width modulation
- **DISPLAY** - Display output
- **PULSE** - Pulse timing

### Data Types
- Integer, Decimal, Percentage (0.0-100.0), Boolean, String, Byte (0-255)
- No implicit type coercion - range violations caught at compile-time

### Ownership Keywords
- **GIVE** - Transfer ownership
- **LEND** - Read-only borrow
- **BORROW** - Mutable borrow

### Program Structure
Programs are organized in ordered sections:
```
CONFIG → DEFINE → CREATE → DECLARE → FUNCTION → RUN
```
(Only `RUN` is required)

## Project Structure

```
ferrum/                          ← project root
│
├── spec/
│   ├── ferrum_spec_v1.3.md
│   └── ferrum_grammar_v1.3.ebnf
│
├── compiler/                     ← the core pipeline
│   ├── src/
│   │   ├── main.rs               ← CLI entry: reads .fe file, runs pipeline
│   │   ├── lexer/
│   │   │   ├── mod.rs
│   │   │   ├── token.rs          ← Token enum + Span
│   │   │   └── lexer.rs          ← case-normalising lexer
│   │   ├── parser/
│   │   │   ├── mod.rs
│   │   │   └── parser.rs         ← recursive descent, produces AST
│   │   ├── ast/
│   │   │   ├── mod.rs
│   │   │   └── nodes.rs          ← AST node definitions
│   │   ├── semantic/
│   │   │   ├── mod.rs
│   │   │   ├── declaration_collector.rs ← registers symbols before checking
│   │   │   ├── symbol_table.rs   ← scope stack, identifier resolution
│   │   │   ├── diagnostic.rs     ← diagnostic context
│   │   │   ├── type_checker.rs   ← constraint validation 6, 8–14, 18, 22, 25
│   │   │   ├── ownership.rs      ← ownership constraints 10–14, 19
│   │   │   └── device_checker.rs ← device constraints 3–7, 20, 21
│   │   ├── codegen/
│   │   │   ├── mod.rs
│   │   │   └── rust_emit.rs      ← AST → Rust source
│   │   └── diagnostics/
│   │       ├── mod.rs
│   │       └── reporter.rs       ← error + warning formatting
│   └── Cargo.toml
│
├── runtime/                      ← no_std runtime bridge, scheduler, debounce, board support
│   ├── src/
│   │   ├── boards/
│   │   │   ├── microbit_v2.rs
│   │   │   └── rp2040.rs
│   │   ├── debounce.rs
│   │   ├── lib.rs
│   │   ├── scheduler.rs
│   │   └── traits.rs
│   └── Cargo.toml
│
├── stdlib/                       ← built-in functions (spec §13)
│   ├── src/
│   │   ├── lib.rs               ← crate root and re-exports
│   │   ├── math.rs              ← abs, clamp, map, min, max
│   │   ├── convert.rs           ← type conversions (to_integer, to_decimal, etc.)
│   │   ├── arrays.rs            ← array_length, array_add, array_remove
│   │   ├── strings.rs           ← concat, includes, str_length
│   │   └── format.rs            ← formatting utilities
│   └── Cargo.toml
│
└── examples/                     ← 7 progressive curriculum examples
    ├── README.md                ← example index and running instructions
    ├── 01_blink.fe              ← rung 1: basic output and delay
    ├── 02_button_toggle.fe      ← rung 2: input and boolean state
    ├── 03_traffic_light.fe      ← rung 2–3: multiple devices, EVERY, BORROW
    ├── 04_soil_moisture.fe      ← rung 3–4: full spec with GIVE/LEND/BORROW
    ├── 05_temperature_display.fe ← rung 3: analog input and display
    ├── 06_rgb_mood_lamp.fe      ← rung 3–4: PWM and color cycling
    └── 07_distance_alarm.fe     ← rung 4: pulse, clamp, advanced patterns
```

## Current Status

Ferrum is feature-complete at the architecture level. The active codebase today includes:

- a working CLI driver with token, AST, check, and full compile modes
- a lexer, parser, AST, semantic analysis, diagnostics, and Rust code generation pipeline
- a real no_std runtime bridge with traits, scheduler, debounce logic, and board implementations for micro:bit v2 and RP2040
- a complete stdlib crate with all spec §13 built-in functions across five modules (math, convert, arrays, strings, format)
- seven progressive curriculum examples demonstrating all language features from basic I/O to advanced patterns

Remaining work is primarily integration (end-to-end testing, generated code validation against real hardware, VS Code extension).

## Compiler Architecture

The compiler is a multi-stage pipeline that transforms `.fe` source files to Rust:

### 1. **Lexer** (`compiler/src/lexer/`)
Tokenizes source code into a stream of tokens.

**Files:**
- `token.rs` - `Token` enum and `Span` type for source location tracking
- `lexer.rs` - Case-normalizing lexer (keywords normalized to uppercase, identifiers case-preserved)

**Responsibilities:**
- Break source text into tokens
- Track source positions (Span) for accurate error reporting
- Handle comments and whitespace
- Case-insensitive keyword handling

### 2. **Parser** (`compiler/src/parser/`)
Builds an Abstract Syntax Tree (AST) from the token stream via recursive descent parsing.

**Files:**
- `parser.rs` - Recursive descent parser, enforces grammar per `ferrum_grammar_v1.3.ebnf`
- `mod.rs` - Module exports and integration

**Responsibilities:**
- Parse tokens into AST following EBNF grammar
- Maintain source spans for accurate error reporting
- Fail gracefully with diagnostic information

### 3. **AST** (`compiler/src/ast/`)
Defines all Abstract Syntax Tree node types.

**Files:**
- `nodes.rs` - All AST node definitions
- `mod.rs` - Module exports

**Node Types:**
- `Program` - Top-level program structure
- `ConfigSection` - Configuration block
- `DefineItem` - Device template definitions
- `CreateItem` - Device instantiation
- `DeclareItem` - Variable/constant declarations
- `FunctionDef` - User-defined functions
- `RunSection` - Program entry point
- `Expr`, `Stmt` - Expressions and statements

### 4. **Semantic Analysis** (`compiler/src/semantic/`) - *Complete*
Validates semantic constraints and type safety.

**Files:**
- `declaration_collector.rs` - top-level symbol registration before checking
- `symbol_table.rs` - Scope stack and identifier resolution
- `diagnostic.rs` - Diagnostic context for error collection
- `type_checker.rs` - Type constraint validation (constraints 6, 8–14, 18, 22, 25)
- `ownership.rs` - Ownership and borrowing validation (constraints 10–14, 19)
- `device_checker.rs` - Device pin and config validation (constraints 3–7, 20, 21)
- `mod.rs` - Module coordination

**Responsibilities:**
- Build symbol table and track scopes
- Type checking and inference
- Ownership and borrowing validation
- Pin conflict detection
- Device configuration validation
- Range checking (e.g., `Percentage` must be 0.0–100.0)

### 5. **Code Generation** (`compiler/src/codegen/`) - *Complete*
Transforms validated AST to Rust source code.

**Files:**
- `rust_emit.rs` - AST → Rust source code emission
- `mod.rs` - Module exports

**Responsibilities:**
- Generate idiomatic Rust code from AST
- Call runtime library functions for device operations
- Emit setup code for board initialization

### 6. **Diagnostics** (`compiler/src/diagnostics/`) - *Complete*
Formats and reports errors and warnings.

**Files:**
- `reporter.rs` - Error and warning formatting with suggestions
- `mod.rs` - Module exports

**Responsibilities:**
- Collect errors/warnings with source locations
- Format readable error messages
- Provide actionable suggestions
- Source snippet highlighting

### 7. **Runtime Bridge** (`runtime/`)

The runtime crate is the hardware abstraction layer that generated Ferrum code links against.

**Files:**
- `lib.rs` - crate root and `no_std` re-exports
- `traits.rs` - the interface contract generated code uses for I/O, analog, PWM, display, delay, and pulse handling
- `scheduler.rs` - polling scheduler used for `EVERY` blocks
- `debounce.rs` - button debounce support
- `boards/microbit_v2.rs` - concrete BBC micro:bit v2 support
- `boards/rp2040.rs` - concrete RP2040 support

**Responsibilities:**
- Provide the generated code with stable runtime traits
- Wrap board-specific HAL details behind a crate-level contract
- Keep the emitted Rust `#![no_std]` friendly
- Centralize polling and button stabilization logic

### 8. **Main Entry Point** (`compiler/src/main.rs`)
CLI orchestration: reads `.fe` files and runs the complete pipeline.

## Getting Started

### Prerequisites
- Rust 1.70+ with Cargo
- Basic understanding of hardware concepts (GPIO, PWM, etc.)

### Building the Project

```bash
# Build all crates (compiler, runtime, stdlib)
cargo build

# Build in release mode
cargo build --release

# Build only the compiler
cd compiler && cargo build

# Check for compilation errors without building
cargo check

# Run tests (if available)
cargo test
```

### Project Layout for Development

Each crate is independent but works together:
- **compiler**: Contains the Ferrum→Rust compiler
- **runtime**: Provides runtime support for compiled Ferrum programs
- **stdlib**: Contains Ferrum standard library implementations

## Language Example

A typical Ferrum program (saved as `.fe`):

```fe
CONFIG {
   TARGET = "microbit_v2",
   DEBUG = TRUE
}

DEFINE Button AS INPUT
DEFINE Led AS OUTPUT

CREATE Button btn ON PIN 14 PULL UP
CREATE Led led ON PIN 3

DECLARE Integer clicks INIT 0

FUNCTION toggle GIVE Led: light {
   TURN light HIGH
   DELAY 500ms
   TURN light LOW
}

RUN {
   LOOP {
      IF btn IS LOW {
         CALL toggle GIVE led
         clicks = clicks + 1
      }
   }
}
```

See [examples/README.md](examples/README.md) for a complete index of all examples and their running instructions.

## Supported Targets

- **BBC micro:bit v2**
- **Raspberry Pi Pico**
- Other ARM Cortex-M microcontrollers (extensible)

## Development Status

| Component | Status |
|-----------|--------|
| Lexer | ✅ Complete |
| Parser | ✅ Complete |
| AST | ✅ Complete + modularized |
| Type System | ✅ Complete |
| Grammar Specification | ✅ Complete |
| Symbol Table | ✅ Complete (scope stack, resolution, device tracking) |
| Declaration Collector | ✅ Complete (top-level symbol registration) |
| Type Checker | ✅ Complete (constraint validation, type inference) |
| Ownership Checker | ✅ Complete (GIVE / LEND / BORROW rules) |
| Device Checker | ✅ Complete (pin uniqueness, ambiguous SET detection) |
| Code Generation | ✅ Complete (Rust emission, Cargo metadata, linker setup) |
| Runtime | ✅ Complete (traits, scheduler, debounce, microbit_v2, rp2040) |
| Stdlib | ✅ Complete (math, convert, arrays, strings, format; spec §13 full coverage) |
| Diagnostics System | ✅ Complete (comprehensive DiagnosticKind coverage) |
| Examples | ✅ Complete (7 progressive examples, rung 1–4 coverage) |
| License & Attribution | ✅ Complete (dual Apache 2.0 / GPL-3.0-or-later) |

## Documentation

- [Ferrum Language Specification](spec/ferrum_spec_v1.3.md) - Complete language reference
- [EBNF Grammar](spec/ferrum_grammar_v1.3.ebnf) - Formal grammar definition

## Architecture Highlights

### Two-Level Ownership Model
Mirrors Rust's ownership system while modeling actual hardware constraints (e.g., pin exclusivity - each physical pin can only be used by one device at a time).

### Case-Insensitive Keywords, Case-Preserving Identifiers
Makes code more user-friendly while maintaining compiler consistency:
```ferrum
RUN { }    # Keyword (case-insensitive)
my_var     # Identifier (case-preserving)
```

### Compile-Time Error-First Philosophy
- All errors caught at compile time (no runtime type coercion)
- Explicit device declarations prevent pin conflicts
- Type system prevents range violations (e.g., `Percentage` must be 0.0-100.0)

### Educational Ladder to Rust
Intentional syntax and concepts prepare students for real Rust, introducing:
- Ownership and borrowing concepts
- Strong type systems
- Compile-time safety guarantees

## Contributing

When working on the compiler, focus on:
1. **Lexer/Parser**: Ensure tokenization and parsing follow `ferrum_grammar_v1.3.ebnf`
2. **Semantic Analysis**: 
   - `symbol_table.rs` - scope resolution
   - `type_checker.rs` - type constraints (6, 8–14, 18, 22, 25)
   - `ownership.rs` - ownership constraints (10–14, 19)
   - `device_checker.rs` - device constraints (3–7, 20, 21)
3. **Code Generation**: Transform validated AST to Rust code using the runtime library
4. **Diagnostics**: Generate helpful, actionable error messages with source highlighting

## License

Ferrum is dual-licensed under your choice of either:

- Apache License 2.0
- GNU General Public License v3.0 or later

This gives users permissive reuse with attribution, an express patent grant, NOTICE handling, and a copyleft option for derivative works.

See the root `LICENSE` file for the full license notice and licensing terms.

## Authors

- Ferrum
- Author: Ainebyoona Dickson
- GitHub: https://github.com/Aine-dickson
- Email: ainedixon01@gmail.com
- Project sponsor and steward: Bitcraft, a learning lab and open-source forge by BitPulse
- Bitcraft GitHub: https://github.com/bitcraft-dev
- Ferrum repository: https://github.com/bitcraft-dev/ferrum
- Bitcraft contact: dev@craft.bitpulse.dev
- Sole contributor so far: Ainebyoona Dickson

---

**Status**: Active Development - Ferrum is currently being built and refined. Contributions and feedback are welcome!
