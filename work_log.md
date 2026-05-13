
## Actual standing

**What is done:**
- The full language spec (v1.3, locked, EBNF formalised)
- The complete compiler: lexer → parser → semantic (4 sub-passes) → codegen
- Board support for micro:bit v2 and RP2040
- A working CLI driver with 4 subcommands

**What remains — in priority order:**

---

### 1. End-to-End Integration & Validation

The compiler pipeline produces valid Rust source and Cargo projects. The next priority is validating that generated code:
- Compiles cleanly against `thumbv7em-none-eabihf` for embedded targets
- Runs correctly on real hardware (micro:bit v2, RP2040)
- Produces correct output and behavior

This requires flashing and testing on actual boards and fixing any remaining codegen issues.

---

### 2. End-to-end integration test

The 7 example programs provide this coverage. Each should pass `cargo check` and eventually flash to real hardware.

---

### 3. The VS Code language extension (`tooling/`)

For students to actually write `.fe` files comfortably:
- Syntax highlighting (TextMate grammar from the EBNF)
- Error squiggles (LSP or simple problem matcher against CLI output)
- Snippets for common patterns (`DEFINE ... AS`, `RUN { LOOP { } }`)

---

### 4. The curriculum materials

The spec was always meant to teach. The language exists to serve the curriculum rungs described at the very start of this conversation. Those materials — session packages, trainer guides, pupil worksheets — need to be written now that the language is fully specified.
