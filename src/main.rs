use std::path::PathBuf;

use anyhow::Context;
use config_parser::read_config;
use itertools::Itertools;
use rand::Rng;

use crate::{module_parser::ModuleParser, virtual_system::VirtualSystem};

mod config_parser;
mod module_parser;
mod virtual_system;

fn main() -> anyhow::Result<()> {
    let config = read_config("config.toml");
    let mut parsed_modules = vec![];
    for module_config in config.module.iter() {
        let parsed_module = ModuleParser::from_configs(module_config, &config.global).parse()?;
        parsed_modules.push(parsed_module);
    }
    let generated_links = parsed_modules
        .into_iter()
        .zip(config.module.iter())
        .flat_map(|(m, conf)| m.emplace(conf.target.clone()))
        .collect_vec();
    for link in generated_links.iter() {
        println!("{:?} => {:?}", link.abs_target, link.abs_source);
    }
    let build_id = format!(
        "{}-{}",
        chrono::Local::now().to_rfc3339(),
        rand::thread_rng().gen::<u32>()
    );
    let build_dir = PathBuf::from("builds").join(build_id);
    VirtualSystem::build(build_dir, generated_links)
        .context("virtual system generation failed, possibly conflicting modules")?
        .deploy(true)?;
    // glob("./*")
    //     .unwrap()
    //     .flatten()
    //     .map(Module::parse)
    //     .collect_vec();
    Ok(())
}
