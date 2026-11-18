#[macro_export]
macro_rules! here {
    () => {
        concat!(module_path!(), ":", file!(), ":", line!())
    };
}

pub mod app;
pub mod config;
pub mod error;
pub mod option;

pub use error::{Code, Error, Result};

#[cfg(test)]
mod test_support;
