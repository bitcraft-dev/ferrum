
// ╔══ CODEGEN HOOK: stdlib function name mapping ══╗
//
// The emitter must map Ferrum CALL built-in names to their Rust
// counterparts. Add this table to rust_emit.rs alongside the
// existing emit_call_stmt method.
//
// Built-in functions are registered in the symbol table by the
// declaration_collector before user symbols, so they are always
// available. The emitter checks this table first when it sees a
// CALL to a known name.
//
// Replace the emit_call_stmt body with:
//
//   fn emit_call_stmt(&mut self, stmt: &CallStmt) {
//       if let Some(builtin) = builtin_rust_name(&stmt.function.key) {
//           let args = self.format_call_args(&stmt.args);
//           self.w.line(&format!("{};", builtin_call(builtin, &stmt.args)));
//       } else {
//           let fn_name = to_snake(&stmt.function);
//           let args    = self.format_call_args(&stmt.args);
//           self.w.line(&format!("{}({});", fn_name, args));
//       }
//   }
//
// And add these helpers:

/*
/// Maps a Ferrum built-in function key to its Rust stdlib name.
fn builtin_rust_name(key: &str) -> Option<&'static str> {
    match key {
        "abs"            => Some("ferrum_stdlib::abs"),
        "min"            => Some("ferrum_stdlib::min"),
        "max"            => Some("ferrum_stdlib::max"),
        "clamp"          => Some("ferrum_stdlib::clamp"),
        "map"            => Some("ferrum_stdlib::map"),
        "to_integer"     => Some("ferrum_stdlib::to_integer"),
        "to_decimal"     => Some("ferrum_stdlib::to_decimal"),
        "to_percentage"  => Some("ferrum_stdlib::to_percentage"),
        "to_string"      => Some("ferrum_stdlib::integer_to_string"), // overloaded — resolved by type_checker
        "length"         => None, // resolved by type_checker to str_length or array_length
        "includes"       => Some("ferrum_stdlib::includes"),
        "add"            => Some("ferrum_stdlib::array_add"),
        "remove"         => Some("ferrum_stdlib::array_remove"),
        _                => None,
    }
}
*/
//
// Note: `length` and `to_string` are overloaded — the emitter reads
// the resolved type from the first argument's Expr.ty to pick the
// right Rust function. This is safe because the semantic pass has
// already filled all Expr.ty slots before codegen runs.
//
// ╚══ END CODEGEN HOOK ══╝

