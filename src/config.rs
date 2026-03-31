use clap::Parser;
use core::fmt;
use secrecy::SecretBox;
use std::{net::IpAddr, path::PathBuf, sync::Arc};

#[derive(Parser)]
struct RawConfig {
    #[arg(short, long, env, default_value = "127.0.0.1")]
    pub address: IpAddr,
    #[arg(short, long, env, default_value = "6379", value_parser = clap::value_parser!(u16).range(1024..=65535))]
    pub port: u16,
    #[arg(short = 'r', long, env, default_value = None)]
    pub archive_path: Option<std::path::PathBuf>,
    #[arg(long, env = "AUTH_PASSWORD", hide_env_values = true)]
    pub auth_password: Option<String>,
}

pub struct Config {
    pub address: IpAddr,
    pub port: u16,
    pub archive_path: Option<PathBuf>,
    pub auth_password: Option<Arc<SecretBox<Vec<u8>>>>,
}

impl Config {
    fn from_raw(raw: RawConfig) -> Self {
        Config {
            address: raw.address,
            port: raw.port,
            archive_path: raw.archive_path,
            auth_password: raw
                .auth_password
                .map(|p| Arc::new(SecretBox::new(Box::new(p.into_bytes())))),
        }
    }

    pub fn try_parse_from<I, T>(itr: I) -> Result<Self, clap::Error>
    where
        I: IntoIterator<Item = T>,
        T: Into<std::ffi::OsString> + Clone,
    {
        RawConfig::try_parse_from(itr).map(Self::from_raw)
    }
}

impl fmt::Debug for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let redacted_password = self.auth_password.as_ref().map(|_| "-----");

        f.debug_struct("Config")
            .field("address", &self.address)
            .field("port", &self.port)
            .field("archive_path", &self.archive_path)
            .field("auth_password", &redacted_password)
            .finish()
    }
}

pub fn get_config() -> Config {
    Config::from_raw(RawConfig::parse())
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
    fn debug_formatting_hides_password() {
        let config = Config::try_parse_from([
            "redlike",
            "--address",
            "127.0.0.2",
            "--port",
            "6380",
            "--archive-path",
            "/tmp/redlike.rdb",
            "--auth-password",
            "test_password",
        ])
        .unwrap();

        let debug = format!("{:?}", config);

        assert!(!debug.contains("test_password"));
        assert_eq!(
            "Config { address: 127.0.0.2, port: 6380, archive_path: Some(\"/tmp/redlike.rdb\"), auth_password: Some(\"-----\") }",
            debug
        )
    }

    #[test]
    fn debug_formatting_shows_none_when_password_is_absent() {
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

        assert_eq!(
            "Config { address: 127.0.0.2, port: 6380, archive_path: Some(\"/tmp/redlike.rdb\"), auth_password: None }",
            format!("{:?}", config)
        );
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
