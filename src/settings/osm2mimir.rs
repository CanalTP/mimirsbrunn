use config::{Config, ConfigError, File, FileFormat, Source, Value};
use failure::ResultExt;
use serde::Deserialize;
use slog_scope::{info, warn};
use std::collections::HashMap;
use std::convert::TryFrom;
use std::path::PathBuf;
use structopt::StructOpt;

use crate::osm_reader::poi;
use crate::Error;

#[derive(Debug, Clone, Deserialize)]
pub struct StreetExclusion {
    pub highways: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Way {
    pub import: bool,
    pub exclusion: StreetExclusion,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Admin {
    pub import: bool,
    pub levels: Vec<u32>,
    pub city_level: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Poi {
    pub import: bool,
    pub config: Option<poi::PoiConfig>,
}

#[cfg(feature = "db-storage")]
#[derive(Debug, Clone, Deserialize)]
pub struct Database {
    pub file: PathBuf,
    pub buffer_size: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Elasticsearch {
    pub connection_string: String,
    pub insert_thread_count: usize,
    pub streets_shards: usize,
    pub streets_replicas: usize,
    pub admins_shards: usize,
    pub admins_replicas: usize,
    pub pois_shards: usize,
    pub pois_replicas: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Settings {
    pub dataset: String,
    #[cfg(feature = "db-storage")]
    pub database: Option<Database>,
    pub elasticsearch: Elasticsearch,
    pub way: Option<Way>,
    pub poi: Option<Poi>,
    pub admin: Option<Admin>,
}

impl Settings {
    // To create settings, we first retrieve default settings, merge in specific settings if
    // needed, and finally override them with command line arguments.
    pub fn new(args: Args) -> Result<Self, Error> {
        let config_dir = args.config_dir.clone();
        let settings = args.settings.clone();

        let mut config = Config::new();
        // let config_dir = config_dir.clone();
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
                if let Some(settings) = settings {
                    dir.push(&settings);

                    if let Some(path) = dir.to_str() {
                        info!("using configuration from {}", path);
                        config
                            .merge(File::with_name(path).required(true))
                            .with_context(|e| {
                                format!(
                                    "Could not merge {} configuration in file {}: {}",
                                    settings, path, e
                                )
                            })?;
                    } else {
                        return Err(failure::err_msg(format!(
                            "Could not read configuration for '{}'",
                            settings,
                        )));
                    }
                    dir.pop();
                }
            }
            None => {
                if settings.is_some() {
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
                        include_str!("../../config/default.toml"),
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

        // Now override with command line values
        config
            .merge(args)
            .with_context(|e| format!("Could not merge arguments into configuration: {}", e))?;

        // You can deserialize (and thus freeze) the entire configuration as
        config.try_into().map_err(|e| {
            failure::err_msg(format!(
                "Could not generate settings from configuration: {}",
                e
            ))
        })
    }
}

#[derive(StructOpt, Clone, Debug)]
pub struct Args {
    /// OSM PBF file.
    #[structopt(short = "i", long = "input", parse(from_os_str))]
    pub input: PathBuf,
    /// Admin levels to keep.
    #[structopt(short = "l", long = "level")]
    level: Option<Vec<u32>>,
    /// City level to  calculate weight.
    #[structopt(short = "C", long = "city-level")]
    city_level: Option<u32>,
    /// Elasticsearch parameters.
    #[structopt(short = "c", long = "connection-string")]
    connection_string: Option<String>,
    /// Import ways.
    #[structopt(short = "w", long = "import-way")]
    import_way: bool,
    /// Import admins.
    #[structopt(short = "a", long = "import-admin")]
    import_admin: bool,
    /// Import POIs.
    #[structopt(short = "p", long = "import-poi")]
    import_poi: bool,
    /// Name of the dataset.
    #[structopt(short = "d", long = "dataset")]
    pub dataset: Option<String>,
    /// Number of shards for the admin es index
    #[structopt(long = "nb-admin-shards")]
    nb_admin_shards: Option<usize>,
    /// Number of replicas for the es index
    #[structopt(long = "nb-admin-replicas")]
    nb_admin_replicas: Option<usize>,
    /// Number of shards for the street es index
    #[structopt(long = "nb-street-shards")]
    nb_street_shards: Option<usize>,
    /// Number of replicas for the street es index
    #[structopt(long = "nb-street-replicas")]
    nb_street_replicas: Option<usize>,
    /// Number of shards for the es index
    #[structopt(long = "nb-poi-shards")]
    nb_poi_shards: Option<usize>,
    /// Number of replicas for the es index
    #[structopt(long = "nb-poi-replicas")]
    nb_poi_replicas: Option<usize>,
    /// If you use this option by providing a filename, then we
    /// will use a SQlite database that will be persisted. You
    /// can only do that if osm2mimir was compiled with the
    /// 'db-storage' feature. If you don't provide a value, then
    /// we will use in memory storage.
    #[cfg(feature = "db-storage")]
    #[structopt(long = "db-file", parse(from_os_str))]
    pub db_file: Option<PathBuf>,

    /// DB buffer size.
    #[cfg(feature = "db-storage")]
    #[structopt(long = "db-buffer-size")]
    pub db_buffer_size: Option<usize>,
    /// Number of threads to use to insert into Elasticsearch. Note that Elasticsearch is not able
    /// to handle values that are too high.
    #[structopt(short = "T", long = "nb-insert-threads")]
    nb_insert_threads: Option<usize>,

    /// Path to the config directory
    /// osm2mimir will read the default configuration in there, and maybe
    /// more depending on the settings option.
    /// If no option is given, we'll just read the ./config/default.toml
    /// at compile time.
    #[structopt(short = "D", long = "config-dir")]
    config_dir: Option<PathBuf>,

    /// Specific configuration, on top of the default ones.
    /// You should provide the basename of the file, eg acme, so that
    /// osm2mimir will use {config-dir}/acme.toml. (Requires config_dir to
    /// be set)
    #[structopt(short = "s", long = "settings")]
    settings: Option<String>,
}

impl Source for Args {
    fn clone_into_box(&self) -> Box<dyn Source + Send + Sync> {
        Box::new((*self).clone())
    }

    fn collect(&self) -> Result<HashMap<String, Value>, ConfigError> {
        let mut m = HashMap::new();

        // DATASET
        if let Some(dataset) = self.dataset.clone() {
            m.insert(String::from("dataset"), Value::new(None, dataset));
        }

        // ADMIN
        m.insert(
            String::from("admin.import"),
            Value::new(None, self.import_admin),
        );
        if let Some(city_level) = self.city_level {
            m.insert(
                String::from("admin.city_level"),
                Value::new(
                    None,
                    i64::try_from(city_level).map_err(|e| {
                        ConfigError::Message(format!(
                            "Could not convert admin city_level to integer: {}",
                            e
                        ))
                    })?,
                ),
            );
        }
        if let Some(level) = self.level.clone() {
            m.insert(
                String::from("admin.levels"),
                Value::new(
                    None,
                    level.into_iter().try_fold(Vec::new(), |mut acc, l| {
                        let i = i64::try_from(l).map_err(|e| {
                            ConfigError::Message(format!(
                                "Could not convert admin city_level to integer: {}",
                                e
                            ))
                        })?;
                        acc.push(i);
                        Ok(acc)
                    })?,
                ),
            );
        }

        // WAY
        m.insert(
            String::from("way.import"),
            Value::new(None, self.import_way),
        );

        // POI
        m.insert(
            String::from("poi.import"),
            Value::new(None, self.import_poi),
        );

        // ELASTICSEARCH SETTINGS

        if let Some(connection_string) = self.connection_string.clone() {
            m.insert(
                String::from("elasticsearch.connection_string"),
                Value::new(None, connection_string),
            );
        }

        if let Some(nb_way_shards) = self.nb_street_shards {
            m.insert(
                String::from("elasticsearch.way_shards"),
                Value::new(
                    None,
                    i64::try_from(nb_way_shards).map_err(|e| {
                        ConfigError::Message(format!(
                            "Could not convert count of way shards to integer: {}",
                            e
                        ))
                    })?,
                ),
            );
        }

        if let Some(nb_way_replicas) = self.nb_street_replicas {
            m.insert(
                String::from("elasticsearch.way_replicas"),
                Value::new(
                    None,
                    i64::try_from(nb_way_replicas).map_err(|e| {
                        ConfigError::Message(format!(
                            "Could not convert count of way replicas to integer: {}",
                            e
                        ))
                    })?,
                ),
            );
        }

        if let Some(nb_poi_shards) = self.nb_poi_shards {
            m.insert(
                String::from("elasticsearch.poi_shards"),
                Value::new(
                    None,
                    i64::try_from(nb_poi_shards).map_err(|e| {
                        ConfigError::Message(format!(
                            "Could not convert count of poi shards to integer: {}",
                            e
                        ))
                    })?,
                ),
            );
        }

        if let Some(nb_poi_replicas) = self.nb_poi_replicas {
            m.insert(
                String::from("elasticsearch.poi_replicas"),
                Value::new(
                    None,
                    i64::try_from(nb_poi_replicas).map_err(|e| {
                        ConfigError::Message(format!(
                            "Could not convert count of poi replicas to integer: {}",
                            e
                        ))
                    })?,
                ),
            );
        }

        if let Some(nb_admin_shards) = self.nb_admin_shards {
            m.insert(
                String::from("elasticsearch.admin_shards"),
                Value::new(
                    None,
                    i64::try_from(nb_admin_shards).map_err(|e| {
                        ConfigError::Message(format!(
                            "Could not convert count of admin shards to integer: {}",
                            e
                        ))
                    })?,
                ),
            );
        }

        if let Some(nb_admin_replicas) = self.nb_admin_replicas {
            m.insert(
                String::from("elasticsearch.admin_replicas"),
                Value::new(
                    None,
                    i64::try_from(nb_admin_replicas).map_err(|e| {
                        ConfigError::Message(format!(
                            "Could not convert count of admin replicas to integer: {}",
                            e
                        ))
                    })?,
                ),
            );
        }

        if let Some(nb_insert_threads) = self.nb_insert_threads {
            m.insert(
                String::from("elasticsearch.insert_thread_count"),
                Value::new(
                    None,
                    i64::try_from(nb_insert_threads).map_err(|e| {
                        ConfigError::Message(format!(
                            "Could not convert elasticsearch insert thread count to integer: {}",
                            e
                        ))
                    })?,
                ),
            );
        }

        // DATABASE
        #[cfg(feature = "db-storage")]
        if let Some(db_file) = self.db_file.clone() {
            m.insert(
                String::from("database.file"),
                Value::new(None, db_file.to_str().expect("valid utf-8 filename")),
            );
        }

        #[cfg(feature = "db-storage")]
        if let Some(db_buffer_size) = self.db_buffer_size {
            m.insert(
                String::from("database.buffer_size"),
                Value::new(
                    None,
                    i64::try_from(db_buffer_size).map_err(|e| {
                        ConfigError::Message(format!(
                            "Could not convert database buffer size to integer: {}",
                            e
                        ))
                    })?,
                ),
            );
        }

        Ok(m)
    }
}
