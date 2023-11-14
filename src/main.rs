use std::path::PathBuf;

use anyhow::Context;
use clap::Parser;

use virtual_system::{VirtualSystem, VirtualSystemBuilder};

mod config_parser;
mod module_parser;
mod utils;
mod virtual_system;

#[derive(clap::Parser)]
#[command(author, version, about)]
struct CliArgs {
    #[arg(short, long, value_name = "FILE", default_value = "config.toml")]
    config: PathBuf,
    #[arg(short, long, default_value = "false")]
    verbose: bool,
    #[command(subcommand)]
    command: CliCommand,
}

#[derive(clap::Subcommand)]
enum CliCommand {
    Build {
        #[arg(short, long)]
        name: Option<String>,
    },
    Deploy {
        #[arg(short, long, value_name = "PATH")]
        build: Option<PathBuf>,
        #[arg(long, default_value = "false")]
        hard: bool,
        #[arg(short, long, default_value = "false")]
        force: bool,
    },
    Undeploy,
    Info,
}

fn main() -> anyhow::Result<()> {
    let cli = CliArgs::parse();
    let config = utils::read_config(cli.config);
    match cli.command {
        CliCommand::Build { name } => {
            let virt_system =
                VirtualSystemBuilder::from_config(&config).build(name, cli.verbose)?;
            utils::set_state(".", &config.global, virt_system.path.to_str().unwrap())?;
            println!(
                "build {:?} complete at path {:?}",
                virt_system.name, virt_system.path
            )
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
                utils::read_state(".", &config.global)
                    .context(format!(
                        "no state was found, explicitly supply the target using --build"
                    ))?
                    .into()
            };
            let virt_system = VirtualSystem::read(effective_build_path, &config.global.build_file)?;
            virt_system.deploy(hard, force, cli.verbose, &ignore_filenames)?;
        }
        CliCommand::Undeploy => {
            let last_build_path = utils::read_state(".", &config.global)
                .context("no build was deployed, cannot undeploy")?
                .into();
            let virt_system = VirtualSystem::read(last_build_path, &config.global.build_file)?;
            virt_system.undeploy(cli.verbose)?;
        }
        CliCommand::Info => todo!(),
    }
    Ok(())
}
