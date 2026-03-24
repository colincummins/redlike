use std::net::IpAddr;

use clap::Parser;

#[derive(Parser, Debug)]
pub struct Config {
    #[arg(short, long, env, default_value = "127.0.0.1")]
    pub address: IpAddr,
    #[arg(short, long, env, default_value = "6379", value_parser = clap::value_parser!(u16).range(1024..=65535))]
    pub port: u16,
    #[arg(short = 'r', long, env, default_value = None)]
    pub archive_path: Option<std::path::PathBuf>,
}

pub fn get_config() -> Config {
    Config::parse()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::{Mutex, OnceLock};

    fn env_lock() -> &'static Mutex<()> {
        static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        ENV_LOCK.get_or_init(|| Mutex::new(()))
    }

    fn set_env_var(key: &str, value: &str) {
        unsafe {
            std::env::set_var(key, value);
        }
    }

    fn remove_env_var(key: &str) {
        unsafe {
            std::env::remove_var(key);
        }
    }

    #[test]
    fn defaults_apply_when_no_args_or_env_are_present() {
        let _guard = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        remove_env_var("ADDRESS");
        remove_env_var("PORT");
        remove_env_var("ARCHIVE_PATH");

        let config = Config::try_parse_from(["redlike"]).unwrap();

        assert_eq!(config.address, "127.0.0.1".parse::<IpAddr>().unwrap());
        assert_eq!(config.port, 6379);
        assert_eq!(config.archive_path, None);
    }

    #[test]
    fn long_flags_override_defaults() {
        let config = Config::try_parse_from([
            "redlike",
            "--address",
            "127.0.0.2",
            "--port",
            "6380",
            "--archive-path",
            "/tmp/redlike.rdb",
        ])
        .unwrap();

        assert_eq!(config.address, "127.0.0.2".parse::<IpAddr>().unwrap());
        assert_eq!(config.port, 6380);
        assert_eq!(config.archive_path, Some(PathBuf::from("/tmp/redlike.rdb")));
    }

    #[test]
    fn short_flags_override_defaults() {
        let config = Config::try_parse_from([
            "redlike",
            "-a",
            "127.0.0.3",
            "-p",
            "6381",
            "-r",
            "/tmp/redlike-short.rdb",
        ])
        .unwrap();

        assert_eq!(config.address, "127.0.0.3".parse::<IpAddr>().unwrap());
        assert_eq!(config.port, 6381);
        assert_eq!(
            config.archive_path,
            Some(PathBuf::from("/tmp/redlike-short.rdb"))
        );
    }

    #[test]
    fn env_vars_fill_values_when_args_are_absent() {
        let _guard = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        remove_env_var("ADDRESS");
        remove_env_var("PORT");
        remove_env_var("ARCHIVE_PATH");
        set_env_var("ADDRESS", "127.0.0.4");
        set_env_var("PORT", "6382");
        set_env_var("ARCHIVE_PATH", "/tmp/redlike-env.rdb");

        let config = Config::try_parse_from(["redlike"]).unwrap();

        assert_eq!(config.address, "127.0.0.4".parse::<IpAddr>().unwrap());
        assert_eq!(config.port, 6382);
        assert_eq!(
            config.archive_path,
            Some(PathBuf::from("/tmp/redlike-env.rdb"))
        );
    }

    #[test]
    fn cli_args_take_precedence_over_env_vars() {
        let _guard = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        remove_env_var("ADDRESS");
        remove_env_var("PORT");
        remove_env_var("ARCHIVE_PATH");
        set_env_var("ADDRESS", "127.0.0.4");
        set_env_var("PORT", "6382");
        set_env_var("ARCHIVE_PATH", "/tmp/redlike-env.rdb");

        let config = Config::try_parse_from([
            "redlike",
            "--address",
            "127.0.0.5",
            "--port",
            "6383",
            "--archive-path",
            "/tmp/redlike-cli.rdb",
        ])
        .unwrap();

        assert_eq!(config.address, "127.0.0.5".parse::<IpAddr>().unwrap());
        assert_eq!(config.port, 6383);
        assert_eq!(
            config.archive_path,
            Some(PathBuf::from("/tmp/redlike-cli.rdb"))
        );
    }

    #[test]
    fn invalid_port_is_rejected() {
        let result = Config::try_parse_from(["redlike", "--port", "1000"]);

        assert!(result.is_err());
    }

    #[test]
    fn invalid_ip_address_is_rejected() {
        let result = Config::try_parse_from(["redlike", "--address", "not-an-ip"]);

        assert!(result.is_err());
    }
}
