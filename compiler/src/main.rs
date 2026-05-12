// Ferrum compiler driver — CLI entry point.
//
// USAGE
//   ferrum <source.fe>              Compile to Rust and invoke cargo
//   ferrum check <source.fe>        Type-check only, no codegen
//   ferrum tokens <source.fe>       Dump the token stream (debug)
//   ferrum ast <source.fe>          Dump the parsed AST (debug)
//
// EXIT CODES
//   0  — success
//   1  — compile error
//   2  — file not found / IO error

use std::process;

mod ast;
mod diagnostics;
mod lexer;
mod parser;
mod semantic;
mod codegen;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: ferrum <source.fe>");
        eprintln!("       ferrum check <source.fe>");
        eprintln!("       ferrum tokens <source.fe>");
        eprintln!("       ferrum ast <source.fe>");
        process::exit(2);
    }

    // Determine subcommand
    let (subcommand, source_path) = if args.len() >= 3 {
        (args[1].as_str(), args[2].as_str())
    } else {
        ("compile", args[1].as_str())
    };

    // Read source file
    let source = match std::fs::read_to_string(source_path) {
        Ok(s)  => s,
        Err(e) => {
            eprintln!("error: cannot read '{}': {}", source_path, e);
            process::exit(2);
        }
    };

    match subcommand {
        "tokens" => run_tokens(&source, source_path),
        "ast"    => run_ast(&source, source_path),
        "check"  => run_check(&source, source_path),
        _        => run_compile(&source, source_path),
    }
}

// ── tokens — dump lexer output ────────────────────────────────────

fn run_tokens(source: &str, path: &str) {
    let result = lexer::lex(source, path);

    for (i, st) in result.tokens.iter().enumerate() {
        println!("{:>4}  {:?}", i, st.node);
    }
    if !result.errors.is_empty() {
        eprintln!("\n{} lex error(s):", result.errors.len());
        for e in &result.errors {
            eprintln!("  {}", e);
        }
        process::exit(1);
    }
}

// ── ast — dump parser output ──────────────────────────────────────

fn run_ast(source: &str, path: &str) {
    let lex_result   = lexer::lex(source, path);
    let parse_result = parser::parser::parse(lex_result.tokens, path);

    if !parse_result.errors.is_empty() {
        for e in &parse_result.errors {
            eprintln!("{}", e);
        }
        process::exit(1);
    }

    if let Some(program) = parse_result.program {
        println!("{:#?}", program);
    }
}

// ── check — semantic analysis only ───────────────────────────────

fn run_check(source: &str, path: &str) {
    let program = lex_and_parse(source, path);
    let result  = semantic::analyse(program);
    let reporter = diagnostics::reporter::Reporter::new(source, path);
    let output  = reporter.render(&result.diagnostics);

    if !output.is_empty() { print!("{}", output); }

    if result.has_errors() {
        process::exit(1);
    } else {
        println!("✓ {} — no errors", path);
    }
}

// ── compile — full pipeline ───────────────────────────────────────

fn run_compile(source: &str, path: &str) {
    let program  = lex_and_parse(source, path);
    let result   = semantic::analyse(program);
    let reporter = diagnostics::reporter::Reporter::new(source, path);

    // Print warnings even on success
    let output = reporter.render(&result.diagnostics);
    if !output.is_empty() { print!("{}", output); }

    if result.has_errors() {
        process::exit(1);
    }

    // Codegen — emit Rust source
    let annotated = result.program.unwrap();
    let stem      = std::path::Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("output");
    let out_path  = format!("{}_generated.rs", stem);

    let codegen_result = match codegen::emit(&annotated) {
        Ok(res) => res,
        Err(e) => {
            eprintln!("error: codegen failed: {}", e);
            process::exit(1);
        }
    };

    // Create output directory structure: {stem}/
    let out_dir = stem;
    if let Err(e) = std::fs::create_dir_all(out_dir) {
        eprintln!("error: cannot create directory '{}': {}", out_dir, e);
        process::exit(1);
    }

    // Create src/ subdirectory
    let src_dir = format!("{}/src", out_dir);
    if let Err(e) = std::fs::create_dir_all(&src_dir) {
        eprintln!("error: cannot create directory '{}': {}", src_dir, e);
        process::exit(1);
    }

    // Create .cargo/ subdirectory
    let cargo_dir = format!("{}/.cargo", out_dir);
    if let Err(e) = std::fs::create_dir_all(&cargo_dir) {
        eprintln!("error: cannot create directory '{}': {}", cargo_dir, e);
        process::exit(1);
    }

    // Write src/main.rs
    let main_rs_path = format!("{}/main.rs", src_dir);
    match std::fs::write(&main_rs_path, &codegen_result.rust_source) {
        Ok(_)  => println!("✓ Written to {}", main_rs_path),
        Err(e) => {
            eprintln!("error: cannot write '{}': {}", main_rs_path, e);
            process::exit(1);
        }
    }

    // Write Cargo.toml with proper metadata
    let cargo_toml_path = format!("{}/Cargo.toml", out_dir);
    let cargo_toml_content = format!(
        "[package]\nname = \"{}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n{}\n[workspace]\n",
        stem, codegen_result.cargo_deps
    );
    match std::fs::write(&cargo_toml_path, &cargo_toml_content) {
        Ok(_)  => println!("✓ Written to {}", cargo_toml_path),
        Err(e) => {
            eprintln!("error: cannot write '{}': {}", cargo_toml_path, e);
            process::exit(1);
        }
    }

    // Write .cargo/config.toml
    let cargo_config_path = format!("{}/config.toml", cargo_dir);
    match std::fs::write(&cargo_config_path, &codegen_result.cargo_config) {
        Ok(_)  => println!("✓ Written to {}", cargo_config_path),
        Err(e) => {
            eprintln!("error: cannot write '{}': {}", cargo_config_path, e);
            process::exit(1);
        }
    }

    // Write memory.x linker script
    let memory_x_path = format!("{}/memory.x", out_dir);
    match std::fs::write(&memory_x_path, &codegen_result.memory_x) {
        Ok(_)  => println!("✓ Written to {}", memory_x_path),
        Err(e) => {
            eprintln!("error: cannot write '{}': {}", memory_x_path, e);
            process::exit(1);
        }
    }

    println!("✓ Generated project in {}/", out_dir);
}

// ── Shared lex + parse ────────────────────────────────────────────

fn lex_and_parse(source: &str, path: &str) -> ast::Program {
    let lex_result = lexer::lex(source, path);

    if !lex_result.errors.is_empty() {
        for e in &lex_result.errors {
            eprintln!("{}", e);
        }
        process::exit(1);
    }

    let parse_result = parser::parser::parse(lex_result.tokens, path);

    if !parse_result.errors.is_empty() {
        for e in &parse_result.errors {
            eprintln!("{}", e);
        }
        process::exit(1);
    }

    parse_result.program.unwrap_or_else(|| {
        eprintln!("error: parse produced no program");
        process::exit(1);
    })
}