# Ferrum: Embedded Systems DSL

Ferrum is a **domain-specific language (DSL) for embedded systems programming**, specifically designed for secondary education. It teaches hardware interaction through capability-based interfaces with an ownership and borrowing model inspired by Rust, while compiling to Rust as an intermediate target.

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
ferrum/
├── Cargo.toml                    # Workspace configuration
├── spec/                         # Language specification
│   ├── ferrum_spec_v1.3.md       # Complete language specification
│   └── ferrum_grammar_v1.3.ebnf  # Formal EBNF grammar
├── compiler/                     # Ferrum compiler (compiles to Rust)
│   ├── src/
│   │   ├── main.rs               # Compiler entry point
│   │   ├── lexer/                # Tokenization
│   │   ├── parser/               # Syntax parsing → AST
│   │   ├── ast/                  # Abstract syntax tree definitions
│   │   ├── types/                # Type system & span tracking
│   │   ├── semantic/             # Semantic analysis (in progress)
│   │   ├── codegen/              # Code generation (in progress)
│   │   └── diagnostics/          # Error reporting (in progress)
│   └── Cargo.toml
├── runtime/                      # Runtime support library
│   ├── src/lib.rs
│   └── Cargo.toml
├── stdlib/                       # Standard library for Ferrum
│   ├── src/lib.rs
│   └── Cargo.toml
└── examples/                     # Example Ferrum programs
```

## Compiler Architecture

The compiler follows a multi-pass architecture:

### 1. Lexer (`compiler/src/lexer/`)
- Tokenizes source code into tokens
- Case-insensitive keyword handling (normalized to uppercase)
- Case-preserving identifiers
- Whitespace and comment consumption

### 2. Parser (`compiler/src/parser/`)
- Builds abstract syntax tree (AST) from tokens
- Enforces grammar per `ferrum_grammar_v1.3.ebnf`
- Tracks source locations (Span) for accurate error reporting

### 3. AST (`compiler/src/ast/`)
Defines program structure with nodes including:
- **Program** - Full program structure
- **ConfigSection** - Configuration options
- **DefineItem** - Device templates
- **CreateItem** - Device instantiation
- **DeclareItem** - Variables and constants
- **FunctionDef** - User-defined functions
- **RunSection** - Entry point

### 4. Type System (`compiler/src/types/`)
- **Span** - Source location tracking for error messages
- **Ident** - Case-preserving, case-insensitive identifiers
- Type definitions and validation

### 5. Semantic Analysis (`compiler/src/semantic/`) - *In Progress*
- Symbol table management
- Type checking and inference
- Device ownership validation
- Pin conflict detection
- Configuration validation

### 6. Code Generation (`compiler/src/codegen/`) - *In Progress*
- Transforms AST to Rust code
- Generates runtime library calls

### 7. Diagnostics (`compiler/src/diagnostics/`) - *In Progress*
- Formatted error reporting with suggestions
- Source highlighting

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

While examples are in development, a typical Ferrum program would look like:

```ferrum
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

## Supported Targets

- **BBC micro:bit v2**
- **Raspberry Pi Pico**
- Other ARM Cortex-M microcontrollers (extensible)

## Development Status

| Component | Status |
|-----------|--------|
| Lexer | ✅ Complete |
| Parser | ✅ Complete |
| AST | ✅ Complete |
| Type System | ✅ Complete |
| Grammar Specification | ✅ Complete |
| Semantic Analysis | 🚧 In Progress |
| Code Generation | 🚧 In Progress |
| Diagnostics System | 🚧 In Progress |
| Examples | 📋 Planned |

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
2. **Semantic Analysis**: Implement symbol table, type checking, and device validation
3. **Code Generation**: Transform validated AST to Rust code using the runtime library
4. **Diagnostics**: Generate helpful, actionable error messages

## License

[Add license information here]

## Authors

[Add author information here]

---

**Status**: Active Development - Ferrum is currently being built and refined. Contributions and feedback are welcome!
