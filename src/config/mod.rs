//! Strong configuration validation backed by a JSON Schema.

pub mod validator;

pub use validator::{
    validate_config, validate_schema, validate_semantic, ConfigValidationError,
    ConfigValidatorError,
};
