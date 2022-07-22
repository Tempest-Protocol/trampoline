use std::fmt::Write;

use bollard::{
    container::{CreateContainerOptions, RemoveContainerOptions},
    models::PortBinding,
    network::CreateNetworkOptions,
};
use jsonrpc_core::futures_util::future::join_all;
use serde::{Deserialize, Serialize};

use crate::project::TrampolineProject;

#[derive(Serialize, Deserialize, Clone, Debug)]
enum ServiceKind {
    Ckb,
    CkbIndexer,
}

#[derive(Serialize, Deserialize, Default)]
pub struct TrampolineNetwork {
    pub name: String,
    pub network: String,
    pub services: Vec<Service>,
}

impl std::fmt::Display for TrampolineNetwork {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "New Trampoline development network inititalized.\n
Network name: {}-network\n
Network ID: {}\n
Network Services: {:?}",
            self.name,
            self.id(),
            self.services
                .iter()
                .map(|service| service.name.clone())
                .collect::<Vec<String>>()
        )
    }
}

impl TrampolineNetwork {
    pub async fn new(project: &TrampolineProject, from_config: bool) -> Self {
        match from_config {
            true => {
                // Regenerate network from config file

                // Create default network
                let mut network = TrampolineNetwork::default();

                // Recreate all services from file
                let old_network = TrampolineNetwork::load(project);
                network.name = old_network.name.clone();

                let services = old_network.services.clone();
                old_network.delete().await;
                network.network = create_new_network(project)
                    .await
                    .expect("Failed to create new network");
                // First the nodes so we can get their IP for other services
                let nodes: Vec<&Service> = services
                    .iter()
                    .filter(|&service| matches!(&service.kind, ServiceKind::Ckb))
                    .collect();

                for node in &nodes {
                    network.add_ckb(&node.name, node.ports.clone()).await;
                }

                // Then indexers
                let indexers: Vec<&Service> = services
                    .iter()
                    .filter(|&service| matches!(&service.kind, ServiceKind::CkbIndexer))
                    .collect();
                for indexer in indexers {
                    network
                        .add_indexer(
                            nodes.get(0).expect("No nodes in network"),
                            indexer.ports.clone(),
                        )
                        .await;
                }

                network
            }
            false => {
                // Create a new default network
                let network_id = create_new_network(project)
                    .await
                    .expect("Failed creating new network");
                Self {
                    name: project.config.name.clone(),
                    services: vec![],
                    network: network_id,
                }
            }
        }
    }

    pub async fn run(&self) {
        let docker = bollard::Docker::connect_with_local_defaults()
            .expect("Failed to connect to Docker API");

        // First run nodes
        let nodes: Vec<&Service> = self
            .services
            .iter()
            .filter(|&service| matches!(&service.kind, ServiceKind::Ckb))
            .collect();
        let mut starting_nodes = Vec::with_capacity(nodes.len());
        for node in nodes {
            starting_nodes.push(docker.start_container::<String>(&node.id, None));
        }
        join_all(starting_nodes).await;
        println!("Nodes should have started!");

        // Then run indexers
        let indexers: Vec<&Service> = self
            .services
            .iter()
            .filter(|&service| matches!(&service.kind, ServiceKind::CkbIndexer))
            .collect();
        let mut starting_indexers = Vec::with_capacity(indexers.len());
        for indexer in indexers {
            starting_indexers.push(docker.start_container::<String>(&indexer.id, None));
        }
        join_all(starting_indexers).await;
        println!("Indexer should have started!");
    }

    pub async fn status(&self) {
        for service in &self.services {
            let service_status = ServiceStatus::from(&service).await;
            println!("{:#?}", service_status);
        }
    }

    pub async fn stop(&self) {
        let docker = bollard::Docker::connect_with_local_defaults()
            .expect("Failed to connect to Docker API");
        let mut stopping_services = Vec::with_capacity(self.services.len());
        for service in &self.services {
            stopping_services.push(docker.stop_container(&service.name, None));
        }
        join_all(stopping_services).await;
        println!("Trampoline Network stopped")
    }

    pub async fn reset(&self, service_name: String) {
        let service = self
            .services
            .iter()
            .find(|service| service.name == service_name);
        match service {
            None => {
                println!("Service not found in current network config")
            }
            Some(service) => {
                let docker = bollard::Docker::connect_with_local_defaults()
                    .expect("Failed to connect to Docker API");
                docker
                    .stop_container(&service.name, None)
                    .await
                    .unwrap_or_else(|_| panic!("Failed to stop {}", &service.name));
                docker
                    .start_container::<String>(&service.id, None)
                    .await
                    .unwrap_or_else(|_| panic!("Failed to start {}", &service.name));
                println!("Service {} restarted", &service.name);
            }
        }
    }

    pub fn load(project: &TrampolineProject) -> Self {
        let path = project.root_dir.join("network.toml");
        let toml = std::fs::read_to_string(path).expect("Unable to read network config from file");
        toml::from_str(&toml).expect("Failed parsing network config file")
    }

    pub fn save(&self, project: &TrampolineProject) {
        let path = project.root_dir.join("network.toml");
        let toml = toml::to_string(self).expect("Failed converting network config into TOML");
        std::fs::write(path, toml).expect("Failed to write network config to file");
    }

    pub async fn delete(self) {
        let docker = bollard::Docker::connect_with_local_defaults()
            .expect("Failed to connect to Docker API");
        // Remove all associated containers and their volumes
        let remove_opts = Some(RemoveContainerOptions {
            v: true,
            force: true,
            link: false,
        });
        for service in self.services {
            docker
                .remove_container(&service.name, remove_opts)
                .await
                .expect("Failed to remove docker container");
        }

        // Remove user defined network
        docker
            .remove_network(&format!("{}-network", self.name))
            .await
            .expect("Failed to remove docker network");
    }

    pub fn add_service(&mut self, service: &Service) {
        if !self.contains(&service.id) {
            self.services.push(service.clone());
        }
    }

    pub async fn add_indexer(&mut self, node: &Service, ports: Vec<(String, String)>) -> Service {
        let docker = bollard::Docker::connect_with_local_defaults()
            .expect("Failed to connect to Docker API");

        let mut port_bindings = ::std::collections::HashMap::new();

        for port in &ports {
            port_bindings.insert(
                format!("{}/tcp", port.0),
                Some(vec![PortBinding {
                    host_ip: Some(String::from("127.0.0.1")),
                    host_port: Some(port.1.to_string()),
                }]),
            );
        }

        let host_config = Some(bollard::models::HostConfig {
            port_bindings: Some(port_bindings),
            network_mode: Some(self.id()),
            ..Default::default()
        });

        let node_url = format!("http://{}:8114", node.name);
        let indexer_config = bollard::container::Config {
            image: Some("nervos/ckb-indexer:latest"),
            host_config,
            cmd: Some(vec!["-s", "data", "-c", &node_url, "-l", "0.0.0.0:8116"]),
            ..Default::default()
        };

        let opts = CreateContainerOptions {
            name: format!("{}-indexer", node.name),
        };

        let indexer_container = docker
            .create_container(Some(opts.clone()), indexer_config)
            .await
            .expect("Failed creating Indexer container");
        let indexer_id = indexer_container.id;

        let indexer_service = Service {
            name: opts.name,
            id: indexer_id,
            kind: ServiceKind::CkbIndexer,
            ports,
        };

        self.add_service(&indexer_service);
        indexer_service
    }

    pub async fn add_ckb(&mut self, name: &str, ports: Vec<(String, String)>) -> Service {
        let docker = bollard::Docker::connect_with_local_defaults()
            .expect("Failed to connect to Docker API");

        let mut port_bindings = ::std::collections::HashMap::new();

        for port in &ports {
            port_bindings.insert(
                format!("{}/tcp", port.0),
                Some(vec![PortBinding {
                    host_ip: Some(String::from("127.0.0.1")),
                    host_port: Some(port.1.to_string()),
                }]),
            );
        }

        let host_config = Some(bollard::models::HostConfig {
            port_bindings: Some(port_bindings),
            network_mode: Some(self.id()),
            ..Default::default()
        });

        let ckb_config = bollard::container::Config {
            image: Some("pablitx/ckb-testchain:latest"),
            host_config,
            ..Default::default()
        };

        let opts = CreateContainerOptions { name };

        let ckb_container = docker
            .create_container(Some(opts.clone()), ckb_config)
            .await
            .expect("Failed creating CKB container");
        let ckb_id = ckb_container.id;

        let ckb_service = Service {
            name: opts.name.to_string(),
            id: ckb_id,
            kind: ServiceKind::Ckb,
            ports,
        };

        self.add_service(&ckb_service);
        ckb_service
    }

    pub fn contains(&self, container_id: &String) -> bool {
        for service in &self.services {
            if &service.id == container_id {
                return true;
            }
        }
        false
    }

    pub fn get_service(&self, service_name: String) -> Option<&Service> {
        self.services
            .iter()
            .find(|service| service.name == service_name)
    }

    pub fn id(&self) -> String {
        self.network.clone()
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Service {
    name: String,
    id: String,
    kind: ServiceKind,
    ports: Vec<(String, String)>,
}

#[derive(Debug)]
struct ServiceStatus {
    service: Service,
    running: bool,
    created: bool,
    up_time: Option<String>,
    ports: String,
}

impl ServiceStatus {
    pub async fn from(service: &Service) -> Self {
        let docker = bollard::Docker::connect_with_local_defaults()
            .expect("Failed to connect to Docker API");
        let container_info = docker
            .inspect_container(&service.name.to_string(), None)
            .await
            .unwrap();
        let ports = {
            let map = &container_info
                .clone()
                .network_settings
                .unwrap()
                .ports
                .unwrap();
            let mut ports_string = String::new();

            for (container_port, bindings) in map {
                match bindings {
                    Some(binding) => {
                        for b in binding {
                            let host_port = b.host_port.as_ref().unwrap();
                            let formatted_ports = format!("{}:{}", container_port, host_port);
                            ports_string.push_str(&formatted_ports);
                        }
                    }
                    None => {}
                }
            }

            ports_string
        };

        Self {
            service: service.clone(),
            running: container_info.clone().state.unwrap().running.unwrap(),
            created: true,
            up_time: Some(format!(
                "Started at: {}",
                container_info.state.unwrap().started_at.unwrap()
            )),
            ports,
        }
    }
}

pub async fn create_new_network(
    project: &TrampolineProject,
) -> Result<String, bollard::errors::Error> {
    let docker =
        bollard::Docker::connect_with_local_defaults().expect("Failed to connect to Docker API");
    let network = CreateNetworkOptions {
        name: format!("{}-network", project.config.name),
        check_duplicate: true,
        ..Default::default()
    };
    let network_respose = docker.create_network(network).await?;
    Ok(network_respose.id.unwrap())
}
