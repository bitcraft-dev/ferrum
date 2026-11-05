# Errata for Spec v1.3

v1.3 is frozen and not edited. Corrections live here until v2 supersedes it.

1. **CONFIG commas.** The CONFIG example shows commas between items.
   Correct form has none (decision D-002). Section 16's complete example
   is the same as `examples/` and should be read as the reference.
2. **Integer range.** The spec calls Integer unrestricted. It is a signed
   32-bit value. Overflow halts at runtime (decision D-003).
3. **"All detectable errors are compile-time errors."** Too broad. Values
   that depend on hardware readings are checked at runtime.
