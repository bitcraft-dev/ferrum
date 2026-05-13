# Ferrum Example Programs

Each example is a complete `.fe` source file that demonstrates
a specific set of language features. They are ordered by complexity
and map to the curriculum rungs described in the Ferrum teaching guide.

## Example index

| File | Rung | Key features |
|---|---|---|
| `01_blink.fe` | 1 | DEFINE, CREATE, LOOP, TURN, DELAY |
| `02_button_toggle.fe` | 2 | INPUT, Boolean state, IS, IS NOT, edge detection |
| `03_traffic_light.fe` | 2–3 | Multiple devices, EVERY, FOR, RANGE, BORROW |
| `04_soil_moisture.fe` | 3–4 | Full spec example — all features including GIVE/BORROW/LEND |
| `05_temperature_display.fe` | 3 | ANALOG_INPUT, DISPLAY, to_string, map, EVERY |
| `06_rgb_mood_lamp.fe` | 3–4 | Composite PWM, colour cycling, BORROW with multi-interface device |
| `07_distance_alarm.fe` | 4 | PULSE, clamp, map, integer conversion, alarm pattern |

## Running an example

```bash
# Type-check only (no hardware required)
ferrum check examples/01_blink.fe

# Compile to Rust
ferrum compile examples/01_blink.fe

# The compiler writes:
#   01_blink_generated.rs   — the Rust source
#   01_blink_memory.x       — linker memory map
#   01_blink_cargo_deps.txt — dependency snippet for Cargo.toml
#   01_blink_cargo_config.txt — .cargo/config.toml snippet
```

## Curriculum alignment

### Rung 1 — Blink and Feel
Start with `01_blink.fe`. Students only change the DELAY values.
Goal: "I changed a number and the hardware changed."

### Rung 2 — Read and React
Move to `02_button_toggle.fe`. Students write the IF logic block.
The `IS` and `IS NOT` operators are introduced here.

### Rung 3 — Build a Thing
`03_traffic_light.fe` and `05_temperature_display.fe` are student
project templates. Students write most of the code with light
scaffolding. This is where ownership (BORROW) first appears naturally.

### Rung 4 — Peek Under the Hood
`04_soil_moisture.fe` uses all three ownership keywords.
Show the generated Rust alongside the `.fe` source:
  GIVE  → pass by value (move)
  LEND  → &T
  BORROW→ &mut T

The students were writing Rust semantics the whole time.
