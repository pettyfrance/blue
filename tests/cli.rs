use std::path::Path;
use std::process::Command;

fn blue() -> Command {
    Command::new(env!("CARGO_BIN_EXE_blue"))
}

#[test]
fn help_lists_validate_command() {
    let output = blue().arg("--help").output().unwrap();
    assert!(output.status.success());
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .contains("validate")
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn validate_reports_scope_and_block_counts_only_after_graph_validation() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/toml/valid-graph.toml");
    let output = blue().arg("validate").arg(&path).output().unwrap();
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        format!(
            "Validated declarations and dependencies in {}: 1 providers, 1 parameters, 1 resources, 0 data blocks\nProvider implementations loaded. Provider schemas and authentication were not checked.\n",
            path.display()
        )
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn provider_loading_failures_report_all_locations_without_success_output() {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/toml/provider-load-errors.toml");
    let output = blue().arg("validate").arg(&path).output().unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let errors = String::from_utf8(output.stderr).unwrap();
    assert_eq!(errors.lines().count(), 2);
    assert!(
        errors.contains(&format!(
            "{}:2:9: unknown built-in provider type `unknown` for `alpha`",
            path.display()
        )),
        "{errors}"
    );
    assert!(
        errors.contains(&format!(
            "{}:5:11: unsupported source for provider `beta`",
            path.display()
        )),
        "{errors}"
    );
    assert!(!errors.contains("fake-secret"));
    assert!(!errors.contains("SourceSpan"));
}

#[test]
fn missing_provider_errors_are_all_reported_with_locations() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/toml/example.toml");
    let output = blue().arg("validate").arg(&path).output().unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let errors = String::from_utf8(output.stderr).unwrap();
    assert!(
        errors.contains(&format!(
            "{}:7:1: no provider is configured for `resource.database.main`",
            path.display()
        )),
        "{errors}"
    );
    assert!(
        errors.contains(&format!(
            "{}:11:1: no provider is configured for `resource.server.main`",
            path.display()
        )),
        "{errors}"
    );
    assert!(!errors.contains("SourceSpan"));
}

#[test]
fn missing_reference_errors_are_all_reported_with_locations() {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/toml/missing-references.toml");
    let output = blue().arg("validate").arg(&path).output().unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let errors = String::from_utf8(output.stderr).unwrap();
    assert_eq!(errors.lines().count(), 2);
    assert!(
        errors.contains(&format!(
            "{}:2:16: reference `parameter.alpha`",
            path.display()
        )),
        "{errors}"
    );
    assert!(
        errors.contains(&format!(
            "{}:2:41: reference `parameter.beta`",
            path.display()
        )),
        "{errors}"
    );
    assert!(!errors.contains("SourceSpan"));
}

#[test]
fn cycles_report_complete_paths_at_file_level() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/toml/cycle.toml");
    let output = blue().arg("validate").arg(&path).output().unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let errors = String::from_utf8(output.stderr).unwrap();
    assert!(errors.starts_with(&format!("error: {}: dependency cycle: ", path.display())));
    let cycle = errors.split("dependency cycle: ").nth(1).unwrap().trim();
    let nodes = cycle.split(" → ").collect::<Vec<_>>();
    assert_eq!(nodes.len(), 3);
    assert_eq!(nodes.first(), nodes.last());
    assert!(nodes.contains(&"parameter.a"));
    assert!(nodes.contains(&"parameter.b"));
}

#[test]
fn conflicting_defaults_report_related_locations() {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/toml/multiple-defaults.toml");
    let output = blue().arg("validate").arg(&path).output().unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let errors = String::from_utf8(output.stderr).unwrap();
    assert!(
        errors.contains(&format!(
            "{}:3:11: multiple providers are marked default: provider.alpha, provider.beta",
            path.display()
        )),
        "{errors}"
    );
    assert!(
        errors.contains(&format!(
            "note: {}:6:11: `provider.beta` is also marked default",
            path.display()
        )),
        "{errors}"
    );
    assert!(!errors.contains("SourceSpan"));
}

#[test]
fn syntax_failure_reports_file_and_eof_location() {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/toml/invalid-syntax.toml");
    let output = blue().arg("validate").arg(&path).output().unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let error = String::from_utf8(output.stderr).unwrap();
    assert!(
        error.starts_with(&format!(
            "error: {}:1:16: failed to parse TOML syntax:",
            path.display()
        )),
        "{error}"
    );
}

#[test]
fn expression_failure_reports_character_column_after_unicode() {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/toml/invalid-expression.toml");
    let output = blue().arg("validate").arg(&path).output().unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let error = String::from_utf8(output.stderr).unwrap();
    assert!(
        error.starts_with(&format!(
            "error: {}:2:19: failed to parse expression:",
            path.display()
        )),
        "{error}"
    );
}

#[test]
fn missing_file_reports_path_and_exits_unsuccessfully() {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/toml/example.toml/missing.toml");
    let output = blue().arg("validate").arg(&path).output().unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let error = String::from_utf8(output.stderr).unwrap();
    assert!(error.contains("failed to read"));
    assert!(error.contains(&path.display().to_string()));
}

#[test]
fn missing_config_argument_exits_unsuccessfully() {
    let output = blue().arg("validate").output().unwrap();
    assert!(!output.status.success());
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("<CONFIG>")
    );
}
