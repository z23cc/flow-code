//! trycmd integration tests for flowctl CLI.
//!
//! Each `.toml` file under `tests/cmd/` defines a CLI invocation and its
//! expected stdout, stderr, and exit code.  trycmd runs the real binary
//! and diffs the output.

#[test]
fn cli_tests() {
    let t = trycmd::TestCases::new();
    t.case("../../tests/cmd/*.toml");
}
