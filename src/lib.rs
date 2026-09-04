//! thefuzz — fuzzy string matching in Rust.
//!
//! A faithful port of the Python `thefuzz` library (which delegates to
//! RapidFuzz). Provides the `fuzz` scorers, the `process` extraction helpers,
//! and the `utils` preprocessing functions.

pub mod distance;
pub mod fuzz;
pub mod process;
pub mod utils;

/// Library version, matching the Python package `__version__`.
pub const VERSION: &str = "0.22.1";
