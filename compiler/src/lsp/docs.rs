// Teaching one-liners for hover. When the cursor is on a keyword,
// type name, built-in, or operator (rather than a user-defined name
// with a resolved type), the hover shows the matching blurb here.
//
// This is deliberately a flat, hand-written table rather than anything
// generated: the whole point of Ferrum's hover is that a beginner can
// read what a keyword *means* without leaving the editor, so the
// wording is a teaching decision, not a mechanical one. Keep entries to
// a single line — syntax shape plus the concept it teaches. Deeper,
// multi-line teaching cards (with examples and the Rust reveal) are a
// planned follow-up; this first pass is the one-liners.
//
// Keys are the canonical UPPERCASE spelling. Callers uppercase the
// token text before looking up, since Ferrum keywords are
// case-insensitive (see lexer/token.rs "CASE POLICY").

/// One-line documentation for a keyword / type / builtin / operator,
/// or `None` if `upper` is not a known language word.
pub fn keyword_doc(upper: &str) -> Option<&'static str> {
    let doc = match upper {
        // ── Sections ────────────────────────────────────────────────
        "CONFIG"   => "Board settings: TARGET, DEBUG, clock. Comes first.",
        "DEFINE"   => "Declares a device *type* — the interfaces it has (INPUT, OUTPUT, PWM …).",
        "CREATE"   => "Makes a device *instance* of a DEFINEd type and wires it to physical PINs.",
        "DECLARE"  => "Declares a data variable or CONSTANT (Integer, Boolean, Percentage …).",
        "FUNCTION" => "A named, reusable block. Device parameters need an ownership keyword (GIVE/LEND/BORROW).",
        "RUN"      => "The program body. Holds EVERY blocks and one LOOP — this is what the board executes.",

        // ── Control flow ────────────────────────────────────────────
        "EVERY"    => "Runs its block on a fixed schedule, e.g. EVERY 1000ms. Only valid at RUN top level.",
        "LOOP"     => "Repeats its block forever — the board's main heartbeat.",
        "IF"       => "Runs a block only when a condition is true. Pair device checks with IS, values with ==.",
        "ELSE"     => "The branch taken when the IF condition is false. (ELSE IF is not allowed — nest instead.)",
        "FOR"      => "Counts over a RANGE, e.g. FOR i IN RANGE 0 TO 5.",
        "IN"       => "Binds the FOR loop variable to each value in a RANGE.",
        "RANGE"    => "A span of integers for FOR, e.g. RANGE 1 TO 10.",
        "BREAK"    => "Leaves the enclosing LOOP or FOR immediately.",
        "CONTINUE" => "Skips to the next iteration of the enclosing LOOP or FOR.",
        "RETURN"   => "Hands a value back from a FUNCTION to its caller.",
        "CALL"     => "Invokes a FUNCTION. The CALL keyword is required for every function call.",

        // ── Hardware commands ───────────────────────────────────────
        "TURN"     => "Sets a digital output HIGH or LOW, e.g. TURN led HIGH.",
        "SET"      => "Sets a numeric interface value, e.g. SET led BRIGHTNESS 80.",
        "TOGGLE"   => "Flips a digital output to the opposite state.",
        "READ"         => "Reads a device's current value — PinState for digital, Integer for analog.",
        "READ_PERCENT" => "Reads an analog input as a Percentage (0–100).",
        "DELAY"    => "Pauses for a duration, e.g. DELAY 500ms. Accepts a variable duration too.",
        "PRINT"    => "Sends text to the debug console (when DEBUG = TRUE).",

        // ── Ownership (the Rust reveal) ─────────────────────────────
        "GIVE"     => "Move: the caller hands the device over and can no longer use it. (Rust: move.)",
        "LEND"     => "Read-only borrow: the function may read the device; the caller keeps it. (Rust: &.)",
        "BORROW"   => "Read-write borrow: the function may change the device; the caller keeps it. (Rust: &mut.)",

        // ── Interface types ─────────────────────────────────────────
        "INPUT"        => "A digital input interface (buttons, switches).",
        "OUTPUT"       => "A digital output interface (LEDs, relays).",
        "ANALOG_INPUT" => "An analog input interface (sensors returning a range of values).",
        "PWM"          => "A pulse-width-modulated output — dimmable brightness or motor speed.",
        "DISPLAY"      => "A text/graphic display interface (LED matrix, LCD).",
        "PULSE"        => "A pulse-timing interface (ultrasonic TRIGGER/ECHO).",

        // ── Pin / create syntax ─────────────────────────────────────
        "PIN"      => "The physical board pin a device connects to, e.g. ON PIN 14.",
        "PULL"     => "Sets an input's pull resistor: PULL UP or PULL DOWN.",
        "INIT"     => "The device's starting state, set once when the program begins.",
        "AS"       => "Introduces a device type's interface spec in DEFINE.",
        "ON"       => "Assigns a device to its pin(s) in CREATE.",

        // ── Types ───────────────────────────────────────────────────
        "INTEGER"    => "A whole number, e.g. 0, 42, -7.",
        "DECIMAL"    => "A fractional number, e.g. 3.14, 0.5.",
        "PERCENTAGE" => "A value from 0 to 100 — out-of-range values are an error (E008).",
        "BOOLEAN"    => "TRUE or FALSE.",
        "STRING"     => "Text between double quotes, e.g. \"hello\".",
        "BYTE"       => "A whole number from 0 to 255.",
        "CONSTANT"   => "A DECLARE that can never be reassigned (E021 if you try).",

        // ── Literals ────────────────────────────────────────────────
        "HIGH"   => "Digital output on — full voltage.",
        "LOW"    => "Digital output off — ground.",
        "TRUE"   => "Boolean true.",
        "FALSE"  => "Boolean false.",

        // ── Operators (keyword form) ────────────────────────────────
        "IS"   => "Compares a device's state or a Boolean, e.g. IF button IS LOW. Use == for data values.",
        "NOT"  => "Logical negation, e.g. IF NOT ready. Also part of IS NOT.",
        "AND"  => "Logical and — true only when both sides are true.",
        "OR"   => "Logical or — true when either side is true.",

        // ── Built-in functions ──────────────────────────────────────
        "ABS"           => "abs(x) — absolute (non-negative) value of x.",
        "MIN"           => "min(a, b) — the smaller of two values.",
        "MAX"           => "max(a, b) — the larger of two values.",
        "CLAMP"         => "clamp(x, lo, hi) — x forced into the range [lo, hi].",
        "MAP"           => "map(x, in_lo, in_hi, out_lo, out_hi) — rescale x from one range to another.",
        "TO_INTEGER"    => "to_integer(x) — convert to Integer.",
        "TO_DECIMAL"    => "to_decimal(x) — convert to Decimal.",
        "TO_PERCENTAGE" => "to_percentage(x) — convert to Percentage (0–100).",
        "TO_STRING"     => "to_string(x) — convert to String.",
        "LENGTH"        => "length(arr) — number of elements in an array.",
        "INCLUDES"      => "includes(arr, x) — TRUE if the array contains x.",
        "ADD"           => "add(arr, x) — append x to an array.",
        "REMOVE"        => "remove(arr, x) — remove x from an array.",

        _ => return None,
    };
    Some(doc)
}
