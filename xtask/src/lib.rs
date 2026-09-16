//! Penguin Redis — `xtask`: the Phase 0 measurement and fault-injection harnesses.
//!
//! A library as well as a binary, so that product tests and `xtask` use **the same
//! instrument**. Two counting allocators that agree today and drift tomorrow is how a budget
//! quietly stops being enforced, and §31.2's objection to two subtly different implementations
//! applies to measuring tools as much as to decoders.

pub mod catalog;
pub mod fault;
pub mod measure;
