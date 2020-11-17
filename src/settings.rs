use config::{Config, File, FileFormat};
use failure::ResultExt;
use serde::Deserialize;
use slog_scope::{info, warn};
use std::path::PathBuf;

use crate::osm_reader::poi;
use crate::Error;

#[derive(Debug, Clone, Deserialize)]
pub struct StreetExclusion {
    pub highways: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Street {
    pub exclusion: StreetExclusion,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Admin {}

#[derive(Debug, Clone, Deserialize)]
pub struct Settings {
    pub street: Option<Street>,
    pub poi: Option<poi::PoiConfig>,
    pub admin: Option<Admin>,
}

impl Settings {
    pub fn new(config_dir: &Option<PathBuf>, name: &Option<String>) -> Result<Self, Error> {
        let mut config = Config::new();
        let config_dir = config_dir.clone();
        match config_dir {
            Some(mut dir) => {
                dir.push("default");
                // Start off by merging in the "default" configuration file

                if let Some(path) = dir.to_str() {
                    info!("using configuration from {}", path);
                    config.merge(File::with_name(path)).with_context(|e| {
                        format!(
                            "Could not merge default configuration from file {}: {}",
                            path, e
                        )
                    })?;
                } else {
                    return Err(failure::err_msg(format!(
                        "Could not read default settings in '{}'",
                        dir.display()
                    )));
                }

                dir.pop(); // remove the default
                           // If we provided a special configuration, merge it.
                if let Some(name) = name {
                    dir.push(name);

                    if let Some(path) = dir.to_str() {
                        info!("using configuration from {}", path);
                        config
                            .merge(File::with_name(path).required(true))
                            .with_context(|e| {
                                format!(
                                    "Could not merge {} configuration in file {}: {}",
                                    name, path, e
                                )
                            })?;
                    } else {
                        return Err(failure::err_msg(format!(
                            "Could not read configuration for '{}'",
                            name,
                        )));
                    }
                    dir.pop();
                }
            }
            None => {
                if name.is_some() {
                    // If the user set the 'settings' at the command line, he should
                    // also have used the 'config_dir' option. So we issue a warning,
                    // and leave with an error because the expected configuration can
                    // not be read.
                    warn!("settings option used without the 'config_dir' option. Please set the config directory with --config-dir.");
                    return Err(failure::err_msg(String::from(
                        "Could not build program settings",
                    )));
                }
                config
                    .merge(File::from_str(
                        include_str!("../config/default.toml"),
                        FileFormat::Toml,
                    ))
                    .with_context(|e| {
                        format!(
                            "Could not merge default configuration from file at compile time: {}",
                            e
                        )
                    })?;
            }
        }
        // You can deserialize (and thus freeze) the entire configuration as
        config.try_into().map_err(|e| {
            failure::err_msg(format!(
                "Could not generate settings from configuration: {}",
                e
            ))
        })
    }
}
