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
//! use garden_common::compatibility::{Predicate, HostFacts};
//!
//! let p = Predicate::parse("host.ai.runtime LACKS cuda")?;
//! let facts = HostFacts::from_capabilities(&caps);
//! assert!(p.check(&facts));
//! ```

mod facts;
mod predicate;

pub use facts::HostFacts;
pub use predicate::{check_all, CmpOp, Condition, Fact, FactType, Predicate, PredicateError};
