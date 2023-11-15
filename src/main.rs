use std::path::PathBuf;

use anyhow::Context;
use clap::Parser;

use virtual_system::{VirtualSystem, VirtualSystemBuilder};

mod config_parser;
mod module_parser;
mod transaction;
mod utils;
mod virtual_system;

#[derive(clap::Parser)]
#[command(author, version, about)]
struct CliArgs {
    #[arg(short, long, value_name = "FILE", default_value = "config.toml")]
    /// The configuration file to use
    config: PathBuf,

    #[arg(short, long, default_value = "false")]
    /// Show more detailed information for debugging
    verbose: bool,

    #[command(subcommand)]
    command: CliCommand,
}

#[derive(clap::Subcommand)]
enum CliCommand {
    /// Build the modules and generates a virtual filesystem
    Build {
        #[arg(short, long)]
        /// Name of the generated build
        name: Option<String>,
    },

    /// Deploy a build to the system
    Deploy {
        #[arg(short, long, value_name = "PATH")]
        /// Path to the build to deploy
        build: Option<PathBuf>,

        #[arg(long, default_value = "false")]
        /// Perform a hard deploy
        hard: bool,

        #[arg(short, long, default_value = "false")]
        /// Remove the targets before deployment (destructive, not advised)
        force: bool,
    },
    /// Clear the deployed files of the latest build
    Undeploy,

    /// Show information about the builds
    Info,
}

fn main() -> anyhow::Result<()> {
    let cli = CliArgs::parse();
    let config = utils::read_config(cli.config);
    match cli.command {
        CliCommand::Build { name } => {
            let build_path = VirtualSystemBuilder::from_config(&config).build(name, cli.verbose)?;
            utils::set_state(
                ".",
                &config.global,
                &build_path.clone().into_os_string().to_string_lossy(),
            )?;
            println!("Build complete at path {:?}", build_path)
        }
        CliCommand::Deploy {
            build: build_path,
            hard,
            force,
        } => {
            let ignore_filenames = utils::ignore_filenames(&config.global);
            let effective_build_path = if let Some(given_path) = build_path {
                given_path
            } else {
                utils::get_state(".", &config.global)
                    .context(format!(
                        "no state was found, explicitly supply the target using --build"
                    ))?
                    .into()
            };
            let virt_system = VirtualSystem::read(effective_build_path, &config.global.build_file)?
                .prepare_deployment(force, cli.verbose)
                .context("could not prepare for deployment")?;
            let res = if hard {
                virt_system.hard_deploy(&ignore_filenames, cli.verbose)
            } else {
                virt_system.soft_deploy(cli.verbose)
            }
            .context("deployment failed")?;
            res.display_report();
        }
        CliCommand::Undeploy => {
            let last_build_path = utils::get_state(".", &config.global)
                .context("no build was deployed, cannot undeploy")?
                .into();
            let virt_system = VirtualSystem::read(last_build_path, &config.global.build_file)?;
            virt_system
                .undeploy(cli.verbose)
                .context("undeployment failed")?
                .display_report();
        }
        CliCommand::Info => {
            let latest_build = utils::get_state(".", &config.global)
                .and_then(|s| VirtualSystem::read(s.into(), &config.global.build_file))
                .map(|vs| vs.name)
                .unwrap_or(String::from("N/A"));
            println!("Latest build: {:?}", latest_build);
            let virt_systems = glob::glob("./**/.dull-build")
                .context("could not query the filesystem for builds")?
                .flatten()
                .flat_map(|path| path.parent().map(|p| p.to_path_buf()))
                .flat_map(|path| VirtualSystem::read(path, &config.global.build_file));
            for virt_system in virt_systems {
                println!("=> build {:?} at {:?}", virt_system.name, virt_system.path);
            }
        }
    }
    Ok(())
}
