// Ferrum recursive descent parser.
// Consumes Vec<SpannedToken> produced by the lexer.
// Produces a Program AST node or a Vec<ParseError>.
//
// STRUCTURE
//   One method per grammar rule.
//   Method names mirror the EBNF rule names exactly.
//   Every method returns Result<Node, ParseError>.
//
// ERROR RECOVERY
//   The parser never panics.
//   On a syntax error it records the error and attempts to
//   synchronise by advancing to the next safe recovery point
//   (next section keyword, next closing brace, or EOF).
//   This allows multiple errors to be reported in one pass.
//
// EVERY PLACEMENT
//   TopIfBlock and statement-level IfStmt are distinct AST nodes
//   with different body types. The parser decides which to produce
//   based on whether it is at the RUN top level or inside a block.
//   This enforces the EVERY placement constraint structurally.

use std::sync::Arc;

use crate::lexer::token::{Ident, Span, SpannedToken, Token};
use crate::ast::*;

// ----------------------------------------------------------------
// ParseError
// ----------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct ParseError {
    pub message:    String,
    pub span:       Span,
    pub suggestion: Option<String>,
}

impl ParseError {
    fn new(message: impl Into<String>, span: Span) -> Self {
        ParseError { message: message.into(), span, suggestion: None }
    }

    fn with_suggestion(
        message: impl Into<String>,
        span: Span,
        suggestion: impl Into<String>,
    ) -> Self {
        ParseError {
            message:    message.into(),
            span,
            suggestion: Some(suggestion.into()),
        }
    }
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] ParseError: {}", self.span, self.message)?;
        if let Some(s) = &self.suggestion {
            write!(f, "\n  Suggestion: {}", s)?;
        }
        Ok(())
    }
}

// ----------------------------------------------------------------
// ParseResult
// ----------------------------------------------------------------

pub struct ParseResult {
    pub program: Option<Program>,
    pub errors:  Vec<ParseError>,
}

// ----------------------------------------------------------------
// Parser
// ----------------------------------------------------------------

pub struct Parser {
    tokens:  Vec<SpannedToken>,
    pos:     usize,
    errors:  Vec<ParseError>,
    /// Shared file path — used when building synthetic spans.
    file:    Arc<String>,
    /// Whether the parser is currently at the RUN top level.
    /// Controls whether EVERY and TopIfBlock are valid productions.
    at_run_top_level: bool,
    /// Nesting depth of LOOP / FOR blocks.
    /// Controls whether BREAK / CONTINUE are valid.
    loop_depth: usize,
}

impl Parser {
    // ── Construction ────────────────────────────────────────────

    pub fn new(tokens: Vec<SpannedToken>, file: impl Into<String>) -> Self {
        let file = Arc::new(file.into());
        Parser {
            tokens,
            pos: 0,
            errors: Vec::new(),
            file,
            at_run_top_level: false,
            loop_depth: 0,
        }
    }

    // ── Public entry point ───────────────────────────────────────

    pub fn parse(mut self) -> ParseResult {
        let program = self.parse_program();
        ParseResult {
            program: if self.errors.is_empty() { Some(program) } else { None }
                .or_else(|| if self.errors.iter().all(|_| false) { None } else { Some(program) }),
            errors: self.errors,
        }
    }

    // ── program ─────────────────────────────────────────────────
    //
    // program = [ config_section ]
    //           { define_section }
    //           { create_section }
    //           { declare_section }
    //           { function_def }
    //           run_section
    //         ;

    fn parse_program(&mut self) -> Program {
        let start = self.current_span();

        let config    = if self.check(Token::Config)   { Some(self.parse_config_section()) }
                        else                           { None };
        let defines   = self.parse_many(Token::Define,   |p| p.parse_define_item());
        let creates   = self.parse_many(Token::Create,   |p| p.parse_create_item());
        let declares  = self.parse_many(Token::Declare,  |p| p.parse_declare_item());
        let functions = self.parse_many(Token::Function, |p| p.parse_function_def());
        let run       = self.parse_run_section();
        let span: Span      = start.to(&self.current_span());

        Program { config, defines, creates, declares, functions, run, span }
    }

    // ── CONFIG section ───────────────────────────────────────────
    //
    // config_section = "CONFIG" "{" { config_item } "}"

    fn parse_config_section(&mut self) -> ConfigSection {
        let start = self.expect(Token::Config);
        self.expect(Token::LBrace);
        let mut items = Vec::new();
        while !self.check(Token::RBrace) && !self.is_at_end() {
            if let Some(item) = self.try_parse(|p| p.parse_config_item()) {
                items.push(item);
            } else {
                self.synchronise_past(Token::Comma);
            }
        }
        let end = self.expect(Token::RBrace);
        ConfigSection { items, span: start.to(&end) }
    }

    // config_item = CONFIG_KEY "=" config_value
    //
    // CONFIG_KEY is parsed as an IDENTIFIER and validated in the
    // semantic pass. No comma after value (unlike the v1.2 draft —
    // the locked spec removed CONFIG commas).

    fn parse_config_item(&mut self) -> Result<ConfigItem, ParseError> {
        let key_span = self.current_span();
        let key_ident = self.expect_ident()?;
        let key = ConfigKey::from_str(&key_ident.key)
            .unwrap_or(ConfigKey::Unknown(key_ident.original.clone()));

        self.expect_token(Token::Eq)?;
        let value = self.parse_config_value()?;
        let span  = key_span.to(&self.prev_span());
        Ok(ConfigItem { key, value, span })
    }

    fn parse_config_value(&mut self) -> Result<ConfigValue, ParseError> {
        match self.current_token() {
            Token::StringLit(_) => {
                if let Token::StringLit(s) = self.advance().node {
                    Ok(ConfigValue::Str(s))
                } else { unreachable!() }
            }
            Token::ClockLit(_) => {
                if let Token::ClockLit(n) = self.advance().node {
                    Ok(ConfigValue::Clock(n))
                } else { unreachable!() }
            }
            Token::True  => { self.advance(); Ok(ConfigValue::Bool(true))  }
            Token::False => { self.advance(); Ok(ConfigValue::Bool(false)) }
            Token::IntLit(_) => {
                if let Token::IntLit(n) = self.advance().node {
                    Ok(ConfigValue::Int(n))
                } else { unreachable!() }
            }
            _ => Err(ParseError::new(
                format!("expected config value (string, integer, clock speed, or boolean), \
                         found {}", self.current_token().describe()),
                self.current_span(),
            )),
        }
    }

    // ── DEFINE section ───────────────────────────────────────────
    //
    // define_section = "DEFINE" definition_item { "," definition_item }

    fn parse_define_item(&mut self) -> Result<DefineItem, ParseError> {
        let start = self.expect_token(Token::Define)?;
        let name  = self.expect_ident()?;
        self.expect_token(Token::As)?;
        let spec  = self.parse_device_spec()?;
        let span  = start.to(&self.prev_span());

        // Inline limit: if there is a comma next, check we are not
        // in an inline context. The semantic pass enforces the hard
        // rule; here we just continue collecting comma-separated items
        // into a synthetic block to give better recovery.
        while self.consume_if(Token::Comma) {
            // Additional items after a comma are collected and will
            // be reported as an inline-limit error in the semantic pass.
            // We parse them anyway so the rest of the file stays valid.
            let _ = self.try_parse(|p| {
                let n = p.expect_ident()?;
                p.expect_token(Token::As)?;
                let s = p.parse_device_spec()?;
                let sp = n.span.clone().to(&p.prev_span());
                Ok(DefineItem { name: n, spec: s, span: sp })
            });
        }

        Ok(DefineItem { name, spec, span })
    }

    // device_spec = interface_spec
    //             | "{" interface_spec { "," interface_spec } "}"

    fn parse_device_spec(&mut self) -> Result<DeviceSpec, ParseError> {
        if self.check(Token::LBrace) {
            self.advance();
            let first = self.parse_interface_spec()?;
            let mut specs = vec![first];
            while self.consume_if(Token::Comma) {
                if self.check(Token::RBrace) { break; } // trailing comma
                specs.push(self.parse_interface_spec()?);
            }
            self.expect_token(Token::RBrace)?;
            Ok(DeviceSpec::Composite(specs))
        } else {
            Ok(DeviceSpec::Simple(self.parse_interface_spec()?))
        }
    }

    // interface_spec = INTERFACE_TYPE [ QUALIFIER ]

    fn parse_interface_spec(&mut self) -> Result<InterfaceSpec, ParseError> {
        let start     = self.current_span();
        let interface = self.parse_interface_type()?;
        let qualifier = if self.current_is_qualifier() {
            Some(self.parse_qualifier()?)
        } else {
            None
        };
        let span = start.to(&self.prev_span());
        Ok(InterfaceSpec { interface, qualifier, span })
    }

    fn parse_interface_type(&mut self) -> Result<InterfaceType, ParseError> {
        let span = self.current_span();
        match self.current_token() {
            Token::Input       => { self.advance(); Ok(InterfaceType::Input)       }
            Token::Output      => { self.advance(); Ok(InterfaceType::Output)      }
            Token::AnalogInput => { self.advance(); Ok(InterfaceType::AnalogInput) }
            Token::Pwm         => { self.advance(); Ok(InterfaceType::Pwm)         }
            Token::Display     => { self.advance(); Ok(InterfaceType::Display)     }
            Token::Pulse       => { self.advance(); Ok(InterfaceType::Pulse)       }
            _ => Err(ParseError::new(
                format!("expected interface type (INPUT, OUTPUT, ANALOG_INPUT, \
                         PWM, DISPLAY, PULSE), found {}",
                        self.current_token().describe()),
                span,
            )),
        }
    }

    fn current_is_qualifier(&self) -> bool {
        matches!(self.current_token(),
            Token::Brightness | Token::Speed  | Token::Angle  |
            Token::Red        | Token::Green  | Token::Blue   |
            Token::Lcd        | Token::Oled   |
            Token::Trigger    | Token::Echo   | Token::Enable
        )
    }

    fn parse_qualifier(&mut self) -> Result<Qualifier, ParseError> {
        let span = self.current_span();
        match self.current_token() {
            Token::Brightness => { self.advance(); Ok(Qualifier::Brightness) }
            Token::Speed      => { self.advance(); Ok(Qualifier::Speed)      }
            Token::Angle      => { self.advance(); Ok(Qualifier::Angle)      }
            Token::Red        => { self.advance(); Ok(Qualifier::Red)        }
            Token::Green      => { self.advance(); Ok(Qualifier::Green)      }
            Token::Blue       => { self.advance(); Ok(Qualifier::Blue)       }
            Token::Lcd        => { self.advance(); Ok(Qualifier::Lcd)        }
            Token::Oled       => { self.advance(); Ok(Qualifier::Oled)       }
            Token::Trigger    => { self.advance(); Ok(Qualifier::Trigger)    }
            Token::Echo       => { self.advance(); Ok(Qualifier::Echo)       }
            Token::Enable     => { self.advance(); Ok(Qualifier::Enable)     }
            _ => Err(ParseError::new(
                format!("expected qualifier, found {}", self.current_token().describe()),
                span,
            )),
        }
    }

    // ── CREATE section ───────────────────────────────────────────
    //
    // create_section = "CREATE" creation_item { "," creation_item }

    fn parse_create_item(&mut self) -> Result<CreateItem, ParseError> {
        let start         = self.expect_token(Token::Create)?;
        let device_type   = self.expect_ident()?;
        let instance_name = self.expect_ident()?;
        self.expect_token(Token::On)?;
        let pins          = self.parse_pin_spec()?;
        let pull          = if self.check(Token::Pull) {
                                Some(self.parse_pull_config()?)
                            } else { None };
        let init          = if self.check(Token::Init) {
                                Some(self.parse_init_block()?)
                            } else { None };
        let span          = start.to(&self.prev_span());

        // Consume trailing commas for block-form CREATE
        while self.consume_if(Token::Comma) {
            // Additional items parsed and discarded here for recovery;
            // the semantic pass validates inline-vs-block constraints.
            let _ = self.try_parse(|p| p.parse_create_item_body());
        }

        Ok(CreateItem { device_type, instance_name, pins, pull, init, span })
    }

    // Inner body used during comma-recovery — does not consume CREATE keyword
    fn parse_create_item_body(&mut self) -> Result<CreateItem, ParseError> {
        let start         = self.current_span();
        let device_type   = self.expect_ident()?;
        let instance_name = self.expect_ident()?;
        self.expect_token(Token::On)?;
        let pins          = self.parse_pin_spec()?;
        let pull          = if self.check(Token::Pull)  { Some(self.parse_pull_config()?) } else { None };
        let init          = if self.check(Token::Init)  { Some(self.parse_init_block()?)  } else { None };
        let span          = start.to(&self.prev_span());
        Ok(CreateItem { device_type, instance_name, pins, pull, init, span })
    }

    // pin_spec = "PIN" INTEGER_LIT
    //          | "{" pin_assignment { "," pin_assignment } "}"

    fn parse_pin_spec(&mut self) -> Result<PinSpec, ParseError> {
        if self.check(Token::Pin) {
            let start = self.advance().span;
            let n     = self.expect_integer()?;
            let span  = start.to(&self.prev_span());
            Ok(PinSpec::Single { pin: n as u32, span })
        } else if self.check(Token::LBrace) {
            self.advance();
            let first = self.parse_pin_assignment()?;
            let mut assignments = vec![first];
            while self.consume_if(Token::Comma) {
                if self.check(Token::RBrace) { break; }
                assignments.push(self.parse_pin_assignment()?);
            }
            self.expect_token(Token::RBrace)?;
            Ok(PinSpec::Multi(assignments))
        } else {
            Err(ParseError::new(
                format!("expected 'PIN' or '{{' for pin assignment, found {}",
                        self.current_token().describe()),
                self.current_span(),
            ))
        }
    }

    // pin_assignment = [ IDENTIFIER ":" ] "PIN" INTEGER_LIT [ IDENTIFIER ]
    //
    // Three forms:
    //   Positional:  PIN 3
    //   Named:       output: PIN 3
    //   Mixed:       PIN 3 OUTPUT

    fn parse_pin_assignment(&mut self) -> Result<PinAssignment, ParseError> {
        let start = self.current_span();

        // Determine form by lookahead
        // Named form: IDENTIFIER ":"  PIN  NUMBER
        let interface_name = if self.check_ident() && self.peek_is(Token::Colon) {
            let name = self.expect_ident()?;
            self.expect_token(Token::Colon)?;
            Some(name)
        } else {
            None
        };

        self.expect_token(Token::Pin)?;
        let pin_number = self.expect_integer()? as u32;

        // Mixed form: optional trailing IDENTIFIER for disambiguation
        let disambiguator = if self.check_ident() && interface_name.is_none() {
            Some(self.expect_ident()?)
        } else {
            None
        };

        let span = start.to(&self.prev_span());
        Ok(PinAssignment { interface_name, pin_number, disambiguator, span })
    }

    // pull_config = "PULL" ( "UP" | "DOWN" )

    fn parse_pull_config(&mut self) -> Result<PullConfig, ParseError> {
        self.expect_token(Token::Pull)?;
        match self.current_token() {
            Token::Up   => { self.advance(); Ok(PullConfig::Up)   }
            Token::Down => { self.advance(); Ok(PullConfig::Down) }
            _ => Err(ParseError::new(
                format!("expected 'UP' or 'DOWN' after PULL, found {}",
                        self.current_token().describe()),
                self.current_span(),
            )),
        }
    }

    // init_block = "INIT" init_body
    // init_body  = init_value
    //            | "{" named_init_value { "," named_init_value } "}"

    fn parse_init_block(&mut self) -> Result<InitBlock, ParseError> {
        let start = self.expect_token(Token::Init)?;
        if self.check(Token::LBrace) {
            self.advance();
            let first = self.parse_named_init_value()?;
            let mut values = vec![first];
            while self.consume_if(Token::Comma) {
                if self.check(Token::RBrace) { break; }
                values.push(self.parse_named_init_value()?);
            }
            let end = self.expect_token(Token::RBrace)?;
            Ok(InitBlock { values, span: start.to(&end) })
        } else {
            let value = self.parse_init_value()?;
            let span  = start.to(&self.prev_span());
            Ok(InitBlock { values: vec![InitEntry { interface_name: None, value, span: span.clone() }], span })
        }
    }

    // named_init_value = [ IDENTIFIER ":" ] init_value

    fn parse_named_init_value(&mut self) -> Result<InitEntry, ParseError> {
        let start = self.current_span();
        let interface_name = if self.check_ident() && self.peek_is(Token::Colon) {
            let name = self.expect_ident()?;
            self.expect_token(Token::Colon)?;
            Some(name)
        } else {
            None
        };
        let value = self.parse_init_value()?;
        let span  = start.to(&self.prev_span());
        Ok(InitEntry { interface_name, value, span })
    }

    // init_value = "HIGH" | "LOW" | DECIMAL_LIT | STRING | INTEGER_LIT

    fn parse_init_value(&mut self) -> Result<InitValue, ParseError> {
        match self.current_token() {
            Token::High           => { self.advance(); Ok(InitValue::High)              }
            Token::Low            => { self.advance(); Ok(InitValue::Low)               }
            Token::DecimalLit(_)  => {
                if let Token::DecimalLit(f) = self.advance().node { Ok(InitValue::Decimal(f)) }
                else { unreachable!() }
            }
            Token::StringLit(_)  => {
                if let Token::StringLit(s) = self.advance().node { Ok(InitValue::Str(s)) }
                else { unreachable!() }
            }
            Token::IntLit(_)     => {
                if let Token::IntLit(n) = self.advance().node { Ok(InitValue::Int(n)) }
                else { unreachable!() }
            }
            _ => Err(ParseError::new(
                format!("expected init value (HIGH, LOW, decimal, string, or integer), \
                         found {}", self.current_token().describe()),
                self.current_span(),
            )),
        }
    }

    // ── DECLARE section ──────────────────────────────────────────
    //
    // declare_section = "DECLARE" declaration_item { "," declaration_item }

    fn parse_declare_item(&mut self) -> Result<DeclareItem, ParseError> {
        let start = self.expect_token(Token::Declare)?;

        // CONSTANT form
        if self.check(Token::Constant) {
            self.advance();
            let ty    = self.parse_data_type()?;
            let name  = self.expect_ident()?;
            self.expect_token(Token::Eq)?;
            let value = self.parse_literal()?;
            let span  = start.to(&self.prev_span());
            return Ok(DeclareItem {
                kind: DeclareKind::Constant(ConstantDecl { ty, name, value, span: span.clone() }),
                span,
            });
        }

        // Variable form
        let ty         = self.parse_data_type()?;
        let array_size = if self.check(Token::LBracket) {
            self.advance();
            let n = self.expect_integer()? as usize;
            self.expect_token(Token::RBracket)?;
            Some(n)
        } else { None };
        let name = self.expect_ident()?;
        let init = if self.check(Token::Init) {
            self.advance();
            Some(self.parse_init_expr()?)
        } else { None };
        let span = start.to(&self.prev_span());

        Ok(DeclareItem {
            kind: DeclareKind::Variable(VariableDecl { ty, array_size, name, init, span: span.clone() }),
            span,
        })
    }

    // init_expr = literal | "[" literal { "," literal } "]"

    fn parse_init_expr(&mut self) -> Result<InitExpr, ParseError> {
        if self.check(Token::LBracket) {
            self.advance();
            let first = self.parse_literal()?;
            let mut items = vec![first];
            while self.consume_if(Token::Comma) {
                if self.check(Token::RBracket) { break; }
                items.push(self.parse_literal()?);
            }
            self.expect_token(Token::RBracket)?;
            Ok(InitExpr::Array(items))
        } else {
            Ok(InitExpr::Single(self.parse_literal()?))
        }
    }

    fn parse_data_type(&mut self) -> Result<DataType, ParseError> {
        let span = self.current_span();
        match self.current_token() {
            Token::TyInteger    => { self.advance(); Ok(DataType::Integer)    }
            Token::TyDecimal    => { self.advance(); Ok(DataType::Decimal)    }
            Token::TyPercentage => { self.advance(); Ok(DataType::Percentage) }
            Token::TyBoolean    => { self.advance(); Ok(DataType::Boolean)    }
            Token::TyString     => { self.advance(); Ok(DataType::String)     }
            Token::TyByte       => { self.advance(); Ok(DataType::Byte)       }
            _ => Err(ParseError::new(
                format!("expected type name (Integer, Decimal, Percentage, \
                         Boolean, String, Byte), found {}",
                        self.current_token().describe()),
                span,
            )),
        }
    }

    // ── FUNCTION section ─────────────────────────────────────────
    //
    // function_def = "FUNCTION" IDENTIFIER [ param_list ]
    //                "{" { statement } [ return_stmt ] "}"

    fn parse_function_def(&mut self) -> Result<FunctionDef, ParseError> {
        let start  = self.expect_token(Token::Function)?;
        let name   = self.expect_ident()?;
        let params = if !self.check(Token::LBrace) {
            self.parse_param_list()?
        } else {
            Vec::new()
        };
        self.expect_token(Token::LBrace)?;
        let (body, ret) = self.parse_function_body()?;
        let end         = self.expect_token(Token::RBrace)?;
        let span        = start.to(&end);
        Ok(FunctionDef { name, params, body, ret, return_type: None, span })
    }

    // param_list = param { "," param }

    fn parse_param_list(&mut self) -> Result<Vec<Param>, ParseError> {
        let mut params = vec![self.parse_param()?];
        while self.consume_if(Token::Comma) {
            // Stop if we hit the opening brace — no trailing commas in params
            if self.check(Token::LBrace) { break; }
            params.push(self.parse_param()?);
        }
        Ok(params)
    }

    // param = data_type ":" IDENTIFIER            -- data parameter
    //       | ownership IDENTIFIER ":" IDENTIFIER -- device parameter

    fn parse_param(&mut self) -> Result<Param, ParseError> {
        let start = self.current_span();

        // Device parameter: ownership keyword first
        if let Some(ownership) = self.try_parse_ownership() {
            let device_ty = self.expect_ident()?;
            self.expect_token(Token::Colon)?;
            let name      = self.expect_ident()?;
            let span      = start.to(&self.prev_span());
            return Ok(Param::Device { ownership, device_ty, name, span });
        }

        // Data parameter: type name first
        let ty   = self.parse_data_type()?;
        self.expect_token(Token::Colon)?;
        let name = self.expect_ident()?;
        let span = start.to(&self.prev_span());
        Ok(Param::Data { ty, name, span })
    }

    fn try_parse_ownership(&mut self) -> Option<Ownership> {
        match self.current_token() {
            Token::Give   => { self.advance(); Some(Ownership::Give)   }
            Token::Lend   => { self.advance(); Some(Ownership::Lend)   }
            Token::Borrow => { self.advance(); Some(Ownership::Borrow) }
            _             => None,
        }
    }

    // Parse the body of a function: statements followed by an optional RETURN.
    // RETURN must be the last statement (or absent).

    fn parse_function_body(&mut self) -> Result<(Vec<Statement>, Option<ReturnStmt>), ParseError> {
        let mut body = Vec::new();
        let mut ret  = None;

        loop {
            if self.check(Token::RBrace) || self.is_at_end() { break; }

            if self.check(Token::Return) {
                ret = Some(self.parse_return_stmt()?);
                break; // RETURN is always last
            }

            match self.try_parse(|p| p.parse_statement()) {
                Some(stmt) => body.push(stmt),
                None       => { self.synchronise_to_next_statement(); }
            }
        }

        Ok((body, ret))
    }

    fn parse_return_stmt(&mut self) -> Result<ReturnStmt, ParseError> {
        let start = self.expect_token(Token::Return)?;
        // Void return: RETURN with no expression (next token is } or another keyword)
        let value = if !self.check(Token::RBrace) && !self.is_at_end() {
            Some(self.parse_expression()?)
        } else {
            None
        };
        let span = start.to(&self.prev_span());
        Ok(ReturnStmt { value, span })
    }

    // ── RUN section ──────────────────────────────────────────────
    //
    // run_section = "RUN" "{" { run_item } "}"

    fn parse_run_section(&mut self) -> RunSection {
        let start = self.current_span();
        if !self.check(Token::Run) {
            self.errors.push(ParseError::new(
                "expected 'RUN' section — every Ferrum program must have a RUN block",
                self.current_span(),
            ));
            return RunSection { items: Vec::new(), span: start };
        }
        let kw = self.advance().span;
        if let Err(e) = self.expect_token(Token::LBrace) {
            self.errors.push(e);
            return RunSection { items: Vec::new(), span: kw };
        }

        self.at_run_top_level = true;
        let mut items = Vec::new();

        loop {
            if self.check(Token::RBrace) || self.is_at_end() { break; }
            match self.try_parse(|p| p.parse_run_item()) {
                Some(item) => items.push(item),
                None       => { self.synchronise_to_next_statement(); }
            }
        }

        self.at_run_top_level = false;
        let end = self.current_span();
        if let Err(e) = self.expect_token(Token::RBrace) {
            self.errors.push(e);
        }
        let span = start.to(&end);
        RunSection { items, span }
    }

    // run_item = every_block | top_if_block | loop_block | statement
    //
    // This is the structural enforcement of EVERY placement.
    // every_block and top_if_block are only reachable from this method,
    // which is only called when at_run_top_level == true.

    fn parse_run_item(&mut self) -> Result<RunItem, ParseError> {
        match self.current_token() {
            Token::Every => Ok(RunItem::Every(self.parse_every_block()?)),
            Token::If    => {
                // At RUN top level, IF produces a TopIfBlock (body = Vec<RunItem>)
                // which can contain EVERY. This is different from statement-level IF.
                Ok(RunItem::TopIf(self.parse_top_if_block()?))
            }
            Token::Loop  => Ok(RunItem::Loop(self.parse_loop_block()?)),
            _            => {
                self.at_run_top_level = false;
                let stmt = self.parse_statement()?;
                self.at_run_top_level = true;
                Ok(RunItem::Stmt(stmt))
            }
        }
    }

    // every_block = "EVERY" duration "{" { statement } "}"

    fn parse_every_block(&mut self) -> Result<EveryBlock, ParseError> {
        let start    = self.expect_token(Token::Every)?;
        let period   = self.parse_duration()?;
        self.expect_token(Token::LBrace)?;
        let mut body = Vec::new();

        // EVERY body contains statements only — EVERY is not valid inside here
        loop {
            if self.check(Token::RBrace) || self.is_at_end() { break; }
            if self.check(Token::Every) {
                // Structural enforcement: EVERY inside EVERY is illegal
                self.errors.push(ParseError::with_suggestion(
                    "'EVERY' is not valid inside another EVERY block",
                    self.current_span(),
                    "Move the inner EVERY block to the top level of RUN",
                ));
                self.advance(); // consume the illegal EVERY and skip to next }
                self.synchronise_past(Token::RBrace);
                continue;
            }
            match self.try_parse(|p| p.parse_statement()) {
                Some(s) => body.push(s),
                None    => { self.synchronise_to_next_statement(); }
            }
        }

        let end  = self.expect_token(Token::RBrace)?;
        let span = start.to(&end);
        Ok(EveryBlock { period, body, span })
    }

    // top_if_block = "IF" expression "{" { run_item } "}" [ "ELSE" "{" { run_item } "}" ]
    //
    // The body is Vec<RunItem>, not Vec<Statement>.
    // This allows EVERY to appear inside an IF at the RUN top level.

    fn parse_top_if_block(&mut self) -> Result<TopIfBlock, ParseError> {
        let start     = self.expect_token(Token::If)?;
        let condition = self.parse_expression()?;
        self.expect_token(Token::LBrace)?;
        let then_items = self.parse_run_items_until_brace()?;
        self.expect_token(Token::RBrace)?;

        let else_items = if self.consume_if(Token::Else) {
            self.expect_token(Token::LBrace)?;
            let items = self.parse_run_items_until_brace()?;
            self.expect_token(Token::RBrace)?;
            Some(items)
        } else {
            None
        };

        let span = start.to(&self.prev_span());
        Ok(TopIfBlock { condition, then_items, else_items, span })
    }

    fn parse_run_items_until_brace(&mut self) -> Result<Vec<RunItem>, ParseError> {
        let mut items = Vec::new();
        loop {
            if self.check(Token::RBrace) || self.is_at_end() { break; }
            match self.try_parse(|p| p.parse_run_item()) {
                Some(item) => items.push(item),
                None       => { self.synchronise_to_next_statement(); }
            }
        }
        Ok(items)
    }

    // loop_block = "LOOP" "{" { statement } "}"

    fn parse_loop_block(&mut self) -> Result<LoopBlock, ParseError> {
        let start = self.expect_token(Token::Loop)?;
        self.expect_token(Token::LBrace)?;
        self.loop_depth += 1;
        let body = self.parse_statements_until_brace()?;
        self.loop_depth -= 1;
        let end  = self.expect_token(Token::RBrace)?;
        let span = start.to(&end);
        Ok(LoopBlock { body, span })
    }

    // duration = INTEGER_LIT ( "ms" | "s" )

    fn parse_duration(&mut self) -> Result<Duration, ParseError> {
        let start = self.current_span();
        let value = self.expect_integer()? as u32;
        let unit  = match self.current_token() {
            Token::Ms => { self.advance(); TimeUnit::Milliseconds }
            Token::S  => { self.advance(); TimeUnit::Seconds      }
            _ => return Err(ParseError::with_suggestion(
                format!("expected time unit 'ms' or 's' after {}, found {}",
                        value, self.current_token().describe()),
                self.current_span(),
                format!("Write '{}ms' or '{}s' — no space between number and unit", value, value),
            )),
        };
        let span = start.to(&self.prev_span());
        Ok(Duration { value, unit, span })
    }

    // ── Statements ───────────────────────────────────────────────

    fn parse_statements_until_brace(&mut self) -> Result<Vec<Statement>, ParseError> {
        let mut stmts = Vec::new();
        loop {
            if self.check(Token::RBrace) || self.is_at_end() { break; }
            // EVERY inside a non-top-level block: structural error
            if self.check(Token::Every) {
                self.errors.push(ParseError::with_suggestion(
                    "'EVERY' is not valid inside a LOOP, FOR, or IF block",
                    self.current_span(),
                    "'EVERY' may only appear at the top level of RUN, \
                     or inside an IF at the RUN top level",
                ));
                self.advance();
                self.synchronise_past(Token::RBrace);
                continue;
            }
            match self.try_parse(|p| p.parse_statement()) {
                Some(s) => stmts.push(s),
                None    => { self.synchronise_to_next_statement(); }
            }
        }
        Ok(stmts)
    }

    fn parse_statement(&mut self) -> Result<Statement, ParseError> {
        let start = self.current_span();

        match self.current_token() {
            // SET device [QUALIFIER] expr
            Token::Set    => self.parse_set_stmt(),
            // TURN device HIGH | LOW
            Token::Turn   => self.parse_turn_stmt(),
            // TOGGLE device
            Token::Toggle => self.parse_toggle_stmt(),
            // PRINT expr
            Token::Print  => self.parse_print_stmt(),
            // CALL function [args]
            Token::Call   => self.parse_void_call_or_declare_via_call(),
            // DELAY duration
            Token::Delay  => self.parse_delay_stmt(),
            // IF expr { ... } [ ELSE { ... } ]
            Token::If     => self.parse_if_stmt(),
            // FOR variable IN iterator { ... }
            Token::For    => self.parse_for_stmt(),
            // BREAK
            Token::Break  => {
                let span = self.advance().span;
                if self.loop_depth == 0 {
                    return Err(ParseError::with_suggestion(
                        "'BREAK' is not valid here — no enclosing LOOP or FOR block",
                        span,
                        "BREAK may only appear inside a LOOP or FOR block",
                    ));
                }
                Ok(Statement { kind: StmtKind::Break, span })
            }
            // CONTINUE
            Token::Continue => {
                let span = self.advance().span;
                if self.loop_depth == 0 {
                    return Err(ParseError::with_suggestion(
                        "'CONTINUE' is not valid here — no enclosing LOOP or FOR block",
                        span,
                        "CONTINUE may only appear inside a LOOP or FOR block",
                    ));
                }
                Ok(Statement { kind: StmtKind::Continue, span })
            }
            // DECLARE type [array] name (INIT expr | = CALL ...)
            Token::Declare => self.parse_inline_declare_stmt(),

            // IDENTIFIER = expr   (assignment)
            // Must be last — catch-all after all keyword forms
            Token::Ident(_) => self.parse_assignment_stmt(),

            // Bare function name without CALL — helpful error
            t if self.is_known_function_name_lookahead() => {
                let name_span = self.current_span();
                let name = if let Token::Ident(i) = self.advance().node { i.original } else { "?".into() };
                Err(ParseError::with_suggestion(
                    format!("'{}' is not a variable — did you mean to call a function?", name),
                    name_span,
                    format!("Use CALL to invoke a function: CALL {}", name),
                ))
            }
            _ => Err(ParseError::new(
                format!("unexpected token {} — expected a statement",
                        self.current_token().describe()),
                start,
            )),
        }
    }

    // assignment_stmt = IDENTIFIER "=" expression

    fn parse_assignment_stmt(&mut self) -> Result<Statement, ParseError> {
        let start  = self.current_span();
        let target = self.expect_ident()?;
        self.expect_token(Token::Eq)?;
        let value  = self.parse_expression()?;
        let span   = start.to(&self.prev_span());
        Ok(Statement {
            kind: StmtKind::Assignment(AssignStmt { target, value, span: span.clone() }),
            span,
        })
    }

    // set_stmt = "SET" IDENTIFIER [ QUALIFIER ] expression

    fn parse_set_stmt(&mut self) -> Result<Statement, ParseError> {
        let start     = self.expect_token(Token::Set)?;
        let device    = self.expect_ident()?;
        let qualifier = if self.current_is_qualifier() {
            Some(self.parse_qualifier()?)
        } else { None };
        let value     = self.parse_expression()?;
        let span      = start.to(&self.prev_span());
        Ok(Statement {
            kind: StmtKind::Set(SetStmt { device, qualifier, value, span: span.clone() }),
            span,
        })
    }

    // turn_stmt = "TURN" IDENTIFIER [ QUALIFIER ] ( "HIGH" | "LOW" )

    fn parse_turn_stmt(&mut self) -> Result<Statement, ParseError> {
        let start     = self.expect_token(Token::Turn)?;
        let device    = self.expect_ident()?;
        // Optional qualifier for TURN pump ENABLE HIGH
        let qualifier = if self.current_is_qualifier() {
            Some(self.parse_qualifier()?)
        } else { None };
        let state = match self.current_token() {
            Token::High => { self.advance(); PinState::High }
            Token::Low  => { self.advance(); PinState::Low  }
            _ => return Err(ParseError::new(
                format!("expected HIGH or LOW after TURN {}, found {}",
                        device.original, self.current_token().describe()),
                self.current_span(),
            )),
        };
        let span = start.to(&self.prev_span());
        Ok(Statement {
            kind: StmtKind::Turn(TurnStmt { device, qualifier, state, span: span.clone() }),
            span,
        })
    }

    // toggle_stmt = "TOGGLE" IDENTIFIER

    fn parse_toggle_stmt(&mut self) -> Result<Statement, ParseError> {
        let start  = self.expect_token(Token::Toggle)?;
        let device = self.expect_ident()?;
        let span   = start.to(&self.prev_span());
        Ok(Statement {
            kind: StmtKind::Toggle(ToggleStmt { device, span: span.clone() }),
            span,
        })
    }

    // print_stmt = "PRINT" expression

    fn parse_print_stmt(&mut self) -> Result<Statement, ParseError> {
        let start = self.expect_token(Token::Print)?;
        let value = self.parse_expression()?;
        let span  = start.to(&self.prev_span());
        Ok(Statement {
            kind: StmtKind::Print(PrintStmt { value, span: span.clone() }),
            span,
        })
    }

    // void_call_stmt = "CALL" IDENTIFIER [ call_arg_list ]
    //
    // This method also handles the DECLARE ... = CALL ... form,
    // which begins with DECLARE not CALL, so that is handled in
    // parse_inline_declare_stmt instead.

    fn parse_void_call_or_declare_via_call(&mut self) -> Result<Statement, ParseError> {
        let start    = self.expect_token(Token::Call)?;
        let function = self.expect_ident()?;
        let args     = self.parse_call_arg_list()?;
        let span     = start.to(&self.prev_span());
        Ok(Statement {
            kind: StmtKind::VoidCall(CallStmt { function, args, span: span.clone() }),
            span,
        })
    }

    // delay_stmt = "DELAY" duration

    fn parse_delay_stmt(&mut self) -> Result<Statement, ParseError> {
        let start    = self.expect_token(Token::Delay)?;
        let duration = self.parse_duration()?;
        let span     = start.to(&self.prev_span());
        Ok(Statement {
            kind: StmtKind::Delay(DelayStmt { duration, span: span.clone() }),
            span,
        })
    }

    // if_stmt = "IF" expression "{" { statement } "}" [ "ELSE" "{" { statement } "}" ]
    //
    // Statement-level IF — body is Vec<Statement>, not Vec<RunItem>.
    // EVERY is not permitted here (caught in parse_statements_until_brace).
    // No ELSE IF — the else body is Vec<Statement> only.

    fn parse_if_stmt(&mut self) -> Result<Statement, ParseError> {
        let start     = self.expect_token(Token::If)?;
        let condition = self.parse_expression()?;
        self.expect_token(Token::LBrace)?;
        let then_body = self.parse_statements_until_brace()?;
        self.expect_token(Token::RBrace)?;

        let else_body = if self.consume_if(Token::Else) {
            // Detect illegal ELSE IF and report a helpful error
            if self.check(Token::If) {
                self.errors.push(ParseError::with_suggestion(
                    "ELSE IF is not supported in Ferrum",
                    self.current_span(),
                    "Nest an IF inside the ELSE block instead:\n  \
                     } ELSE {\n    IF condition {\n      ...\n    }\n  }",
                ));
                // Recover: parse the nested IF as a statement
                let nested = self.parse_if_stmt()?;
                Some(ElseClause::Block(vec![nested]))
            } else {
                self.expect_token(Token::LBrace)?;
                let body = self.parse_statements_until_brace()?;
                self.expect_token(Token::RBrace)?;
                Some(ElseClause::Block(body))
            }
        } else {
            None
        };

        let span = start.to(&self.prev_span());
        Ok(Statement {
            kind: StmtKind::If(IfStmt { condition, then_body, else_body, span: span.clone() }),
            span,
        })
    }

    // for_stmt = "FOR" IDENTIFIER "IN" for_iterator "{" { statement } "}"
    // for_iterator = range_expr | expression

    fn parse_for_stmt(&mut self) -> Result<Statement, ParseError> {
        let start    = self.expect_token(Token::For)?;
        let variable = self.expect_ident()?;
        self.expect_token(Token::In)?;

        let iterator = if self.check(Token::Range) {
            self.advance();
            let from = self.parse_expression()?;
            self.expect_token(Token::Comma)?;
            let to   = self.parse_expression()?;
            ForIterator::Range { from, to }
        } else {
            ForIterator::Array(self.parse_expression()?)
        };

        self.expect_token(Token::LBrace)?;
        self.loop_depth += 1;
        let body = self.parse_statements_until_brace()?;
        self.loop_depth -= 1;
        let end  = self.expect_token(Token::RBrace)?;
        let span = start.to(&end);

        Ok(Statement {
            kind: StmtKind::For(ForStmt { variable, iterator, body, span: span.clone() }),
            span,
        })
    }

    // inline_declare_stmt = "DECLARE" data_type [ "[" INTEGER_LIT "]" ] IDENTIFIER
    //                        ( "INIT" init_expr | "=" call_expression )

    fn parse_inline_declare_stmt(&mut self) -> Result<Statement, ParseError> {
        let start      = self.expect_token(Token::Declare)?;
        let ty         = self.parse_data_type()?;
        let array_size = if self.check(Token::LBracket) {
            self.advance();
            let n = self.expect_integer()? as usize;
            self.expect_token(Token::RBracket)?;
            Some(n)
        } else { None };
        let name = self.expect_ident()?;

        let init = if self.check(Token::Init) {
            self.advance();
            InlineDeclareInit::Value(self.parse_init_expr()?)
        } else if self.check(Token::Eq) {
            self.advance();
            // Must be CALL expression
            if !self.check(Token::Call) {
                return Err(ParseError::with_suggestion(
                    format!("expected CALL expression after '=', found {}",
                            self.current_token().describe()),
                    self.current_span(),
                    "Use: DECLARE Integer result = CALL add 5, 3",
                ));
            }
            InlineDeclareInit::Call(self.parse_call_expression()?)
        } else {
            return Err(ParseError::with_suggestion(
                format!("expected 'INIT' or '=' after variable name '{}', \
                         found {}", name.original, self.current_token().describe()),
                self.current_span(),
                format!("Use 'INIT' to set an initial value: DECLARE Integer {} INIT 0", name.original),
            ));
        };

        let span = start.to(&self.prev_span());
        Ok(Statement {
            kind: StmtKind::InlineDeclare(InlineDeclareStmt {
                ty, array_size, name, init, span: span.clone(),
            }),
            span,
        })
    }

    // ── Call arguments ───────────────────────────────────────────
    //
    // call_arg_list = call_arg { "," call_arg }
    // call_arg = ownership IDENTIFIER | expression

    fn parse_call_arg_list(&mut self) -> Result<Vec<CallArg>, ParseError> {
        let mut args = Vec::new();
        // No args: next token starts a new statement or closes a block
        if self.call_arg_list_ends() { return Ok(args); }

        args.push(self.parse_call_arg()?);
        while self.consume_if(Token::Comma) {
            if self.call_arg_list_ends() { break; }
            args.push(self.parse_call_arg()?);
        }
        Ok(args)
    }

    /// True when the call argument list has ended — i.e. the next
    /// token cannot start a call_arg.
    fn call_arg_list_ends(&self) -> bool {
        matches!(self.current_token(),
            Token::RBrace | Token::RBracket | Token::RParen |
            Token::Eof    | Token::Declare  | Token::Set    |
            Token::Turn   | Token::Toggle   | Token::Print  |
            Token::Delay  | Token::If       | Token::For    |
            Token::Loop   | Token::Every    | Token::Break  |
            Token::Continue | Token::Return | Token::Call   |
            Token::Run    | Token::Function
        )
    }

    fn parse_call_arg(&mut self) -> Result<CallArg, ParseError> {
        let start = self.current_span();
        if let Some(ownership) = self.try_parse_ownership() {
            let name = self.expect_ident()?;
            let span = start.to(&self.prev_span());
            Ok(CallArg { kind: CallArgKind::Device { ownership, name }, span })
        } else {
            let expr = self.parse_expression()?;
            let span = start.to(&self.prev_span());
            Ok(CallArg { kind: CallArgKind::Data(expr), span })
        }
    }

    fn parse_call_expression(&mut self) -> Result<CallExpr, ParseError> {
        let start    = self.expect_token(Token::Call)?;
        let function = self.expect_ident()?;
        let args     = self.parse_call_arg_list()?;
        let span     = start.to(&self.prev_span());
        Ok(CallExpr { function, args, span })
    }

    // ── Expressions ──────────────────────────────────────────────
    //
    // Precedence (lowest → highest):
    //   or_expr → and_expr → equality_expr → comparison_expr
    //   → additive_expr → multiplicative_expr → unary_expr → primary_expr

    fn parse_expression(&mut self) -> Result<Expr, ParseError> {
        self.parse_or_expr()
    }

    fn parse_or_expr(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_and_expr()?;
        while self.check(Token::Or) {
            let op_span = self.advance().span;
            let right   = self.parse_and_expr()?;
            let span    = left.span.to(&right.span);
            left = Expr {
                kind: ExprKind::BinOp { op: BinOp::Or, left: Box::new(left), right: Box::new(right) },
                ty: None,
                span,
            };
            let _ = op_span;
        }
        Ok(left)
    }

    fn parse_and_expr(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_equality_expr()?;
        while self.check(Token::And) {
            let op_span = self.advance().span;
            let right   = self.parse_equality_expr()?;
            let span    = left.span.to(&right.span);
            left = Expr {
                kind: ExprKind::BinOp { op: BinOp::And, left: Box::new(left), right: Box::new(right) },
                ty: None,
                span,
            };
            let _ = op_span;
        }
        Ok(left)
    }

    fn parse_equality_expr(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_comparison_expr()?;
        loop {
            let op = match self.current_token() {
                Token::EqEq  => BinOp::Eq,
                Token::NotEq => BinOp::NotEq,
                _            => break,
            };
            self.advance();
            let right = self.parse_comparison_expr()?;
            let span  = left.span.to(&right.span);
            left = Expr {
                kind: ExprKind::BinOp { op, left: Box::new(left), right: Box::new(right) },
                ty: None,
                span,
            };
        }
        Ok(left)
    }

    fn parse_comparison_expr(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_additive_expr()?;
        loop {
            // IS or IS NOT
            if self.check(Token::Is) {
                let op_span = self.advance().span;
                let negated = self.consume_if(Token::Not);
                let state   = self.parse_additive_expr()?;
                let span    = left.span.to(&state.span);
                left = Expr {
                    kind: ExprKind::Is {
                        target:  Box::new(left),
                        negated,
                        state:   Box::new(state),
                    },
                    ty: None,
                    span,
                };
                let _ = op_span;
                continue;
            }

            let op = match self.current_token() {
                Token::Gt   => BinOp::Gt,
                Token::Lt   => BinOp::Lt,
                Token::GtEq => BinOp::Gte,
                Token::LtEq => BinOp::Lte,
                _           => break,
            };
            self.advance();
            let right = self.parse_additive_expr()?;
            let span  = left.span.to(&right.span);
            left = Expr {
                kind: ExprKind::BinOp { op, left: Box::new(left), right: Box::new(right) },
                ty: None,
                span,
            };
        }
        Ok(left)
    }

    fn parse_additive_expr(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_multiplicative_expr()?;
        loop {
            let op = match self.current_token() {
                Token::Plus  => BinOp::Add,
                Token::Minus => BinOp::Sub,
                _            => break,
            };
            self.advance();
            let right = self.parse_multiplicative_expr()?;
            let span  = left.span.to(&right.span);
            left = Expr {
                kind: ExprKind::BinOp { op, left: Box::new(left), right: Box::new(right) },
                ty: None,
                span,
            };
        }
        Ok(left)
    }

    fn parse_multiplicative_expr(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_unary_expr()?;
        loop {
            let op = match self.current_token() {
                Token::Star  => BinOp::Mul,
                Token::Slash => BinOp::Div,
                _            => break,
            };
            self.advance();
            let right = self.parse_unary_expr()?;
            let span  = left.span.to(&right.span);
            left = Expr {
                kind: ExprKind::BinOp { op, left: Box::new(left), right: Box::new(right) },
                ty: None,
                span,
            };
        }
        Ok(left)
    }

    fn parse_unary_expr(&mut self) -> Result<Expr, ParseError> {
        match self.current_token() {
            Token::Not => {
                let start   = self.advance().span;
                let operand = self.parse_unary_expr()?;
                let span    = start.to(&operand.span);
                Ok(Expr {
                    kind: ExprKind::UnaryOp { op: UnaryOp::Not, operand: Box::new(operand) },
                    ty: None,
                    span,
                })
            }
            Token::Minus => {
                let start   = self.advance().span;
                let operand = self.parse_unary_expr()?;
                let span    = start.to(&operand.span);
                Ok(Expr {
                    kind: ExprKind::UnaryOp { op: UnaryOp::Neg, operand: Box::new(operand) },
                    ty: None,
                    span,
                })
            }
            _ => self.parse_primary_expr(),
        }
    }

    // primary_expr = literal | read_expr | call_expression
    //              | if_expr | "(" expression ")" | IDENTIFIER

    fn parse_primary_expr(&mut self) -> Result<Expr, ParseError> {
        let start = self.current_span();

        match self.current_token() {
            // Literal
            Token::IntLit(_)    |
            Token::HexLit(_)    |
            Token::DecimalLit(_)|
            Token::StringLit(_) |
            Token::True         |
            Token::False        |
            Token::High         |
            Token::Low          => {
                let lit  = self.parse_literal()?;
                let span = start.to(&self.prev_span());
                Ok(Expr { kind: ExprKind::Literal(lit), ty: None, span })
            }

            // READ or READ_PERCENT
            Token::Read => {
                self.advance();
                let device = self.expect_ident()?;
                let span   = start.to(&self.prev_span());
                Ok(Expr { kind: ExprKind::Read(device), ty: None, span })
            }
            Token::ReadPercent => {
                self.advance();
                let device = self.expect_ident()?;
                let span   = start.to(&self.prev_span());
                Ok(Expr { kind: ExprKind::ReadPercent(device), ty: None, span })
            }

            // CALL expression
            Token::Call => {
                let call = self.parse_call_expression()?;
                let span = start.to(&self.prev_span());
                Ok(Expr { kind: ExprKind::Call(call), ty: None, span })
            }

            // Inline IF expression
            Token::If => {
                let expr = self.parse_if_expr()?;
                let span = start.to(&self.prev_span());
                Ok(Expr { kind: ExprKind::IfExpr {
                    condition:  expr.condition,
                    then_value: expr.then_value,
                    else_value: expr.else_value,
                }, ty: None, span })
            }

            // Grouped expression
            Token::LParen => {
                self.advance();
                let inner = self.parse_expression()?;
                self.expect_token(Token::RParen)?;
                let span = start.to(&self.prev_span());
                // Wrap in a passthrough — span updated to include parens
                Ok(Expr { span, ..inner })
            }

            // Identifier
            Token::Ident(_) => {
                let ident = self.expect_ident()?;
                let span  = start.to(&self.prev_span());
                Ok(Expr { kind: ExprKind::Ident(ident), ty: None, span })
            }

            _ => Err(ParseError::new(
                format!("expected expression, found {}", self.current_token().describe()),
                start,
            )),
        }
    }

    // if_expr = "IF" expression "{" expression "}" "ELSE" "{" expression "}"

    fn parse_if_expr(&mut self) -> Result<IfExprParts, ParseError> {
        self.expect_token(Token::If)?;
        let condition = self.parse_expression()?;
        self.expect_token(Token::LBrace)?;
        let then_value = self.parse_expression()?;
        self.expect_token(Token::RBrace)?;
        if !self.consume_if(Token::Else) {
            return Err(ParseError::with_suggestion(
                "inline IF expression requires an ELSE branch",
                self.current_span(),
                "Add an ELSE branch: IF condition { value_a } ELSE { value_b }",
            ));
        }
        self.expect_token(Token::LBrace)?;
        let else_value = self.parse_expression()?;
        self.expect_token(Token::RBrace)?;
        Ok(IfExprParts {
            condition:  Box::new(condition),
            then_value: Box::new(then_value),
            else_value: Box::new(else_value),
        })
    }

    // ── Literal parsing ──────────────────────────────────────────

    fn parse_literal(&mut self) -> Result<Literal, ParseError> {
        let span = self.current_span();
        match self.current_token() {
            Token::IntLit(_) => {
                if let Token::IntLit(n) = self.advance().node { Ok(Literal::Int(n)) }
                else { unreachable!() }
            }
            Token::HexLit(_) => {
                if let Token::HexLit(n) = self.advance().node { Ok(Literal::Hex(n)) }
                else { unreachable!() }
            }
            Token::DecimalLit(_) => {
                if let Token::DecimalLit(f) = self.advance().node { Ok(Literal::Decimal(f)) }
                else { unreachable!() }
            }
            Token::StringLit(_) => {
                if let Token::StringLit(s) = self.advance().node { Ok(Literal::Str(s)) }
                else { unreachable!() }
            }
            Token::True  => { self.advance(); Ok(Literal::Bool(true))  }
            Token::False => { self.advance(); Ok(Literal::Bool(false)) }
            Token::High  => { self.advance(); Ok(Literal::High)        }
            Token::Low   => { self.advance(); Ok(Literal::Low)         }
            _ => Err(ParseError::new(
                format!("expected literal value, found {}", self.current_token().describe()),
                span,
            )),
        }
    }

    // ── Primitive helpers ────────────────────────────────────────

    /// Return the current token without consuming.
    fn current_token(&self) -> &Token {
        self.tokens.get(self.pos)
            .map(|st| &st.node)
            .unwrap_or(&Token::Eof)
    }

    /// Return the span of the current token.
    fn current_span(&self) -> Span {
        self.tokens.get(self.pos)
            .map(|st| st.span.clone())
            .unwrap_or_else(|| Span::synthetic(self.file.clone()))
    }

    /// Return the span of the most recently consumed token.
    fn prev_span(&self) -> Span {
        if self.pos == 0 { return Span::synthetic(self.file.clone()); }
        self.tokens.get(self.pos - 1)
            .map(|st| st.span.clone())
            .unwrap_or_else(|| Span::synthetic(self.file.clone()))
    }

    /// Peek at the token one position ahead.
    fn peek_token(&self) -> &Token {
        self.tokens.get(self.pos + 1)
            .map(|st| &st.node)
            .unwrap_or(&Token::Eof)
    }

    fn is_at_end(&self) -> bool {
        matches!(self.current_token(), Token::Eof)
    }

    /// Advance past the current token, returning it.
    fn advance(&mut self) -> SpannedToken {
        let t = self.tokens[self.pos].clone();
        if self.pos < self.tokens.len() - 1 { self.pos += 1; }
        t
    }

    /// Return true if the current token matches `expected` (no consume).
    fn check(&self, expected: Token) -> bool {
        std::mem::discriminant(self.current_token()) == std::mem::discriminant(&expected)
    }

    /// Return true if the current token is any Ident token.
    fn check_ident(&self) -> bool {
        matches!(self.current_token(), Token::Ident(_))
    }

    /// Return true if the *next* (peek) token matches `expected`.
    fn peek_is(&self, expected: Token) -> bool {
        std::mem::discriminant(self.peek_token()) == std::mem::discriminant(&expected)
    }

    /// Consume and return the current token if it matches `expected`.
    fn consume_if(&mut self, expected: Token) -> bool {
        if self.check(expected) { self.advance(); true } else { false }
    }

    /// Consume the current token, returning its span.
    /// Does NOT check the token type — use expect_token for that.
    fn expect(&mut self, _expected: Token) -> Span {
        self.advance().span
    }

    /// Consume the current token if it matches `expected`, returning
    /// its span; otherwise record an error and return a synthetic span.
    fn expect_token(&mut self, expected: Token) -> Result<Span, ParseError> {
        if self.check(expected.clone()) {
            Ok(self.advance().span)
        } else {
            Err(ParseError::new(
                format!("expected {}, found {}",
                        expected.describe(),
                        self.current_token().describe()),
                self.current_span(),
            ))
        }
    }

    /// Consume and return the current token as an Ident.
    fn expect_ident(&mut self) -> Result<Ident, ParseError> {
        match self.current_token() {
            Token::Ident(_) => {
                if let Token::Ident(i) = self.advance().node { Ok(i) }
                else { unreachable!() }
            }
            _ => Err(ParseError::new(
                format!("expected identifier, found {}", self.current_token().describe()),
                self.current_span(),
            )),
        }
    }

    /// Consume the current token as an integer literal.
    fn expect_integer(&mut self) -> Result<i64, ParseError> {
        match self.current_token() {
            Token::IntLit(_) => {
                if let Token::IntLit(n) = self.advance().node { Ok(n) }
                else { unreachable!() }
            }
            _ => Err(ParseError::new(
                format!("expected integer, found {}", self.current_token().describe()),
                self.current_span(),
            )),
        }
    }

    // ── Lookahead helpers ────────────────────────────────────────

    /// Returns true if the current identifier token is the name of
    /// a known user-defined function — used to produce the
    /// "did you mean CALL?" error.
    /// Always returns false at parse time (the symbol table isn't
    /// built yet); overridden to true in a two-pass setup.
    fn is_known_function_name_lookahead(&self) -> bool {
        // At parse time we cannot know — return false.
        // The semantic pass handles this case with its symbol table.
        false
    }

    // ── try_parse ────────────────────────────────────────────────

    /// Attempt to parse something. On error, record it and return None.
    /// The parser position is left at the error site.
    fn try_parse<T, F: FnOnce(&mut Self) -> Result<T, ParseError>>(
        &mut self,
        f: F,
    ) -> Option<T> {
        let saved_pos = self.pos;
        match f(self) {
            Ok(t)  => Some(t),
            Err(e) => {
                self.errors.push(e);
                self.pos = saved_pos;
                None
            }
        }
    }

    // ── parse_many ───────────────────────────────────────────────

    /// Parse zero or more items that each start with `section_token`.
    fn parse_many<T, F>(&mut self, section_token: Token, mut f: F) -> Vec<T>
    where
        F: FnMut(&mut Self) -> Result<T, ParseError>,
    {
        let mut items = Vec::new();
        while self.check(section_token.clone()) {
            match f(self) {
                Ok(item) => items.push(item),
                Err(e)   => {
                    self.errors.push(e);
                    self.synchronise_to_section_boundary();
                }
            }
        }
        items
    }

    // ── Error recovery ───────────────────────────────────────────

    /// Advance past the next occurrence of `token`.
    fn synchronise_past(&mut self, token: Token) {
        while !self.is_at_end() {
            if self.check(token.clone()) {
                self.advance();
                return;
            }
            self.advance();
        }
    }

    /// Advance to the next token that can start a statement.
    fn synchronise_to_next_statement(&mut self) {
        while !self.is_at_end() {
            match self.current_token() {
                Token::Set    | Token::Turn   | Token::Toggle  |
                Token::Print  | Token::Call   | Token::Delay   |
                Token::If     | Token::For    | Token::Break   |
                Token::Continue | Token::Declare | Token::Return |
                Token::RBrace => return,
                _ => { self.advance(); }
            }
        }
    }

    /// Advance to the next top-level section keyword or EOF.
    fn synchronise_to_section_boundary(&mut self) {
        while !self.is_at_end() {
            match self.current_token() {
                Token::Config   | Token::Define   | Token::Create |
                Token::Declare  | Token::Function | Token::Run    => return,
                _ => { self.advance(); }
            }
        }
    }
}

// ----------------------------------------------------------------
// IfExprParts — internal helper to return inline IF branches
// ----------------------------------------------------------------

struct IfExprParts {
    condition:  Box<Expr>,
    then_value: Box<Expr>,
    else_value: Box<Expr>,
}

// ----------------------------------------------------------------
// ConfigKey::from_str
// ----------------------------------------------------------------

impl ConfigKey {
    fn from_str(s: &str) -> Option<ConfigKey> {
        match s {
            "target"          => Some(ConfigKey::Target),
            "clock_speed"     => Some(ConfigKey::ClockSpeed),
            "serial"          => Some(ConfigKey::Serial),
            "default_pull_up" => Some(ConfigKey::DefaultPullUp),
            "debounce_ms"     => Some(ConfigKey::DebounceMs),
            "optimize"        => Some(ConfigKey::Optimize),
            "debug"           => Some(ConfigKey::Debug),
            _                 => None,
        }
    }
}

// ----------------------------------------------------------------
// Public entry point
// ----------------------------------------------------------------

pub fn parse(tokens: Vec<SpannedToken>, filename: impl Into<String>) -> ParseResult {
    Parser::new(tokens, filename).parse()
}

// ----------------------------------------------------------------
// Tests
// ----------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::lexer::lex;

    fn parse_src(src: &str) -> ParseResult {
        let lex_result = lex(src, "test.fe");
        parse(lex_result.tokens, "test.fe")
    }

    fn ok(src: &str) -> Program {
        let result = parse_src(src);
        assert!(
            result.errors.is_empty(),
            "unexpected parse errors:\n{}",
            result.errors.iter().map(|e| e.to_string()).collect::<Vec<_>>().join("\n")
        );
        result.program.unwrap()
    }

    fn has_error(src: &str, fragment: &str) {
        let result = parse_src(src);
        let found  = result.errors.iter().any(|e| e.message.contains(fragment));
        assert!(
            found,
            "expected error containing '{}', got:\n{}",
            fragment,
            result.errors.iter().map(|e| e.to_string()).collect::<Vec<_>>().join("\n")
        );
    }

    // ── CONFIG ───────────────────────────────────────────────────

    #[test]
    fn parse_config_section() {
        let p = ok(r#"
            CONFIG {
                TARGET = "microbit_v2"
                DEBUG = TRUE
            }
            RUN {}
        "#);
        let cfg = p.config.unwrap();
        assert_eq!(cfg.items.len(), 2);
        assert_eq!(cfg.items[0].key, ConfigKey::Target);
        assert_eq!(cfg.items[1].key, ConfigKey::Debug);
    }

    // ── DEFINE ───────────────────────────────────────────────────

    #[test]
    fn parse_simple_define() {
        let p = ok("DEFINE Button AS INPUT\nRUN {}");
        assert_eq!(p.defines.len(), 1);
        assert_eq!(p.defines[0].name.key, "button");
        assert!(matches!(p.defines[0].spec, DeviceSpec::Simple(_)));
    }

    #[test]
    fn parse_composite_define() {
        let p = ok("DEFINE Led AS { OUTPUT, PWM BRIGHTNESS }\nRUN {}");
        assert_eq!(p.defines.len(), 1);
        match &p.defines[0].spec {
            DeviceSpec::Composite(specs) => {
                assert_eq!(specs.len(), 2);
                assert_eq!(specs[0].interface, InterfaceType::Output);
                assert_eq!(specs[1].interface, InterfaceType::Pwm);
                assert_eq!(specs[1].qualifier,  Some(Qualifier::Brightness));
            }
            _ => panic!("expected composite"),
        }
    }

    // ── CREATE ───────────────────────────────────────────────────

    #[test]
    fn parse_create_single_pin() {
        let p = ok("DEFINE Button AS INPUT\nCREATE Button mode_btn ON PIN 14\nRUN {}");
        assert_eq!(p.creates.len(), 1);
        let c = &p.creates[0];
        assert_eq!(c.device_type.key,   "button");
        assert_eq!(c.instance_name.key, "mode_btn");
        assert!(matches!(c.pins, PinSpec::Single { pin: 14, .. }));
    }

    #[test]
    fn parse_create_multi_pin_named() {
        let src = r#"
            DEFINE Led AS { OUTPUT, PWM BRIGHTNESS }
            CREATE Led status ON { output: PIN 3, brightness: PIN 4 }
                INIT { output: LOW, brightness: 0.0 }
            RUN {}
        "#;
        let p = ok(src);
        let c = &p.creates[0];
        assert!(matches!(c.pins, PinSpec::Multi(_)));
        assert!(c.init.is_some());
    }

    #[test]
    fn parse_create_with_pull() {
        let p = ok("DEFINE Button AS INPUT\nCREATE Button stop ON PIN 15 PULL DOWN\nRUN {}");
        let c = &p.creates[0];
        assert_eq!(c.pull, Some(PullConfig::Down));
    }

    // ── DECLARE ──────────────────────────────────────────────────

    #[test]
    fn parse_variable_declaration() {
        let p = ok("DECLARE Integer counter INIT 0\nRUN {}");
        assert_eq!(p.declares.len(), 1);
        match &p.declares[0].kind {
            DeclareKind::Variable(v) => {
                assert_eq!(v.name.key,  "counter");
                assert_eq!(v.ty,        DataType::Integer);
                assert!(v.init.is_some());
            }
            _ => panic!("expected variable"),
        }
    }

    #[test]
    fn parse_constant_declaration() {
        let p = ok("DECLARE CONSTANT Integer MAX = 100\nRUN {}");
        match &p.declares[0].kind {
            DeclareKind::Constant(c) => {
                assert_eq!(c.name.key, "max");
                assert_eq!(c.ty,       DataType::Integer);
            }
            _ => panic!("expected constant"),
        }
    }

    #[test]
    fn parse_array_declaration() {
        let p = ok("DECLARE Integer[5] readings INIT [0, 0, 0, 0, 0]\nRUN {}");
        match &p.declares[0].kind {
            DeclareKind::Variable(v) => {
                assert_eq!(v.array_size, Some(5));
                assert!(matches!(v.init, Some(InitExpr::Array(_))));
            }
            _ => panic!("expected variable"),
        }
    }

    // ── FUNCTION ─────────────────────────────────────────────────

    #[test]
    fn parse_function_no_params() {
        let p = ok("FUNCTION heartbeat { PRINT \"tick\" }\nRUN {}");
        assert_eq!(p.functions.len(), 1);
        assert_eq!(p.functions[0].name.key, "heartbeat");
        assert!(p.functions[0].params.is_empty());
    }

    #[test]
    fn parse_function_data_params() {
        let p = ok("FUNCTION add Integer: a, Integer: b { RETURN a + b }\nRUN {}");
        let f = &p.functions[0];
        assert_eq!(f.params.len(), 2);
        assert!(matches!(f.params[0], Param::Data { .. }));
    }

    #[test]
    fn parse_function_device_param_give() {
        let src = "DEFINE Led AS OUTPUT\nFUNCTION blink GIVE Led: led { TURN led HIGH }\nRUN {}";
        let p   = ok(src);
        let f   = &p.functions[0];
        assert_eq!(f.params.len(), 1);
        match &f.params[0] {
            Param::Device { ownership, .. } => assert_eq!(*ownership, Ownership::Give),
            _ => panic!("expected device param"),
        }
    }

    #[test]
    fn parse_function_mixed_params() {
        let src = r#"
            DEFINE Led AS OUTPUT
            FUNCTION run_blink GIVE Led: led, Integer: count { RETURN count }
            RUN {}
        "#;
        let p = ok(src);
        let f = &p.functions[0];
        assert_eq!(f.params.len(), 2);
        assert!(matches!(f.params[0], Param::Device { .. }));
        assert!(matches!(f.params[1], Param::Data { .. }));
    }

    // ── RUN / LOOP / EVERY ───────────────────────────────────────

    #[test]
    fn parse_run_with_loop() {
        let p = ok("RUN { LOOP { DELAY 500ms } }");
        assert_eq!(p.run.items.len(), 1);
        assert!(matches!(p.run.items[0], RunItem::Loop(_)));
    }

    #[test]
    fn parse_run_with_every() {
        let p = ok("RUN { EVERY 1000ms { PRINT \"tick\" } LOOP {} }");
        assert!(matches!(p.run.items[0], RunItem::Every(_)));
        assert!(matches!(p.run.items[1], RunItem::Loop(_)));
    }

    #[test]
    fn every_inside_loop_is_error() {
        has_error(
            "RUN { LOOP { EVERY 1000ms { PRINT \"x\" } } }",
            "not valid inside a LOOP",
        );
    }

    #[test]
    fn every_inside_top_if_is_allowed() {
        let p = ok(r#"
            DECLARE Boolean debug_mode INIT TRUE
            RUN {
                IF debug_mode IS TRUE {
                    EVERY 2000ms { PRINT "tick" }
                }
                LOOP {}
            }
        "#);
        assert!(matches!(p.run.items[0], RunItem::TopIf(_)));
    }

    // ── Statements ───────────────────────────────────────────────

    #[test]
    fn parse_turn_high() {
        let p = ok("RUN { LOOP { TURN led HIGH } }");
        let body = extract_loop_body(&p);
        assert!(matches!(body[0].kind, StmtKind::Turn(TurnStmt { state: PinState::High, .. })));
    }

    #[test]
    fn parse_toggle() {
        let p = ok("RUN { LOOP { TOGGLE status } }");
        let body = extract_loop_body(&p);
        assert!(matches!(body[0].kind, StmtKind::Toggle(_)));
    }

    #[test]
    fn parse_delay_ms() {
        let p = ok("RUN { LOOP { DELAY 500ms } }");
        let body = extract_loop_body(&p);
        match &body[0].kind {
            StmtKind::Delay(d) => {
                assert_eq!(d.duration.value, 500);
                assert_eq!(d.duration.unit,  TimeUnit::Milliseconds);
            }
            _ => panic!("expected delay"),
        }
    }

    #[test]
    fn parse_delay_s() {
        let p = ok("RUN { LOOP { DELAY 2s } }");
        let body = extract_loop_body(&p);
        match &body[0].kind {
            StmtKind::Delay(d) => assert_eq!(d.duration.unit, TimeUnit::Seconds),
            _ => panic!("expected delay"),
        }
    }

    #[test]
    fn parse_for_range() {
        let p = ok("RUN { LOOP { FOR i IN RANGE 0, 9 { PRINT i } } }");
        let body = extract_loop_body(&p);
        assert!(matches!(body[0].kind, StmtKind::For(ForStmt {
            iterator: ForIterator::Range { .. }, ..
        })));
    }

    #[test]
    fn parse_for_array() {
        let p = ok("RUN { LOOP { FOR x IN items { PRINT x } } }");
        let body = extract_loop_body(&p);
        assert!(matches!(body[0].kind, StmtKind::For(ForStmt {
            iterator: ForIterator::Array(_), ..
        })));
    }

    #[test]
    fn break_outside_loop_is_error() {
        has_error("RUN { BREAK }", "no enclosing LOOP or FOR block");
    }

    #[test]
    fn continue_outside_loop_is_error() {
        has_error("RUN { CONTINUE }", "no enclosing LOOP or FOR block");
    }

    #[test]
    fn break_inside_loop_is_ok() {
        let p = ok("RUN { LOOP { BREAK } }");
        let body = extract_loop_body(&p);
        assert!(matches!(body[0].kind, StmtKind::Break));
    }

    // ── Expressions ──────────────────────────────────────────────

    #[test]
    fn parse_binary_expression() {
        let p = ok("RUN { LOOP { PRINT a + b * 2 } }");
        let body = extract_loop_body(&p);
        // Should parse as PRINT (a + (b * 2)) — multiplication binds tighter
        match &body[0].kind {
            StmtKind::Print(s) => assert!(matches!(s.value.kind, ExprKind::BinOp { op: BinOp::Add, .. })),
            _ => panic!("expected print"),
        }
    }

    #[test]
    fn parse_is_operator() {
        let p = ok("RUN { LOOP { IF mode_btn IS LOW { PRINT \"pressed\" } } }");
        let body = extract_loop_body(&p);
        match &body[0].kind {
            StmtKind::If(s) => {
                assert!(matches!(s.condition.kind, ExprKind::Is { negated: false, .. }));
            }
            _ => panic!("expected if"),
        }
    }

    #[test]
    fn parse_is_not_operator() {
        let p = ok("RUN { LOOP { IF last_button IS NOT TRUE { PRINT \"changed\" } } }");
        let body = extract_loop_body(&p);
        match &body[0].kind {
            StmtKind::If(s) => {
                assert!(matches!(s.condition.kind, ExprKind::Is { negated: true, .. }));
            }
            _ => panic!("expected if"),
        }
    }

    #[test]
    fn parse_inline_if_expression() {
        let p = ok(r#"RUN { LOOP { DECLARE String s = CALL to_string IF flag IS TRUE { "yes" } ELSE { "no" } } }"#);
        let body = extract_loop_body(&p);
        assert!(matches!(body[0].kind, StmtKind::InlineDeclare(_)));
    }

    #[test]
    fn inline_if_missing_else_is_error() {
        has_error(
            r#"RUN { LOOP { PRINT IF flag IS TRUE { "yes" } } }"#,
            "inline IF expression requires an ELSE branch",
        );
    }

    // ── Call ─────────────────────────────────────────────────────

    #[test]
    fn parse_void_call() {
        let p = ok("RUN { LOOP { CALL log_message \"hello\" } }");
        let body = extract_loop_body(&p);
        assert!(matches!(body[0].kind, StmtKind::VoidCall(_)));
    }

    #[test]
    fn parse_call_with_device_give() {
        let p = ok("RUN { LOOP { CALL run_blink GIVE status_led } }");
        let body = extract_loop_body(&p);
        match &body[0].kind {
            StmtKind::VoidCall(c) => {
                assert_eq!(c.args.len(), 1);
                assert!(matches!(c.args[0].kind, CallArgKind::Device { ownership: Ownership::Give, .. }));
            }
            _ => panic!("expected void call"),
        }
    }

    #[test]
    fn parse_declare_from_call() {
        let p = ok("RUN { LOOP { DECLARE Integer sum = CALL add 5, 3 } }");
        let body = extract_loop_body(&p);
        assert!(matches!(body[0].kind, StmtKind::InlineDeclare(_)));
    }

    // ── ELSE IF detection ────────────────────────────────────────

    #[test]
    fn else_if_produces_helpful_error() {
        has_error(
            "RUN { LOOP { IF a == 1 { PRINT \"a\" } ELSE IF b == 2 { PRINT \"b\" } } }",
            "ELSE IF is not supported",
        );
    }

    // ── Complete mini-program ────────────────────────────────────

    #[test]
    fn complete_mini_program_parses_cleanly() {
        let src = r#"
DEFINE
    Button AS INPUT,
    Led AS { OUTPUT, PWM BRIGHTNESS }

CONFIG {
    TARGET = "microbit_v2"
    DEBUG = TRUE
}

CREATE
    Button mode_btn ON PIN 14,
    Led status ON { output: PIN 3, brightness: PIN 4 }
        INIT { output: LOW, brightness: 0.0 }

DECLARE
    Boolean auto_mode INIT TRUE,
    Percentage brightness INIT 50.0

FUNCTION pulse BORROW Led: led {
    TURN led HIGH
    DELAY 100ms
    TURN led LOW
}

RUN {
    EVERY 5000ms {
        PRINT "heartbeat"
    }
    LOOP {
        IF mode_btn IS LOW {
            auto_mode = NOT auto_mode
        }
        CALL pulse BORROW status
        DELAY 100ms
    }
}
        "#;

        let p = ok(src);
        assert!(p.config.is_some());
        assert_eq!(p.defines.len(),    2);
        assert_eq!(p.creates.len(),    2);
        assert_eq!(p.declares.len(),   2);
        assert_eq!(p.functions.len(),  1);
        assert_eq!(p.run.items.len(),  2); // EVERY + LOOP
    }

    // ── Helpers ──────────────────────────────────────────────────

    fn extract_loop_body(p: &Program) -> &Vec<Statement> {
        for item in &p.run.items {
            if let RunItem::Loop(l) = item { return &l.body; }
        }
        panic!("no LOOP found in run section");
    }
}