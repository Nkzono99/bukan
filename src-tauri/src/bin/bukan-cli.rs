use bukan_lib::{build_index, detect_paperpile_roots, organization, workspace};
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "bukan",
    version,
    about = "Paperpile literature workspace manager"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Create a research workspace outside the Bukan application repository.
    Init {
        path: PathBuf,
        #[arg(long)]
        name: Option<String>,
        #[arg(long, default_value = "auto")]
        paperpile: String,
    },
    /// Find mounted Paperpile libraries without modifying them.
    Detect,
    /// Validate workspace configuration and its Paperpile connection.
    Doctor {
        #[arg(default_value = ".")]
        workspace: PathBuf,
    },
    /// Scan the Paperpile library configured by a workspace.
    Scan {
        #[arg(default_value = ".")]
        workspace: PathBuf,
        #[arg(long)]
        json: bool,
    },
    /// Generate a reviewable Bukan folder and label assignment plan.
    Organize {
        #[command(subcommand)]
        command: OrganizeCommand,
    },
}

#[derive(Subcommand)]
enum OrganizeCommand {
    /// Suggest assignments from taxonomy.toml and save them under reports/.
    Suggest {
        #[arg(default_value = ".")]
        workspace: PathBuf,
        #[arg(long)]
        output: Option<PathBuf>,
    },
}

fn main() {
    if let Err(error) = run(Cli::parse()) {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<(), String> {
    match cli.command {
        Command::Init {
            path,
            name,
            paperpile,
        } => {
            let descriptor =
                workspace::init_workspace(&path, name.as_deref(), Some(paperpile.as_str()))?;
            println!("Initialized: {}", descriptor.root);
            println!("Name: {}", descriptor.name);
            println!(
                "Paperpile: {}",
                descriptor
                    .paperpile_root
                    .as_deref()
                    .unwrap_or("not detected")
            );
        }
        Command::Detect => {
            let roots = detect_paperpile_roots();
            if roots.is_empty() {
                println!("No mounted Paperpile library detected.");
            } else {
                for root in roots {
                    println!("{}", root.display());
                }
            }
        }
        Command::Doctor { workspace: root } => {
            let descriptor = workspace::describe_workspace(&root)?;
            println!(
                "Workspace: {} (format v{})",
                descriptor.name, descriptor.version
            );
            println!("Root: {}", descriptor.root);
            println!("Paperpile mode: {}", descriptor.paperpile_mode);
            match descriptor.paperpile_root {
                Some(path) => println!("Paperpile: {path} [read-only, OK]"),
                None => println!("Paperpile: not detected"),
            }
        }
        Command::Scan {
            workspace: root,
            json,
        } => {
            let descriptor = workspace::describe_workspace(&root)?;
            let paperpile_root = descriptor
                .paperpile_root
                .ok_or_else(|| "Paperpile library was not detected".to_string())?;
            let index = build_index(PathBuf::from(paperpile_root).as_path())?;
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&index)
                        .map_err(|error| format!("could not serialize index: {error}"))?
                );
            } else {
                println!("Workspace: {}", descriptor.name);
                println!("Papers: {}", index.stats.paper_count);
                println!("Collections: {}", index.stats.collection_count);
                println!("Starred: {}", index.stats.starred_count);
                println!("Warnings: {}", index.warnings.len());
            }
        }
        Command::Organize { command } => match command {
            OrganizeCommand::Suggest {
                workspace: root,
                output,
            } => {
                let descriptor = workspace::describe_workspace(&root)?;
                let paperpile_root = descriptor
                    .paperpile_root
                    .ok_or_else(|| "Paperpile library was not detected".to_string())?;
                let index = build_index(PathBuf::from(paperpile_root).as_path())?;
                let taxonomy =
                    organization::load_taxonomy(PathBuf::from(&descriptor.root).as_path())?;
                let plan = organization::generate_plan(&index, &taxonomy);
                let output = output.unwrap_or_else(|| {
                    PathBuf::from(&descriptor.root)
                        .join("reports")
                        .join("bukan-organization-plan.json")
                });
                let contents = serde_json::to_string_pretty(&plan)
                    .map_err(|error| format!("could not serialize plan: {error}"))?;
                std::fs::write(&output, contents)
                    .map_err(|error| format!("could not write {}: {error}", output.display()))?;
                println!("Plan: {}", output.display());
                println!("Papers: {}", plan.paper_count);
                println!("Classified: {}", plan.classified_count);
                println!("Needs manual review: {}", plan.review_required_count);
            }
        },
    }
    Ok(())
}
