// ferrum/compiler/src/semantic/symbol_table.rs
//
// Scope-aware symbol table for the Ferrum semantic pass.
//
// DESIGN
//   A symbol table is a stack of scopes. Each scope is a HashMap
//   keyed on Ident.key (lowercase). When a name is looked up, the
//   stack is searched from top (innermost) to bottom (outermost).
//
//   Scopes are pushed on entry to: functions, IF bodies, LOOP bodies,
//   FOR bodies, EVERY bodies, and the RUN section itself.
//
//   The original_text of the first declaration site is used in all
//   error messages — case-preserving as required by constraint 23.
//
// SYMBOL KINDS
//   Variable  — mutable program-level or block-scoped value
//   Constant  — immutable, must not be reassigned
//   Device    — hardware instance, carries ownership state
//   Function  — callable, carries parameter and return type info
//   DeviceType — a DEFINE entry, used to resolve CREATE references

use std::collections::HashMap;

use crate::ast::*;
use crate::lexer::token::{Ident, Span};
use crate::semantic::diagnostic::{Diagnostic, DiagnosticKind, Severity};

// ----------------------------------------------------------------
// Symbol
// ----------------------------------------------------------------

/// A single entry in the symbol table.
#[derive(Debug, Clone)]
pub struct Symbol {
    /// The name as first declared — used in diagnostics.
    pub ident:       Ident,
    /// What kind of thing this symbol represents.
    pub kind:        SymbolKind,
    /// Resolved type — always set on insertion for variables/constants;
    /// Void for functions until their body is analysed.
    pub ty:          Type,
    /// Source location of the declaration site.
    pub defined_at:  Span,
}

#[derive(Debug, Clone)]
pub enum SymbolKind {
    /// A variable declared with DECLARE or inline DECLARE.
    Variable {
        mutable:    bool,
        array_size: Option<usize>,
    },
    /// A constant declared with DECLARE CONSTANT.
    Constant,
    /// A hardware device instance (from CREATE).
    /// Ownership state is tracked here during ownership analysis.
    Device {
        /// The device type name (references a DeviceType symbol).
        device_type: Ident,
        /// Current ownership state — updated by the ownership checker.
        state:       DeviceState,
    },
    /// A user-defined function.
    Function {
        params:      Vec<ParamInfo>,
        return_type: Type,
    },
    /// A device type template (from DEFINE).
    DeviceType {
        spec: DeviceSpec,
    },
}

/// Summarised parameter information stored in the symbol table.
/// The full `Param` AST node is not needed after resolution.
#[derive(Debug, Clone)]
pub struct ParamInfo {
    pub name:      Ident,
    pub kind:      ParamKind,
}

#[derive(Debug, Clone)]
pub enum ParamKind {
    Data(Type),
    Device { ownership: Ownership, device_type: Ident },
}

/// The current ownership state of a device in scope.
#[derive(Debug, Clone, PartialEq)]
pub enum DeviceState {
    /// Available — the caller owns it and can use it freely.
    Available,
    /// Given away via GIVE — no longer accessible in this scope.
    /// Carries the span of the GIVE call for the error message.
    GivenAway { at: Span },
    /// Currently lent to a function via LEND — still readable here.
    Lent,
    /// Currently borrowed by a function via BORROW — temporarily unavailable.
    Borrowed,
}

// ----------------------------------------------------------------
// Scope
// ----------------------------------------------------------------

/// One level of the scope stack.
#[derive(Debug)]
struct Scope {
    symbols: HashMap<String, Symbol>,
    /// Label describing this scope — used in error messages.
    /// e.g. "function 'blink'", "LOOP block", "IF block"
    label:   String,
}

impl Scope {
    fn new(label: impl Into<String>) -> Self {
        Scope {
            symbols: HashMap::new(),
            label:   label.into(),
        }
    }
}

// ----------------------------------------------------------------
// SymbolTable
// ----------------------------------------------------------------

pub struct SymbolTable {
    scopes:      Vec<Scope>,
    diagnostics: Vec<Diagnostic>,
}

impl SymbolTable {
    pub fn new() -> Self {
        SymbolTable {
            scopes:      vec![Scope::new("global")],
            diagnostics: Vec::new(),
        }
    }

    // ── Scope management ─────────────────────────────────────────

    pub fn push_scope(&mut self, label: impl Into<String>) {
        self.scopes.push(Scope::new(label));
    }

    pub fn pop_scope(&mut self) {
        if self.scopes.len() > 1 {
            self.scopes.pop();
        }
    }

    /// Return the label of the current innermost scope.
    pub fn current_scope_label(&self) -> &str {
        self.scopes.last().map(|s| s.label.as_str()).unwrap_or("global")
    }

    // ── Symbol insertion ──────────────────────────────────────────

    /// Insert a symbol into the current (innermost) scope.
    /// Reports a duplicate-definition error if the name already exists
    /// in the *current* scope (shadowing an outer scope is allowed).
    pub fn insert(&mut self, symbol: Symbol) {
        let key  = symbol.ident.key.clone();
        let span = symbol.defined_at.clone();
        let name = symbol.ident.original.clone();

        if let Some(existing) = self.scopes.last().and_then(|s| s.symbols.get(&key)) {
            self.diagnostics.push(Diagnostic {
                severity:   Severity::Error,
                kind:       DiagnosticKind::DuplicateDefinition {
                    name:           name.clone(),
                    first_defined:  existing.defined_at.clone(),
                },
                span:       span.clone(),
                message:    format!("'{}' is already defined in this scope", name),
                suggestion: Some(format!(
                    "'{}' was first defined at {}. \
                     Use a different name or remove the duplicate.",
                    existing.ident.original, existing.defined_at
                )),
            });
            return;
        }

        if let Some(scope) = self.scopes.last_mut() {
            scope.symbols.insert(key, symbol);
        }
    }

    // ── Symbol lookup ─────────────────────────────────────────────

    /// Look up a name, searching from innermost to outermost scope.
    /// Returns the first match or None.
    pub fn lookup(&self, key: &str) -> Option<&Symbol> {
        for scope in self.scopes.iter().rev() {
            if let Some(sym) = scope.symbols.get(key) {
                return Some(sym);
            }
        }
        None
    }

    /// Look up a name and return a mutable reference.
    /// Used by the ownership checker to update DeviceState.
    pub fn lookup_mut(&mut self, key: &str) -> Option<&mut Symbol> {
        for scope in self.scopes.iter_mut().rev() {
            if let Some(sym) = scope.symbols.get_mut(key) {
                return Some(sym);
            }
        }
        None
    }

    /// Look up a name and report an error if not found.
    pub fn resolve(&mut self, ident: &Ident) -> Option<&Symbol> {
        if self.lookup(&ident.key).is_none() {
            self.diagnostics.push(Diagnostic {
                severity:   Severity::Error,
                kind:       DiagnosticKind::UndefinedVariable {
                    name: ident.original.clone(),
                },
                span:       ident.span.clone(),
                message:    format!("'{}' is not defined", ident.original),
                suggestion: self.suggest_similar(&ident.key),
            });
            return None;
        }
        self.lookup(&ident.key)
    }

    /// Look up a device and verify it is currently available.
    /// Reports an ownership error if it has been given away.
    pub fn resolve_device_available(&mut self, ident: &Ident) -> Option<&Symbol> {
        match self.lookup(&ident.key) {
            None => {
                self.diagnostics.push(Diagnostic {
                    severity:   Severity::Error,
                    kind:       DiagnosticKind::UndefinedVariable {
                        name: ident.original.clone(),
                    },
                    span:       ident.span.clone(),
                    message:    format!("'{}' is not defined", ident.original),
                    suggestion: None,
                });
                None
            }
            Some(sym) => {
                if let SymbolKind::Device { state: DeviceState::GivenAway { at }, .. } = &sym.kind {
                    let at_span = at.clone();
                    let original = sym.ident.original.clone();
                    self.diagnostics.push(Diagnostic {
                        severity:   Severity::Error,
                        kind:       DiagnosticKind::OwnershipViolation {
                            device: original.clone(),
                        },
                        span:       ident.span.clone(),
                        message:    format!(
                            "'{}' is not available here — it was given away at {}",
                            original, at_span
                        ),
                        suggestion: Some(format!(
                            "A device passed with GIVE belongs to the receiving function. \
                             Use BORROW instead if the caller needs to keep the device."
                        )),
                    });
                    None
                } else {
                    self.lookup(&ident.key)
                }
            }
        }
    }

    // ── Diagnostic access ─────────────────────────────────────────

    pub fn take_diagnostics(&mut self) -> Vec<Diagnostic> {
        std::mem::take(&mut self.diagnostics)
    }

    pub fn push_diagnostic(&mut self, d: Diagnostic) {
        self.diagnostics.push(d);
    }

    pub fn has_errors(&self) -> bool {
        self.diagnostics.iter().any(|d| d.severity == Severity::Error)
    }

    // ── Unused variable detection ─────────────────────────────────

    /// Walk the current scope and emit warnings for any variables
    /// that were declared but never read.
    /// Called at scope pop time for block scopes.
    pub fn warn_unused_in_current_scope(&mut self) {
        if let Some(scope) = self.scopes.last() {
            let unused: Vec<_> = scope.symbols.values()
                .filter(|sym| {
                    matches!(sym.kind,
                        SymbolKind::Variable { .. }) &&
                    !sym.ident.key.starts_with('_') // _ prefix suppresses warning
                })
                .map(|sym| (sym.ident.clone(), sym.defined_at.clone()))
                .collect();

            for (ident, span) in unused {
                // Mark as used is tracked externally via `mark_used`.
                // This method only emits the warning if the usage flag
                // was never set. Since we don't carry a used-flag on Symbol
                // (keeping it simple), unused detection is done in the
                // UsageTracker instead. This method is a hook for future use.
                let _ = (ident, span);
            }
        }
    }

    // ── Typo suggestion ───────────────────────────────────────────

    /// Return a suggestion for a similar name in scope, if one exists
    /// within Levenshtein distance 2. Used in "did you mean X?" messages.
    fn suggest_similar(&self, key: &str) -> Option<String> {
        let mut best: Option<(&str, usize)> = None;

        for scope in &self.scopes {
            for (k, sym) in &scope.symbols {
                let dist = levenshtein(key, k);
                if dist <= 2 {
                    match best {
                        None                        => { best = Some((k, dist)); }
                        Some((_, bd)) if dist < bd  => { best = Some((k, dist)); }
                        _                           => {}
                    }
                }
                let _ = sym;
            }
        }

        best.map(|(k, _)| {
            // Return the original spelling from the symbol
            self.lookup(k)
                .map(|s| format!("Did you mean '{}'?", s.ident.original))
                .unwrap_or_default()
        })
    }
}

// ----------------------------------------------------------------
// Usage tracker
// ----------------------------------------------------------------

/// Tracks which variable names have been read at least once.
/// Used to drive unused-variable warnings (constraint 18.2).
pub struct UsageTracker {
    used: std::collections::HashSet<String>,
}

impl UsageTracker {
    pub fn new() -> Self {
        UsageTracker { used: std::collections::HashSet::new() }
    }

    /// Record that an identifier was read.
    pub fn mark_used(&mut self, key: &str) {
        self.used.insert(key.to_string());
    }

    /// True if the identifier was ever read.
    pub fn is_used(&self, key: &str) -> bool {
        self.used.contains(key)
    }
}

// ----------------------------------------------------------------
// Levenshtein distance (simple iterative implementation)
// ----------------------------------------------------------------

fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let (m, n) = (a.len(), b.len());
    let mut dp = vec![vec![0usize; n + 1]; m + 1];
    for i in 0..=m { dp[i][0] = i; }
    for j in 0..=n { dp[0][j] = j; }
    for i in 1..=m {
        for j in 1..=n {
            dp[i][j] = if a[i-1] == b[j-1] {
                dp[i-1][j-1]
            } else {
                1 + dp[i-1][j].min(dp[i][j-1]).min(dp[i-1][j-1])
            };
        }
    }
    dp[m][n]
}

// ----------------------------------------------------------------
// Tests
// ----------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn span() -> Span {
        Span::new(Arc::new("test.fe".into()), 1, 1, 1)
    }

    fn ident(name: &str) -> Ident {
        Ident::new(name, span())
    }

    fn var_symbol(name: &str, ty: Type) -> Symbol {
        Symbol {
            ident:      ident(name),
            kind:       SymbolKind::Variable { mutable: true, array_size: None },
            ty,
            defined_at: span(),
        }
    }

    // ── Scope push/pop ───────────────────────────────────────────

    #[test]
    fn symbol_visible_in_inner_scope() {
        let mut st = SymbolTable::new();
        st.insert(var_symbol("counter", Type::Integer));
        st.push_scope("IF block");
        assert!(st.lookup("counter").is_some());
        st.pop_scope();
    }

    #[test]
    fn symbol_not_visible_after_scope_pop() {
        let mut st = SymbolTable::new();
        st.push_scope("IF block");
        st.insert(var_symbol("temp", Type::Decimal));
        assert!(st.lookup("temp").is_some());
        st.pop_scope();
        assert!(st.lookup("temp").is_none());
    }

    #[test]
    fn shadowing_is_allowed() {
        let mut st = SymbolTable::new();
        st.insert(var_symbol("count", Type::Integer));
        st.push_scope("IF block");
        st.insert(var_symbol("count", Type::Decimal)); // shadow
        let sym = st.lookup("count").unwrap();
        assert_eq!(sym.ty, Type::Decimal); // inner shadow wins
        st.pop_scope();
        let sym = st.lookup("count").unwrap();
        assert_eq!(sym.ty, Type::Integer); // outer restored
        assert!(st.diagnostics.is_empty(), "shadow should not emit an error");
    }

    #[test]
    fn duplicate_in_same_scope_is_error() {
        let mut st = SymbolTable::new();
        st.insert(var_symbol("x", Type::Integer));
        st.insert(var_symbol("x", Type::Integer)); // duplicate
        assert!(st.has_errors());
    }

    // ── Case-insensitive lookup ──────────────────────────────────

    #[test]
    fn lookup_is_case_insensitive() {
        let mut st = SymbolTable::new();
        st.insert(var_symbol("StatusLed", Type::Integer));
        assert!(st.lookup("statusled").is_some());
        assert!(st.lookup("STATUSLED").is_some());
        assert!(st.lookup("StatusLed").is_some());
    }

    // ── Resolve with error ────────────────────────────────────────

    #[test]
    fn resolve_undefined_emits_error() {
        let mut st = SymbolTable::new();
        let result = st.resolve(&ident("moisture"));
        assert!(result.is_none());
        assert!(st.has_errors());
    }

    // ── Typo suggestion ───────────────────────────────────────────

    #[test]
    fn typo_suggestion_within_distance_2() {
        let suggestion = levenshtein("moiture", "moisture");
        assert!(suggestion <= 2);
    }

    #[test]
    fn typo_no_suggestion_for_very_different_names() {
        let dist = levenshtein("xyz", "moisture");
        assert!(dist > 2);
    }

    // ── Device state ─────────────────────────────────────────────

    #[test]
    fn given_away_device_is_unavailable() {
        let mut st = SymbolTable::new();
        let give_span = span();
        st.insert(Symbol {
            ident:      ident("status_led"),
            kind:       SymbolKind::Device {
                device_type: ident("Led"),
                state:       DeviceState::GivenAway { at: give_span },
            },
            ty:         Type::Device(ident("Led")),
            defined_at: span(),
        });

        let result = st.resolve_device_available(&ident("status_led"));
        assert!(result.is_none());
        assert!(st.has_errors());
    }

    #[test]
    fn available_device_resolves_cleanly() {
        let mut st = SymbolTable::new();
        st.insert(Symbol {
            ident:      ident("pump"),
            kind:       SymbolKind::Device {
                device_type: ident("WaterPump"),
                state:       DeviceState::Available,
            },
            ty:         Type::Device(ident("WaterPump")),
            defined_at: span(),
        });

        let result = st.resolve_device_available(&ident("pump"));
        assert!(result.is_some());
        assert!(!st.has_errors());
    }
}