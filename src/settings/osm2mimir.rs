// use failure::ResultExt;
// use mimir::rubber::{IndexSettings, Rubber};
// use mimirsbrunn::admin_geofinder::AdminGeoFinder;
// use mimirsbrunn::osm_reader::admin::read_administrative_regions;
// use mimirsbrunn::osm_reader::make_osm_reader;
// use mimirsbrunn::osm_reader::poi::{add_address, compute_poi_weight, pois, PoiConfig};
// use mimirsbrunn::osm_reader::street::{compute_street_weight, streets};
// use mimirsbrunn::settings::Settings;
// use slog_scope::{debug, info};
// use std::path::PathBuf;
use config::{Config, File, FileFormat};
use failure::ResultExt;
use serde::Deserialize;
use slog_scope::{info, warn};
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
    pub fn new(args: &Args) -> Result<Self, Error> {
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
        // For the flags (eg import_admin), we can't just test if a flag has been used at the
        // command line, because we can't use Option<bool>. We can, however, test if the value in
        // the args is different from the one set by default. If it is different, we use the value
        // from the command line.

        if let Some(dataset) = args.dataset.clone() {
            config
                .set("dataset", dataset)
                .with_context(|e| format!("Could not set dataset: {}", e))?;
        }

        //// ADMIN

        let settings_import_admin = config
            .get_bool("admin.import")
            .with_context(|e| format!("could not read flag admin.import from settings: {}", e))?;
        if args.import_admin != settings_import_admin {
            config
                .set("admin.import", args.import_admin)
                .with_context(|e| format!("Could not set import_admin: {}", e))?;
        }

        if let Some(city_level) = args.city_level {
            config
                .set("admin.city_level", i64::try_from(city_level)?)
                .with_context(|e| format!("Could not set city level: {}", e))?;
        }

        if let Some(level) = args.level.clone() {
            config
                .set(
                    "admin.levels",
                    level
                        .into_iter()
                        .filter_map(|l| i64::try_from(l).ok())
                        .collect::<Vec<_>>(),
                )
                .with_context(|e| format!("Could not set city level: {}", e))?;
        }

        //// WAY

        let settings_import_way = config
            .get_bool("way.import")
            .with_context(|e| format!("could not read flag way.import from settings: {}", e))?;
        if args.import_way != settings_import_way {
            config
                .set("way.import", args.import_way)
                .with_context(|e| format!("Could not set import_way: {}", e))?;
        }

        //// POI

        let settings_import_poi = config
            .get_bool("poi.import")
            .with_context(|e| format!("could not read flag poi.import from settings: {}", e))?;
        if args.import_poi != settings_import_poi {
            config
                .set("poi.import", args.import_poi)
                .with_context(|e| format!("Could not set import_poi: {}", e))?;
        }

        //// ELASTICSEARCH SETTINGS

        if let Some(connection_string) = args.connection_string.clone() {
            config
                .set("elasticsearch.connection_string", connection_string)
                .with_context(|e| {
                    format!("Could not set elasticsearch connection string: {}", e)
                })?;
        }

        if let Some(nb_way_shards) = args.nb_street_shards {
            config
                .set("elasticsearch.way_shards", i64::try_from(nb_way_shards)?)
                .with_context(|e| format!("Could not set way shards count: {}", e))?;
        }
        if let Some(nb_way_replicas) = args.nb_street_replicas {
            config
                .set(
                    "elasticsearch.way_replicas",
                    i64::try_from(nb_way_replicas)?,
                )
                .with_context(|e| format!("Could not set way replicas count: {}", e))?;
        }
        if let Some(nb_poi_shards) = args.nb_poi_shards {
            config
                .set("elasticsearch.poi_shards", i64::try_from(nb_poi_shards)?)
                .with_context(|e| format!("Could not set poi shards count: {}", e))?;
        }
        if let Some(nb_poi_replicas) = args.nb_poi_replicas {
            config
                .set(
                    "elasticsearch.poi_replicas",
                    i64::try_from(nb_poi_replicas)?,
                )
                .with_context(|e| format!("Could not set poi replicas count: {}", e))?;
        }
        if let Some(nb_admin_shards) = args.nb_admin_shards {
            config
                .set(
                    "elasticsearch.admin_shards",
                    i64::try_from(nb_admin_shards)?,
                )
                .with_context(|e| format!("Could not set admin shards count: {}", e))?;
        }
        if let Some(nb_admin_replicas) = args.nb_admin_replicas {
            config
                .set(
                    "elasticsearch.admin_replicas",
                    i64::try_from(nb_admin_replicas)?,
                )
                .with_context(|e| format!("Could not set admin replicas count: {}", e))?;
        }
        if let Some(nb_insert_threads) = args.nb_insert_threads {
            config
                .set(
                    "elasticsearch.insert_thread_count",
                    i64::try_from(nb_insert_threads)?,
                )
                .with_context(|e| {
                    format!("Could not set elasticsearch insert thread count: {}", e)
                })?;
        }

        //// DATABASE

        // /// If you use this option by providing a filename, then we
        // /// will use a SQlite database that will be persisted. You
        // /// can only do that if osm2mimir was compiled with the
        // /// 'db-storage' feature. If you don't provide a value, then
        // /// we will use in memory storage.
        // #[structopt(long = "db-file", parse(from_os_str))]
        // db_file: Option<PathBuf>,
        // /// DB buffer size.
        // #[structopt(long = "db-buffer-size", default_value = "50000")]
        // db_buffer_size: usize,
        #[cfg(feature = "db-storage")]
        if let Some(db_file) = args.db_file.clone() {
            config
                .set(
                    "database.file",
                    db_file.to_str().expect("valid utf-8 filename"),
                )
                .with_context(|e| format!("Could not set database file: {}", e))?;
        }

        #[cfg(feature = "db-storage")]
        if let Some(db_buffer_size) = args.db_buffer_size {
            config
                .set("database.buffer_size", i64::try_from(db_buffer_size)?)
                .with_context(|e| format!("Could not set database buffer size: {}", e))?;
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

#[derive(StructOpt, Debug)]
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
    db_file: Option<PathBuf>,

    /// DB buffer size.
    #[cfg(feature = "db-storage")]
    #[structopt(long = "db-buffer-size")]
    db_buffer_size: Option<usize>,
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
