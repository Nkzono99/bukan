use clap::{Parser, Subcommand, ValueEnum};
use serde_json::{json, Value};
use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::{
    build_index, detect_paperpile_roots, organization, runtime, settings, storage, workspace,
    workspace_collections,
};

#[derive(Parser)]
#[command(
    name = "bukan",
    version,
    about = "Paperpile literature workspace and research MCP manager"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Create a workspace outside the Bukan source repository and Paperpile.
    Init {
        path: PathBuf,
        #[arg(long)]
        name: Option<String>,
        #[arg(long, default_value = "auto")]
        paperpile: String,
    },
    /// Find mounted Paperpile libraries without modifying them.
    Detect,
    /// Show data, settings and runtime paths without creating any files.
    Paths {
        #[arg(long)]
        json: bool,
    },
    /// Prepare research; with no existing binding, create the managed user-data workspace.
    Setup {
        workspace: Option<PathBuf>,
        #[arg(long)]
        default: bool,
    },
    /// Run the literature MCP over stdio. Workspace: argument > env > saved default.
    Mcp { workspace: Option<PathBuf> },
    /// Set up or check the dedicated Paperpile registration browser.
    Paperpile {
        #[command(subcommand)]
        command: PaperpileCommand,
    },
    /// Run the research MCP over stdio, using the workspace's existing store.
    ResearchMcp { workspace: Option<PathBuf> },
    /// Pass engine arguments after --. Relative file arguments use the workspace directory.
    Research {
        workspace: Option<PathBuf>,
        #[arg(last = true, required = true)]
        args: Vec<String>,
    },
    /// Print configuration for both MCP servers without changing any client settings.
    McpConfig {
        workspace: Option<PathBuf>,
        #[arg(long, value_enum, default_value_t = ConfigFormat::Json)]
        format: ConfigFormat,
    },
    /// Diagnose the workspace, library, research runtime and Poppler without setup or writes.
    Doctor {
        workspace: Option<PathBuf>,
        #[arg(long)]
        json: bool,
    },
    /// Scan the workspace's Paperpile library read-only.
    Scan {
        workspace: Option<PathBuf>,
        #[arg(long)]
        json: bool,
    },
    /// Generate a reviewable Bukan folder and label assignment plan.
    Organize {
        #[command(subcommand)]
        command: OrganizeCommand,
    },
    /// Manage workspace collections without changing Paperpile.
    Collection {
        #[command(subcommand)]
        command: CollectionCommand,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum ConfigFormat {
    Json,
    Toml,
}

#[derive(Subcommand)]
enum PaperpileCommand {
    /// Open a separate Chrome profile. Sign in, then close that window.
    Login { workspace: Option<PathBuf> },
    /// Check live library access without registering any references.
    Status { workspace: Option<PathBuf> },
}

#[derive(Subcommand)]
enum OrganizeCommand {
    /// Suggest assignments from taxonomy.toml and save a plan inside the workspace.
    Suggest {
        workspace: Option<PathBuf>,
        #[arg(long)]
        output: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum CollectionCommand {
    /// List saved collections without creating any files.
    List { workspace: Option<PathBuf> },
    /// Create a workspace collection (the path can contain / to group it).
    Create {
        name: String,
        workspace: Option<PathBuf>,
    },
    /// Add an indexed paper ID to a workspace collection.
    Add {
        name: String,
        paper_id: String,
        workspace: Option<PathBuf>,
    },
    /// Remove a paper ID from a workspace collection.
    Remove {
        name: String,
        paper_id: String,
        workspace: Option<PathBuf>,
    },
}

fn bind(path: Option<&Path>) -> Result<PathBuf, String> {
    let binding = settings::bind_workspace(path)?;
    eprintln!("Workspace: {} ({})", binding.root.display(), binding.source);
    Ok(binding.root)
}

fn print_json(value: &impl serde::Serialize) -> Result<(), String> {
    println!(
        "{}",
        serde_json::to_string_pretty(value).map_err(|error| error.to_string())?
    );
    Ok(())
}

pub fn run() -> Result<i32, String> {
    match Cli::parse().command {
        Command::Init {
            path,
            name,
            paperpile,
        } => {
            let descriptor = workspace::init_workspace(&path, name.as_deref(), Some(&paperpile))?;
            println!(
                "Initialized: {}\nName: {}",
                descriptor.root, descriptor.name
            );
            println!(
                "Paperpile: {}",
                descriptor
                    .paperpile_root
                    .as_deref()
                    .unwrap_or("not detected")
            );
            println!("Next: bukan setup \"{}\"", descriptor.root);
        }
        Command::Detect => {
            let roots = detect_paperpile_roots();
            if roots.is_empty() {
                println!("No mounted Paperpile library detected.");
            }
            for root in roots {
                println!("{}", root.display());
            }
        }
        Command::Paths { json: as_json } => {
            settings::validate_settings()?;
            let binding = settings::configured_workspace(None)?;
            let data_dir = settings::data_dir()?;
            let config_dir = settings::config_dir()?;
            let cache_dir = settings::cache_dir()?;
            if let Some(binding) = &binding {
                for directory in [&data_dir, &config_dir, &cache_dir] {
                    settings::validate_external_destination(&binding.root, directory)?;
                }
            }
            // A valid environment binding may override an invalid saved value.
            // Report that value separately without rejecting the selected workspace.
            let (default_workspace, default_error) = match settings::read_default() {
                Ok(path) => (path, None),
                Err(error) => (None, Some(error)),
            };
            let mut paths = json!({
                "dataDir": data_dir,
                "configDir": config_dir,
                "cacheDir": cache_dir,
                "managedWorkspace": settings::managed_workspace()?,
                "defaultWorkspace": default_workspace,
                "selectedWorkspace": binding.as_ref().map(|binding| &binding.root),
                "workspaceSource": binding.as_ref().map(|binding| binding.source),
            });
            if let Some(error) = default_error {
                paths["defaultWorkspaceError"] = json!(error);
            }
            if as_json {
                print_json(&paths)?;
            } else {
                for (name, path) in paths.as_object().expect("path object") {
                    println!("{name}: {}", path.as_str().unwrap_or("not configured"));
                }
            }
        }
        Command::Setup { workspace, default } => {
            // Fail on unreadable settings before creating a managed workspace.
            settings::validate_settings()?;
            let binding = settings::setup_workspace(workspace.as_deref())?;
            let root = binding.root;
            eprintln!("Workspace: {} ({})", root.display(), binding.source);
            settings::validate_external_destination(
                &root,
                &settings::config_dir()?.join("settings.toml"),
            )?;
            let initialized = runtime::setup(&root)?;
            let saved = settings::save_default(&root, default)?;
            println!("Research runtime is ready.");
            println!(
                "Research store: {}",
                if initialized {
                    "initialized"
                } else {
                    "existing store preserved (no migration)"
                }
            );
            if saved {
                println!("Default workspace: {}", root.display());
            } else {
                println!("Saved default workspace unchanged; use --default to switch it.");
            }
        }
        Command::Mcp { workspace } => {
            crate::mcp::run_stdio(&bind(workspace.as_deref())?)?;
        }
        Command::Paperpile { command } => {
            let (action, workspace) = match command {
                PaperpileCommand::Login { workspace } => ("login", workspace),
                PaperpileCommand::Status { workspace } => ("status", workspace),
            };
            print_json(&runtime::paperpile(
                &bind(workspace.as_deref())?,
                action,
                &json!({}),
            )?)?;
        }
        Command::ResearchMcp { workspace } => {
            let root = bind(workspace.as_deref())?;
            return Ok(exit_code(runtime::run(&root, &["serve".into()])?));
        }
        Command::Research { workspace, args } => {
            let root = bind(workspace.as_deref())?;
            return Ok(exit_code(runtime::run(&root, &args)?));
        }
        Command::McpConfig { workspace, format } => {
            let root = bind(workspace.as_deref())?;
            let executable = std::env::current_exe().map_err(|error| error.to_string())?;
            let servers = server_configs(&executable, &root);
            match format {
                ConfigFormat::Json => print_json(&json!({"mcpServers": servers}))?,
                ConfigFormat::Toml => println!(
                    "{}",
                    toml::to_string_pretty(&json!({"mcp_servers": servers}))
                        .map_err(|error| error.to_string())?
                ),
            }
        }
        Command::Doctor {
            workspace,
            json: as_json,
        } => {
            let root = bind(workspace.as_deref())?;
            let descriptor = workspace::describe_workspace(&root)?;
            let runtime = runtime::status();
            let mut dependency_errors = Vec::new();
            let poppler = ["pdfinfo", "pdftotext", "pdftoppm"]
                .into_iter()
                .map(|name| {
                    let path = runtime::find_poppler(name).unwrap_or_else(|error| {
                        dependency_errors.push(format!("{name}: {error}"));
                        None
                    });
                    (name.to_string(), json!(path))
                })
                .collect::<serde_json::Map<_, _>>();
            let uv = runtime::find_uv().unwrap_or_else(|error| {
                dependency_errors.push(format!("uv: {error}"));
                None
            });
            let store = storage::store_path(&root)?;
            let healthy = descriptor.paperpile_root.is_some()
                && runtime.ready
                && store.is_file()
                && poppler.values().all(|path| !path.is_null());
            if as_json {
                print_json(
                    &json!({"healthy": healthy, "workspace": descriptor, "researchRuntime": runtime, "researchStore": {"path": store, "initialized": store.is_file()}, "uv": uv, "poppler": poppler, "dependencyErrors": dependency_errors}),
                )?;
            } else {
                println!(
                    "Workspace: {} (format v{})\nRoot: {}",
                    descriptor.name, descriptor.version, descriptor.root
                );
                println!(
                    "Paperpile: {}",
                    descriptor
                        .paperpile_root
                        .as_deref()
                        .unwrap_or("not connected")
                );
                if let Some(error) = descriptor.paperpile_error {
                    println!("Library detail: {error}");
                }
                println!("Research runtime: {}", runtime.detail);
                println!(
                    "Research store: {} [{}]",
                    store.display(),
                    if store.is_file() {
                        "present"
                    } else {
                        "not initialized; run bukan setup <workspace>"
                    }
                );
                for (name, path) in poppler {
                    println!(
                        "{name}: {}",
                        path.as_str()
                            .unwrap_or("not found; run the Bukan toolkit installer")
                    );
                }
                for error in dependency_errors {
                    println!("Dependency: {error}");
                }
            }
        }
        Command::Scan {
            workspace,
            json: as_json,
        } => {
            let root = bind(workspace.as_deref())?;
            let descriptor = workspace::describe_workspace(&root)?;
            let library = descriptor.paperpile_root.ok_or_else(|| {
                descriptor
                    .paperpile_error
                    .unwrap_or_else(|| "Paperpile library was not detected".into())
            })?;
            let index = build_index(Path::new(&library))?;
            if as_json {
                print_json(&index)?;
            } else {
                println!(
                    "Workspace: {}\nPapers: {}\nCollections: {}\nStarred: {}\nWarnings: {}",
                    descriptor.name,
                    index.stats.paper_count,
                    index.stats.collection_count,
                    index.stats.starred_count,
                    index.warnings.len()
                );
            }
        }
        Command::Organize {
            command: OrganizeCommand::Suggest { workspace, output },
        } => {
            let root = bind(workspace.as_deref())?;
            let descriptor = workspace::describe_workspace(&root)?;
            let library = descriptor.paperpile_root.ok_or_else(|| {
                descriptor
                    .paperpile_error
                    .unwrap_or_else(|| "Paperpile library was not detected".into())
            })?;
            let index = build_index(Path::new(&library))?;
            let taxonomy = organization::load_taxonomy(&root)?;
            let plan = organization::generate_plan(&index, &taxonomy);
            let output = root.join(
                output.unwrap_or_else(|| PathBuf::from("reports/bukan-organization-plan.json")),
            );
            let output = storage::validate_workspace_destination(&root, &output)?;
            fs::write(
                &output,
                serde_json::to_string_pretty(&plan).map_err(|error| error.to_string())?,
            )
            .map_err(|error| format!("Could not write {}: {error}", output.display()))?;
            println!(
                "Plan: {}\nPapers: {}\nClassified: {}\nNeeds manual review: {}",
                output.display(),
                plan.paper_count,
                plan.classified_count,
                plan.review_required_count
            );
        }
        Command::Collection { command } => {
            let index = match command {
                CollectionCommand::List { workspace } => {
                    workspace_collections::load(&bind(workspace.as_deref())?)?
                }
                CollectionCommand::Create { workspace, name } => {
                    let root = bind(workspace.as_deref())?;
                    workspace_collections::initialize_from_paperpile(&root)?;
                    workspace_collections::create_collection(&root, &name)?
                }
                CollectionCommand::Add {
                    workspace,
                    name,
                    paper_id,
                } => workspace_collections::set_membership(
                    &bind(workspace.as_deref())?,
                    &name,
                    &paper_id,
                    true,
                )?,
                CollectionCommand::Remove {
                    workspace,
                    name,
                    paper_id,
                } => workspace_collections::set_membership(
                    &bind(workspace.as_deref())?,
                    &name,
                    &paper_id,
                    false,
                )?,
            };
            print_json(&index)?;
        }
    }
    Ok(0)
}

fn server_configs(executable: &Path, root: &Path) -> Value {
    json!({
        "bukan": {"command": executable, "args": ["mcp", root]},
        "bukan_research": {"command": executable, "args": ["research-mcp", root]}
    })
}

fn exit_code(status: std::process::ExitStatus) -> i32 {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        status
            .code()
            .unwrap_or_else(|| 128 + status.signal().unwrap_or(1))
    }
    #[cfg(not(unix))]
    {
        status.code().unwrap_or(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn research_requires_separator_and_preserves_engine_arguments() {
        for args in [
            vec!["bukan", "research", "--", "search", "a query"],
            vec![
                "bukan",
                "research",
                "研究 O'Brien",
                "--",
                "search",
                "a query",
            ],
        ] {
            let cli = Cli::try_parse_from(args).unwrap();
            let Command::Research { args, .. } = cli.command else {
                panic!("wrong command")
            };
            assert_eq!(args, ["search", "a query"]);
        }
        assert!(Cli::try_parse_from(["bukan", "research", "search", "a query"]).is_err());
    }

    #[test]
    fn generated_config_round_trips_special_paths_in_both_formats() {
        let executable = Path::new("C:\\a \"quoted\" O'Brien\\bukan.exe");
        let root = Path::new("C:\\研究 \\workspace");
        let servers = server_configs(executable, root);
        let json: Value =
            serde_json::from_str(&serde_json::to_string(&json!({"mcpServers": &servers})).unwrap())
                .unwrap();
        assert_eq!(
            json["mcpServers"]["bukan"]["args"][1],
            root.to_string_lossy().as_ref()
        );
        let text = toml::to_string_pretty(&json!({"mcp_servers": servers})).unwrap();
        let parsed: toml::Value = toml::from_str(&text).unwrap();
        assert_eq!(
            parsed["mcp_servers"]["bukan_research"]["command"]
                .as_str()
                .unwrap(),
            executable.to_string_lossy()
        );
    }
}
