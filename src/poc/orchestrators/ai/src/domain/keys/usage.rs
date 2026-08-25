//! Usage accounting — present on every primitive output with billable usage.

use crate::domain::field_path::FieldPath;

pub const TOKENS_INPUT: FieldPath = FieldPath::new("usage.tokens.input");
pub const TOKENS_OUTPUT: FieldPath = FieldPath::new("usage.tokens.output");
pub const TOKENS_TOTAL: FieldPath = FieldPath::new("usage.tokens.total");
pub const CHARACTERS: FieldPath = FieldPath::new("usage.characters");
pub const BYTES_INPUT: FieldPath = FieldPath::new("usage.bytes.input");
pub const BYTES_OUTPUT: FieldPath = FieldPath::new("usage.bytes.output");
pub const COST_USD: FieldPath = FieldPath::new("usage.cost_usd");
