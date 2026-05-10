use crate::types::*;


pub struct Program {
    pub config:    Option<ConfigSection>,
    pub defines:   Vec<DefineItem>,
    pub creates:   Vec<CreateItem>,
    pub declares:  Vec<DeclareItem>,
    pub functions: Vec<FunctionDef>,
    pub run:       RunSection,
    pub span:      Span,
}

pub struct ConfigSection {
    pub items: Vec<ConfigItem>,
    pub span:  Span,
}

pub struct ConfigItem {
    pub key:   ConfigKey,
    pub value: ConfigValue,
    pub span:  Span,
}

/// Strongly typed config keys — validated in semantic pass.
#[derive(Debug, Clone, PartialEq)]
pub enum ConfigKey {
    Target,
    ClockSpeed,
    Serial,
    DefaultPullUp,
    DebounceMs,
    Optimize,
    Debug,
    Unknown(String),    // caught and errored in semantic pass
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConfigValue {
    Str(String),
    Int(i64),
    Clock(u32),         // the numeric part — MHZ suffix stripped
    Bool(bool),
}

pub struct DefineItem {
    pub name:   Ident,
    pub spec:   DeviceSpec,
    pub span:   Span,
}

pub enum DeviceSpec {
    Simple(InterfaceSpec),
    Composite(Vec<InterfaceSpec>),
}

pub struct InterfaceSpec {
    pub interface: InterfaceType,
    pub qualifier: Option<Qualifier>,
    pub span:      Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum InterfaceType {
    Input,
    Output,
    AnalogInput,
    Pwm,
    Display,
    Pulse,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Qualifier {
    Brightness,
    Speed,
    Angle,
    Red, Green, Blue,
    Lcd, Oled,
    Trigger, Echo,
    Enable,
}

pub struct CreateItem {
    pub device_type:   Ident,           // references a DefineItem name
    pub instance_name: Ident,           // the name used in RUN
    pub pins:          PinSpec,
    pub pull:          Option<PullConfig>,
    pub init:          Option<InitBlock>,
    pub span:          Span,
}

pub enum PinSpec {
    Single(u32),                        // ON PIN 14
    Multi(Vec<PinAssignment>),          // ON { ... }
}

pub struct PinAssignment {
    pub interface_name: Option<Ident>,  // named form: output: PIN 3
    pub pin_number:     u32,
    pub disambiguator:  Option<Ident>,  // mixed form: PIN 3 OUTPUT
    pub span:           Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PullConfig {
    Up,
    Down,
}

pub struct InitBlock {
    pub values: Vec<InitEntry>,
    pub span:   Span,
}

pub struct InitEntry {
    pub interface_name: Option<Ident>,  // named: output: LOW
    pub value:          InitValue,
    pub span:           Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum InitValue {
    High,
    Low,
    Decimal(f64),
    Str(String),
    Int(i64),
}

pub struct DeclareItem {
    pub kind: DeclareKind,
    pub span: Span,
}

pub enum DeclareKind {
    Variable(VariableDecl),
    Constant(ConstantDecl),
}

pub struct VariableDecl {
    pub ty:          DataType,
    pub array_size:  Option<usize>,     // Some(n) if array declaration
    pub name:        Ident,
    pub init:        Option<InitExpr>,
    pub span:        Span,
}

pub struct ConstantDecl {
    pub ty:    DataType,
    pub name:  Ident,
    pub value: Literal,
    pub span:  Span,
}

pub enum InitExpr {
    Single(Literal),
    Array(Vec<Literal>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum DataType {
    Integer,
    Decimal,
    Percentage,
    Boolean,
    String,
    Byte,
}

pub struct FunctionDef {
    pub name:   Ident,
    pub params: Vec<Param>,
    pub body:   Vec<Statement>,
    pub ret:    Option<ReturnStmt>,
    /// Filled by semantic pass — None until resolved
    pub return_type: Option<Type>,
    pub span:   Span,
}

pub enum Param {
    Data {
        ty:   DataType,
        name: Ident,
        span: Span,
    },
    Device {
        ownership: Ownership,
        device_ty: Ident,        // references a DefineItem
        name:      Ident,
        span:      Span,
    },
}

pub struct ReturnStmt {
    pub value: Option<Expr>,    // None → void return
    pub span:  Span,
}

pub struct RunSection {
    pub items: Vec<RunItem>,
    pub span:  Span,
}

pub enum RunItem {
    Every(EveryBlock),
    TopIf(TopIfBlock),      // IF at RUN top level — may contain EVERY
    Loop(LoopBlock),
    Stmt(Statement),
}

pub struct EveryBlock {
    pub period:     Duration,
    pub body:       Vec<Statement>,
    pub span:       Span,
}

/// IF at RUN top level — body is Vec<RunItem> so it can contain EVERY.
/// Statement-level IF uses a different node (IfStmt) whose body
/// is Vec<Statement> — this structural difference enforces the
/// EVERY placement constraint at the AST level, not just semantically.
pub struct TopIfBlock {
    pub condition:  Expr,
    pub then_items: Vec<RunItem>,
    pub else_items: Option<Vec<RunItem>>,
    pub span:       Span,
}

pub struct LoopBlock {
    pub body: Vec<Statement>,
    pub span: Span,
}

pub struct Duration {
    pub value: u32,
    pub unit:  TimeUnit,
    pub span:  Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TimeUnit {
    Milliseconds,
    Seconds,
}

pub struct Statement {
    pub kind: StmtKind,
    pub span: Span,
}

pub enum StmtKind {
    Assignment(AssignStmt),
    Set(SetStmt),
    Turn(TurnStmt),
    Toggle(ToggleStmt),
    Print(PrintStmt),
    VoidCall(CallStmt),
    Delay(DelayStmt),
    If(IfStmt),
    For(ForStmt),
    Break(Span),
    Continue(Span),
    InlineDeclare(InlineDeclareStmt),
}

pub struct AssignStmt {
    pub target: Ident,
    pub value:  Expr,
    pub span:   Span,
}

pub struct SetStmt {
    pub device:    Ident,
    pub qualifier: Option<Qualifier>,
    pub value:     Expr,
    pub span:      Span,
}

pub struct TurnStmt {
    pub device:    Ident,
    pub qualifier: Option<Qualifier>,   // ENABLE for composite devices
    pub state:     PinState,
    pub span:      Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PinState { High, Low }

pub struct ToggleStmt {
    pub device: Ident,
    pub span:   Span,
}

pub struct PrintStmt {
    pub value: Expr,
    pub span:  Span,
}

pub struct CallStmt {
    pub function: Ident,
    pub args:     Vec<CallArg>,
    pub span:     Span,
}

pub struct DelayStmt {
    pub duration: Duration,
    pub span:     Span,
}

/// Statement-level IF — body is Vec<Statement>, NOT Vec<RunItem>.
/// This is the structural enforcement of EVERY placement.
pub struct IfStmt {
    pub condition:  Expr,
    pub then_body:  Vec<Statement>,
    pub else_body:  Option<ElseClause>,
    pub span:       Span,
}

/// Else can only hold statements or another nested IF.
/// No ELSE IF keyword — this is the structural enforcement of that rule.
pub enum ElseClause {
    Block(Vec<Statement>),
    // No ElseIf variant — intentionally absent
}

pub struct ForStmt {
    pub variable: Ident,
    pub iterator: ForIterator,
    pub body:     Vec<Statement>,
    pub span:     Span,
}

pub enum ForIterator {
    Array(Expr),                        // FOR x IN array_var
    Range { from: Expr, to: Expr },     // FOR x IN RANGE 0, 9
}

pub struct InlineDeclareStmt {
    pub ty:         DataType,
    pub array_size: Option<usize>,
    pub name:       Ident,
    pub init:       InlineDeclareInit,
    pub span:       Span,
}

pub enum InlineDeclareInit {
    Value(InitExpr),
    Call(CallExpr),
}

pub struct CallArg {
    pub kind: CallArgKind,
    pub span: Span,
}

pub enum CallArgKind {
    Data(Expr),
    Device {
        ownership: Ownership,
        name:      Ident,
    },
}

pub struct Expr {
    pub kind: ExprKind,
    pub ty:   Option<Type>,     // None until semantic pass fills it
    pub span: Span,
}

pub enum ExprKind {
    // Literals
    Literal(Literal),

    // Variable / device reference
    Ident(Ident),

    // Hardware reads
    Read(Ident),                // READ device
    ReadPercent(Ident),         // READ_PERCENT device

    // Binary operations
    BinOp {
        op:    BinOp,
        left:  Box<Expr>,
        right: Box<Expr>,
    },

    // Unary operations
    UnaryOp {
        op:      UnaryOp,
        operand: Box<Expr>,
    },

    // Device / boolean state check
    Is {
        target:  Box<Expr>,
        negated: bool,          // true = IS NOT
        state:   Box<Expr>,
    },

    // Function call in expression position
    Call(CallExpr),

    // Inline IF expression
    IfExpr {
        condition:  Box<Expr>,
        then_value: Box<Expr>,
        else_value: Box<Expr>,
    },
}

pub struct CallExpr {
    pub function: Ident,
    pub args:     Vec<CallArg>,
    pub span:     Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum BinOp {
    Add, Sub, Mul, Div,
    Gt, Lt, Gte, Lte,
    Eq, NotEq,
    And, Or,
}

#[derive(Debug, Clone, PartialEq)]
pub enum UnaryOp {
    Not,
    Neg,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Literal {
    Int(i64),
    Decimal(f64),
    Str(String),
    Bool(bool),
    High,               // pin state literal
    Low,                // pin state literal
    Byte(u8),           // stored after range validation
    Percentage(f64),    // stored after range validation
}