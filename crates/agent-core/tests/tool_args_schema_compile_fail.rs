#[test]
fn tool_args_schema_diagnostics_remain_clear() {
    let tests = trybuild::TestCases::new();
    tests.compile_fail("tests/ui/tool_args_schema/*.rs");
}
