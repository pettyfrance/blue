use std::error::Error;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};

use crate::config::{ConfigReader, SourceId, SourceMap, SourceSpan, TomlReader, TomlReaderError};
use crate::graph::{DesiredGraphBuilder, GraphBuildError};
use crate::provider::{BuiltinProviderLoader, ProviderLoadError};

#[derive(Debug, Parser)]
#[command(name = "blue", version, about = "Blue infrastructure tool")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Validate a configuration file
    Validate { config: PathBuf },
}

/// Parse process arguments and dispatch the selected command.
pub fn run() -> Result<(), CliError> {
    let cli = Cli::parse();
    match cli.command {
        Command::Validate { config } => validate(&config),
    }
}

fn validate(config: &Path) -> Result<(), CliError> {
    let loaded = load_config_source(config).map_err(|error| CliError::Read {
        path: config.to_path_buf(),
        error,
    })?;
    let source = loaded
        .source_map
        .get(loaded.source_id)
        .expect("loaded source is registered");
    let parsed = TomlReader
        .read(loaded.source_id, &source.text)
        .map_err(|error| CliError::parse(config, &loaded, error))?;
    let _graph = DesiredGraphBuilder::build(&parsed)
        .map_err(|errors| CliError::graph(config, &loaded, errors))?;
    let _registry = BuiltinProviderLoader::load(&parsed.providers)
        .map_err(|errors| CliError::provider_load(config, &loaded, errors))?;
    println!(
        "Validated declarations and dependencies in {}: {} providers, {} parameters, {} resources, {} data blocks",
        source.name,
        parsed.providers.len(),
        parsed.parameters.len(),
        parsed.resources.len(),
        parsed.data.len(),
    );
    println!(
        "Provider implementations loaded. Provider schemas and authentication were not checked."
    );
    Ok(())
}

/// Command failures retain their underlying cause, but not source file contents.
#[derive(Debug)]
pub enum CliError {
    Read {
        path: PathBuf,
        error: io::Error,
    },
    Parse {
        path: PathBuf,
        location: Option<(usize, usize)>,
        error: TomlReaderError,
    },
    Graph {
        path: PathBuf,
        diagnostics: Vec<Diagnostic<GraphBuildError>>,
    },
    ProviderLoad {
        path: PathBuf,
        diagnostics: Vec<Diagnostic<ProviderLoadError>>,
    },
}

/// An error and its resolved locations, without retaining source text.
#[derive(Debug)]
pub struct Diagnostic<E> {
    pub error: E,
    pub location: Option<(usize, usize)>,
    pub related: Vec<RelatedLocation>,
}

#[derive(Debug)]
pub struct RelatedLocation {
    pub message: String,
    pub location: Option<(usize, usize)>,
}

impl CliError {
    fn parse(path: &Path, loaded: &LoadedSource, error: TomlReaderError) -> Self {
        let location = source_location(loaded, error.span);
        Self::Parse {
            path: path.to_path_buf(),
            location,
            error,
        }
    }

    fn provider_load(path: &Path, loaded: &LoadedSource, errors: Vec<ProviderLoadError>) -> Self {
        let diagnostics = errors
            .into_iter()
            .map(|error| {
                let location = source_location(loaded, Some(error.primary_span()));
                let related = match &error {
                    ProviderLoadError::DuplicateName { first_span, .. } => vec![RelatedLocation {
                        message: "first declared here".into(),
                        location: source_location(loaded, Some(*first_span)),
                    }],
                    _ => vec![],
                };
                Diagnostic {
                    error,
                    location,
                    related,
                }
            })
            .collect();
        Self::ProviderLoad {
            path: path.to_path_buf(),
            diagnostics,
        }
    }

    fn graph(path: &Path, loaded: &LoadedSource, errors: Vec<GraphBuildError>) -> Self {
        let diagnostics = errors
            .into_iter()
            .map(|error| {
                let location = source_location(loaded, error.primary_span());
                let related = match &error {
                    GraphBuildError::DuplicateDeclaration { first_span, .. } => {
                        vec![RelatedLocation {
                            message: "first declared here".into(),
                            location: source_location(loaded, Some(*first_span)),
                        }]
                    }
                    GraphBuildError::MultipleDefaultProviders { providers } => providers
                        .iter()
                        .skip(1)
                        .map(|(provider, span)| RelatedLocation {
                            message: format!("`{provider}` is also marked default"),
                            location: source_location(loaded, Some(*span)),
                        })
                        .collect(),
                    _ => vec![],
                };
                Diagnostic {
                    error,
                    location,
                    related,
                }
            })
            .collect();
        Self::Graph {
            path: path.to_path_buf(),
            diagnostics,
        }
    }
}

fn source_location(loaded: &LoadedSource, span: Option<SourceSpan>) -> Option<(usize, usize)> {
    let span = span?;
    if span.source != loaded.source_id {
        return None;
    }
    let source = loaded.source_map.get(span.source)?;
    // Validate both bounds and UTF-8 boundaries before reporting a location.
    source.text.get(span.span.start..span.span.end)?;
    source.line_column(span.span.start)
}

fn write_location(
    f: &mut fmt::Formatter<'_>,
    path: &Path,
    location: Option<(usize, usize)>,
) -> fmt::Result {
    write!(f, "{}", path.display())?;
    if let Some((line, column)) = location {
        write!(f, ":{line}:{column}")?;
    }
    Ok(())
}

fn write_diagnostics<E: fmt::Display>(
    f: &mut fmt::Formatter<'_>,
    path: &Path,
    diagnostics: &[Diagnostic<E>],
) -> fmt::Result {
    for (index, diagnostic) in diagnostics.iter().enumerate() {
        if index > 0 {
            writeln!(f)?;
        }
        write_location(f, path, diagnostic.location)?;
        write!(f, ": {}", diagnostic.error)?;
        for related in &diagnostic.related {
            write!(f, "\n  note: ")?;
            write_location(f, path, related.location)?;
            write!(f, ": {}", related.message)?;
        }
    }
    Ok(())
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read { path, error } => write!(f, "failed to read `{}`: {error}", path.display()),
            Self::Parse {
                path,
                location,
                error,
            } => {
                write_location(f, path, *location)?;
                write!(f, ": {error}")
            }
            Self::ProviderLoad { path, diagnostics } => write_diagnostics(f, path, diagnostics),
            Self::Graph { path, diagnostics } => write_diagnostics(f, path, diagnostics),
        }
    }
}

impl Error for CliError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Read { error, .. } => Some(error),
            Self::Parse { error, .. } => Some(error),
            Self::Graph { .. } | Self::ProviderLoad { .. } => None, // Multiple independent causes.
        }
    }
}

struct LoadedSource {
    source_map: SourceMap,
    source_id: SourceId,
}

fn load_config_source(path: &Path) -> io::Result<LoadedSource> {
    let text = fs::read_to_string(path)?;
    let mut source_map = SourceMap::new();
    let source_id = source_map.add(path.display().to_string(), text);
    Ok(LoadedSource {
        source_map,
        source_id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::error::ErrorKind;

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            use std::sync::atomic::{AtomicUsize, Ordering};
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            loop {
                let path = std::env::temp_dir().join(format!(
                    "blue-source-test-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed),
                ));
                match fs::create_dir(&path) {
                    Ok(()) => return Self(path),
                    Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                    Err(error) => panic!("cannot create test directory: {error}"),
                }
            }
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn empty_diagnostic_lists_render_without_extra_output() {
        for error in [
            CliError::Graph {
                path: "project.toml".into(),
                diagnostics: vec![],
            },
            CliError::ProviderLoad {
                path: "project.toml".into(),
                diagnostics: vec![],
            },
        ] {
            assert_eq!(error.to_string(), "");
        }
    }

    #[test]
    fn validate_handler_parses_loaded_source() {
        let directory = TestDirectory::new();
        let path = directory.0.join("project.toml");
        fs::write(&path, "[parameter.environment]\ndefault = \"prod\"\n").unwrap();
        assert!(validate(&path).is_ok());
    }

    #[test]
    fn validate_loads_ovh_implementations_without_checking_credentials() {
        let directory = TestDirectory::new();
        let path = directory.0.join("project.toml");
        fs::write(&path, "[provider.production]\ntype = \"ovh\"\n[provider.staging]\ntype = \"ovh\"\nsource = \"builtin\"\n").unwrap();
        assert!(validate(&path).is_ok());
    }

    #[test]
    fn validate_preserves_all_provider_loading_errors() {
        let directory = TestDirectory::new();
        let path = directory.0.join("project.toml");
        fs::write(&path, "[provider.a]\ntype = \"unknown\"\n[provider.b]\ntype = \"ovh\"\nsource = \"https://example.com/provider.git\"\n").unwrap();
        let failure = validate(&path).unwrap_err();
        let rendered = failure.to_string();
        let CliError::ProviderLoad {
            path: error_path,
            diagnostics,
        } = failure
        else {
            panic!("expected provider loading failure")
        };
        assert_eq!(error_path, path);
        let errors = diagnostics
            .iter()
            .map(|diagnostic| &diagnostic.error)
            .collect::<Vec<_>>();
        assert!(matches!(
            errors.as_slice(),
            [
                ProviderLoadError::UnknownType { .. },
                ProviderLoadError::UnsupportedSource { .. }
            ]
        ));
        assert_eq!(diagnostics[0].location, Some((2, 9)));
        assert_eq!(diagnostics[1].location, Some((5, 11)));
        assert_eq!(rendered.lines().count(), 2);
        assert!(
            rendered
                .lines()
                .all(|line| line.starts_with(&format!("{}:", path.display())))
        );
        assert!(rendered.contains("unknown built-in provider type `unknown`"));
        assert!(rendered.contains("unsupported source for provider `b`"));
    }

    #[test]
    fn provider_duplicate_diagnostic_preserves_both_locations_and_error() {
        let mut source_map = SourceMap::new();
        let source_id = source_map.add("project.toml", "first\nsecond\n");
        let loaded = LoadedSource {
            source_map,
            source_id,
        };
        let error = ProviderLoadError::DuplicateName {
            name: "production".into(),
            first_span: SourceSpan::new(source_id, 0, 5),
            duplicate_span: SourceSpan::new(source_id, 6, 12),
        };
        let failure =
            CliError::provider_load(Path::new("project.toml"), &loaded, vec![error.clone()]);
        assert_eq!(
            failure.to_string(),
            "project.toml:2:1: duplicate provider configuration name `production`\n  note: project.toml:1:1: first declared here"
        );
        let CliError::ProviderLoad { diagnostics, .. } = failure else {
            panic!("expected provider loading failure")
        };
        assert_eq!(diagnostics[0].error, error);
    }

    #[test]
    fn provider_diagnostics_fall_back_for_invalid_primary_and_related_spans() {
        let mut source_map = SourceMap::new();
        let source_id = source_map.add("project.toml", "é\n");
        let loaded = LoadedSource {
            source_map,
            source_id,
        };
        let errors = vec![
            ProviderLoadError::UnknownType {
                name: "a".into(),
                provider_type: "unknown".into(),
                span: SourceSpan::new(source_id, 1, 2),
            },
            ProviderLoadError::UnsupportedSource {
                name: "b".into(),
                span: SourceSpan::new(source_id, 0, 99),
            },
            ProviderLoadError::DuplicateName {
                name: "c".into(),
                first_span: SourceSpan::new(SourceId(99), 0, 0),
                duplicate_span: SourceSpan::new(source_id, 2, 1),
            },
        ];
        let failure = CliError::provider_load(Path::new("project.toml"), &loaded, errors.clone());
        let rendered = failure.to_string();
        assert_eq!(rendered.lines().count(), 4);
        assert!(rendered.lines().all(|line| {
            line.trim_start_matches("  note: ")
                .starts_with("project.toml: ")
        }));
        let CliError::ProviderLoad { diagnostics, .. } = failure else {
            panic!("expected provider loading failure")
        };
        assert_eq!(
            diagnostics
                .into_iter()
                .map(|diagnostic| diagnostic.error)
                .collect::<Vec<_>>(),
            errors
        );
    }

    #[test]
    fn provider_locations_use_character_columns_and_handle_eof() {
        let mut source_map = SourceMap::new();
        let source_id = source_map.add("project.toml", "é\n");
        let loaded = LoadedSource {
            source_map,
            source_id,
        };
        for (offset, expected) in [(2, (1, 2)), (3, (2, 1))] {
            let failure = CliError::provider_load(
                Path::new("project.toml"),
                &loaded,
                vec![ProviderLoadError::UnsupportedSource {
                    name: "test".into(),
                    span: SourceSpan::new(source_id, offset, offset),
                }],
            );
            let CliError::ProviderLoad { diagnostics, .. } = failure else {
                panic!("expected provider loading failure")
            };
            assert_eq!(diagnostics[0].location, Some(expected));
        }
    }

    #[test]
    fn validate_handler_propagates_file_read_errors() {
        let directory = TestDirectory::new();
        let error = validate(&directory.0.join("missing.toml")).unwrap_err();
        assert!(
            matches!(error, CliError::Read { ref error, .. } if error.kind() == io::ErrorKind::NotFound)
        );
        let path = directory.0.join("invalid.toml");
        fs::write(&path, [0xff, 0xfe]).unwrap();
        assert!(
            matches!(validate(&path).unwrap_err(), CliError::Read { ref error, .. } if error.kind() == io::ErrorKind::InvalidData)
        );
    }

    #[test]
    fn parse_errors_fall_back_to_file_name_for_missing_or_invalid_spans() {
        use crate::config::{SourceSpan, TomlReaderErrorKind};
        let mut source_map = SourceMap::new();
        let source_id = source_map.add("project.toml", "é\n");
        let loaded = LoadedSource {
            source_map,
            source_id,
        };
        for span in [
            None,
            Some(SourceSpan::new(SourceId(99), 0, 0)),
            Some(SourceSpan::new(source_id, 1, 2)),
            Some(SourceSpan::new(source_id, 0, 1)),
            Some(SourceSpan::new(source_id, 2, 1)),
            Some(SourceSpan::new(source_id, 0, 99)),
        ] {
            let error = CliError::parse(
                Path::new("project.toml"),
                &loaded,
                TomlReaderError {
                    kind: TomlReaderErrorKind::Syntax("test failure".into()),
                    span,
                },
            );
            assert_eq!(
                error.to_string(),
                "project.toml: failed to parse TOML syntax: test failure"
            );
            assert!(Error::source(&error).unwrap().is::<TomlReaderError>());
        }
    }

    #[test]
    fn parse_error_locations_count_unicode_characters_and_allow_eof() {
        use crate::config::{SourceSpan, TomlReaderErrorKind};
        let mut source_map = SourceMap::new();
        let source_id = source_map.add("project.toml", "é\n");
        let loaded = LoadedSource {
            source_map,
            source_id,
        };
        for (offset, suffix) in [(2, "1:2"), (3, "2:1")] {
            let error = CliError::parse(
                Path::new("project.toml"),
                &loaded,
                TomlReaderError {
                    kind: TomlReaderErrorKind::UnexpectedEof,
                    span: Some(SourceSpan::new(source_id, offset, offset)),
                },
            );
            assert_eq!(
                error.to_string(),
                format!("project.toml:{suffix}: unexpected end of TOML input")
            );
        }
    }

    #[test]
    fn graph_duplicate_errors_preserve_both_locations_and_original_cause() {
        use crate::graph::DeclarationId;
        let mut source_map = SourceMap::new();
        let source_id = source_map.add("project.toml", "first\nsecond\n");
        let loaded = LoadedSource {
            source_map,
            source_id,
        };
        let duplicate_span = SourceSpan::new(source_id, 6, 12);
        let error = GraphBuildError::DuplicateDeclaration {
            id: DeclarationId::Parameter {
                name: "name".into(),
            },
            first_span: SourceSpan::new(source_id, 0, 5),
            duplicate_span,
        };
        assert_eq!(error.primary_span(), Some(duplicate_span));
        let failure = CliError::graph(Path::new("project.toml"), &loaded, vec![error.clone()]);
        assert_eq!(
            failure.to_string(),
            "project.toml:2:1: duplicate declaration `parameter.name`\n  note: project.toml:1:1: first declared here"
        );
        let CliError::Graph { diagnostics, .. } = failure else {
            panic!("expected graph failure")
        };
        assert_eq!(diagnostics[0].error, error);
    }

    #[test]
    fn graph_errors_with_invalid_spans_fall_back_without_losing_errors() {
        let mut source_map = SourceMap::new();
        let source_id = source_map.add("project.toml", "é");
        let loaded = LoadedSource {
            source_map,
            source_id,
        };
        let errors = [
            None,
            Some(SourceSpan::new(source_id, 1, 2)),
            Some(SourceSpan::new(SourceId(99), 0, 0)),
        ]
        .into_iter()
        .map(|span| match span {
            None => GraphBuildError::Cycle { path: vec![] },
            Some(span) => GraphBuildError::MalformedReference {
                path: vec!["bad".into()],
                span,
            },
        })
        .collect::<Vec<_>>();
        let failure = CliError::graph(Path::new("project.toml"), &loaded, errors.clone());
        let rendered = failure.to_string();
        assert_eq!(rendered.lines().count(), errors.len());
        assert!(
            rendered
                .lines()
                .all(|line| line.starts_with("project.toml: "))
        );
        let CliError::Graph { diagnostics, .. } = failure else {
            panic!("expected graph failure")
        };
        assert_eq!(
            diagnostics
                .into_iter()
                .map(|diagnostic| diagnostic.error)
                .collect::<Vec<_>>(),
            errors
        );
    }

    #[test]
    fn loads_utf8_text_and_retains_source_id_and_file_name() {
        let directory = TestDirectory::new();
        let path = directory.0.join("project file.toml");
        let text = "[parameter.environment]\ndefault = \"世界\"\n";
        fs::write(&path, text).unwrap();
        let loaded = load_config_source(&path).unwrap();
        assert_eq!(loaded.source_map.sources.len(), 1);
        let source = loaded.source_map.get(loaded.source_id).unwrap();
        assert_eq!(source.name, path.display().to_string());
        assert_eq!(source.text, text);
    }

    #[test]
    fn loading_does_not_parse_or_validate_the_contents() {
        let directory = TestDirectory::new();
        let path = directory.0.join("not-toml.txt");
        fs::write(&path, "not valid TOML at all").unwrap();
        let loaded = load_config_source(&path).unwrap();
        assert_eq!(
            loaded.source_map.get(loaded.source_id).unwrap().text,
            "not valid TOML at all"
        );
    }

    #[test]
    fn loading_missing_file_returns_read_error() {
        let directory = TestDirectory::new();
        let error = load_config_source(&directory.0.join("missing.toml"))
            .err()
            .unwrap();
        assert_eq!(error.kind(), io::ErrorKind::NotFound);
    }

    #[test]
    fn loading_invalid_utf8_returns_read_error() {
        let directory = TestDirectory::new();
        let path = directory.0.join("invalid.toml");
        fs::write(&path, [0xff, 0xfe]).unwrap();
        let error = load_config_source(&path).err().unwrap();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn loading_a_directory_returns_read_error() {
        let directory = TestDirectory::new();
        assert!(load_config_source(&directory.0).is_err());
    }

    #[test]
    fn parses_validate_with_config_path() {
        let cli = Cli::try_parse_from(["blue", "validate", "./project.toml"]).unwrap();
        let Command::Validate { config } = cli.command;
        assert_eq!(config, PathBuf::from("./project.toml"));
    }

    #[test]
    fn config_path_can_contain_spaces_and_need_not_exist_yet() {
        let cli =
            Cli::try_parse_from(["blue", "validate", "not present/project file.toml"]).unwrap();
        let Command::Validate { config } = cli.command;
        assert_eq!(config, PathBuf::from("not present/project file.toml"));
    }

    #[test]
    fn rejects_missing_config_path() {
        let error = Cli::try_parse_from(["blue", "validate"]).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::MissingRequiredArgument);
    }

    #[test]
    fn rejects_unknown_subcommand() {
        let error = Cli::try_parse_from(["blue", "plan", "project.toml"]).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidSubcommand);
    }

    #[test]
    fn rejects_extra_arguments() {
        let error =
            Cli::try_parse_from(["blue", "validate", "project.toml", "extra.toml"]).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::UnknownArgument);
    }

    #[test]
    fn exposes_help_for_the_cli_and_validate_command() {
        for args in [vec!["blue", "--help"], vec!["blue", "validate", "--help"]] {
            let error = Cli::try_parse_from(args).unwrap_err();
            assert_eq!(error.kind(), ErrorKind::DisplayHelp);
            assert!(error.to_string().contains("validate"));
        }
    }

    #[test]
    fn exposes_package_version() {
        let error = Cli::try_parse_from(["blue", "--version"]).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::DisplayVersion);
        assert!(error.to_string().contains(env!("CARGO_PKG_VERSION")));
    }
}
