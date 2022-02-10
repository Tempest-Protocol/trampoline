use ckb_app_config::{CKBAppConfig, MinerAppConfig};
use ckb_types::{h256, H256};

use crate::compose::{Service, Volume, VolumeType};
use crate::parse_hex;
use std::fs;
// use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn init_ckb_volume(volume_name: &str) {
    // Create a named volume
    let _create_volume = Command::new("docker")
        .arg("volume")
        .arg("create")
        .arg(&volume_name)
        .status()
        .expect("failed to create volume");

    // Init a CKB dev chain in that volume
    let _init_volume = Command::new("docker")
        .arg("run")
        .arg("-v")
        .arg(format!("{}:/var/lib/ckb", &volume_name))
        // .arg("-e")
        // .arg("\"CKB_CHAIN=dev\"")
        .arg("nervos/ckb:latest")
        .arg("init")
        .arg("--chain")
        .arg("dev")
        .arg("--force")
        .status()
        .expect("failed to init CKB in volume");

    // Create a container
    let _create_container = Command::new("docker")
        // .arg("container")
        .arg("create")
        .arg("-v")
        .arg(format!("{}:/var/lib/ckb", &volume_name))
        .arg("--name")
        .arg(&volume_name)
        .arg("nervos/ckb:latest")
        .arg("run")
        .status()
        .expect("Failed running node container");

    // Copy config files to host
    let _copy_ckb_config = Command::new("docker")
        .arg("cp")
        .arg(format!("{}:/var/lib/ckb/ckb.toml", &volume_name))
        .arg(".trampoline/network/ckb.template")
        .status();
    let _copy_miner_config = Command::new("docker")
        .arg("cp")
        .arg(format!("{}:/var/lib/ckb/ckb-miner.toml", &volume_name))
        .arg(".trampoline/network/ckb-miner.template")
        .status();

    // Stop node container
    let _stop_node = Command::new("docker")
        .arg("stop")
        .arg(&volume_name)
        .status()
        .expect("Failed stopping container");
}

pub fn setup_ckb_config(from: &str, lockarg: &str) -> CKBAppConfig {
    // Load config from path
    let mut config = load_ckb_config(from);
    // Edit config
    let block_assembler = create_block_assembler_from_pkhash(&parse_hex(lockarg).unwrap());
    config.block_assembler = Some(block_assembler);
    config
}

pub fn setup_miner_config(path: &str, node_name: &str) -> MinerAppConfig {
    // Load config from path
    let file = fs::read_to_string(path).expect("Could not load ckb config file");
    let mut config = MinerAppConfig::load_from_slice(file.as_bytes())
        .expect("Error loading CKBConfig from slice");
    // Edit config
    config.miner.client.rpc_url = format!("http://{}:8114", &node_name);

    // Write config to path
    config
}

impl Service {
    pub fn node(name: &str, template_volume: &str, ckb_config: CKBAppConfig) -> Self {
        // Define pre-init CKB volume
        let template_volume = Volume {
            volume_type: VolumeType::Volume,
            source: "ckb".to_string(),
            target: "/var/lib/ckb".to_string(),
            external: Some(template_volume.to_string()),
        };

        // Create ckb.toml config volume
        setup_config(
            &ckb_config,
            Path::new("./.trampoline/network/ckb"),
            "ckb.toml",
        )
        .expect("failed to setup ckb.toml config");
        let config_volume = Volume {
            volume_type: VolumeType::Bind,
            source: "./.trampoline/network/ckb/ckb.toml".to_string(),
            target: "/var/lib/ckb/ckb.toml".to_string(),
            external: None,
        };

        Service {
            name: name.to_string(),
            image: "nervos/ckb:latest".to_string(),
            volumes: Some(vec![template_volume, config_volume]),
            expose: Some(vec!["8114".to_string(), "8115".to_string()]),
            command: Some("run".to_string()),
            // command: None,
            environment: None,
            ports: Some(vec!["8114:8114".to_string()]),
            // entrypoint: Some("ls && stat ckb.toml && stat ckb-miner.toml".to_string()),
            entrypoint: None,
            depends_on: None,
        }
    }
}

impl Service {
    pub fn miner(template_volume: &str, miner_config: MinerAppConfig, node_dep_name: &str) -> Self {
        // Setup miner config
        // Define pre-init CKB volume
        let template_volume = Volume {
            volume_type: VolumeType::Volume,
            source: "ckb".to_string(),
            target: "/var/lib/ckb".to_string(),
            external: Some(template_volume.to_string()),
        };
        // Create ckb-miner.toml config volume
        setup_config(
            &miner_config,
            Path::new("./.trampoline/network/ckb"),
            "ckb-miner.toml",
        )
        .expect("failed to setup ckb.toml config");
        let config_volume = Volume {
            volume_type: VolumeType::Bind,
            source: "./.trampoline/network/ckb/ckb-miner.toml".to_string(),
            target: "/var/lib/ckb/ckb-miner.toml".to_string(),
            external: None,
        };

        Service {
            name: format!("{}-miner", &node_dep_name).to_string(),
            image: "nervos/ckb:latest".to_string(),
            volumes: Some(vec![template_volume, config_volume]),
            expose: None,
            command: Some("miner".to_string()),
            // command: None,
            environment: None,
            ports: None,
            // entrypoint: Some("ls && stat ckb.toml && stat ckb-miner.toml".to_string()),
            entrypoint: None,
            depends_on: Some(vec![node_dep_name.to_string()]),
        }
    }
}

impl Service {
    pub fn indexer(node_dep_name: &str) -> Self {
        // Define indexer data volume
        let data_volume = Volume {
            volume_type: VolumeType::Volume,
            source: format!("{}-indexer-data", &node_dep_name),
            target: "/data/".to_string(),
            external: None,
        };

        Service {
            name: format!("{}-indexer", &node_dep_name).to_string(),
            image: "nervos/ckb-indexer:latest".to_string(),
            volumes: Some(vec![data_volume]),
            expose: Some(vec!["8116".to_string()]),
            command: Some(format!(
                "-c http://{}:8114 -s /data -l 0.0.0.0:8116",
                &node_dep_name
            )),
            // command: None,
            environment: None,
            ports: Some(vec!["8116:8116".to_string()]),
            // entrypoint: Some("ls && stat ckb.toml && stat ckb-miner.toml".to_string()),
            entrypoint: None,
            depends_on: Some(vec![node_dep_name.to_string()]),
        }
    }
}

// create_ckb_config creates a default ckb.toml configuration
// based on a given lockhash used for setting up the
// block assembler.
pub fn ckb_config_from_lock(lock_arg: &str) -> CKBAppConfig {
    let lock_arg_hash = parse_hex(lock_arg).expect("Failed parsing lock_arg");

    let modules = vec![
        ckb_app_config::RpcModule::Net,
        ckb_app_config::RpcModule::Pool,
        ckb_app_config::RpcModule::Miner,
        ckb_app_config::RpcModule::Chain,
        ckb_app_config::RpcModule::Stats,
        ckb_app_config::RpcModule::Subscription,
        ckb_app_config::RpcModule::Experiment,
        ckb_app_config::RpcModule::Debug,
    ];

    CKBAppConfig {
        bin_name: "ckb".to_string(),
        data_dir: PathBuf::from("data"),
        block_assembler: Some(create_block_assembler_from_pkhash(&lock_arg_hash)),
        root_dir: PathBuf::from(""),
        ancient: PathBuf::from(""),
        tmp_dir: None,
        logger: ckb_app_config::LogConfig::default(),
        sentry: ckb_app_config::SentryConfig {
            dsn: "".to_string(),
            org_contact: None,
            org_ident: None,
        },
        metrics: ckb_app_config::MetricsConfig::default(),
        memory_tracker: ckb_app_config::MemoryTrackerConfig::default(),
        chain: ckb_app_config::ChainConfig {
            spec: ckb_resource::Resource::file_system(PathBuf::from("specs/dev.toml")),
        },
        db: ckb_app_config::DBConfig::default(),
        network: ckb_app_config::NetworkConfig::default(),
        rpc: ckb_app_config::RpcConfig {
            listen_address: "127.0.0.1:8114".to_string(),
            tcp_listen_address: Some("127.0.0.1:18114".to_string()),
            ws_listen_address: Some("127.0.0.1:28114".to_string()),
            reject_ill_transactions: true,
            max_request_body_size: 10 * 1024 * 1024,
            threads: None,
            modules,
            enable_deprecated_rpc: false,
            extra_well_known_lock_scripts: vec![],
            extra_well_known_type_scripts: vec![],
        },
        tx_pool: ckb_app_config::TxPoolConfig::default(),
        store: ckb_app_config::StoreConfig::default(),
        alert_signature: None,
        notify: ckb_app_config::NotifyConfig::default(),
    }
}

// SECP type hash is used to create a blockassembler config
// used in every ckb config
const SECP_TYPE_HASH: H256 =
    h256!("0x9bd7e06f3ecf4be0f2fcd2188b23f1b9fcc88e5d4b65a8637b17723bbda3cce8");

fn create_block_assembler_from_pkhash(hash: &[u8]) -> ckb_app_config::BlockAssemblerConfig {
    use ckb_jsonrpc_types::{JsonBytes, ScriptHashType};
    ckb_app_config::BlockAssemblerConfig {
        code_hash: SECP_TYPE_HASH,
        hash_type: ScriptHashType::Type,
        use_binary_version_as_message_prefix: false,
        args: JsonBytes::from_bytes(bytes::Bytes::copy_from_slice(hash)),
        message: JsonBytes::default(),
        binary_version: "".to_string(),
    }
}

pub fn setup_config<C: serde::Serialize>(
    config: &C,
    folder: &Path,
    filename: &str,
) -> Result<(), ckb_resource::Error> {
    // Check if config already exists
    let config_exists = check_config(folder, filename);
    match config_exists {
        (true, true) => {
            println!("Using {} configuration found at {:?}.", &filename, &folder);
            Ok(())
        }

        (true, false) => {
            println!(
                "Found service folder, but not matching {} file. Creating a new one at {}",
                &filename,
                &folder.to_str().unwrap()
            );
            write_config_file(config, folder, filename)
        }

        _ => {
            println!(
                "Creating config file {} at {}",
                &filename,
                &folder.to_str().unwrap()
            );
            std::fs::create_dir(folder).unwrap();
            write_config_file(config, folder, filename)
        }
    }
}

// Check if config exists
pub fn check_config(folder: &Path, filename: &str) -> (bool, bool) {
    // Construct file path
    let file_string = format!("{}/{}", folder.to_str().unwrap(), filename);
    let file_path = Path::new(&file_string);

    // Return config check
    (folder.exists(), file_path.exists())
}

// Write config
pub fn write_config_file<C: serde::Serialize>(
    config: &C,
    folder: &Path,
    filename: &str,
) -> Result<(), ckb_resource::Error> {
    let toml = toml::Value::try_from(config)
        .expect(&format!("Failed converting config into {}", &filename));
    let file_string = format!(
        "{}/{}",
        &folder.to_str().expect("Failed converting folder to string"),
        &filename
    );
    std::fs::write(Path::new(&file_string), toml.to_string())
}

pub fn load_ckb_config(path: &str) -> CKBAppConfig {
    let file = fs::read_to_string(path).expect("Could not load ckb config file");
    CKBAppConfig::load_from_slice(file.as_bytes()).expect("Error loading CKBConfig from slice")
    // let new_toml = toml::Value::try_from(config).expect("Could not encode config into TOML");
    // let new_toml_string = toml::to_string(&new_toml).expect("Could not create TOML string");
    // let result = fs::write("generated-ckb.toml", new_toml_string).expect("Unable to write file");
    // assert_eq!(result, ());
    // let config = CKBAppConfig::load_from_slice(slice: &[u8])
}

pub fn write_ckb_config(path: &str, config: CKBAppConfig) -> Result<(), ckb_resource::Error> {
    let new_toml = toml::Value::try_from(config).expect("Could not encode config into TOML");
    let new_toml_string = toml::to_string(&new_toml).expect("Could not create TOML string");
    fs::write(path, new_toml_string)
    // assert_eq!(result, ());
    // let config = CKBAppConfig::load_from_slice(slice: &[u8])
}
