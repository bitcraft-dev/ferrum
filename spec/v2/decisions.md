# Ferrum v2 Design Decisions

Decision log for the v2 specification. Newest last. Each entry is final
unless new evidence of a problem is recorded here.

## D-001 Spec v1.3 is frozen (2026-11-05)
`ferrum_spec_v1.3.md` and `ferrum_grammar_v1.3.ebnf` are read-only
historical records. All corrections and new semantics go in `spec/v2/`.
Known v1.3 errata are listed in `../ERRATA_v1.3.md`.

## D-002 CONFIG items take no commas (2026-11-05)
Items are separated by newlines. This matches the parser and every
example. v1.3 showed commas; that was wrong.

## D-003 Integer overflow halts at runtime (2026-11-05)
Integer add, subtract and multiply overflow, division by zero, and
`MIN / -1` are checked. On failure the program halts. Behaviour is
identical in debug and release builds (no build-profile-dependent
wrapping). In debug builds the halt reports the source location.
Intentional wrapping, for counters, will be an explicit built-in later.

## Open
- Out-of-bounds array index: proposed compile error when statically
  known, runtime halt with location otherwise. Awaiting confirmation.
- Surface syntax for recoverable runtime errors (ROADMAP C4).
