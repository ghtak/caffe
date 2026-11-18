pub mod app;
pub mod config;
pub mod error;
pub mod option;

pub use error::{Code, Error, Result};

#[cfg(test)]
mod test_support;
