---

# Ferrum Specification v1.3

---

## 1. Introduction

This document defines the complete syntax and semantics of Ferrum a domain-specific language for embedded systems programming.

The language is designed for secondary education, teaching hardware interaction through **capability-based interfaces**. The syntax is intentionally close to spoken English while preserving the discipline, explicitness, and compile-time safety philosophy of Rust.

The language:

* Compiles to Rust
* Targets embedded boards (e.g., BBC micro:bit v2, Raspberry Pi Pico)
* Treats all detectable errors as **compile-time errors**

### Key Design Insight

The language introduces an **ownership and borrowing model for devices**, directly modeled on Rust:

* Hardware is **physically exclusive**
* A pin cannot be driven by multiple sources simultaneously
* Ownership rules reflect real hardware constraints

Students *feel* the rules through hardware → making Rust later feel familiar.

---

## 2. Core Philosophy

The language is built on five principles:

### 2.1 Hardware is Explicit

Every pin, interface, and device must be declared before use.

### 2.2 Types Carry Meaning

Types encode real-world constraints:

* `Percentage` ≠ generic number
* `Byte` ≠ unrestricted integer

### 2.3 Errors are Teachers

Errors:

* Explain the issue
* Point to location
* Suggest fixes

### 2.4 Syntax Maps to Hardware Thinking

Keywords reflect **hardware behavior**, not abstract programming patterns.

### 2.5 Ownership Reflects Physical Reality

Devices belong to one place at a time.

---

## 3. Type System

### 3.1 Interface Types

| Interface    | Description              |
| ------------ | ------------------------ |
| INPUT        | Digital input (HIGH/LOW) |
| OUTPUT       | Digital output           |
| ANALOG_INPUT | Reads voltage            |
| PWM          | Pulse-width modulation   |
| DISPLAY      | Display output           |
| PULSE        | Pulse timing             |

---

### 3.2 Qualifiers

| Interface    | Valid Qualifiers                           |
| ------------ | ------------------------------------------ |
| PWM          | BRIGHTNESS, SPEED, ANGLE, RED, GREEN, BLUE |
| DISPLAY      | LCD, OLED                                  |
| PULSE        | TRIGGER, ECHO                              |
| OUTPUT       | ENABLE (composite only)                    |
| INPUT        | none                                       |
| ANALOG_INPUT | none                                       |

**Invalid qualifier → compile-time error**

```text
Error: Invalid qualifier 'LCD' for interface 'PWM'.
Did you mean: DISPLAY LCD?
```

---

### 3.3 Data Types

| Type       | Description    | Range       |
| ---------- | -------------- | ----------- |
| Integer    | Whole number   | Any         |
| Decimal    | Floating point | Any         |
| Percentage | 0.0–100.0      | constrained |
| Boolean    | TRUE/FALSE     |             |
| String     | Text           | "..."       |
| Byte       | 0–255          | constrained |

#### Rules

* No implicit type coercion
* Range violations → compile-time error

```dsl
DECLARE Percentage level INIT 105.0
```

```text
Error: Value out of range (0.0–100.0)
```

---

### 3.4 Device-Dependent Types

| Interface    | READ     | READ_PERCENT |
| ------------ | -------- | ------------ |
| INPUT        | HIGH/LOW | invalid      |
| ANALOG_INPUT | Integer  | Percentage   |

---

### 3.5 `IS` vs `==`

| Operator | Use                    |
| -------- | ---------------------- |
| IS       | Device state / Boolean |
| IS NOT   | Negated state          |
| ==       | Value comparison       |

```dsl
IF button IS LOW { ... }
IF counter == 10 { ... }
```

---

## 4. Program Structure

Sections must appear in this order:

```
CONFIG → DEFINE → CREATE → DECLARE → FUNCTION → RUN
```

Only `RUN` is required.

---

## 5. CONFIG Section

```dsl
CONFIG {
   TARGET = "microbit_v2",
   CLOCK_SPEED = 64MHZ,
   DEBUG = TRUE
}
```

### Keys

| Key             | Type          | Description   |
| --------------- | ------------- | ------------- |
| TARGET          | String        | Board         |
| CLOCK_SPEED     | Integer + MHZ | CPU speed     |
| SERIAL          | Integer       | Baud rate     |
| DEFAULT_PULL_UP | Boolean       | Input default |
| DEBOUNCE_MS     | Integer       | Debounce      |
| OPTIMIZE        | String        | speed/size    |
| DEBUG           | Boolean       | Debug output  |

---

## 6. DEFINE Section

Defines device templates.

### Simple Device

```dsl
DEFINE Button AS INPUT
```

### Block Form

```dsl
DEFINE
   Button AS INPUT,
   Sensor AS ANALOG_INPUT
```

---

### Composite Device

```dsl
DEFINE Led AS {
   OUTPUT,
   PWM BRIGHTNESS
}
```

---

## 7. CREATE Section

Instantiates devices and assigns pins.

### Examples

```dsl
CREATE Button mode_btn ON PIN 14
```

```dsl
CREATE Led status_led ON {
   PIN 3,
   PIN 4
}
```

---

### Rules

* Every interface must have a pin
* No duplicate pins
* Order matters (positional)
* Named assignment allowed

---

### INIT Block

```dsl
INIT {
   LOW,
   0.0
}
```

| Interface    | INIT Allowed   |
| ------------ | -------------- |
| OUTPUT       | HIGH/LOW       |
| PWM          | 0.0–1.0        |
| DISPLAY      | String/Integer |
| INPUT        | ❌              |
| ANALOG_INPUT | ❌              |

---

### PULL Configuration

```dsl
CREATE Button btn ON PIN 14 PULL UP
```

---

## 8. DECLARE Section

### Variables

```dsl
DECLARE Integer counter INIT 0
DECLARE Boolean auto_mode INIT TRUE
```

---

### Arrays

```dsl
DECLARE Integer[5] readings INIT [0,0,0,0,0]
```

---

### Constants

```dsl
DECLARE CONSTANT Integer MAX = 100
```

---

### Scope

* Block-scoped variables exist only inside blocks
* Shadowing allowed

---

## 9. FUNCTION Section

### Syntax

```dsl
FUNCTION name parameters {
   statements
   RETURN value
}
```

---

### Parameter Types

* Data parameters
* Device parameters (**must include ownership keyword**)

---

### Ownership Keywords

| Keyword | Meaning            |
| ------- | ------------------ |
| GIVE    | Transfer ownership |
| LEND    | Read-only borrow   |
| BORROW  | Mutable borrow     |

---

### Example

```dsl
FUNCTION blink GIVE Led: led {
   TURN led HIGH
   DELAY 500ms
}
```

---

### Calling

```dsl
CALL blink GIVE status_led
```

---

## 10. RUN Section

Entry point:

```dsl
RUN {
   LOOP {
      -- logic
   }
}
```

---

### LOOP

Infinite loop.

---

### EVERY

```dsl
EVERY 1000ms {
   PRINT "tick"
}
```

Restrictions:

* Not allowed inside LOOP

---

### IF / ELSE

```dsl
IF temp > 30 {
   ...
} ELSE {
   ...
}
```

---

### FOR

```dsl
FOR i IN RANGE 0, 9 {
   PRINT i
}
```

---

### BREAK / CONTINUE

Valid only inside loops.

---

### DELAY

```dsl
DELAY 500ms
```

---

### PRINT

```dsl
PRINT "Hello"
```

Only active when `DEBUG = TRUE`.

---

## 11. Expressions

### Operator Precedence

| Level | Operators    |
| ----- | ------------ |
| High  | NOT, unary - |
|       | * /          |
|       | + -          |
|       | > < >= <= IS |
|       | == !=        |
|       | AND          |
| Low   | OR           |

---

### Inline IF

```dsl
DECLARE String mode =
   IF auto_mode IS TRUE { "Auto" } ELSE { "Manual" }
```

---

## 12. Device Commands

### Output

```dsl
TURN led HIGH
TOGGLE led
```

---

### PWM

```dsl
SET motor SPEED 0.6
```

---

### Display

```dsl
SET screen "Hello"
```

---

### Reading

```dsl
READ sensor
READ_PERCENT sensor
```

---

## 13. Built-in Functions

### Math

* `abs`
* `min`
* `max`
* `clamp`
* `map`

### Conversion

* `to_integer`
* `to_decimal`
* `to_percentage`
* `to_string`

### Strings

* `length`
* `includes`

### Arrays

* `ADD`
* `REMOVE`

---

## 14. PRINT vs DISPLAY

| Command     | Purpose         |
| ----------- | --------------- |
| PRINT       | Debug           |
| SET display | Hardware output |

---

## 15. Comments

```dsl
-- This is a comment
```

---

## 16. Example (Summary)

A full working example includes:

* DEFINE devices
* CREATE instances
* DECLARE variables
* FUNCTION logic
* RUN loop with ownership usage

*(Your full example can be appended here unchanged for clarity.)*

---

## 17. Ownership Model

### GIVE

* Transfers ownership permanently

### LEND

* Read-only access
* Caller retains ownership

### BORROW

* Temporary read/write access
* Ownership returns after call

---

## 18. Key Insight

This model enforces **real hardware constraints at compile time**, making:

* Bugs → compile errors
* Learning → intuitive
* Transition to Rust → natural

---

If you want, next step I can:

* Turn this into a **clean GitHub README + docs site structure**
* Or generate a **formal spec + parser-ready grammar doc**
* Or align it with your **Rust transpiler architecture (AST mapping)**
