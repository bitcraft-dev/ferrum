// Public entry point for the codegen pass.

pub mod context;
pub mod name_mangler;
pub mod rust_emit;
pub mod type_map;

use crate::ast::Program;
use context::EmitContext;

/// Result of the codegen pass.
pub struct CodegenResult {
    /// The emitted Rust source as a String.
    pub rust_source:     String,
    /// The Cargo.toml [dependencies] snippet for the target board.
    pub cargo_deps:      String,
    /// The .cargo/config.toml snippet for the target board.
    pub cargo_config:    String,
    /// The memory.x linker script snippet for the target board.
    pub memory_x:      String,
}

/// Run the codegen pass on an annotated program.
/// Returns an error string if the TARGET is missing or unsupported.
pub fn emit(program: &Program) -> Result<CodegenResult, String> {
    let ctx = EmitContext::from_config(&program.config)?;
    let rust_source = rust_emit::Emitter::new(&ctx).emit(program);
    
    let cargo_deps = format!(
        "[dependencies]\n\
         ferrum-runtime = {{ path = \"../runtime\", features = [\"{}\"] }}\n\
         {name} = \"{ver}\"\n\
         defmt          = \"0.3\"\n\
         defmt-rtt      = \"0.4\"\n\
         panic-probe    = {{ version = \"0.3\", features = [\"print-defmt\"] }}\n\
         cortex-m-rt    = \"0.7\"\n",
        ctx.profile.crate_name.replace('-', "_"),   // feature flag
        name = ctx.profile.crate_name,
        ver  = ctx.profile.crate_version,
    );
    
    let cargo_config = format!(
        "# .cargo/config.toml\n\
         [build]\ntarget = \"thumbv7em-none-eabihf\"\n\n\
         [target.thumbv7em-none-eabihf]\n\
         rustflags = [\"-C\", \"link-arg=-Tlink.x\"]\n"
    );
    
    // Linker memory map — values depend on the target chip.
    // micro:bit v2 (nRF52833): 512K FLASH, 128K RAM
    // RP2040:                  2MB  FLASH, 264K RAM
    let memory_x = memory_x_for(ctx.profile.crate_name);
    //
    Ok(CodegenResult { rust_source, cargo_deps, cargo_config, memory_x })
}

fn memory_x_for(board: &str) -> String {
    match board {
        "microbit-v2" => "\
            MEMORY {\n  \
                FLASH : ORIGIN = 0x00000000, LENGTH = 512K\n  \
                RAM   : ORIGIN = 0x20000000, LENGTH = 128K\n\
            }\n".into(),
        "rp-pico" => "\
            MEMORY {\n  \
                BOOT2  : ORIGIN = 0x10000000, LENGTH = 0x100\n  \
                FLASH  : ORIGIN = 0x10000100, LENGTH = 2048K - 0x100\n  \
                RAM    : ORIGIN = 0x20000000, LENGTH = 264K\n\
            }\n".into(),
        _ => "/* memory.x — fill in for your board */\n".into(),
    }
}