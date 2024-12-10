use std::collections::HashMap;

use serde::Deserialize;
use serde_json::Error;

#[derive(Clone)]
pub struct RuntimeConfig {
    pub onebot: OneBotConfig,
    pub notifications: HashMap<String, NotifyConfig>,
}

#[derive(Deserialize, Clone)]
pub struct Config {
    pub server: ServerConfig,
    pub onebot: OneBotConfig,
    pub log: Option<LogConfig>,
    pub notifications: Vec<NotifyConfig>,
}

impl Config {
    pub fn parse(conf: String) -> Result<Config, Error> {
        serde_json::from_str(conf.as_str())
    }
}

#[derive(Deserialize, Clone)]
pub struct ServerConfig {
    pub ip: String,
    pub port: u16,
}

#[derive(Deserialize, Clone)]
pub struct OneBotConfig {
    pub url: String,
}

#[derive(Deserialize, Clone)]
pub struct LogConfig {
    pub path: Option<String>,
    pub size: Option<u64>,
    pub backup: Option<u32>,
    pub compress: Option<bool>,
}

impl LogConfig {
    pub fn default() -> LogConfig {
        LogConfig {
            path: None,
            size: None,
            backup: None,
            compress: None,
        }
    }
}

#[derive(Deserialize, Clone)]
pub struct NotifyConfig {
    // ID of the notification service
    pub id: String,

    // Token for the notification service
    pub token: Option<String>,

    // Notification targets
    pub groups: Option<Vec<String>>,
    pub users: Option<Vec<String>>,

    // Notification content
    pub message: String,
    pub mentions: Option<Vec<String>>,

    // Custom content extraction
    pub extra: Option<bool>,
    pub extractors: Option<Vec<ContentExtractConfig>>,
}

#[derive(Deserialize, Clone)]
pub struct ContentExtractConfig {
    pub name: String,
    pub path: String,
    pub fallback: Option<String>,
    pub sep: Option<String>,
}
