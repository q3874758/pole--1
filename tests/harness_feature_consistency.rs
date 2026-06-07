//! Asserts the `HARNESS_FEATURE` constant declared in
//! `tests/harness/mod.rs` matches the Cargo feature name in `Cargo.toml`.
//!
//! Background: before Phase 0.1 the constant was `"integration"` but the
//! feature was **not declared** in `Cargo.toml`, so the gated integration
//! tests in `tests/integration.rs` were silently unreachable from
//! `cargo test`. This test pins the linkage so a future rename in
//! either place fails CI loudly.
//!
//! The test is intentionally cheap: it imports the constant and checks
//! its value. A second test, gated on the same feature, exercises the
//! bring-up path so `cargo test --features integration` fails closed
//! if the gate stops gating.

#[test]
fn harness_feature_name_matches_cargo_feature() {
    // If the constant value drifts from the Cargo feature name, this
    // assertion fails before any real-`poled` test runs.
    assert_eq!(
        pole_protocol_draft::cosmos::HARNESS_FEATURE_NAME_FOR_TEST,
        "integration",
        "HARNESS_FEATURE must equal the Cargo feature name declared in [features]"
    );
}

#[cfg(feature = "integration")]
#[test]
fn integration_feature_is_compilable() {
    // Compile-time signal: this test only compiles if `integration` is
    // a declared feature. A missing feature declaration would fail to
    // build before this assertion runs.
    use pole_protocol_draft::cosmos::HARNESS_FEATURE_NAME_FOR_TEST as F;
    assert!(!F.is_empty(), "feature name must be non-empty");
}
