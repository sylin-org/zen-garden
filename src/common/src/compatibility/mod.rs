//! Hardware Compatibility Predicate DSL (COMPAT-0002)
//!
//! String-based predicate language for hardware compatibility rules.
//!
//! ```text
//! host.ai.runtime LACKS cuda
//! host.ram.total.mb < 8192
//! host.architecture IN (armv7l,armv6l)
//! host.gpu IS present
//! ```
//!
//! # Usage
//! ```rust,ignore
//! use garden_common::compatibility::{Predicate, FactSource};
//!
//! let p = Predicate::parse("host.ai.runtime LACKS cuda")?;
//! assert!(p.check(&caps)); // caps: &dyn FactSource
//! ```

mod facts;
mod predicate;

pub use facts::FactSource;
pub use predicate::{check_all, CmpOp, Condition, Fact, FactType, Predicate, PredicateError};
