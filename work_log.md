
## Actual standing

**What is done:**
- The full language spec (v1.3, locked, EBNF formalised)
- The complete compiler: lexer → parser → semantic (4 sub-passes) → codegen
- Board support for micro:bit v2 and RP2040
- A working CLI driver with 4 subcommands

**What remains — in priority order:**

---

### 1. The Standard Library (`stdlib/`)

The spec defines built-in functions: `abs`, `min`, `max`, `clamp`, `map`, `to_integer`, `to_decimal`, `to_percentage`, `to_string`, `length`, `includes`, `ADD`, `REMOVE`. These still need fuller Rust implementations in the `stdlib` crate.

Right now the compiler can emit calls into the runtime bridge, but the stdlib crate still needs to catch up with the full built-in surface.

---

### 2. The `TurnStmt` emitter bug

There is a known issue in `rust_emit.rs` inside `emit_statement` for `StmtKind::Turn`. The code emits two lines — once with `self.w.line(...)` and then computes a `let _ = line` — because of a refactor that got interrupted mid-thought. This produces duplicate output for every `TURN` statement. It needs a clean fix before any real code is flashed.

---

### 3. End-to-end integration test

A `.fe` file that exercises every language feature should be run through the full pipeline — `lex → parse → semantic → codegen → cargo check` — and produce a Rust file that at least passes `cargo check` against a `thumbv7em-none-eabihf` target. This is the first real validation that the pipeline is correct end to end.

---

### 4. The VS Code language extension (`tooling/`)

For students to actually write `.fe` files comfortably:
- Syntax highlighting (TextMate grammar from the EBNF)
- Error squiggles (LSP or simple problem matcher against CLI output)
- Snippets for common patterns (`DEFINE ... AS`, `RUN { LOOP { } }`)

---

### 5. The curriculum materials

The spec was always meant to teach. The language exists to serve the curriculum rungs described at the very start of this conversation. Those materials — session packages, trainer guides, pupil worksheets — need to be written now that the language is fully specified.
