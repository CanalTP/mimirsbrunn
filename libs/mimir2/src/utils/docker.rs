use bollard::service::{HostConfig, PortBinding};
use bollard::{
    container::{Config, CreateContainerOptions, ListContainersOptions, StartContainerOptions},
    errors::Error as BollardError,
    Docker,
};
use elasticsearch::{
    http::transport::{
        BuildError as TransportBuilderError, SingleNodeConnectionPool, TransportBuilder,
    },
    indices::{IndicesDeleteAliasParts, IndicesDeleteIndexTemplateParts, IndicesDeleteParts},
    Elasticsearch, Error as ElasticsearchError,
};
use lazy_static::lazy_static;
use snafu::{ResultExt, Snafu};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::time::{sleep, Duration};
use url::Url;

lazy_static! {
    pub static ref AVAILABLE: Arc<Mutex<()>> = Arc::new(Mutex::new(()));
}

// pub async fn initialize() -> Result<MutexGuard<'static, ()>, Error> {
pub async fn initialize() -> Result<(), Error> {
    // let mtx = Arc::clone(&AVAILABLE);
    let mut docker = DockerWrapper::new();
    // let guard = AVAILABLE.lock().unwrap();
    let is_available = docker.is_container_available().await?;
    if !is_available {
        docker.create_container().await
    } else {
        docker.cleanup().await
    }
}

#[derive(Debug, Snafu)]
pub enum Error {
    #[snafu(display("Connection to docker socket: {}", source))]
    Connection { source: BollardError },

    #[snafu(display("docker version: {}", source))]
    Version { source: BollardError },

    #[snafu(display("url parsing error: {}", source))]
    UrlParse { source: url::ParseError },

    #[snafu(display("elasticsearch transport error: {}", source))]
    ElasticsearchTransport { source: TransportBuilderError },

    #[snafu(display("elasticsearch transport error: {}", source))]
    ElasticsearchClient { source: ElasticsearchError },

    #[snafu(display("docker error: {}", source))]
    DockerError { source: BollardError },
}

pub struct DockerWrapper {
    ports: Vec<(u32, u32)>, // list of ports to publish (host port, container port)
    docker_image: String,
    container_name: String, // ip: String,
}

impl DockerWrapper {
    pub fn new() -> DockerWrapper {
        DockerWrapper {
            ports: vec![(9201, 9200), (9301, 9300)],
            docker_image: String::from("docker.elastic.co/elasticsearch/elasticsearch:7.13.0"),
            container_name: String::from("mimir-test-elasticsearch"),
        }
    }

    // Returns true if the container self.container_name is running
    // TODO Probably should run a check on Elasticsearch status
    pub async fn is_container_available(&mut self) -> Result<bool, Error> {
        let docker = Docker::connect_with_unix(
            "unix:///var/run/docker.sock",
            120,
            &bollard::ClientVersion {
                major_version: 1,
                minor_version: 24,
            },
        )
        .context(Connection)?;

        let docker = &docker.negotiate_version().await.context(Version)?;

        &docker.version().await.context(Version);

        let mut filters = HashMap::new();
        filters.insert("name", vec![self.container_name.as_str()]);

        let options = Some(ListContainersOptions {
            all: false, // only running containers
            filters,
            ..Default::default()
        });

        let containers = docker.list_containers(options).await.context(DockerError)?;

        Ok(!containers.is_empty())
    }

    // If the container is already created, then start it.
    // If it is not created, then create it and start it.
    pub async fn create_container(&mut self) -> Result<(), Error> {
        let docker = Docker::connect_with_unix(
            "unix:///var/run/docker.sock",
            120,
            &bollard::ClientVersion {
                major_version: 1,
                minor_version: 24,
            },
        )
        .context(Connection)?;

        let docker = docker.negotiate_version().await.context(Version)?;

        let _ = docker.version().await.context(Version);

        let mut filters = HashMap::new();
        filters.insert("name", vec![self.container_name.as_str()]);

        let options = Some(ListContainersOptions {
            all: true, // only running containers
            filters,
            ..Default::default()
        });

        let containers = docker.list_containers(options).await.context(DockerError)?;

        if containers.is_empty() {
            let options = CreateContainerOptions {
                name: &self.container_name,
            };

            let mut port_bindings = HashMap::new();
            for (host_port, container_port) in self.ports.iter() {
                port_bindings.insert(
                    format!("{}/tcp", &container_port),
                    Some(vec![PortBinding {
                        host_ip: Some(String::from("0.0.0.0")),
                        host_port: Some(host_port.to_string()),
                    }]),
                );
            }

            let host_config = HostConfig {
                port_bindings: Some(port_bindings),
                ..Default::default()
            };

            let mut exposed_ports = HashMap::new();
            self.ports.iter().for_each(|(_, container)| {
                let v: HashMap<(), ()> = HashMap::new();
                exposed_ports.insert(format!("{}/tcp", container), v);
            });

            let env_vars = vec![String::from("discovery.type=single-node")];

            let config = Config {
                image: Some(String::from(self.docker_image.clone())),
                exposed_ports: Some(exposed_ports),
                host_config: Some(host_config),
                env: Some(env_vars),
                ..Default::default()
            };

            let _ = docker
                .create_container(Some(options), config)
                .await
                .context(DockerError)?;

            sleep(Duration::from_secs(5)).await;
        }
        let _ = docker
            .start_container(&self.container_name, None::<StartContainerOptions<String>>)
            .await
            .context(DockerError)?;

        sleep(Duration::from_secs(15)).await;

        Ok(())
    }

    async fn cleanup(&mut self) -> Result<(), Error> {
        let port = self.ports[0].0;
        // FIXME Hardcoded URL, need to extract it from self.
        let url = Url::parse(&format!("http://localhost:{}", port)).context(UrlParse)?;
        let conn_pool = SingleNodeConnectionPool::new(url);
        let transport = TransportBuilder::new(conn_pool)
            .disable_proxy()
            .build()
            .context(ElasticsearchTransport)?;
        let client = Elasticsearch::new(transport);

        let _ = client
            .indices()
            .delete(IndicesDeleteParts::Index(&["*"]))
            .send()
            .await
            .context(ElasticsearchClient)?;

        let _ = client
            .indices()
            .delete_alias(IndicesDeleteAliasParts::IndexName(&["*"], &["*"]))
            .send()
            .await
            .context(ElasticsearchClient)?;

        let _ = client
            .indices()
            .delete_index_template(IndicesDeleteIndexTemplateParts::Name("*"))
            .send()
            .await
            .context(ElasticsearchClient)?;

        println!("done with cleanup");
        sleep(Duration::from_secs(1)).await;
        Ok(())
    }

    async fn _drop(&mut self) {
        if std::env::var("DONT_KILL_THE_WHALE") == Ok("1".to_string()) {
            println!(
                "the docker won't be stoped at the end, you can debug it.
                Note: ES has been mapped to the port 9242 in you localhost
                manually stop and rm the container mimirsbrunn_tests after debug"
            );
            return;
        }
        let docker = Docker::connect_with_unix(
            "unix:///var/run/docker.sock",
            120,
            &bollard::ClientVersion {
                major_version: 1,
                minor_version: 24,
            },
        )
        .expect("docker connection");

        let options = Some(bollard::container::StopContainerOptions { t: 0 });
        docker
            .stop_container(&self.container_name, options)
            .await
            .expect("stop container");

        let options = Some(bollard::container::RemoveContainerOptions {
            force: true,
            ..Default::default()
        });

        let _res = docker
            .remove_container(&self.container_name, options)
            .await
            .expect("remove container");
    }
}