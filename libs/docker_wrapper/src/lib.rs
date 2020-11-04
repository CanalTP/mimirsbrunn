// Copyright © 2016, Canal TP and/or its affiliates. All rights reserved.
//
// This file is part of Navitia,
//     the software to build cool stuff with public transport.
//
// Hope you'll enjoy and contribute to this project,
//     powered by Canal TP (www.canaltp.fr).
// Help us simplify mobility and open public transport:
//     a non ending quest to the responsive locomotion way of traveling!
//
// LICENCE: This program is free software; you can redistribute it
// and/or modify it under the terms of the GNU Affero General Public
// License as published by the Free Software Foundation, either
// version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but
// WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU
// Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public
// License along with this program. If not, see
// <http://www.gnu.org/licenses/>.
//
// Stay tuned using
// twitter @navitia
// IRC #navitia on freenode
// https://groups.google.com/d/forum/navitia
// www.navitia.io
use mimir::rubber::Rubber;
use slog_scope::{info, warn};
use std::error::Error;
use std::process::Command;
use std::time::Duration;

/// This struct wraps a docker (for the moment explicitly ElasticSearch)
/// Allowing to setup a docker, tear it down and to provide its address and port
pub struct DockerWrapper {
    ip: String,
}

impl DockerWrapper {
    pub fn host(&self) -> String {
        format!("http://{}:9200", self.ip)
    }

    fn setup(&mut self) -> Result<(), Box<dyn Error>> {
        info!("Launching ES docker");
        let mut args = vec!["run", "--detach", "--name=mimirsbrunn_tests"];
        if dont_kill_the_wale() {
            args.push("-p=9242:9200");
        }
        args.push("elasticsearch:2");
        let status = Command::new("docker").args(args).status()?;

        if !status.success() {
            return Err(format!("`docker run` failed {}", &status).into());
        }

        // we need to get the ip of the container if the container has been run on another machine
        let container_ip_cmd = Command::new("docker")
            .args(&[
                "inspect",
                "--format={{.NetworkSettings.IPAddress}}",
                "mimirsbrunn_tests",
            ])
            .output()?;

        let container_ip = std::str::from_utf8(container_ip_cmd.stdout.as_slice())?.trim();

        warn!("container ip = {:?}", container_ip);
        self.ip = container_ip.to_string();

        info!("Waiting for ES in docker to be up and running...");
        let retry = retry::retry(
            200,
            100,
            || reqwest::blocking::get(&self.host()),
            |response| {
                response
                    .as_ref()
                    .map(|res| res.status() == reqwest::StatusCode::OK)
                    .unwrap_or(false)
            },
        );
        match retry {
            Ok(_) => Ok(()),
            Err(_) => Err("ES is down".into()),
        }
    }

    pub fn new() -> Result<DockerWrapper, Box<dyn Error>> {
        let mut wrapper = DockerWrapper { ip: "".to_string() };
        wrapper.setup()?;
        let rubber = Rubber::new_with_timeout(&wrapper.host(), Duration::from_secs(10)); // use a long timeout
        rubber.initialize_templates().unwrap();
        Ok(wrapper)
    }
}

fn docker_command(args: &[&'static str]) {
    info!("Running docker {:?}", args);
    let status = Command::new("docker").args(args).status();
    match status {
        Ok(s) => {
            if !s.success() {
                warn!("`docker {:?}` failed {}", args, s)
            }
        }
        Err(e) => warn!("command `docker {:?}` failed {}", args, e),
    }
}

fn dont_kill_the_wale() -> bool {
    std::env::var("DONT_KILL_THE_WHALE") == Ok("1".to_string())
}

impl Drop for DockerWrapper {
    fn drop(&mut self) {
        if dont_kill_the_wale() {
            warn!(
                "the docker won't be stoped at the end, you can debug it.
            Note: ES has been mapped to the port 9242 in you localhost
            manually stop and rm the container mimirsbrunn_tests after debug"
            );
        } else {
            docker_command(&["stop", "mimirsbrunn_tests"]);
            docker_command(&["rm", "mimirsbrunn_tests"]);
        }
    }
}
