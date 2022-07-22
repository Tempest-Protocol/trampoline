use std::collections::HashMap;
use std::fs::File;
use std::fs::OpenOptions;
use std::io::Write;
use std::str::FromStr;

use anyhow::anyhow;
use anyhow::Result;
use bollard::container::LogsOptions;
use bytes::Bytes;
use ckb_app_config::BlockAssemblerConfig;
use ckb_hash::blake2b_256;

use ckb_types::{h256, H256};

use jsonrpc_core::futures_util::TryStreamExt;
use structopt::StructOpt;

use trampoline::docker::*;
use trampoline::network::TrampolineNetwork;
use trampoline::opts::{NetworkCommands, SchemaCommand, TrampolineCommand};
use trampoline::parse_hex;
use trampoline::project::*;
use trampoline::schema::{Schema, SchemaInitArgs};
use trampoline::TrampolineResource;
use trampoline::TrampolineResourceType;
use trampoline_sdk::rpc;

// use bollard::Docker;

const SECP_TYPE_HASH: H256 =
    h256!("0x9bd7e06f3ecf4be0f2fcd2188b23f1b9fcc88e5d4b65a8637b17723bbda3cce8");
fn create_block_assembler_from_pkhash(hash: &[u8]) -> BlockAssemblerConfig {
    use ckb_jsonrpc_types::{JsonBytes, ScriptHashType};
    BlockAssemblerConfig {
        code_hash: SECP_TYPE_HASH,
        hash_type: ScriptHashType::Type,
        use_binary_version_as_message_prefix: false,
        args: JsonBytes::from_bytes(bytes::Bytes::copy_from_slice(hash)),
        message: JsonBytes::default(),
        binary_version: "".to_string(),
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let opts = TrampolineCommand::from_args();

    let project = TrampolineProject::load(std::env::current_dir()?);

    match opts {
        TrampolineCommand::NewProject { name } => match project {
            Ok(project) => {
                if let TrampolineResourceType::Project(project) = project {
                    return Err(TrampolineProjectError::ProjectAlreadyExists {
                        name: project.config.name.to_string(),
                        path: project.root_dir.as_path().to_str().unwrap().to_string(),
                    }
                    .into());
                }
            }
            Err(_e) => {
                let project = TrampolineProject::from(TrampolineProject::init(name)?);
                std::env::set_current_dir(&project.root_dir)?;
                Docker::default().build()?;
            }
        },
        TrampolineCommand::Schema { command } => {
            match command {
                SchemaCommand::New { name, def } => {
                    let mut schema_args: SchemaInitArgs =
                        (TrampolineProject::from(project?), name, None);
                    if def.is_some() {
                        schema_args.2 = def;
                    }

                    //println!("{:?}", schema_args);
                    let _result = Schema::init(schema_args)?;
                }
                SchemaCommand::Build { name } => {
                    let schema_args: SchemaInitArgs =
                        (TrampolineProject::from(project?), name, None);
                    let _result = Schema::init(schema_args)?;
                }
            }
        }
        TrampolineCommand::Network { command } => {
            let project = TrampolineProject::from(project?);
            match command {
                // TODO add --recreate flag to init
                NetworkCommands::Init {} => {
                    // Set up new network
                    let mut network = TrampolineNetwork::new(&project, false).await;

                    // Add CKB node
                    let node = network
                        .add_ckb(
                            &format!("{}-ckb", project.config.name),
                            vec![("8114".to_string(), "8114".to_string())],
                        )
                        .await;

                    // Add Indexer
                    let _indexer = network
                        .add_indexer(&node, vec![("8116".to_string(), "8116".to_string())])
                        .await;

                    // Write config
                    network.save(&project);

                    // TODO drop everything into a TrampolineNetwork type and implement Display for it
                    // @arnur
                    println!("{}", network);
                    // println!("New Trampoline development network created\n\
                    //         Network name:{}-network\n\
                    //         Network ID:{}\n\
                    //         CKB node port: 8114\n\
                    //         Indexer port: 8116
                    //         ",
                    //     network.name,
                    //     network.id());
                }

                NetworkCommands::Recreate {} => {
                    // Stop all containers related to this project (ckb, ckb-indexer)
                    let network = TrampolineNetwork::new(&project, true).await;
                    network.save(&project);
                    println!("{}", network);
                }

                NetworkCommands::Stop {} => {
                    // Stop all containers related to this project (ckb, ckb-indexer)
                    let network = TrampolineNetwork::load(&project);
                    network.stop().await;
                }

                NetworkCommands::Reset { service } => {
                    let network = TrampolineNetwork::load(&project);

                    match service {
                        None => {
                            network.stop().await;
                            network.run().await;
                            println!("Trampoline network restarted");
                        }
                        Some(service) => {
                            network.reset(service).await;
                        }
                    }
                }

                NetworkCommands::Status {} => {
                    // Show information about running services
                    // https://docs.rs/bollard/0.1.0/bollard/struct.Docker.html#method.logs
                    let network = TrampolineNetwork::load(&project);

                    network.status().await;
                }

                NetworkCommands::Logs { service, output } => {
                    let docker = bollard::Docker::connect_with_local_defaults()
                        .expect("Failed to connect to Docker API");

                    let opts = LogsOptions {
                        tail: "50".to_string(),
                        follow: true,
                        stdout: true,
                        stderr: true,
                        ..Default::default()
                    };

                    let logs = &docker
                        .logs(&service, Some(opts))
                        .try_collect::<Vec<_>>()
                        .await?;

                    // TODO
                    // Depending on parameters filter the logs array

                    match output {
                        None => {
                            for line in logs {
                                match line {
                                    bollard::container::LogOutput::StdErr { message } => {
                                        println!("ERR: {}", std::str::from_utf8(message).unwrap())
                                    }
                                    bollard::container::LogOutput::StdOut { message } => {
                                        println!("OUT: {}", std::str::from_utf8(message).unwrap())
                                    }
                                    bollard::container::LogOutput::StdIn { message } => {
                                        println!("IN: {}", std::str::from_utf8(message).unwrap())
                                    }
                                    bollard::container::LogOutput::Console { message } => println!(
                                        "CONSOLE: {}",
                                        std::str::from_utf8(message).unwrap()
                                    ),
                                }
                            }
                        }
                        Some(path) => {
                            let mut file = OpenOptions::new()
                                .write(true)
                                .append(true)
                                .create(true)
                                .open(path)
                                .unwrap();

                            for line in logs {
                                match line {
                                    bollard::container::LogOutput::StdErr { message } => {
                                        append_log_to_file(message, &mut file)
                                    }
                                    bollard::container::LogOutput::StdOut { message } => {
                                        append_log_to_file(message, &mut file)
                                    }
                                    bollard::container::LogOutput::StdIn { message } => {
                                        append_log_to_file(message, &mut file)
                                    }
                                    bollard::container::LogOutput::Console { message } => {
                                        append_log_to_file(message, &mut file)
                                    }
                                }
                            }
                            // Save logs to file
                        }
                    }
                }

                NetworkCommands::Delete {} => {
                    // Remove network and all containers related to this project
                    let network = TrampolineNetwork::load(&project);
                    network.delete().await;
                }

                NetworkCommands::Launch {} => {
                    let network = TrampolineNetwork::load(&project);
                    network.run().await;
                }

                NetworkCommands::LaunchOld {} => {
                    let image = DockerImage {
                        name: "tempest/trampoline-env".to_string(),
                        tag: Some("latest".to_string()),
                        file_path: Some("./".to_string()),
                        host_mappings: vec![],
                        build_args: HashMap::new(),
                    };

                    let cmd: DockerCommand<DockerImage> =
                        DockerCommand::default().build(&image, false).unwrap();
                    cmd.execute(Some(vec!["-f".to_string(), "./Dockerfile".to_string()]))?;

                    //std::thread::sleep(std::time::Duration::from_millis(5000));
                    let container_port = project.config.env.as_ref().unwrap().chain.container_port;
                    let host_port = project.config.env.as_ref().unwrap().chain.host_port;

                    let host_volume = project
                        .config
                        .env
                        .as_ref()
                        .unwrap()
                        .chain
                        .local_binding
                        .as_path();

                    let container_volume = project
                        .config
                        .env
                        .as_ref()
                        .unwrap()
                        .chain
                        .container_mount
                        .as_str();

                    let docker_volume = Volume {
                        host: host_volume,
                        container: std::path::Path::new(container_volume),
                    };

                    let container = DockerContainer {
                        name: format!("{}-node", &project.config.name),
                        port_bindings: vec![DockerPort {
                            host: host_port,
                            container: container_port,
                        }],
                        volumes: vec![docker_volume],
                        env_vars: HashMap::default(),
                        image,
                    };
                    let run: DockerCommand<DockerContainer> = DockerCommand::default()
                        .run(&container, false, true)
                        .unwrap();

                    println!("{}", run.command_string.as_ref().unwrap());

                    run.execute(Some(vec!["run".to_string()]))?;
                }
                NetworkCommands::SetMiner { pubkey, lock_arg } => {
                    let mut config = project.load_ckb_config()?;
                    if let Some(pubkey) = pubkey {
                        let pubkey_bytes = parse_hex(pubkey.as_str())?;
                        let pubkey_hash = blake2b_256(&pubkey_bytes);
                        let pkey_hash_slice = &pubkey_hash[0..20];
                        let block_assembler = create_block_assembler_from_pkhash(pkey_hash_slice);
                        config.block_assembler = Some(block_assembler);
                        project.save_ckb_config(config)?;
                    } else if let Some(lock_arg) = lock_arg {
                        let lock_arg_bytes = parse_hex(lock_arg.as_str())?;
                        let block_assembler = create_block_assembler_from_pkhash(&lock_arg_bytes);
                        config.block_assembler = Some(block_assembler);
                        project.save_ckb_config(config)?;
                    }
                    Docker::default()
                        .name(format!("{}-node", project.config.name.as_str()).as_str())
                        .restart()?;
                }
                NetworkCommands::Miner { one_block: _ } => {
                    let config = project.load_ckb_config()?;
                    let block_assembler_args = config.block_assembler.as_ref();
                    if block_assembler_args.is_none() {
                        return Err(anyhow!("No miner address set. Refer to `trampoline net set-miner --help` for more information."));
                    }
                    let container_name = project.config.name.as_str();
                    let miner_mount_path = &project.config.env.unwrap().miner.container_mount;
                    Docker::exec(
                        format!("{}-node", container_name).as_str(),
                        vec!["ckb", "miner"],
                        miner_mount_path,
                    )?;
                }
                NetworkCommands::Indexer {} => {
                    let image = DockerImage {
                        name: "tempest/trampoline-indexer".to_string(),
                        tag: Some("latest".to_string()),
                        file_path: Some("./".to_string()),
                        host_mappings: vec![],
                        build_args: HashMap::new(),
                    };

                    let cmd: DockerCommand<DockerImage> =
                        DockerCommand::default().build(&image, true).unwrap();

                    println!("{}", cmd.command_string.as_ref().unwrap());
                    cmd.execute(Some(vec!["-f".to_string(), "./DockerIndexer".to_string()]))?;
                    //std::thread::sleep(std::time::Duration::from_millis(5000));

                    let container_port =
                        project.config.env.as_ref().unwrap().indexer.container_port;
                    let host_port = project.config.env.as_ref().unwrap().indexer.host_port;

                    let host_volume = project
                        .config
                        .env
                        .as_ref()
                        .unwrap()
                        .indexer
                        .local_binding
                        .as_path();

                    let container_volume = project
                        .config
                        .env
                        .as_ref()
                        .unwrap()
                        .indexer
                        .container_mount
                        .as_str();

                    let docker_volume = Volume {
                        host: host_volume,
                        container: std::path::Path::new(container_volume),
                    };

                    let container = DockerContainer {
                        name: format!("{}-indexer", &project.config.name),
                        port_bindings: vec![DockerPort {
                            host: host_port,
                            container: container_port,
                        }],
                        volumes: vec![docker_volume],
                        env_vars: HashMap::default(),
                        image,
                    };
                    let run: DockerCommand<DockerContainer> = DockerCommand::default()
                        .run(&container, false, true)
                        .unwrap();
                    println!("{}", run.command_string.as_ref().unwrap());

                    run.execute(Some(vec![
                        "/indexer/ckb-indexer".into(),
                        "-l".into(),
                        "0.0.0.0:8114".into(),
                        "-s".into(),
                        "/indexer/data".into(),
                        "-c".into(),
                        "http://172.17.0.2:8114".into(),
                    ]))?;
                }
                NetworkCommands::Rpc { hash } => {
                    let hash = H256::from_str(hash.as_str())?;
                    //let mut rpc_client = rpc::RpcClient::new();
                    let url = format!(
                        "{}:{}",
                        project.config.env.as_ref().unwrap().chain.host,
                        project.config.env.as_ref().unwrap().chain.host_port
                    );
                    let mut rpc_client = rpc::blocking::CkbRpcClient::new(url.as_str());
                    let result = rpc_client.get_transaction(hash)?;
                    println!("Transaction with status: {}", serde_json::json!(result));
                }
                _ => {
                    println!("Command not yet implemented!");
                    std::process::exit(0);
                }
            }
        }
    }

    Ok(())
}

fn append_log_to_file(message: &Bytes, file: &mut File) {
    let string = std::str::from_utf8(message).unwrap().to_string();
    if let Err(e) = writeln!(file, "{}", string) {
        eprintln!("Couldn't write to file: {}", e);
    }
}
