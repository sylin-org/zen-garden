//! Predicate parser and evaluator for the compatibility DSL (COMPAT-0002).
//!
//! Grammar:
//! ```text
//! predicate   = fact OPERATOR value_expr
//! fact        = dotted.identifier (lowercase)
//! OPERATOR    = HAS | LACKS | IS | IS NOT | IN | NOT IN | >= | > | < | <=
//! value_expr  = value (('AND' | 'OR') value)* | '(' value (',' value)* ')'
//! value       = identifier | number | 'present'
//! ```

use super::facts::HostFacts;
use std::fmt;

// ============================================================================
// Types
// ============================================================================

/// A parsed, validated predicate ready for evaluation.
#[derive(Debug, Clone, PartialEq)]
pub struct Predicate {
    pub fact: Fact,
    pub condition: Condition,
}

/// A typed fact from the `host.*` namespace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Fact {
    Architecture,
    OsFamily,
    CpuModel,
    CpuPattern,
    CpuFeatures,
    RamTotalMb,
    Gpu,
    GpuCount,
    GpuVramTotalMb,
    GpuVramTotalGb,
    Npu,
    AiRuntime,
}

/// The type category of a fact, used for operator validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FactType {
    Set,
    Scalar,
    Boolean,
    Numeric,
}

/// The condition asserted about a fact.
#[derive(Debug, Clone, PartialEq)]
pub enum Condition {
    /// Set contains at least one of listed values (OR).
    Has(Vec<String>),
    /// Set contains all listed values (AND).
    HasAll(Vec<String>),
    /// Set contains none of listed values.
    Lacks(Vec<String>),
    /// Scalar equals value.
    Is(String),
    /// Scalar does not equal value.
    IsNot(String),
    /// Scalar is one of listed values.
    In(Vec<String>),
    /// Scalar is none of listed values.
    NotIn(Vec<String>),
    /// Boolean presence check.
    Present(bool),
    /// Numeric comparison.
    Cmp { op: CmpOp, value: f64 },
}

/// Numeric comparison operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CmpOp {
    Gte,
    Gt,
    Lte,
    Lt,
}

/// Parse or validation error with context.
#[derive(Debug, Clone)]
pub struct PredicateError {
    pub input: String,
    pub message: String,
    pub position: Option<usize>,
}

impl fmt::Display for PredicateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(pos) = self.position {
            write!(f, "at position {}: {}", pos, self.message)
        } else {
            write!(f, "{}", self.message)
        }
    }
}

impl std::error::Error for PredicateError {}

// ============================================================================
// Fact registry
// ============================================================================

impl Fact {
    /// Resolve a dotted fact name to a typed Fact.
    pub fn from_name(name: &str) -> Result<Self, String> {
        match name.to_ascii_lowercase().as_str() {
            "host.architecture" => Ok(Fact::Architecture),
            "host.os.family" => Ok(Fact::OsFamily),
            "host.cpu.model" => Ok(Fact::CpuModel),
            "host.cpu.pattern" => Ok(Fact::CpuPattern),
            "host.cpu.features" => Ok(Fact::CpuFeatures),
            "host.ram.total.mb" => Ok(Fact::RamTotalMb),
            "host.gpu" => Ok(Fact::Gpu),
            "host.gpu.count" => Ok(Fact::GpuCount),
            "host.gpu.vram.total.mb" => Ok(Fact::GpuVramTotalMb),
            "host.gpu.vram.total.gb" => Ok(Fact::GpuVramTotalGb),
            "host.npu" => Ok(Fact::Npu),
            "host.ai.runtime" => Ok(Fact::AiRuntime),
            other => Err(format!(
                "Unknown fact '{}'. Valid: host.architecture, host.os.family, host.cpu.model, \
                 host.cpu.pattern, host.cpu.features, host.ram.total.mb, host.gpu, host.gpu.count, \
                 host.gpu.vram.total.mb, host.gpu.vram.total.gb, host.npu, host.ai.runtime",
                other
            )),
        }
    }

    /// The type category of this fact.
    pub fn fact_type(&self) -> FactType {
        match self {
            Fact::CpuPattern | Fact::CpuFeatures | Fact::AiRuntime => FactType::Set,
            Fact::Architecture | Fact::OsFamily | Fact::CpuModel => FactType::Scalar,
            Fact::Gpu | Fact::Npu => FactType::Boolean,
            Fact::RamTotalMb | Fact::GpuCount | Fact::GpuVramTotalMb | Fact::GpuVramTotalGb => {
                FactType::Numeric
            }
        }
    }

    /// Human-readable dotted name (for error messages).
    pub fn name(&self) -> &'static str {
        match self {
            Fact::Architecture => "host.architecture",
            Fact::OsFamily => "host.os.family",
            Fact::CpuModel => "host.cpu.model",
            Fact::CpuPattern => "host.cpu.pattern",
            Fact::CpuFeatures => "host.cpu.features",
            Fact::RamTotalMb => "host.ram.total.mb",
            Fact::Gpu => "host.gpu",
            Fact::GpuCount => "host.gpu.count",
            Fact::GpuVramTotalMb => "host.gpu.vram.total.mb",
            Fact::GpuVramTotalGb => "host.gpu.vram.total.gb",
            Fact::Npu => "host.npu",
            Fact::AiRuntime => "host.ai.runtime",
        }
    }
}

impl fmt::Display for CmpOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CmpOp::Gte => write!(f, ">="),
            CmpOp::Gt => write!(f, ">"),
            CmpOp::Lte => write!(f, "<="),
            CmpOp::Lt => write!(f, "<"),
        }
    }
}

// ============================================================================
// Parser
// ============================================================================

impl Predicate {
    /// Parse a predicate string into a validated, typed `Predicate`.
    ///
    /// Validates fact names against the registry and enforces operator-type
    /// compatibility. Returns a clear error on any syntax or type mismatch.
    ///
    /// ```rust,ignore
    /// let p = Predicate::parse("host.ai.runtime LACKS cuda")?;
    /// let p = Predicate::parse("host.ram.total.mb < 8192")?;
    /// let p = Predicate::parse("host.architecture IN (armv7l,armv6l)")?;
    /// ```
    pub fn parse(input: &str) -> Result<Self, PredicateError> {
        let input = input.trim();
        if input.is_empty() {
            return Err(PredicateError {
                input: input.to_string(),
                message: "Empty predicate".to_string(),
                position: None,
            });
        }

        let mut tokens = Tokenizer::new(input);

        // 1. Parse fact name
        let fact_token = tokens.next_token().ok_or_else(|| PredicateError {
            input: input.to_string(),
            message: "Expected fact name".to_string(),
            position: Some(0),
        })?;

        let fact = Fact::from_name(fact_token).map_err(|msg| PredicateError {
            input: input.to_string(),
            message: msg,
            position: Some(0),
        })?;

        // 2. Parse operator
        let op_pos = tokens.pos();
        let op_token = tokens.next_token().ok_or_else(|| PredicateError {
            input: input.to_string(),
            message: format!(
                "Expected operator after '{}'. Valid: HAS, LACKS, IS, IS NOT, IN, NOT IN, >=, <, <=, >",
                fact.name()
            ),
            position: Some(op_pos),
        })?;

        let op_upper = op_token.to_ascii_uppercase();

        // 3. Parse condition based on operator + handle multi-word operators (IS NOT, NOT IN)
        let condition = match op_upper.as_str() {
            "HAS" => parse_set_has(&mut tokens, input)?,
            "LACKS" => {
                let values = parse_value_list(&mut tokens, input)?;
                Condition::Lacks(values)
            }
            "IS" => parse_is(&mut tokens, input, fact.fact_type())?,
            "NOT" => {
                // NOT IN
                let next_pos = tokens.pos();
                let next = tokens.next_token().ok_or_else(|| PredicateError {
                    input: input.to_string(),
                    message: "Expected 'IN' after 'NOT'".to_string(),
                    position: Some(next_pos),
                })?;
                if next.to_ascii_uppercase() != "IN" {
                    return Err(PredicateError {
                        input: input.to_string(),
                        message: format!(
                            "Unknown operator 'NOT {}'. Did you mean 'NOT IN'?",
                            next
                        ),
                        position: Some(op_pos),
                    });
                }
                let values = parse_value_list(&mut tokens, input)?;
                Condition::NotIn(values)
            }
            "IN" => {
                let values = parse_value_list(&mut tokens, input)?;
                Condition::In(values)
            }
            ">=" => {
                let value = parse_number(&mut tokens, input)?;
                Condition::Cmp {
                    op: CmpOp::Gte,
                    value,
                }
            }
            ">" => {
                let value = parse_number(&mut tokens, input)?;
                Condition::Cmp {
                    op: CmpOp::Gt,
                    value,
                }
            }
            "<=" => {
                let value = parse_number(&mut tokens, input)?;
                Condition::Cmp {
                    op: CmpOp::Lte,
                    value,
                }
            }
            "<" => {
                let value = parse_number(&mut tokens, input)?;
                Condition::Cmp {
                    op: CmpOp::Lt,
                    value,
                }
            }
            other => {
                return Err(PredicateError {
                    input: input.to_string(),
                    message: format!(
                        "Unknown operator '{}'. Valid: HAS, LACKS, IS, IS NOT, IN, NOT IN, >=, <, <=, >",
                        other
                    ),
                    position: Some(op_pos),
                });
            }
        };

        // 4. Type enforcement
        validate_type_compat(fact, &condition, input, op_pos)?;

        Ok(Predicate { fact, condition })
    }

    /// Evaluate this predicate against a host's facts.
    ///
    /// Returns `false` if the referenced fact is missing/None.
    pub fn check(&self, host: &HostFacts) -> bool {
        match (&self.fact, &self.condition) {
            // ── Set facts ──────────────────────────────────────────
            (Fact::AiRuntime, Condition::Has(vals)) => {
                vals.iter().any(|v| host.ai_runtimes.contains(v))
            }
            (Fact::AiRuntime, Condition::HasAll(vals)) => {
                vals.iter().all(|v| host.ai_runtimes.contains(v))
            }
            (Fact::AiRuntime, Condition::Lacks(vals)) => {
                vals.iter().all(|v| !host.ai_runtimes.contains(v))
            }

            (Fact::CpuFeatures, Condition::Has(vals)) => {
                vals.iter().any(|v| host.cpu_features.contains(v))
            }
            (Fact::CpuFeatures, Condition::HasAll(vals)) => {
                vals.iter().all(|v| host.cpu_features.contains(v))
            }
            (Fact::CpuFeatures, Condition::Lacks(vals)) => {
                vals.iter().all(|v| !host.cpu_features.contains(v))
            }

            (Fact::CpuPattern, Condition::Has(vals)) => {
                vals.iter().any(|v| host.cpu_patterns.contains(v))
            }
            (Fact::CpuPattern, Condition::HasAll(vals)) => {
                vals.iter().all(|v| host.cpu_patterns.contains(v))
            }
            (Fact::CpuPattern, Condition::Lacks(vals)) => {
                vals.iter().all(|v| !host.cpu_patterns.contains(v))
            }

            // ── Scalar facts ───────────────────────────────────────
            (fact, Condition::Is(val)) => scalar_value(fact, host)
                .map(|s| s.eq_ignore_ascii_case(val))
                .unwrap_or(false),
            (fact, Condition::IsNot(val)) => scalar_value(fact, host)
                .map(|s| !s.eq_ignore_ascii_case(val))
                .unwrap_or(false),
            (fact, Condition::In(vals)) => scalar_value(fact, host)
                .map(|s| vals.iter().any(|v| s.eq_ignore_ascii_case(v)))
                .unwrap_or(false),
            (fact, Condition::NotIn(vals)) => scalar_value(fact, host)
                .map(|s| vals.iter().all(|v| !s.eq_ignore_ascii_case(v)))
                .unwrap_or(false),

            // ── Boolean facts ──────────────────────────────────────
            (Fact::Gpu, Condition::Present(expected)) => host.gpu_present == *expected,
            (Fact::Npu, Condition::Present(expected)) => host.npu_present == *expected,

            // ── Numeric facts ──────────────────────────────────────
            (fact, Condition::Cmp { op, value }) => {
                let actual = numeric_value(fact, host);
                match op {
                    CmpOp::Gte => actual >= *value,
                    CmpOp::Gt => actual > *value,
                    CmpOp::Lte => actual <= *value,
                    CmpOp::Lt => actual < *value,
                }
            }

            // Type-validated at parse time — unreachable in practice
            _ => false,
        }
    }
}

/// Evaluate all predicates against host facts (AND semantics).
///
/// Short-circuits on the first `false`.
pub fn check_all(predicates: &[Predicate], host: &HostFacts) -> bool {
    predicates.iter().all(|p| p.check(host))
}

// ============================================================================
// Scalar / numeric value extraction
// ============================================================================

fn scalar_value<'a>(fact: &Fact, host: &'a HostFacts) -> Option<&'a str> {
    match fact {
        Fact::Architecture => host.architecture.as_deref(),
        Fact::OsFamily => host.os_family.as_deref(),
        Fact::CpuModel => host.cpu_model.as_deref(),
        _ => None,
    }
}

fn numeric_value(fact: &Fact, host: &HostFacts) -> f64 {
    match fact {
        Fact::RamTotalMb => host.ram_total_mb.unwrap_or(0) as f64,
        Fact::GpuCount => host.gpu_count as f64,
        Fact::GpuVramTotalMb => host.gpu_vram_total_mb as f64,
        Fact::GpuVramTotalGb => (host.gpu_vram_total_mb as f64) / 1024.0,
        _ => 0.0,
    }
}

// ============================================================================
// Type enforcement
// ============================================================================

fn validate_type_compat(
    fact: Fact,
    condition: &Condition,
    input: &str,
    op_pos: usize,
) -> Result<(), PredicateError> {
    let ft = fact.fact_type();
    let valid = match condition {
        Condition::Has(_) | Condition::HasAll(_) | Condition::Lacks(_) => ft == FactType::Set,
        Condition::Is(_) | Condition::IsNot(_) => ft == FactType::Scalar,
        Condition::In(_) | Condition::NotIn(_) => ft == FactType::Scalar,
        Condition::Present(_) => ft == FactType::Boolean,
        Condition::Cmp { .. } => ft == FactType::Numeric,
    };

    if !valid {
        let op_name = match condition {
            Condition::Has(_) | Condition::HasAll(_) => "HAS",
            Condition::Lacks(_) => "LACKS",
            Condition::Is(_) => "IS",
            Condition::IsNot(_) => "IS NOT",
            Condition::In(_) => "IN",
            Condition::NotIn(_) => "NOT IN",
            Condition::Present(_) => "IS present",
            Condition::Cmp { op, .. } => match op {
                CmpOp::Gte => ">=",
                CmpOp::Gt => ">",
                CmpOp::Lte => "<=",
                CmpOp::Lt => "<",
            },
        };

        let valid_ops = match ft {
            FactType::Set => "HAS, LACKS",
            FactType::Scalar => "IS, IS NOT, IN, NOT IN",
            FactType::Boolean => "IS present, IS NOT present",
            FactType::Numeric => ">=, >, <, <=",
        };

        return Err(PredicateError {
            input: input.to_string(),
            message: format!(
                "Type mismatch: '{}' is {:?}, but {} requires a {:?} fact. Valid operators for '{}': {}",
                fact.name(),
                ft,
                op_name,
                condition_type(condition),
                fact.name(),
                valid_ops
            ),
            position: Some(op_pos),
        });
    }

    Ok(())
}

fn condition_type(condition: &Condition) -> FactType {
    match condition {
        Condition::Has(_) | Condition::HasAll(_) | Condition::Lacks(_) => FactType::Set,
        Condition::Is(_) | Condition::IsNot(_) | Condition::In(_) | Condition::NotIn(_) => {
            FactType::Scalar
        }
        Condition::Present(_) => FactType::Boolean,
        Condition::Cmp { .. } => FactType::Numeric,
    }
}

// ============================================================================
// Sub-parsers
// ============================================================================

/// Parse HAS — may be followed by AND/OR connectives or a parenthesized list.
fn parse_set_has(tokens: &mut Tokenizer, input: &str) -> Result<Condition, PredicateError> {
    // Check for HAS ALL modifier
    let checkpoint = tokens.pos();
    if let Some(next) = tokens.peek_token() {
        if next.to_ascii_uppercase() == "ALL" {
            tokens.next_token(); // consume ALL
            let values = parse_value_list(tokens, input)?;
            return Ok(Condition::HasAll(values));
        }
    }
    tokens.set_pos(checkpoint);

    let values = parse_value_list_with_connectives(tokens, input)?;
    match values {
        ValueList::Or(vals) => Ok(Condition::Has(vals)),
        ValueList::And(vals) => Ok(Condition::HasAll(vals)),
        ValueList::Single(val) => Ok(Condition::Has(vec![val])),
    }
}

/// Parse IS — handles IS, IS NOT, IS present, IS NOT present.
fn parse_is(
    tokens: &mut Tokenizer,
    input: &str,
    _fact_type: FactType,
) -> Result<Condition, PredicateError> {
    let pos = tokens.pos();
    let next = tokens.next_token().ok_or_else(|| PredicateError {
        input: input.to_string(),
        message: "Expected value after 'IS'".to_string(),
        position: Some(pos),
    })?;

    let upper = next.to_ascii_uppercase();

    if upper == "NOT" {
        // IS NOT ...
        let val_pos = tokens.pos();
        let val = tokens.next_token().ok_or_else(|| PredicateError {
            input: input.to_string(),
            message: "Expected value after 'IS NOT'".to_string(),
            position: Some(val_pos),
        })?;

        if val.to_ascii_lowercase() == "present" {
            Ok(Condition::Present(false))
        } else {
            Ok(Condition::IsNot(val.to_ascii_lowercase()))
        }
    } else if upper == "PRESENT" {
        Ok(Condition::Present(true))
    } else {
        // IS <value>
        Ok(Condition::Is(next.to_ascii_lowercase()))
    }
}

/// Parse a comma-separated value list, optionally wrapped in parens.
fn parse_value_list(tokens: &mut Tokenizer, input: &str) -> Result<Vec<String>, PredicateError> {
    let pos = tokens.pos();
    let rest = tokens.remaining().trim();

    if rest.is_empty() {
        return Err(PredicateError {
            input: input.to_string(),
            message: "Expected value(s)".to_string(),
            position: Some(pos),
        });
    }

    // Strip optional parens
    let inner = if rest.starts_with('(') {
        let end = rest.find(')').ok_or_else(|| PredicateError {
            input: input.to_string(),
            message: "Unclosed parenthesis".to_string(),
            position: Some(pos),
        })?;
        tokens.advance_to_end();
        &rest[1..end]
    } else {
        tokens.advance_to_end();
        rest
    };

    // Split on comma, strip AND/OR keywords (they're implicit in comma lists)
    let values: Vec<String> = inner
        .split(',')
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| !s.is_empty())
        .collect();

    if values.is_empty() {
        return Err(PredicateError {
            input: input.to_string(),
            message: "Empty value list".to_string(),
            position: Some(pos),
        });
    }

    Ok(values)
}

/// Parse values with AND/OR connectives or comma-separated in parens.
fn parse_value_list_with_connectives(
    tokens: &mut Tokenizer,
    input: &str,
) -> Result<ValueList, PredicateError> {
    let pos = tokens.pos();
    let rest = tokens.remaining().trim();

    if rest.is_empty() {
        return Err(PredicateError {
            input: input.to_string(),
            message: "Expected value(s) after operator".to_string(),
            position: Some(pos),
        });
    }

    // Parenthesized list — comma = OR
    if rest.starts_with('(') {
        let values = parse_value_list(tokens, input)?;
        return Ok(if values.len() == 1 {
            ValueList::Single(values.into_iter().next().unwrap())
        } else {
            ValueList::Or(values)
        });
    }

    // Token-based: value (AND|OR value)*
    tokens.advance_to_end();
    let parts: Vec<&str> = rest.split_whitespace().collect();

    if parts.is_empty() {
        return Err(PredicateError {
            input: input.to_string(),
            message: "Expected value(s)".to_string(),
            position: Some(pos),
        });
    }

    // Single value (possibly comma-separated without spaces)
    if parts.len() == 1 {
        let val = parts[0];
        if val.contains(',') {
            // "cuda,rocm" — comma = OR
            let values: Vec<String> = val
                .split(',')
                .map(|s| s.trim().to_ascii_lowercase())
                .filter(|s| !s.is_empty())
                .collect();
            return Ok(if values.len() == 1 {
                ValueList::Single(values.into_iter().next().unwrap())
            } else {
                ValueList::Or(values)
            });
        }
        return Ok(ValueList::Single(val.to_ascii_lowercase()));
    }

    // Multiple tokens: check for AND/OR connectives
    let mut values = vec![parts[0].to_ascii_lowercase()];
    let mut connective: Option<&str> = None;
    let mut i = 1;

    while i < parts.len() {
        let upper = parts[i].to_ascii_uppercase();
        if upper == "AND" || upper == "OR" {
            if let Some(prev) = connective {
                if prev != upper {
                    return Err(PredicateError {
                        input: input.to_string(),
                        message: format!(
                            "Cannot mix AND and OR in a single expression. Use separate when: entries."
                        ),
                        position: Some(pos),
                    });
                }
            }
            connective = Some(if upper == "AND" { "AND" } else { "OR" });
            i += 1;
            if i >= parts.len() {
                return Err(PredicateError {
                    input: input.to_string(),
                    message: format!("Expected value after {}", upper),
                    position: Some(pos),
                });
            }
            values.push(parts[i].to_ascii_lowercase());
        } else {
            // Bare token after a value — treat as comma-separated without commas?
            values.push(parts[i].to_ascii_lowercase());
        }
        i += 1;
    }

    match connective {
        Some("AND") => Ok(ValueList::And(values)),
        Some("OR") => Ok(ValueList::Or(values)),
        _ if values.len() == 1 => Ok(ValueList::Single(values.into_iter().next().unwrap())),
        _ => Ok(ValueList::Or(values)), // bare multi-value defaults to OR
    }
}

enum ValueList {
    Single(String),
    Or(Vec<String>),
    And(Vec<String>),
}

fn parse_number(tokens: &mut Tokenizer, input: &str) -> Result<f64, PredicateError> {
    let pos = tokens.pos();
    let val = tokens.next_token().ok_or_else(|| PredicateError {
        input: input.to_string(),
        message: "Expected numeric value".to_string(),
        position: Some(pos),
    })?;

    val.parse::<f64>().map_err(|_| PredicateError {
        input: input.to_string(),
        message: format!("'{}' is not a valid number", val),
        position: Some(pos),
    })
}

// ============================================================================
// Tokenizer — splits on whitespace, respects parens as boundaries
// ============================================================================

struct Tokenizer<'a> {
    input: &'a str,
    pos: usize,
}

impl<'a> Tokenizer<'a> {
    fn new(input: &'a str) -> Self {
        Self { input, pos: 0 }
    }

    fn pos(&self) -> usize {
        self.pos
    }

    fn set_pos(&mut self, pos: usize) {
        self.pos = pos;
    }

    fn remaining(&self) -> &'a str {
        &self.input[self.pos..]
    }

    fn advance_to_end(&mut self) {
        self.pos = self.input.len();
    }

    fn peek_token(&self) -> Option<&'a str> {
        let rest = self.input[self.pos..].trim_start();
        if rest.is_empty() {
            return None;
        }
        let offset = self.input.len() - rest.len();
        let end = rest
            .find(|c: char| c.is_whitespace() || c == '(' || c == ')')
            .unwrap_or(rest.len());
        if end == 0 {
            // Paren or other delimiter
            Some(&self.input[offset..offset + 1])
        } else {
            Some(&self.input[offset..offset + end])
        }
    }

    fn next_token(&mut self) -> Option<&'a str> {
        let rest = self.input[self.pos..].trim_start();
        if rest.is_empty() {
            return None;
        }
        let offset = self.input.len() - rest.len();

        // Check for multi-char comparison operators
        if rest.starts_with(">=") {
            self.pos = offset + 2;
            return Some(&self.input[offset..offset + 2]);
        }
        if rest.starts_with("<=") {
            self.pos = offset + 2;
            return Some(&self.input[offset..offset + 2]);
        }
        if rest.starts_with('>') {
            self.pos = offset + 1;
            return Some(&self.input[offset..offset + 1]);
        }
        if rest.starts_with('<') {
            self.pos = offset + 1;
            return Some(&self.input[offset..offset + 1]);
        }

        let end = rest
            .find(|c: char| c.is_whitespace() || c == '(' || c == ')')
            .unwrap_or(rest.len());

        if end == 0 {
            self.pos = offset + 1;
            Some(&self.input[offset..offset + 1])
        } else {
            self.pos = offset + end;
            Some(&self.input[offset..offset + end])
        }
    }
}

// ============================================================================
// Display
// ============================================================================

impl fmt::Display for Predicate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.fact.name(), self.condition)
    }
}

impl fmt::Display for Condition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Condition::Has(vals) => write!(f, "HAS {}", vals.join(",")),
            Condition::HasAll(vals) => write!(f, "HAS ALL {}", vals.join(",")),
            Condition::Lacks(vals) => write!(f, "LACKS {}", vals.join(",")),
            Condition::Is(val) => write!(f, "IS {}", val),
            Condition::IsNot(val) => write!(f, "IS NOT {}", val),
            Condition::In(vals) => write!(f, "IN ({})", vals.join(",")),
            Condition::NotIn(vals) => write!(f, "NOT IN ({})", vals.join(",")),
            Condition::Present(true) => write!(f, "IS present"),
            Condition::Present(false) => write!(f, "IS NOT present"),
            Condition::Cmp { op, value } => {
                if *value == (*value as u64) as f64 {
                    write!(f, "{} {}", op, *value as u64)
                } else {
                    write!(f, "{} {}", op, value)
                }
            }
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn test_host() -> HostFacts {
        HostFacts {
            architecture: Some("x86_64".into()),
            os_family: Some("linux".into()),
            cpu_model: Some("Intel Celeron J4105".into()),
            cpu_patterns: HashSet::from(["j4105".into()]),
            cpu_features: HashSet::from(["sse4_2".into()]),
            ram_total_mb: Some(8192),
            gpu_present: true,
            gpu_count: 1,
            gpu_vram_total_mb: 6144,
            npu_present: false,
            ai_runtimes: HashSet::from(["rocm".into()]),
        }
    }

    // ── Parse tests ────────────────────────────────────────────────

    #[test]
    fn parse_lacks_single() {
        let p = Predicate::parse("host.ai.runtime LACKS cuda").unwrap();
        assert_eq!(p.fact, Fact::AiRuntime);
        assert_eq!(p.condition, Condition::Lacks(vec!["cuda".into()]));
    }

    #[test]
    fn parse_lacks_multi_comma() {
        let p = Predicate::parse("host.ai.runtime LACKS cuda,rocm,metal").unwrap();
        assert_eq!(
            p.condition,
            Condition::Lacks(vec!["cuda".into(), "rocm".into(), "metal".into()])
        );
    }

    #[test]
    fn parse_has_single() {
        let p = Predicate::parse("host.ai.runtime HAS rocm").unwrap();
        assert_eq!(p.condition, Condition::Has(vec!["rocm".into()]));
    }

    #[test]
    fn parse_has_comma() {
        let p = Predicate::parse("host.ai.runtime HAS cuda,rocm").unwrap();
        assert_eq!(
            p.condition,
            Condition::Has(vec!["cuda".into(), "rocm".into()])
        );
    }

    #[test]
    fn parse_has_parens() {
        let p = Predicate::parse("host.ai.runtime HAS (cuda,rocm)").unwrap();
        assert_eq!(
            p.condition,
            Condition::Has(vec!["cuda".into(), "rocm".into()])
        );
    }

    #[test]
    fn parse_has_or_keyword() {
        let p = Predicate::parse("host.ai.runtime HAS cuda OR rocm").unwrap();
        assert_eq!(
            p.condition,
            Condition::Has(vec!["cuda".into(), "rocm".into()])
        );
    }

    #[test]
    fn parse_has_and_keyword() {
        let p = Predicate::parse("host.ai.runtime HAS cuda AND rocm").unwrap();
        assert_eq!(
            p.condition,
            Condition::HasAll(vec!["cuda".into(), "rocm".into()])
        );
    }

    #[test]
    fn parse_has_all_modifier() {
        let p = Predicate::parse("host.ai.runtime HAS ALL cuda,rocm").unwrap();
        assert_eq!(
            p.condition,
            Condition::HasAll(vec!["cuda".into(), "rocm".into()])
        );
    }

    #[test]
    fn parse_numeric_lt() {
        let p = Predicate::parse("host.ram.total.mb < 8192").unwrap();
        assert_eq!(p.fact, Fact::RamTotalMb);
        assert_eq!(
            p.condition,
            Condition::Cmp {
                op: CmpOp::Lt,
                value: 8192.0
            }
        );
    }

    #[test]
    fn parse_numeric_gte() {
        let p = Predicate::parse("host.gpu.vram.total.mb >= 4096").unwrap();
        assert_eq!(p.fact, Fact::GpuVramTotalMb);
        assert_eq!(
            p.condition,
            Condition::Cmp {
                op: CmpOp::Gte,
                value: 4096.0
            }
        );
    }

    #[test]
    fn parse_is_scalar() {
        let p = Predicate::parse("host.architecture IS armv6l").unwrap();
        assert_eq!(p.fact, Fact::Architecture);
        assert_eq!(p.condition, Condition::Is("armv6l".into()));
    }

    #[test]
    fn parse_is_not_scalar() {
        let p = Predicate::parse("host.architecture IS NOT armv7l").unwrap();
        assert_eq!(p.condition, Condition::IsNot("armv7l".into()));
    }

    #[test]
    fn parse_in() {
        let p = Predicate::parse("host.architecture IN (armv7l,armv6l)").unwrap();
        assert_eq!(
            p.condition,
            Condition::In(vec!["armv7l".into(), "armv6l".into()])
        );
    }

    #[test]
    fn parse_not_in() {
        let p = Predicate::parse("host.os.family NOT IN (linux,macos)").unwrap();
        assert_eq!(
            p.condition,
            Condition::NotIn(vec!["linux".into(), "macos".into()])
        );
    }

    #[test]
    fn parse_is_present() {
        let p = Predicate::parse("host.gpu IS present").unwrap();
        assert_eq!(p.fact, Fact::Gpu);
        assert_eq!(p.condition, Condition::Present(true));
    }

    #[test]
    fn parse_is_not_present() {
        let p = Predicate::parse("host.npu IS NOT present").unwrap();
        assert_eq!(p.fact, Fact::Npu);
        assert_eq!(p.condition, Condition::Present(false));
    }

    // ── Type enforcement ───────────────────────────────────────────

    #[test]
    fn reject_has_on_numeric() {
        let err = Predicate::parse("host.ram.total.mb HAS 4096").unwrap_err();
        assert!(err.message.contains("Type mismatch"));
    }

    #[test]
    fn reject_lt_on_set() {
        let err = Predicate::parse("host.ai.runtime < 5").unwrap_err();
        assert!(err.message.contains("Type mismatch"));
    }

    #[test]
    fn reject_unknown_fact() {
        let err = Predicate::parse("host.ai.gpu_type HAS nvidia").unwrap_err();
        assert!(err.message.contains("Unknown fact"));
    }

    #[test]
    fn reject_unknown_operator() {
        let err = Predicate::parse("host.ai.runtime CONTAINS cuda").unwrap_err();
        assert!(err.message.contains("Unknown operator"));
    }

    #[test]
    fn reject_mixed_and_or() {
        let err = Predicate::parse("host.ai.runtime HAS cuda AND rocm OR metal").unwrap_err();
        assert!(err.message.contains("Cannot mix AND and OR"));
    }

    // ── Evaluation tests ───────────────────────────────────────────

    #[test]
    fn eval_lacks_true() {
        let p = Predicate::parse("host.ai.runtime LACKS cuda").unwrap();
        assert!(p.check(&test_host())); // host has rocm, not cuda
    }

    #[test]
    fn eval_lacks_false() {
        let p = Predicate::parse("host.ai.runtime LACKS rocm").unwrap();
        assert!(!p.check(&test_host())); // host has rocm
    }

    #[test]
    fn eval_has_true() {
        let p = Predicate::parse("host.ai.runtime HAS rocm").unwrap();
        assert!(p.check(&test_host()));
    }

    #[test]
    fn eval_has_false() {
        let p = Predicate::parse("host.ai.runtime HAS cuda").unwrap();
        assert!(!p.check(&test_host()));
    }

    #[test]
    fn eval_numeric_lt_true() {
        let p = Predicate::parse("host.ram.total.mb < 16384").unwrap();
        assert!(p.check(&test_host())); // 8192 < 16384
    }

    #[test]
    fn eval_numeric_lt_false() {
        let p = Predicate::parse("host.ram.total.mb < 4096").unwrap();
        assert!(!p.check(&test_host())); // 8192 >= 4096
    }

    #[test]
    fn eval_in_true() {
        let p = Predicate::parse("host.architecture IN (x86_64,aarch64)").unwrap();
        assert!(p.check(&test_host()));
    }

    #[test]
    fn eval_in_false() {
        let p = Predicate::parse("host.architecture IN (armv7l,armv6l)").unwrap();
        assert!(!p.check(&test_host())); // host is x86_64
    }

    #[test]
    fn eval_is_present_true() {
        let p = Predicate::parse("host.gpu IS present").unwrap();
        assert!(p.check(&test_host()));
    }

    #[test]
    fn eval_is_not_present_true() {
        let p = Predicate::parse("host.npu IS NOT present").unwrap();
        assert!(p.check(&test_host()));
    }

    #[test]
    fn eval_cpu_pattern_has() {
        let p = Predicate::parse("host.cpu.pattern HAS j4105,j3455").unwrap();
        assert!(p.check(&test_host())); // host has j4105
    }

    #[test]
    fn eval_cpu_features_lacks() {
        let p = Predicate::parse("host.cpu.features LACKS avx").unwrap();
        assert!(p.check(&test_host())); // host only has sse4_2
    }

    #[test]
    fn eval_missing_fact_is_false() {
        let empty = HostFacts::default();
        let p = Predicate::parse("host.architecture IS x86_64").unwrap();
        assert!(!p.check(&empty));
    }

    // ── check_all ──────────────────────────────────────────────────

    #[test]
    fn check_all_and_semantics() {
        let predicates = vec![
            Predicate::parse("host.ai.runtime LACKS cuda").unwrap(),
            Predicate::parse("host.ai.runtime HAS rocm").unwrap(),
        ];
        assert!(check_all(&predicates, &test_host()));
    }

    #[test]
    fn check_all_short_circuits() {
        let predicates = vec![
            Predicate::parse("host.ai.runtime HAS cuda").unwrap(), // false
            Predicate::parse("host.ai.runtime HAS rocm").unwrap(), // true
        ];
        assert!(!check_all(&predicates, &test_host()));
    }

    #[test]
    fn check_all_empty_is_true() {
        assert!(check_all(&[], &test_host()));
    }

    // ── Display roundtrip ──────────────────────────────────────────

    #[test]
    fn display_roundtrip() {
        let cases = [
            "host.ai.runtime HAS cuda",
            "host.ai.runtime LACKS cuda,rocm",
            "host.ram.total.mb < 8192",
            "host.architecture IS armv6l",
            "host.architecture IN (armv7l,armv6l)",
            "host.os.family NOT IN (linux,macos)",
            "host.gpu IS present",
            "host.npu IS NOT present",
        ];
        for input in cases {
            let p = Predicate::parse(input).unwrap();
            let displayed = p.to_string();
            let reparsed = Predicate::parse(&displayed).unwrap();
            assert_eq!(p, reparsed, "Roundtrip failed: '{}' → '{}'", input, displayed);
        }
    }
}
