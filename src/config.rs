use std::net::IpAddr;

use clap::Parser;

#[derive(Parser, Debug)]
pub struct Config {
    #[arg(short, long, env, default_value = "127.0.0.1")]
    address: IpAddr,
    #[arg(short, long, env, default_value = "6379", value_parser = clap::value_parser!(u16).range(1024..=65535))]
    port: u16,
    #[arg(short, long, env, default_value = None)]
    archive_path: Option<std::path::PathBuf>,
}

pub fn get_config() -> Config {
    Config::parse()
}
