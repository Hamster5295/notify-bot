mod config;
mod service;

use std::{
    collections::HashMap,
    fs::{self},
    io::Result,
    path::Path,
    time::Duration,
};

use actix_web::{web::Data, App, HttpServer};
use clap::{command, Parser};
use config::{Config, LogConfig, RuntimeConfig};
use reqwest::Client;
use service::notify;
use tklog::{info, warn, Format, LEVEL, LOG};

#[derive(Parser)]
#[command(version, about)]
/// A simple notification server for QQBot.
struct Args {
    /// The config file path.
    #[arg(short, long)]
    config: Option<String>,
}

#[actix_web::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    LOG.set_console(true)
        .set_level(LEVEL::Info)
        .set_format(Format::LevelFlag | Format::Time | Format::ShortFileName)
        .set_formatter("{level} {time} {file}\t> {message}\n")
        .set_attr_format(|attr_fmt| {
            attr_fmt.set_body_fmt(|lvl, body| {
                match lvl {
                    LEVEL::Trace => format!("{}{}{}", "\x1b[34m", body, "\x1b[0m"), //blue
                    LEVEL::Debug => format!("{}{}{}", "\x1b[36m", body, "\x1b[0m"), //cyan
                    LEVEL::Info => format!("{}{}{}", "\x1b[32m", body, "\x1b[0m"),  //green
                    LEVEL::Warn => format!("{}{}{}", "\x1b[33m", body, "\x1b[0m"),  //yellow
                    LEVEL::Error => format!("{}{}{}", "\x1b[31m", body, "\x1b[0m"), //red
                    LEVEL::Fatal => format!("{}{}{}", "\x1b[41m", body, "\x1b[0m"), //red-background
                    LEVEL::Off => "".to_string(),
                }
            });
            attr_fmt.set_level_fmt(|lvl| {
                match lvl {
                    LEVEL::Trace => "[T]",
                    LEVEL::Debug => "[D]",
                    LEVEL::Info => "[I]",
                    LEVEL::Warn => "[W]",
                    LEVEL::Error => "[E]",
                    LEVEL::Fatal => "[F]",
                    LEVEL::Off => "",
                }
                .to_string()
            })
        });

    let conf_path = if let Some(path) = args.config {
        path
    } else {
        println!("No Config File Specified. Looking for ./config.json...");
        "./config.json".to_string()
    };
    let conf_path = Path::new(&conf_path);

    let config_exists = fs::exists(&conf_path);
    match config_exists {
        Ok(exists) => {
            if !exists {
                println!("\x1b[31mConfig file {:?} is not found!", conf_path);
                println!("Use --config or -c to explicitly specify one.\x1b[0m");
                return Ok(());
            }
        }
        Err(e) => {
            println!("Failed to check if config file exists: {}", e);
            return Ok(());
        }
    }

    println!(
        "\x1b[34mUsing Config File '{}'.\x1b[0m",
        conf_path.canonicalize().unwrap().display()
    );

    let conf = fs::read_to_string(&conf_path);
    if let Err(e) = conf {
        println!("\x1b[31mFailed to read config file: {}\x1b[0m", e);
        return Ok(());
    }

    let conf = Config::parse(conf.ok().unwrap());
    if let Err(e) = conf {
        println!("\x1b[31mFailed to parse config file: {}\nMight be caused by incorrect json syntax or missing fields.\x1b[0m", e);
        return Ok(());
    }
    let conf = conf.ok().unwrap();

    println!();

    let log_conf = conf.log.clone().unwrap_or(LogConfig::default());
    LOG.set_cutmode_by_size(
        &log_conf.path.unwrap_or("notify_bot.log".to_string()),
        log_conf.size.unwrap_or(1 << 20),
        log_conf.backup.unwrap_or(1),
        log_conf.compress.unwrap_or(true),
    );

    info!("Config Loaded");

    let ip = conf.server.ip.clone();
    let port = conf.server.port;
    info!(format!("Server Listening at {}:{}", ip, port));

    let server = HttpServer::new(move || {
        let mut notifications = HashMap::new();
        for n in &conf.notifications {
            notifications.insert(n.id.clone(), n.clone());
            if n.groups.is_none() && n.people.is_none() {
                warn!(format!(
                    "Notification with ID [{}] has no group or person specified. It won't take any effect.",
                    n.id
                ));
            }
        }

        let runtime_conf = RuntimeConfig {
            onebot: conf.onebot.clone(),
            notifications,
        };

        let client = Client::builder()
            .timeout(Duration::from_secs(1))
            .build()
            .unwrap();

        App::new()
            .service(notify)
            .app_data(Data::new(runtime_conf))
            .app_data(Data::new(client))
    })
    .bind((ip, port))?
    .run();
    server.await
}
