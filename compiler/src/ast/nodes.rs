// ferrum/compiler/src/ast/nodes.rs
//
// Complete AST node definitions for the Ferrum compiler.
//
// IMPORTS
//   Span and Ident come from crate::lexer::token — they are defined
//   once there and used everywhere. No redefinition here.
//
// ORGANISATION
//   Sections mirror the language's program structure top to bottom:
//     Foundation types  (Type, Ownership, DataType, etc.)
//     Program root
//     CONFIG nodes
//     DEFINE nodes
//     CREATE nodes
//     DECLARE nodes
//     FUNCTION nodes
//     RUN / control flow nodes
//     Statement nodes
//     Expression nodes
//     Literal nodes
//
// TYPE ANNOTATION SLOTS
//   Expression nodes carry  `ty: Option<Type>`.
//   Function nodes carry    `return_type: Option<Type>`.
//   Both start as None after parsing. The semantic pass fills them.
//   Codegen reads them; it never runs on None slots.

use crate::lexer::token::{Ident, Span};

// ================================================================
// FOUNDATION TYPES
// ================================================================

// ----------------------------------------------------------------
// Type — the resolved type of any value-producing node
// ----------------------------------------------------------------

/// Every type that a value-producing expression can resolve to.
/// The semantic pass fills `Option<Type>` slots on AST nodes.
#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    // ── Primitive data types ─────────────────────────────────────
    Integer,
    Decimal,
    /// Decimal constrained to 0.0–100.0. Stored as f64 internally.
    Percentage,
    Boolean,
    String,
    /// Integer constrained to 0–255. Stored as u8 internally.
    Byte,

    // ── Array type ───────────────────────────────────────────────
    /// Array of a given element type and fixed compile-time size.
    Array(Box<Type>, usize),

    // ── Hardware value types ─────────────────────────────────────
    /// HIGH or LOW — returned by READ on an INPUT interface.
    PinState,
    /// Raw 0–1023 integer — returned by READ on ANALOG_INPUT.
    AnalogRaw,

    // ── Device type ──────────────────────────────────────────────
    /// A named device type, referencing a DEFINE entry by name.
    /// The Ident key is used for lookup; original for display.
    Device(Ident),

    // ── Void ─────────────────────────────────────────────────────
    /// Functions with no RETURN statement.
    Void,
}

// ----------------------------------------------------------------
// Ownership — device parameter passing mode
// ----------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum Ownership {
    /// GIVE — full ownership transfer. Caller loses the device.
    Give,
    /// LEND — read-only borrow. Caller keeps ownership.
    Lend,
    /// BORROW — mutable borrow. Caller keeps ownership, device may change.
    Borrow,
}

// ----------------------------------------------------------------
// DataType — the type annotation written by the author in source
// ----------------------------------------------------------------

/// Type names as written in DECLARE and function parameters.
/// Distinct from `Type` which is the resolved semantic type.
/// The semantic pass maps DataType → Type during type checking.
#[derive(Debug, Clone, PartialEq)]
pub enum DataType {
    Integer,
    Decimal,
    Percentage,
    Boolean,
    String,
    Byte,
}

impl DataType {
    /// Convert to the corresponding resolved Type.
    pub fn to_type(&self) -> Type {
        match self {
            DataType::Integer    => Type::Integer,
            DataType::Decimal    => Type::Decimal,
            DataType::Percentage => Type::Percentage,
            DataType::Boolean    => Type::Boolean,
            DataType::String     => Type::String,
            DataType::Byte       => Type::Byte,
        }
    }

    /// Human-readable name for error messages.
    pub fn name(&self) -> &'static str {
        match self {
            DataType::Integer    => "Integer",
            DataType::Decimal    => "Decimal",
            DataType::Percentage => "Percentage",
            DataType::Boolean    => "Boolean",
            DataType::String     => "String",
            DataType::Byte       => "Byte",
        }
    }
}

// ----------------------------------------------------------------
// InterfaceType — hardware interface capability
// ----------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum InterfaceType {
    Input,
    Output,
    AnalogInput,
    Pwm,
    Display,
    Pulse,
}

impl InterfaceType {
    pub fn name(&self) -> &'static str {
        match self {
            InterfaceType::Input       => "INPUT",
            InterfaceType::Output      => "OUTPUT",
            InterfaceType::AnalogInput => "ANALOG_INPUT",
            InterfaceType::Pwm         => "PWM",
            InterfaceType::Display     => "DISPLAY",
            InterfaceType::Pulse       => "PULSE",
        }
    }

    /// True if this interface type is read-only (INIT not permitted).
    pub fn is_read_only(&self) -> bool {
        matches!(self,
            InterfaceType::Input |
            InterfaceType::AnalogInput |
            InterfaceType::Pulse
        )
    }

    /// True if PULL configuration is valid on this interface.
    pub fn supports_pull(&self) -> bool {
        matches!(self, InterfaceType::Input)
    }
}

// ----------------------------------------------------------------
// Qualifier — semantic annotation on an interface
// ----------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum Qualifier {
    // PWM qualifiers
    Brightness,
    Speed,
    Angle,
    Red,
    Green,
    Blue,
    // DISPLAY qualifiers
    Lcd,
    Oled,
    // PULSE qualifiers
    Trigger,
    Echo,
    // OUTPUT qualifier (composite devices only)
    Enable,
}

impl Qualifier {
    pub fn name(&self) -> &'static str {
        match self {
            Qualifier::Brightness => "BRIGHTNESS",
            Qualifier::Speed      => "SPEED",
            Qualifier::Angle      => "ANGLE",
            Qualifier::Red        => "RED",
            Qualifier::Green      => "GREEN",
            Qualifier::Blue       => "BLUE",
            Qualifier::Lcd        => "LCD",
            Qualifier::Oled       => "OLED",
            Qualifier::Trigger    => "TRIGGER",
            Qualifier::Echo       => "ECHO",
            Qualifier::Enable     => "ENABLE",
        }
    }

    /// Return the interface types this qualifier is valid on.
    pub fn valid_interfaces(&self) -> &'static [InterfaceType] {
        match self {
            Qualifier::Brightness |
            Qualifier::Speed      |
            Qualifier::Angle      |
            Qualifier::Red        |
            Qualifier::Green      |
            Qualifier::Blue       => &[InterfaceType::Pwm],
            Qualifier::Lcd        |
            Qualifier::Oled       => &[InterfaceType::Display],
            Qualifier::Trigger    |
            Qualifier::Echo       => &[InterfaceType::Pulse],
            Qualifier::Enable     => &[InterfaceType::Output],
        }
    }
}

// ----------------------------------------------------------------
// PinState — HIGH or LOW
// ----------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum PinState {
    High,
    Low,
}

// ----------------------------------------------------------------
// PullConfig — pull-up or pull-down resistor
// ----------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum PullConfig {
    Up,
    Down,
}

// ----------------------------------------------------------------
// TimeUnit — ms or s
// ----------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum TimeUnit {
    Milliseconds,
    Seconds,
}

// ----------------------------------------------------------------
// BinOp / UnaryOp — expression operators
// ----------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum BinOp {
    Add, Sub, Mul, Div,
    Gt, Lt, Gte, Lte,
    Eq, NotEq,
    And, Or,
}

impl BinOp {
    pub fn symbol(&self) -> &'static str {
        match self {
            BinOp::Add   => "+",  BinOp::Sub  => "-",
            BinOp::Mul   => "*",  BinOp::Div  => "/",
            BinOp::Gt    => ">",  BinOp::Lt   => "<",
            BinOp::Gte   => ">=", BinOp::Lte  => "<=",
            BinOp::Eq    => "==", BinOp::NotEq => "!=",
            BinOp::And   => "AND", BinOp::Or  => "OR",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum UnaryOp {
    Not,
    Neg,
}

// ================================================================
// PROGRAM ROOT
// ================================================================

/// The root node of every parsed Ferrum program.
/// Produced by the parser; consumed by the semantic pass and codegen.
#[derive(Debug, Clone)]
pub struct Program {
    pub config:    Option<ConfigSection>,
    pub defines:   Vec<DefineItem>,
    pub creates:   Vec<CreateItem>,
    pub declares:  Vec<DeclareItem>,
    pub functions: Vec<FunctionDef>,
    pub run:       RunSection,
    pub span:      Span,
}

// ================================================================
// CONFIG NODES
// ================================================================

#[derive(Debug, Clone)]
pub struct ConfigSection {
    pub items: Vec<ConfigItem>,
    pub span:  Span,
}

#[derive(Debug, Clone)]
pub struct ConfigItem {
    pub key:   ConfigKey,
    pub value: ConfigValue,
    pub span:  Span,
}

/// Strongly typed config keys.
/// Unknown keys are preserved for a helpful semantic-pass error.
#[derive(Debug, Clone, PartialEq)]
pub enum ConfigKey {
    Target,
    ClockSpeed,
    Serial,
    DefaultPullUp,
    DebounceMs,
    Optimize,
    Debug,
    /// An unrecognised key — reported as an error in the semantic pass.
    /// The original spelling is kept for the error message.
    Unknown(String),
}

impl ConfigKey {
    pub fn name(&self) -> &str {
        match self {
            ConfigKey::Target        => "TARGET",
            ConfigKey::ClockSpeed    => "CLOCK_SPEED",
            ConfigKey::Serial        => "SERIAL",
            ConfigKey::DefaultPullUp => "DEFAULT_PULL_UP",
            ConfigKey::DebounceMs    => "DEBOUNCE_MS",
            ConfigKey::Optimize      => "OPTIMIZE",
            ConfigKey::Debug         => "DEBUG",
            ConfigKey::Unknown(s)    => s.as_str(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConfigValue {
    Str(String),
    Int(i64),
    /// Clock speed in MHz, e.g. 64MHZ → Clock(64).
    Clock(u32),
    Bool(bool),
}

// ================================================================
// DEFINE NODES
// ================================================================

/// One entry from a DEFINE section.
#[derive(Debug, Clone)]
pub struct DefineItem {
    /// The device type name, e.g. `Button`, `Led`, `WaterPump`.
    pub name: Ident,
    pub spec: DeviceSpec,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum DeviceSpec {
    /// Single interface: DEFINE Button AS INPUT
    Simple(InterfaceSpec),
    /// Multiple interfaces: DEFINE Led AS { OUTPUT, PWM BRIGHTNESS }
    Composite(Vec<InterfaceSpec>),
}

/// One interface within a device spec, with an optional qualifier.
#[derive(Debug, Clone)]
pub struct InterfaceSpec {
    pub interface: InterfaceType,
    pub qualifier: Option<Qualifier>,
    pub span:      Span,
}

// ================================================================
// CREATE NODES
// ================================================================

/// One entry from a CREATE section — instantiates a device on pins.
#[derive(Debug, Clone)]
pub struct CreateItem {
    /// References a DefineItem by name.
    pub device_type:   Ident,
    /// The instance name used in RUN code.
    pub instance_name: Ident,
    pub pins:          PinSpec,
    pub pull:          Option<PullConfig>,
    pub init:          Option<InitBlock>,
    pub span:          Span,
}

#[derive(Debug, Clone)]
pub enum PinSpec {
    /// ON PIN 14
    Single { pin: u32, span: Span },
    /// ON { PIN 3, PIN 4 } or ON { output: PIN 3, brightness: PIN 4 }
    Multi(Vec<PinAssignment>),
}

/// One pin assignment within a multi-pin spec.
///
/// Three valid forms:
///   Positional:  PIN 3             → interface_name: None, disambiguator: None
///   Named:       output: PIN 3     → interface_name: Some("output"), disambiguator: None
///   Mixed:       PIN 3 OUTPUT      → interface_name: None, disambiguator: Some("OUTPUT")
#[derive(Debug, Clone)]
pub struct PinAssignment {
    pub interface_name: Option<Ident>,
    pub pin_number:     u32,
    pub disambiguator:  Option<Ident>,
    pub span:           Span,
}

/// INIT block — initial hardware state when the program starts.
#[derive(Debug, Clone)]
pub struct InitBlock {
    pub values: Vec<InitEntry>,
    pub span:   Span,
}

/// One entry within an INIT block.
#[derive(Debug, Clone)]
pub struct InitEntry {
    /// Named form: output: LOW — interface_name is Some("output").
    /// Positional form: LOW    — interface_name is None.
    pub interface_name: Option<Ident>,
    pub value:          InitValue,
    pub span:           Span,
}

/// Valid values inside an INIT block, by interface type:
///   OUTPUT  → High | Low
///   PWM     → Decimal (0.0–1.0)
///   DISPLAY → Str | Int
#[derive(Debug, Clone, PartialEq)]
pub enum InitValue {
    High,
    Low,
    Decimal(f64),
    Str(String),
    Int(i64),
}

// ================================================================
// DECLARE NODES
// ================================================================

/// One entry from a DECLARE section.
#[derive(Debug, Clone)]
pub struct DeclareItem {
    pub kind: DeclareKind,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum DeclareKind {
    Variable(VariableDecl),
    Constant(ConstantDecl),
}

#[derive(Debug, Clone)]
pub struct VariableDecl {
    pub ty:         DataType,
    /// Some(n) if declared as an array: DECLARE Integer[5] readings
    pub array_size: Option<usize>,
    pub name:       Ident,
    pub init:       Option<InitExpr>,
    pub span:       Span,
}

#[derive(Debug, Clone)]
pub struct ConstantDecl {
    pub ty:    DataType,
    pub name:  Ident,
    pub value: Literal,
    pub span:  Span,
}

/// Initial value expression for a DECLARE ... INIT ...
#[derive(Debug, Clone)]
pub enum InitExpr {
    Single(Literal),
    Array(Vec<Literal>),
}

// ================================================================
// FUNCTION NODES
// ================================================================

#[derive(Debug, Clone)]
pub struct FunctionDef {
    pub name:        Ident,
    pub params:      Vec<Param>,
    pub body:        Vec<Statement>,
    pub ret:         Option<ReturnStmt>,
    /// Filled by the semantic pass — None until type checking runs.
    pub return_type: Option<Type>,
    pub span:        Span,
}

#[derive(Debug, Clone)]
pub enum Param {
    /// Ordinary value parameter: Integer: count
    Data {
        ty:   DataType,
        name: Ident,
        span: Span,
    },
    /// Device parameter with ownership: GIVE Led: led
    Device {
        ownership: Ownership,
        device_ty: Ident,
        name:      Ident,
        span:      Span,
    },
}

#[derive(Debug, Clone)]
pub struct ReturnStmt {
    /// None → void return (bare RETURN with no expression).
    pub value: Option<Expr>,
    pub span:  Span,
}

// ================================================================
// RUN / CONTROL FLOW NODES
// ================================================================

#[derive(Debug, Clone)]
pub struct RunSection {
    pub items: Vec<RunItem>,
    pub span:  Span,
}

/// Items at the RUN top level.
/// EVERY and TopIfBlock are only reachable here — not from Statement.
/// This structural distinction enforces the EVERY placement rule.
#[derive(Debug, Clone)]
pub enum RunItem {
    Every(EveryBlock),
    /// IF at the RUN top level — body is Vec<RunItem>, so it can
    /// contain EVERY. Distinct from statement-level IfStmt.
    TopIf(TopIfBlock),
    Loop(LoopBlock),
    Stmt(Statement),
}

/// EVERY block — scheduled periodic execution.
#[derive(Debug, Clone)]
pub struct EveryBlock {
    pub period: Duration,
    /// Statements only — EVERY may not appear inside here.
    pub body:   Vec<Statement>,
    pub span:   Span,
}

/// IF at the RUN top level.
/// Body is Vec<RunItem> — EVERY is legal inside here.
#[derive(Debug, Clone)]
pub struct TopIfBlock {
    pub condition:  Expr,
    pub then_items: Vec<RunItem>,
    pub else_items: Option<Vec<RunItem>>,
    pub span:       Span,
}

/// Infinite LOOP block.
#[derive(Debug, Clone)]
pub struct LoopBlock {
    /// Statements only — EVERY may not appear inside here.
    pub body: Vec<Statement>,
    pub span: Span,
}

/// Time duration: value + unit, e.g. 500ms, 2s.
#[derive(Debug, Clone)]
pub struct Duration {
    pub value: u32,
    pub unit:  TimeUnit,
    pub span:  Span,
}

impl Duration {
    /// Convert to milliseconds as u64 for comparison.
    pub fn as_millis(&self) -> u64 {
        match self.unit {
            TimeUnit::Milliseconds => self.value as u64,
            TimeUnit::Seconds      => self.value as u64 * 1000,
        }
    }
}

// ================================================================
// STATEMENT NODES
// ================================================================

/// A single executable statement.
#[derive(Debug, Clone)]
pub struct Statement {
    pub kind: StmtKind,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum StmtKind {
    Assignment(AssignStmt),
    Set(SetStmt),
    Turn(TurnStmt),
    Toggle(ToggleStmt),
    Print(PrintStmt),
    VoidCall(CallStmt),
    Delay(DelayStmt),
    /// Statement-level IF — body is Vec<Statement>, not Vec<RunItem>.
    /// EVERY is not permitted here (caught structurally).
    If(IfStmt),
    For(ForStmt),
    Break,
    Continue,
    InlineDeclare(InlineDeclareStmt),
}

// ── Individual statement structs ─────────────────────────────────

#[derive(Debug, Clone)]
pub struct AssignStmt {
    pub target: Ident,
    pub value:  Expr,
    pub span:   Span,
}

#[derive(Debug, Clone)]
pub struct SetStmt {
    pub device:    Ident,
    /// Qualifier required when the device has multiple numeric interfaces.
    pub qualifier: Option<Qualifier>,
    pub value:     Expr,
    pub span:      Span,
}

#[derive(Debug, Clone)]
pub struct TurnStmt {
    pub device:    Ident,
    /// ENABLE qualifier for composite devices: TURN pump ENABLE HIGH
    pub qualifier: Option<Qualifier>,
    pub state:     PinState,
    pub span:      Span,
}

#[derive(Debug, Clone)]
pub struct ToggleStmt {
    pub device: Ident,
    pub span:   Span,
}

#[derive(Debug, Clone)]
pub struct PrintStmt {
    pub value: Expr,
    pub span:  Span,
}

/// A CALL used as a standalone statement — return value discarded.
#[derive(Debug, Clone)]
pub struct CallStmt {
    pub function: Ident,
    pub args:     Vec<CallArg>,
    pub span:     Span,
}

#[derive(Debug, Clone)]
pub struct DelayStmt {
    pub duration: Duration,
    pub span:     Span,
}

/// Statement-level IF.
/// Body type is Vec<Statement> — EVERY structurally excluded.
#[derive(Debug, Clone)]
pub struct IfStmt {
    pub condition: Expr,
    pub then_body: Vec<Statement>,
    /// None → no ELSE clause.
    pub else_body: Option<ElseClause>,
    pub span:      Span,
}

/// The ELSE clause of a statement-level IF.
///
/// There is no ElseIf variant. The absence is structural enforcement
/// of semantic constraint 24 (no ELSE IF construct). The parser
/// detects `ELSE IF` tokens, reports a helpful error, and recovers
/// by treating the nested IF as a block statement.
#[derive(Debug, Clone)]
pub enum ElseClause {
    Block(Vec<Statement>),
}

#[derive(Debug, Clone)]
pub struct ForStmt {
    pub variable: Ident,
    pub iterator: ForIterator,
    pub body:     Vec<Statement>,
    pub span:     Span,
}

#[derive(Debug, Clone)]
pub enum ForIterator {
    /// FOR x IN array_variable
    Array(Expr),
    /// FOR i IN RANGE 0, 9
    Range { from: Expr, to: Expr },
}

/// DECLARE inside a block — block-scoped variable or captured call result.
#[derive(Debug, Clone)]
pub struct InlineDeclareStmt {
    pub ty:         DataType,
    pub array_size: Option<usize>,
    pub name:       Ident,
    pub init:       InlineDeclareInit,
    pub span:       Span,
}

#[derive(Debug, Clone)]
pub enum InlineDeclareInit {
    /// DECLARE Integer x INIT 5
    Value(InitExpr),
    /// DECLARE Integer sum = CALL add 5, 3
    Call(CallExpr),
}

// ================================================================
// CALL ARGUMENT NODES
// ================================================================

/// A single argument in a function call.
#[derive(Debug, Clone)]
pub struct CallArg {
    pub kind: CallArgKind,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum CallArgKind {
    /// Data argument: a plain expression value.
    Data(Expr),
    /// Device argument: GIVE led, LEND btn, BORROW pump
    Device {
        ownership: Ownership,
        name:      Ident,
    },
}

/// A CALL expression — used in expression position (return value expected).
/// Distinct from CallStmt which discards the return value.
#[derive(Debug, Clone)]
pub struct CallExpr {
    pub function: Ident,
    pub args:     Vec<CallArg>,
    pub span:     Span,
}

// ================================================================
// EXPRESSION NODES
// ================================================================

/// A value-producing expression.
///
/// `ty` starts as None after parsing. The semantic pass fills it.
/// Codegen reads it — it will always be Some by that stage.
#[derive(Debug, Clone)]
pub struct Expr {
    pub kind: ExprKind,
    /// Resolved type — None until the semantic pass runs.
    pub ty:   Option<Type>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum ExprKind {
    /// A literal value: 42, 3.14, "hello", TRUE, HIGH, …
    Literal(Literal),

    /// A named variable or device reference.
    Ident(Ident),

    /// READ device — returns HIGH/LOW for INPUT, Integer for ANALOG_INPUT.
    Read(Ident),

    /// READ_PERCENT device — returns Percentage. ANALOG_INPUT only.
    ReadPercent(Ident),

    /// A binary operation: a + b, x > 0, flag AND ready, …
    BinOp {
        op:    BinOp,
        left:  Box<Expr>,
        right: Box<Expr>,
    },

    /// A unary operation: NOT flag, -value.
    UnaryOp {
        op:      UnaryOp,
        operand: Box<Expr>,
    },

    /// IS / IS NOT — device state or boolean variable check.
    ///
    /// Kept as a distinct variant (not folded into BinOp) so the
    /// semantic pass can enforce IS/IS NOT applicability (constraint 22)
    /// by matching on this variant alone.
    ///
    /// Examples:
    ///   mode_btn IS LOW        → negated: false
    ///   last_button IS NOT TRUE → negated: true
    Is {
        target:  Box<Expr>,
        negated: bool,
        state:   Box<Expr>,
    },

    /// A function call in expression position: CALL add 5, 3
    Call(CallExpr),

    /// Inline IF expression: IF cond { a } ELSE { b }
    IfExpr {
        condition:  Box<Expr>,
        then_value: Box<Expr>,
        else_value: Box<Expr>,
    },
}

// ================================================================
// LITERAL NODES
// ================================================================

/// A compile-time literal value.
///
/// Note: Byte and Percentage are distinct variants even though they
/// hold numeric values. This means any literal range validation done
/// in the semantic pass can produce a correctly typed result directly,
/// without codegen needing to re-check constraints.
#[derive(Debug, Clone, PartialEq)]
pub enum Literal {
    /// Integer literal: 0, 42, -7
    Int(i64),
    /// Hexadecimal literal: 0xFF, 0x27 — stored as parsed u64.
    Hex(u64),
    /// Floating point literal: 3.14, 0.5, 100.0
    Decimal(f64),
    /// String literal: "hello" — content preserved verbatim.
    Str(String),
    /// Boolean literal: TRUE or FALSE.
    Bool(bool),
    /// Pin state literal: HIGH or LOW (used in INIT values and IS checks).
    High,
    Low,
}

impl Literal {
    /// Returns the name of this literal's natural type,
    /// used in type mismatch error messages.
    pub fn type_name(&self) -> &'static str {
        match self {
            Literal::Int(_)     => "Integer",
            Literal::Hex(_)     => "Byte (hex)",
            Literal::Decimal(_) => "Decimal",
            Literal::Str(_)     => "String",
            Literal::Bool(_)    => "Boolean",
            Literal::High |
            Literal::Low        => "PinState",
        }
    }
}

// ================================================================
// AST VISITOR TRAIT
// ================================================================

/// A trait for passes that walk the AST.
///
/// Each method has a default no-op implementation so implementors
/// only override the nodes they care about.
///
/// The semantic pass, ownership checker, device checker, and codegen
/// all implement this trait or use a similar recursive walk pattern.
pub trait AstVisitor {
    fn visit_program(&mut self, node: &Program) {
        self.walk_program(node);
    }
    fn visit_config_section(&mut self, _node: &ConfigSection) {}
    fn visit_define_item(&mut self, _node: &DefineItem)       {}
    fn visit_create_item(&mut self, _node: &CreateItem)       {}
    fn visit_declare_item(&mut self, _node: &DeclareItem)     {}
    fn visit_function_def(&mut self, node: &FunctionDef) {
        self.walk_function_def(node);
    }
    fn visit_run_section(&mut self, node: &RunSection) {
        self.walk_run_section(node);
    }
    fn visit_statement(&mut self, node: &Statement) {
        self.walk_statement(node);
    }
    fn visit_expr(&mut self, node: &Expr) {
        self.walk_expr(node);
    }

    // ── Default walkers — call child visitors ────────────────────

    fn walk_program(&mut self, node: &Program) {
        if let Some(cfg) = &node.config { self.visit_config_section(cfg); }
        for d in &node.defines   { self.visit_define_item(d);   }
        for c in &node.creates   { self.visit_create_item(c);   }
        for d in &node.declares  { self.visit_declare_item(d);  }
        for f in &node.functions { self.visit_function_def(f);  }
        self.visit_run_section(&node.run);
    }

    fn walk_function_def(&mut self, node: &FunctionDef) {
        for s in &node.body { self.visit_statement(s); }
        if let Some(ret) = &node.ret {
            if let Some(e) = &ret.value { self.visit_expr(e); }
        }
    }

    fn walk_run_section(&mut self, node: &RunSection) {
        for item in &node.items {
            match item {
                RunItem::Every(e) => {
                    for s in &e.body { self.visit_statement(s); }
                }
                RunItem::TopIf(t) => {
                    self.visit_expr(&t.condition);
                    for i in &t.then_items { self.walk_run_item(i); }
                    if let Some(els) = &t.else_items {
                        for i in els { self.walk_run_item(i); }
                    }
                }
                RunItem::Loop(l) => {
                    for s in &l.body { self.visit_statement(s); }
                }
                RunItem::Stmt(s) => self.visit_statement(s),
            }
        }
    }

    fn walk_run_item(&mut self, item: &RunItem) {
        match item {
            RunItem::Every(e) => {
                for s in &e.body { self.visit_statement(s); }
            }
            RunItem::TopIf(t) => {
                self.visit_expr(&t.condition);
                for i in &t.then_items { self.walk_run_item(i); }
                if let Some(els) = &t.else_items {
                    for i in els { self.walk_run_item(i); }
                }
            }
            RunItem::Loop(l) => {
                for s in &l.body { self.visit_statement(s); }
            }
            RunItem::Stmt(s) => self.visit_statement(s),
        }
    }

    fn walk_statement(&mut self, node: &Statement) {
        match &node.kind {
            StmtKind::Assignment(s) => self.visit_expr(&s.value),
            StmtKind::Set(s)        => self.visit_expr(&s.value),
            StmtKind::Print(s)      => self.visit_expr(&s.value),
            StmtKind::VoidCall(s)   => {
                for arg in &s.args {
                    if let CallArgKind::Data(e) = &arg.kind { self.visit_expr(e); }
                }
            }
            StmtKind::If(s) => {
                self.visit_expr(&s.condition);
                for st in &s.then_body { self.visit_statement(st); }
                if let Some(ElseClause::Block(stmts)) = &s.else_body {
                    for st in stmts { self.visit_statement(st); }
                }
            }
            StmtKind::For(s) => {
                match &s.iterator {
                    ForIterator::Array(e) => self.visit_expr(e),
                    ForIterator::Range { from, to } => {
                        self.visit_expr(from);
                        self.visit_expr(to);
                    }
                }
                for st in &s.body { self.visit_statement(st); }
            }
            StmtKind::InlineDeclare(s) => {
                match &s.init {
                    InlineDeclareInit::Value(_)    => {}
                    InlineDeclareInit::Call(call)  => {
                        for arg in &call.args {
                            if let CallArgKind::Data(e) = &arg.kind { self.visit_expr(e); }
                        }
                    }
                }
            }
            StmtKind::Turn(_)   |
            StmtKind::Toggle(_) |
            StmtKind::Delay(_)  |
            StmtKind::Break     |
            StmtKind::Continue  => {}
        }
    }

    fn walk_expr(&mut self, node: &Expr) {
        match &node.kind {
            ExprKind::BinOp { left, right, .. } => {
                self.visit_expr(left);
                self.visit_expr(right);
            }
            ExprKind::UnaryOp { operand, .. } => {
                self.visit_expr(operand);
            }
            ExprKind::Is { target, state, .. } => {
                self.visit_expr(target);
                self.visit_expr(state);
            }
            ExprKind::Call(call) => {
                for arg in &call.args {
                    if let CallArgKind::Data(e) = &arg.kind { self.visit_expr(e); }
                }
            }
            ExprKind::IfExpr { condition, then_value, else_value } => {
                self.visit_expr(condition);
                self.visit_expr(then_value);
                self.visit_expr(else_value);
            }
            ExprKind::Literal(_)     |
            ExprKind::Ident(_)       |
            ExprKind::Read(_)        |
            ExprKind::ReadPercent(_) => {}
        }
    }
}

// ================================================================
// TESTS
// ================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn test_span() -> Span {
        Span::new(Arc::new("test.fe".into()), 1, 1, 1)
    }

    fn test_ident(name: &str) -> Ident {
        Ident::new(name, test_span())
    }

    // ── Type ─────────────────────────────────────────────────────

    #[test]
    fn data_type_converts_to_type() {
        assert_eq!(DataType::Integer.to_type(),    Type::Integer);
        assert_eq!(DataType::Percentage.to_type(), Type::Percentage);
        assert_eq!(DataType::Byte.to_type(),       Type::Byte);
    }

    // ── InterfaceType ─────────────────────────────────────────────

    #[test]
    fn read_only_interfaces() {
        assert!(InterfaceType::Input.is_read_only());
        assert!(InterfaceType::AnalogInput.is_read_only());
        assert!(InterfaceType::Pulse.is_read_only());
        assert!(!InterfaceType::Output.is_read_only());
        assert!(!InterfaceType::Pwm.is_read_only());
        assert!(!InterfaceType::Display.is_read_only());
    }

    #[test]
    fn pull_only_valid_on_input() {
        assert!(InterfaceType::Input.supports_pull());
        assert!(!InterfaceType::Output.supports_pull());
        assert!(!InterfaceType::Pwm.supports_pull());
    }

    // ── Qualifier ────────────────────────────────────────────────

    #[test]
    fn qualifier_valid_interfaces() {
        assert_eq!(Qualifier::Brightness.valid_interfaces(), &[InterfaceType::Pwm]);
        assert_eq!(Qualifier::Lcd.valid_interfaces(),        &[InterfaceType::Display]);
        assert_eq!(Qualifier::Trigger.valid_interfaces(),    &[InterfaceType::Pulse]);
        assert_eq!(Qualifier::Enable.valid_interfaces(),     &[InterfaceType::Output]);
    }

    // ── Duration ─────────────────────────────────────────────────

    #[test]
    fn duration_millis_conversion() {
        let ms = Duration { value: 500, unit: TimeUnit::Milliseconds, span: test_span() };
        let s  = Duration { value: 2,   unit: TimeUnit::Seconds,      span: test_span() };
        assert_eq!(ms.as_millis(), 500);
        assert_eq!(s.as_millis(),  2000);
    }

    // ── Literal ──────────────────────────────────────────────────

    #[test]
    fn literal_type_names() {
        assert_eq!(Literal::Int(0).type_name(),        "Integer");
        assert_eq!(Literal::Decimal(0.0).type_name(),  "Decimal");
        assert_eq!(Literal::Bool(true).type_name(),    "Boolean");
        assert_eq!(Literal::High.type_name(),          "PinState");
        assert_eq!(Literal::Str("x".into()).type_name(), "String");
    }

    // ── ConfigKey ────────────────────────────────────────────────

    #[test]
    fn config_key_name() {
        assert_eq!(ConfigKey::Target.name(),        "TARGET");
        assert_eq!(ConfigKey::ClockSpeed.name(),    "CLOCK_SPEED");
        assert_eq!(ConfigKey::Unknown("foo".into()).name(), "foo");
    }

    // ── BinOp ────────────────────────────────────────────────────

    #[test]
    fn binop_symbols() {
        assert_eq!(BinOp::Add.symbol(), "+");
        assert_eq!(BinOp::Gte.symbol(), ">=");
        assert_eq!(BinOp::And.symbol(), "AND");
    }

    // ── Visitor default walk ─────────────────────────────────────

    #[test]
    fn visitor_walks_without_panic() {
        // Build a minimal program and walk it with a no-op visitor.
        struct Counter { stmts: usize }
        impl AstVisitor for Counter {
            fn visit_statement(&mut self, node: &Statement) {
                self.stmts += 1;
                self.walk_statement(node);
            }
        }

        let run = RunSection {
            items: vec![
                RunItem::Loop(LoopBlock {
                    body: vec![
                        Statement {
                            kind: StmtKind::Break,
                            span: test_span(),
                        },
                        Statement {
                            kind: StmtKind::Continue,
                            span: test_span(),
                        },
                    ],
                    span: test_span(),
                })
            ],
            span: test_span(),
        };

        let program = Program {
            config:    None,
            defines:   vec![],
            creates:   vec![],
            declares:  vec![],
            functions: vec![],
            run,
            span: test_span(),
        };

        let mut counter = Counter { stmts: 0 };
        counter.visit_program(&program);
        assert_eq!(counter.stmts, 2);
    }

    // ── Ownership ────────────────────────────────────────────────

    #[test]
    fn ownership_variants_are_distinct() {
        assert_ne!(Ownership::Give, Ownership::Lend);
        assert_ne!(Ownership::Lend, Ownership::Borrow);
        assert_ne!(Ownership::Give, Ownership::Borrow);
    }

    // ── ElseClause has no ElseIf variant ────────────────────────

    #[test]
    fn else_clause_only_has_block() {
        // This test is structural: if someone adds an ElseIf variant
        // to ElseClause, this match will fail to compile — acting as
        // a compile-time guard on semantic constraint 24.
        let clause = ElseClause::Block(vec![]);
        match clause {
            ElseClause::Block(_) => {} // only variant — intentional
        }
    }
}