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

use super::facts::FactSource;
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

    /// Evaluate this predicate against a fact source.
    ///
    /// Returns `false` if the referenced fact is missing/None.
    pub fn check(&self, source: &dyn FactSource) -> bool {
        match &self.condition {
            // ── Set operators ──────────────────────────────────────
            Condition::Has(vals) => {
                let set = source.resolve_set(self.fact);
                vals.iter().any(|v| set.contains(v))
            }
            Condition::HasAll(vals) => {
                let set = source.resolve_set(self.fact);
                vals.iter().all(|v| set.contains(v))
            }
            Condition::Lacks(vals) => {
                let set = source.resolve_set(self.fact);
                vals.iter().all(|v| !set.contains(v))
            }

            // ── Scalar operators ───────────────────────────────────
            Condition::Is(val) => source
                .resolve_scalar(self.fact)
                .map(|s| s.eq_ignore_ascii_case(val))
                .unwrap_or(false),
            Condition::IsNot(val) => source
                .resolve_scalar(self.fact)
                .map(|s| !s.eq_ignore_ascii_case(val))
                .unwrap_or(false),
            Condition::In(vals) => source
                .resolve_scalar(self.fact)
                .map(|s| vals.iter().any(|v| s.eq_ignore_ascii_case(v)))
                .unwrap_or(false),
            Condition::NotIn(vals) => source
                .resolve_scalar(self.fact)
                .map(|s| vals.iter().all(|v| !s.eq_ignore_ascii_case(v)))
                .unwrap_or(false),

            // ── Boolean operators ──────────────────────────────────
            Condition::Present(expected) => source.resolve_bool(self.fact) == *expected,

            // ── Numeric operators ──────────────────────────────────
            Condition::Cmp { op, value } => {
                let actual = source.resolve_numeric(self.fact);
                match op {
                    CmpOp::Gte => actual >= *value,
                    CmpOp::Gt => actual > *value,
                    CmpOp::Lte => actual <= *value,
                    CmpOp::Lt => actual < *value,
                }
            }
        }
    }
}

/// Evaluate all predicates against a fact source (AND semantics).
///
/// Short-circuits on the first `false`.
pub fn check_all(predicates: &[Predicate], source: &dyn FactSource) -> bool {
    predicates.iter().all(|p| p.check(source))
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

    /// Test fact source — lightweight stand-in for HardwareCapabilities.
    #[derive(Default, Clone)]
    struct TestFacts {
        architecture: Option<String>,
        os_family: Option<String>,
        cpu_model: Option<String>,
        cpu_patterns: HashSet<String>,
        cpu_features: HashSet<String>,
        ram_total_mb: u64,
        gpu_present: bool,
        gpu_count: u32,
        gpu_vram_total_mb: u64,
        npu_present: bool,
        runtime_capabilities: HashSet<String>,
    }

    impl FactSource for TestFacts {
        fn resolve_set(&self, fact: Fact) -> HashSet<String> {
            match fact {
                Fact::AiRuntime => self.runtime_capabilities.clone(),
                Fact::CpuFeatures => self.cpu_features.clone(),
                Fact::CpuPattern => self.cpu_patterns.clone(),
                _ => HashSet::new(),
            }
        }
        fn resolve_scalar(&self, fact: Fact) -> Option<String> {
            match fact {
                Fact::Architecture => self.architecture.clone(),
                Fact::OsFamily => self.os_family.clone(),
                Fact::CpuModel => self.cpu_model.clone(),
                _ => None,
            }
        }
        fn resolve_numeric(&self, fact: Fact) -> f64 {
            match fact {
                Fact::RamTotalMb => self.ram_total_mb as f64,
                Fact::GpuCount => self.gpu_count as f64,
                Fact::GpuVramTotalMb => self.gpu_vram_total_mb as f64,
                Fact::GpuVramTotalGb => self.gpu_vram_total_mb as f64 / 1024.0,
                _ => 0.0,
            }
        }
        fn resolve_bool(&self, fact: Fact) -> bool {
            match fact {
                Fact::Gpu => self.gpu_present,
                Fact::Npu => self.npu_present,
                _ => false,
            }
        }
    }

    fn test_host() -> TestFacts {
        TestFacts {
            architecture: Some("x86_64".into()),
            os_family: Some("linux".into()),
            cpu_model: Some("Intel Celeron J4105".into()),
            cpu_patterns: HashSet::from(["j4105".into()]),
            cpu_features: HashSet::from(["sse4_2".into()]),
            ram_total_mb: 8192,
            gpu_present: true,
            gpu_count: 1,
            gpu_vram_total_mb: 6144,
            npu_present: false,
            runtime_capabilities: HashSet::from(["rocm".into()]),
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
        let empty = TestFacts::default();
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

    // ================================================================
    // Extensive test suite
    // ================================================================

    // ── Case insensitivity (operators + facts) ─────────────────────

    #[test]
    fn parse_operator_lowercase() {
        let p = Predicate::parse("host.ai.runtime has cuda").unwrap();
        assert_eq!(p.condition, Condition::Has(vec!["cuda".into()]));
    }

    #[test]
    fn parse_operator_mixed_case() {
        let p = Predicate::parse("host.ai.runtime Has cuda").unwrap();
        assert_eq!(p.condition, Condition::Has(vec!["cuda".into()]));
    }

    #[test]
    fn parse_operator_lacks_lowercase() {
        let p = Predicate::parse("host.ai.runtime lacks cuda").unwrap();
        assert_eq!(p.condition, Condition::Lacks(vec!["cuda".into()]));
    }

    #[test]
    fn parse_operator_is_not_mixed_case() {
        let p = Predicate::parse("host.architecture Is Not armv7l").unwrap();
        assert_eq!(p.condition, Condition::IsNot("armv7l".into()));
    }

    #[test]
    fn parse_operator_not_in_lowercase() {
        let p = Predicate::parse("host.os.family not in (linux,macos)").unwrap();
        assert_eq!(
            p.condition,
            Condition::NotIn(vec!["linux".into(), "macos".into()])
        );
    }

    #[test]
    fn parse_fact_mixed_case() {
        let p = Predicate::parse("HOST.AI.RUNTIME HAS cuda").unwrap();
        assert_eq!(p.fact, Fact::AiRuntime);
    }

    #[test]
    fn parse_fact_camel_case() {
        let p = Predicate::parse("Host.Ai.Runtime HAS cuda").unwrap();
        assert_eq!(p.fact, Fact::AiRuntime);
    }

    // ── Values are case-sensitive ──────────────────────────────────

    #[test]
    fn values_stored_lowercase() {
        let p = Predicate::parse("host.ai.runtime HAS CUDA").unwrap();
        // Values are lowercased during parse
        assert_eq!(p.condition, Condition::Has(vec!["cuda".into()]));
    }

    // ── Whitespace handling ────────────────────────────────────────

    #[test]
    fn parse_leading_trailing_whitespace() {
        let p = Predicate::parse("  host.ai.runtime HAS cuda  ").unwrap();
        assert_eq!(p.condition, Condition::Has(vec!["cuda".into()]));
    }

    #[test]
    fn parse_extra_internal_whitespace() {
        let p = Predicate::parse("host.ai.runtime    HAS    cuda").unwrap();
        assert_eq!(p.condition, Condition::Has(vec!["cuda".into()]));
    }

    #[test]
    fn parse_spaces_in_parens() {
        let p = Predicate::parse("host.architecture IN ( armv7l , armv6l )").unwrap();
        assert_eq!(
            p.condition,
            Condition::In(vec!["armv7l".into(), "armv6l".into()])
        );
    }

    // ── Empty / malformed input ────────────────────────────────────

    #[test]
    fn reject_empty_string() {
        let err = Predicate::parse("").unwrap_err();
        assert!(err.message.contains("Empty predicate"));
    }

    #[test]
    fn reject_whitespace_only() {
        let err = Predicate::parse("   ").unwrap_err();
        assert!(err.message.contains("Empty predicate"));
    }

    #[test]
    fn reject_fact_only() {
        let err = Predicate::parse("host.architecture").unwrap_err();
        assert!(err.message.contains("Expected operator"));
    }

    #[test]
    fn reject_fact_operator_no_value() {
        let err = Predicate::parse("host.ai.runtime HAS").unwrap_err();
        assert!(err.message.contains("Expected value"));
    }

    #[test]
    fn reject_is_no_value() {
        let err = Predicate::parse("host.architecture IS").unwrap_err();
        assert!(err.message.contains("Expected value"));
    }

    #[test]
    fn reject_numeric_no_value() {
        let err = Predicate::parse("host.ram.total.mb <").unwrap_err();
        assert!(err.message.contains("Expected numeric"));
    }

    #[test]
    fn reject_numeric_non_number() {
        let err = Predicate::parse("host.ram.total.mb < abc").unwrap_err();
        assert!(err.message.contains("not a valid number"));
    }

    #[test]
    fn reject_unclosed_paren() {
        let err = Predicate::parse("host.architecture IN (armv7l,armv6l").unwrap_err();
        assert!(err.message.contains("Unclosed parenthesis"));
    }

    #[test]
    fn reject_not_without_in() {
        let err = Predicate::parse("host.os.family NOT AROUND linux").unwrap_err();
        assert!(err.message.contains("Did you mean 'NOT IN'"));
    }

    #[test]
    fn reject_is_not_no_value() {
        let err = Predicate::parse("host.architecture IS NOT").unwrap_err();
        assert!(err.message.contains("Expected value"));
    }

    // ── Every fact name resolves ───────────────────────────────────

    #[test]
    fn parse_all_facts() {
        let cases = [
            ("host.architecture IS x86_64", Fact::Architecture),
            ("host.os.family IS linux", Fact::OsFamily),
            ("host.cpu.model IS test", Fact::CpuModel),
            ("host.cpu.pattern HAS j4105", Fact::CpuPattern),
            ("host.cpu.features HAS avx", Fact::CpuFeatures),
            ("host.ram.total.mb < 1024", Fact::RamTotalMb),
            ("host.gpu IS present", Fact::Gpu),
            ("host.gpu.count >= 1", Fact::GpuCount),
            ("host.gpu.vram.total.mb >= 4096", Fact::GpuVramTotalMb),
            ("host.gpu.vram.total.gb >= 4", Fact::GpuVramTotalGb),
            ("host.npu IS present", Fact::Npu),
            ("host.ai.runtime HAS cuda", Fact::AiRuntime),
        ];
        for (input, expected_fact) in cases {
            let p = Predicate::parse(input).unwrap();
            assert_eq!(p.fact, expected_fact, "Failed for: {}", input);
        }
    }

    // ── Type enforcement: every invalid combo ──────────────────────

    #[test]
    fn reject_has_on_scalar() {
        let err = Predicate::parse("host.architecture HAS x86_64").unwrap_err();
        assert!(err.message.contains("Type mismatch"), "{}", err.message);
    }

    #[test]
    fn reject_has_on_boolean() {
        let err = Predicate::parse("host.gpu HAS true").unwrap_err();
        assert!(err.message.contains("Type mismatch"), "{}", err.message);
    }

    #[test]
    fn reject_lacks_on_scalar() {
        let err = Predicate::parse("host.architecture LACKS x86_64").unwrap_err();
        assert!(err.message.contains("Type mismatch"), "{}", err.message);
    }

    #[test]
    fn reject_lacks_on_numeric() {
        let err = Predicate::parse("host.ram.total.mb LACKS 4096").unwrap_err();
        assert!(err.message.contains("Type mismatch"), "{}", err.message);
    }

    #[test]
    fn reject_is_on_set() {
        let err = Predicate::parse("host.ai.runtime IS cuda").unwrap_err();
        assert!(err.message.contains("Type mismatch"), "{}", err.message);
    }

    #[test]
    fn reject_is_on_numeric() {
        let err = Predicate::parse("host.ram.total.mb IS 4096").unwrap_err();
        assert!(err.message.contains("Type mismatch"), "{}", err.message);
    }

    #[test]
    fn reject_in_on_set() {
        let err = Predicate::parse("host.ai.runtime IN (cuda,rocm)").unwrap_err();
        assert!(err.message.contains("Type mismatch"), "{}", err.message);
    }

    #[test]
    fn reject_in_on_numeric() {
        let err = Predicate::parse("host.ram.total.mb IN (4096,8192)").unwrap_err();
        assert!(err.message.contains("Type mismatch"), "{}", err.message);
    }

    #[test]
    fn reject_not_in_on_set() {
        let err = Predicate::parse("host.cpu.features NOT IN (avx,avx2)").unwrap_err();
        assert!(err.message.contains("Type mismatch"), "{}", err.message);
    }

    #[test]
    fn reject_present_on_scalar() {
        let err = Predicate::parse("host.architecture IS present").unwrap_err();
        assert!(err.message.contains("Type mismatch"), "{}", err.message);
    }

    #[test]
    fn reject_present_on_numeric() {
        let err = Predicate::parse("host.ram.total.mb IS present").unwrap_err();
        assert!(err.message.contains("Type mismatch"), "{}", err.message);
    }

    #[test]
    fn reject_present_on_set() {
        let err = Predicate::parse("host.ai.runtime IS present").unwrap_err();
        assert!(err.message.contains("Type mismatch"), "{}", err.message);
    }

    #[test]
    fn reject_gte_on_set() {
        let err = Predicate::parse("host.ai.runtime >= 5").unwrap_err();
        assert!(err.message.contains("Type mismatch"), "{}", err.message);
    }

    #[test]
    fn reject_gte_on_scalar() {
        let err = Predicate::parse("host.architecture >= 5").unwrap_err();
        assert!(err.message.contains("Type mismatch"), "{}", err.message);
    }

    #[test]
    fn reject_gte_on_boolean() {
        let err = Predicate::parse("host.gpu >= 1").unwrap_err();
        assert!(err.message.contains("Type mismatch"), "{}", err.message);
    }

    // ── Numeric operators: all four ────────────────────────────────

    #[test]
    fn parse_numeric_gt() {
        let p = Predicate::parse("host.ram.total.mb > 4096").unwrap();
        assert_eq!(
            p.condition,
            Condition::Cmp { op: CmpOp::Gt, value: 4096.0 }
        );
    }

    #[test]
    fn parse_numeric_lte() {
        let p = Predicate::parse("host.gpu.vram.total.mb <= 8192").unwrap();
        assert_eq!(
            p.condition,
            Condition::Cmp { op: CmpOp::Lte, value: 8192.0 }
        );
    }

    #[test]
    fn eval_numeric_gt_true() {
        let p = Predicate::parse("host.ram.total.mb > 4096").unwrap();
        assert!(p.check(&test_host())); // 8192 > 4096
    }

    #[test]
    fn eval_numeric_gt_boundary_false() {
        let p = Predicate::parse("host.ram.total.mb > 8192").unwrap();
        assert!(!p.check(&test_host())); // 8192 > 8192 is false
    }

    #[test]
    fn eval_numeric_gte_boundary_true() {
        let p = Predicate::parse("host.ram.total.mb >= 8192").unwrap();
        assert!(p.check(&test_host())); // 8192 >= 8192 is true
    }

    #[test]
    fn eval_numeric_lt_boundary_false() {
        let p = Predicate::parse("host.ram.total.mb < 8192").unwrap();
        assert!(!p.check(&test_host())); // 8192 < 8192 is false
    }

    #[test]
    fn eval_numeric_lte_boundary_true() {
        let p = Predicate::parse("host.ram.total.mb <= 8192").unwrap();
        assert!(p.check(&test_host())); // 8192 <= 8192 is true
    }

    #[test]
    fn eval_numeric_zero() {
        let empty = TestFacts::default();
        let p = Predicate::parse("host.ram.total.mb < 1").unwrap();
        // ram_total_mb is None → 0 < 1 is true
        assert!(p.check(&empty));
    }

    #[test]
    fn eval_gpu_count() {
        let p = Predicate::parse("host.gpu.count >= 1").unwrap();
        assert!(p.check(&test_host())); // gpu_count = 1
    }

    #[test]
    fn eval_vram_gb_derived() {
        // 6144 MB = 6.0 GB
        let p = Predicate::parse("host.gpu.vram.total.gb >= 6").unwrap();
        assert!(p.check(&test_host()));
        let p2 = Predicate::parse("host.gpu.vram.total.gb >= 7").unwrap();
        assert!(!p2.check(&test_host()));
    }

    // ── Set operations: thorough ───────────────────────────────────

    #[test]
    fn eval_has_any_first_match() {
        let host = TestFacts {
            runtime_capabilities: HashSet::from(["cuda".into()]),
            ..Default::default()
        };
        let p = Predicate::parse("host.ai.runtime HAS cuda,rocm").unwrap();
        assert!(p.check(&host)); // has cuda (first)
    }

    #[test]
    fn eval_has_any_second_match() {
        let host = TestFacts {
            runtime_capabilities: HashSet::from(["rocm".into()]),
            ..Default::default()
        };
        let p = Predicate::parse("host.ai.runtime HAS cuda,rocm").unwrap();
        assert!(p.check(&host)); // has rocm (second)
    }

    #[test]
    fn eval_has_any_none_match() {
        let host = TestFacts {
            runtime_capabilities: HashSet::from(["directml".into()]),
            ..Default::default()
        };
        let p = Predicate::parse("host.ai.runtime HAS cuda,rocm").unwrap();
        assert!(!p.check(&host)); // has neither
    }

    #[test]
    fn eval_has_all_true() {
        let host = TestFacts {
            runtime_capabilities: HashSet::from(["cuda".into(), "rocm".into(), "openvino".into()]),
            ..Default::default()
        };
        let p = Predicate::parse("host.ai.runtime HAS cuda AND rocm").unwrap();
        assert!(p.check(&host));
    }

    #[test]
    fn eval_has_all_partial() {
        let host = TestFacts {
            runtime_capabilities: HashSet::from(["cuda".into()]),
            ..Default::default()
        };
        let p = Predicate::parse("host.ai.runtime HAS cuda AND rocm").unwrap();
        assert!(!p.check(&host)); // has cuda but not rocm
    }

    #[test]
    fn eval_lacks_multi_all_absent() {
        let host = TestFacts {
            runtime_capabilities: HashSet::from(["directml".into()]),
            ..Default::default()
        };
        let p = Predicate::parse("host.ai.runtime LACKS cuda,rocm").unwrap();
        assert!(p.check(&host)); // has neither cuda nor rocm
    }

    #[test]
    fn eval_lacks_multi_one_present() {
        let host = TestFacts {
            runtime_capabilities: HashSet::from(["cuda".into()]),
            ..Default::default()
        };
        let p = Predicate::parse("host.ai.runtime LACKS cuda,rocm").unwrap();
        assert!(!p.check(&host)); // has cuda
    }

    #[test]
    fn eval_lacks_empty_set() {
        let host = TestFacts::default();
        let p = Predicate::parse("host.ai.runtime LACKS cuda").unwrap();
        assert!(p.check(&host)); // empty set lacks everything
    }

    #[test]
    fn eval_has_empty_set() {
        let host = TestFacts::default();
        let p = Predicate::parse("host.ai.runtime HAS cuda").unwrap();
        assert!(!p.check(&host)); // empty set has nothing
    }

    // ── Scalar IS / IS NOT ─────────────────────────────────────────

    #[test]
    fn eval_is_true() {
        let p = Predicate::parse("host.architecture IS x86_64").unwrap();
        assert!(p.check(&test_host()));
    }

    #[test]
    fn eval_is_false() {
        let p = Predicate::parse("host.architecture IS aarch64").unwrap();
        assert!(!p.check(&test_host()));
    }

    #[test]
    fn eval_is_case_insensitive() {
        let p = Predicate::parse("host.architecture IS X86_64").unwrap();
        assert!(p.check(&test_host())); // scalar comparison is case-insensitive
    }

    #[test]
    fn eval_is_not_true() {
        let p = Predicate::parse("host.architecture IS NOT aarch64").unwrap();
        assert!(p.check(&test_host()));
    }

    #[test]
    fn eval_is_not_false() {
        let p = Predicate::parse("host.architecture IS NOT x86_64").unwrap();
        assert!(!p.check(&test_host()));
    }

    #[test]
    fn eval_is_missing_fact() {
        let empty = TestFacts::default();
        let p = Predicate::parse("host.os.family IS linux").unwrap();
        assert!(!p.check(&empty));
    }

    #[test]
    fn eval_is_not_missing_fact() {
        let empty = TestFacts::default();
        let p = Predicate::parse("host.os.family IS NOT linux").unwrap();
        // Missing fact → false (not "not equals linux")
        assert!(!p.check(&empty));
    }

    // ── NOT IN ─────────────────────────────────────────────────────

    #[test]
    fn eval_not_in_true() {
        let p = Predicate::parse("host.os.family NOT IN (windows,macos)").unwrap();
        assert!(p.check(&test_host())); // linux not in list
    }

    #[test]
    fn eval_not_in_false() {
        let p = Predicate::parse("host.os.family NOT IN (linux,macos)").unwrap();
        assert!(!p.check(&test_host())); // linux IS in list
    }

    #[test]
    fn eval_not_in_missing_fact() {
        let empty = TestFacts::default();
        let p = Predicate::parse("host.os.family NOT IN (linux,macos)").unwrap();
        assert!(!p.check(&empty)); // missing → false
    }

    // ── Boolean facts ──────────────────────────────────────────────

    #[test]
    fn eval_gpu_present_true() {
        let p = Predicate::parse("host.gpu IS present").unwrap();
        assert!(p.check(&test_host())); // gpu_present = true
    }

    #[test]
    fn eval_gpu_present_false() {
        let host = TestFacts {
            gpu_present: false,
            ..Default::default()
        };
        let p = Predicate::parse("host.gpu IS present").unwrap();
        assert!(!p.check(&host));
    }

    #[test]
    fn eval_gpu_not_present_true() {
        let host = TestFacts {
            gpu_present: false,
            ..Default::default()
        };
        let p = Predicate::parse("host.gpu IS NOT present").unwrap();
        assert!(p.check(&host));
    }

    #[test]
    fn eval_npu_present_false_default() {
        let p = Predicate::parse("host.npu IS present").unwrap();
        assert!(!p.check(&test_host())); // npu_present = false
    }

    // ── cpu.pattern fact ───────────────────────────────────────────

    #[test]
    fn eval_cpu_pattern_no_match() {
        let p = Predicate::parse("host.cpu.pattern HAS j3455,n4100").unwrap();
        assert!(!p.check(&test_host())); // host only has j4105
    }

    #[test]
    fn eval_cpu_pattern_lacks_true() {
        let p = Predicate::parse("host.cpu.pattern LACKS j3455,n4100").unwrap();
        assert!(p.check(&test_host()));
    }

    #[test]
    fn eval_cpu_pattern_lacks_false() {
        let p = Predicate::parse("host.cpu.pattern LACKS j4105").unwrap();
        assert!(!p.check(&test_host())); // host has j4105
    }

    // ── cpu.features fact ──────────────────────────────────────────

    #[test]
    fn eval_cpu_features_has_true() {
        let p = Predicate::parse("host.cpu.features HAS sse4_2").unwrap();
        assert!(p.check(&test_host()));
    }

    #[test]
    fn eval_cpu_features_has_false() {
        let p = Predicate::parse("host.cpu.features HAS avx").unwrap();
        assert!(!p.check(&test_host())); // host only has sse4_2
    }

    #[test]
    fn eval_cpu_features_lacks_multi() {
        let p = Predicate::parse("host.cpu.features LACKS avx,avx2,avx512").unwrap();
        assert!(p.check(&test_host())); // host has none of these
    }

    #[test]
    fn eval_cpu_features_lacks_multi_one_present() {
        let p = Predicate::parse("host.cpu.features LACKS sse4_2,avx").unwrap();
        assert!(!p.check(&test_host())); // host has sse4_2
    }

    // ── check_all: real-world rule combinations ────────────────────

    #[test]
    fn check_all_comfyui_rocm_fallback() {
        // ComfyUI no-nvidia-use-rocm rule
        let predicates = vec![
            Predicate::parse("host.ai.runtime LACKS cuda").unwrap(),
            Predicate::parse("host.ai.runtime HAS rocm").unwrap(),
        ];
        // AMD-only stone
        let host = TestFacts {
            runtime_capabilities: HashSet::from(["rocm".into()]),
            ..Default::default()
        };
        assert!(check_all(&predicates, &host));

        // NVIDIA stone — should NOT match
        let nvidia = TestFacts {
            runtime_capabilities: HashSet::from(["cuda".into()]),
            ..Default::default()
        };
        assert!(!check_all(&predicates, &nvidia));

        // No GPU stone — should NOT match (has neither)
        let no_gpu = TestFacts::default();
        assert!(!check_all(&predicates, &no_gpu));
    }

    #[test]
    fn check_all_comfyui_cpu_fallback() {
        // ComfyUI no-gpu-use-cpu rule
        let predicates = vec![
            Predicate::parse("host.ai.runtime LACKS cuda").unwrap(),
        ];
        let no_gpu = TestFacts::default();
        assert!(check_all(&predicates, &no_gpu));

        // NVIDIA stone — should NOT match
        let nvidia = TestFacts {
            runtime_capabilities: HashSet::from(["cuda".into()]),
            ..Default::default()
        };
        assert!(!check_all(&predicates, &nvidia));
    }

    #[test]
    fn check_all_mongodb_celeron_fallback() {
        let predicates = vec![
            Predicate::parse("host.cpu.pattern HAS j4105,j3455,j3160").unwrap(),
        ];
        let celeron = TestFacts {
            cpu_patterns: HashSet::from(["j4105".into()]),
            ..Default::default()
        };
        assert!(check_all(&predicates, &celeron));

        let ryzen = TestFacts {
            cpu_patterns: HashSet::from(["ryzen9".into()]),
            ..Default::default()
        };
        assert!(!check_all(&predicates, &ryzen));
    }

    #[test]
    fn check_all_milvus_simd_check() {
        // Milvus x86_64-missing-simd rule: x86_64 AND lacks all SIMD
        let predicates = vec![
            Predicate::parse("host.architecture IS x86_64").unwrap(),
            Predicate::parse("host.cpu.features LACKS sse4_2,avx,avx2,avx512").unwrap(),
        ];
        // Old x86_64 without any SIMD
        let old_x86 = TestFacts {
            architecture: Some("x86_64".into()),
            cpu_features: HashSet::from(["sse2".into()]),
            ..Default::default()
        };
        assert!(check_all(&predicates, &old_x86));

        // x86_64 with AVX — should NOT match
        let modern_x86 = TestFacts {
            architecture: Some("x86_64".into()),
            cpu_features: HashSet::from(["sse4_2".into(), "avx".into(), "avx2".into()]),
            ..Default::default()
        };
        assert!(!check_all(&predicates, &modern_x86));

        // ARM — should NOT match (architecture doesn't match)
        let arm = TestFacts {
            architecture: Some("aarch64".into()),
            cpu_features: HashSet::new(),
            ..Default::default()
        };
        assert!(!check_all(&predicates, &arm));
    }

    #[test]
    fn check_all_pihole_windows_reject() {
        let predicates = vec![
            Predicate::parse("host.os.family NOT IN (linux,macos)").unwrap(),
        ];
        let windows = TestFacts {
            os_family: Some("windows".into()),
            ..Default::default()
        };
        assert!(check_all(&predicates, &windows));

        let linux = TestFacts {
            os_family: Some("linux".into()),
            ..Default::default()
        };
        assert!(!check_all(&predicates, &linux));
    }

    #[test]
    fn check_all_ollama_cpu_gpu_present_reject() {
        let predicates = vec![
            Predicate::parse("host.gpu IS present").unwrap(),
        ];
        let gpu_stone = TestFacts {
            gpu_present: true,
            ..Default::default()
        };
        assert!(check_all(&predicates, &gpu_stone));

        let cpu_stone = TestFacts {
            gpu_present: false,
            ..Default::default()
        };
        assert!(!check_all(&predicates, &cpu_stone));
    }

    #[test]
    fn check_all_memory_and_architecture() {
        // Combined: arm32 AND low memory
        let predicates = vec![
            Predicate::parse("host.architecture IN (armv7l,armv6l)").unwrap(),
            Predicate::parse("host.ram.total.mb < 512").unwrap(),
        ];
        let tiny_arm = TestFacts {
            architecture: Some("armv7l".into()),
            ram_total_mb: 256,
            ..Default::default()
        };
        assert!(check_all(&predicates, &tiny_arm));

        // Enough memory — fails second predicate
        let big_arm = TestFacts {
            architecture: Some("armv7l".into()),
            ram_total_mb: 1024,
            ..Default::default()
        };
        assert!(!check_all(&predicates, &big_arm));
    }

    // ── Parse: every actual manifest predicate ─────────────────────

    #[test]
    fn parse_all_manifest_predicates() {
        // Every unique predicate from the 39 migrated manifests
        let predicates = [
            "host.ai.runtime LACKS cuda",
            "host.ai.runtime LACKS cuda,rocm,metal",
            "host.ai.runtime HAS rocm",
            "host.gpu IS present",
            "host.gpu.vram.total.mb < 1024",
            "host.gpu.vram.total.mb < 2048",
            "host.gpu.vram.total.mb < 4096",
            "host.ram.total.mb < 64",
            "host.ram.total.mb < 128",
            "host.ram.total.mb < 256",
            "host.ram.total.mb < 512",
            "host.ram.total.mb < 1024",
            "host.ram.total.mb < 2048",
            "host.ram.total.mb < 4096",
            "host.ram.total.mb < 8192",
            "host.ram.total.mb < 16384",
            "host.architecture IS armv6l",
            "host.architecture IS armv7l",
            "host.architecture IS aarch64",
            "host.architecture IS x86_64",
            "host.architecture IN (armv7l,armv6l)",
            "host.architecture IN (aarch64,arm64,armv7l,armv6l)",
            "host.os.family NOT IN (linux,macos)",
            "host.cpu.pattern HAS j4105,j3455,j3160",
            "host.cpu.pattern HAS j4105,j3455,j3160,j4005,n4100,n5000",
            "host.cpu.pattern HAS j4105,j3455,j3160,j4005,j5005,n4100,n5000",
            "host.cpu.features LACKS avx",
            "host.cpu.features LACKS sse4_2",
            "host.cpu.features LACKS sse4_2,avx,avx2,avx512",
        ];
        for input in predicates {
            let result = Predicate::parse(input);
            assert!(result.is_ok(), "Failed to parse '{}': {}", input, result.unwrap_err());
        }
    }

    // ── Error message quality ──────────────────────────────────────

    #[test]
    fn error_suggests_valid_operators() {
        let err = Predicate::parse("host.ai.runtime CONTAINS cuda").unwrap_err();
        assert!(err.message.contains("HAS"), "Should suggest HAS: {}", err.message);
        assert!(err.message.contains("LACKS"), "Should suggest LACKS: {}", err.message);
    }

    #[test]
    fn error_suggests_valid_facts() {
        let err = Predicate::parse("host.ai.gpu_type HAS nvidia").unwrap_err();
        assert!(err.message.contains("host.ai.runtime"), "Should suggest host.ai.runtime: {}", err.message);
    }

    #[test]
    fn error_type_mismatch_shows_valid_ops() {
        let err = Predicate::parse("host.ram.total.mb HAS 4096").unwrap_err();
        assert!(err.message.contains(">="), "Should suggest >=: {}", err.message);
        assert!(err.message.contains("<"), "Should suggest <: {}", err.message);
    }

    #[test]
    fn error_has_position() {
        let err = Predicate::parse("host.ai.runtime NOPE cuda").unwrap_err();
        assert!(err.position.is_some(), "Should have position");
    }

    #[test]
    fn error_has_input() {
        let err = Predicate::parse("host.ai.runtime NOPE cuda").unwrap_err();
        assert_eq!(err.input, "host.ai.runtime NOPE cuda");
    }
}
