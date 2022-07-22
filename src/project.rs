use crate::{TrampolineResource, TrampolineResourceType, TEMPLATES};
use anyhow::Result;
use ckb_app_config::CKBAppConfig;
use serde::{Deserialize, Serialize};

use std::convert::From;

use std::fmt::Formatter;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use tera::Context as TeraContext;
use thiserror::Error;
use toml;

pub const TRAMPOLINE_ROOT_CONFIG: &str = "trampoline.toml";
pub const TRAMPOLINE_FOLDER: &str = ".trampoline";
pub const TRAMPOLINE_SCHEMAS: &str = "schemas";
pub const TRAMPOLINE_ROOT_DB_DIR: &str = "cache";

#[derive(Debug, Error)]
pub enum TrampolineProjectError {
    #[error("Error loading CKB Configuration File: {0:?}")]
    CkbAppConfig(ckb_app_config::ExitCode),
    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Tera(#[from] tera::Error),
    #[error(transparent)]
    DeserializeToml(#[from] toml::de::Error),

    #[error(transparent)]
    SerializeToml(#[from] toml::ser::Error),

    #[error("No Trampoline project found within directory {0}")]
    ProjectNotFound(String),

    #[error("Invalid initialization: Project {} already exists at {}", .name, .path)]
    ProjectAlreadyExists { path: String, name: String },
}

pub type ProjectResult<T> = std::result::Result<T, TrampolineProjectError>;

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct VirtualEnv {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub container_id: Option<String>,
    pub host: String,
    pub container_port: usize,
    pub host_port: usize,
    pub local_binding: PathBuf,
    pub container_mount: String,
}

impl std::fmt::Display for VirtualEnv {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Host: {}:{}\nSaving data to local path: {}\n",
            self.host,
            self.host_port,
            self.local_binding.to_str().unwrap()
        )
    }
}
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct TrampolineEnv {
    pub chain: VirtualEnv,
    pub miner: VirtualEnv,
    pub indexer: VirtualEnv,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct TrampolineConfig {
    pub name: String,
    pub env: Option<TrampolineEnv>,
}

#[derive(Debug, Clone, Default)]
pub struct TrampolineProject {
    pub config: TrampolineConfig,
    pub root_dir: PathBuf,
}

impl From<TrampolineProject> for TrampolineResourceType {
    fn from(p: TrampolineProject) -> Self {
        Self::Project(p)
    }
}

impl From<TrampolineResourceType> for TrampolineProject {
    fn from(p: TrampolineResourceType) -> Self {
        if let TrampolineResourceType::Project(p) = p {
            p
        } else {
            Self::default()
        }
    }
}

impl TrampolineResource for TrampolineProject {
    type Error = TrampolineProjectError;
    type InitArgs = String;

    fn load(path: impl AsRef<Path>) -> Result<TrampolineResourceType, TrampolineProjectError> {
        let candidate_root = path.as_ref();
        let mut trampoline_config_path = candidate_root.join("trampoline.toml");
        if trampoline_config_path.exists() {
            let raw_conf = fs::read_to_string(&trampoline_config_path).unwrap();
            let env_path = candidate_root.join("trampoline-env.toml");
            let mut trampoline_env = None;
            if env_path.exists() {
                let raw_env = fs::read_to_string(&env_path)?;
                let parsed_env = toml::from_str::<TrampolineEnv>(&raw_env)?;
                trampoline_env = Some(parsed_env);
            }
            let mut config = toml::from_str::<TrampolineConfig>(&raw_conf).unwrap();
            let mut root_dir =
                find_ancestor(&mut trampoline_config_path, "trampoline.toml").unwrap();
            root_dir.pop();
            config.env = trampoline_env;
            Ok(TrampolineProject { config, root_dir }.into())
        } else {
            trampoline_config_path.pop();
            let mut real_path = trampoline_config_path.canonicalize().unwrap();
            let root_trampoline_path = find_ancestor(&mut real_path, "trampoline.toml");
            match root_trampoline_path {
                Some(mut path) => {
                    let raw_conf = fs::read_to_string(&path).map_err(TrampolineProjectError::Io)?;
                    let config = toml::from_str::<TrampolineConfig>(&raw_conf)
                        .map_err(TrampolineProjectError::DeserializeToml)?;
                    path.pop();
                    Ok(TrampolineProject {
                        config,
                        root_dir: path,
                    }
                    .into())
                }
                None => Err(TrampolineProjectError::ProjectNotFound(
                    candidate_root.to_str().unwrap().to_string(),
                )),
            }
        }
    }

    fn init(args: Self::InitArgs) -> Result<TrampolineResourceType, TrampolineProjectError> {
        let name = args;
        let mut project_dir = std::env::current_dir()?;
        project_dir.push(&name);
        fs::create_dir(&project_dir)?;
        project_dir.push("src");
        fs::create_dir(&project_dir)?;
        project_dir.pop();

        project_dir.push(".trampoline");
        fs::create_dir(&project_dir)?;
        project_dir.push("accounts");
        fs::create_dir(&project_dir)?;
        project_dir.pop();
        project_dir.push("cache");
        fs::create_dir(&project_dir)?;
        project_dir.pop();
        project_dir.push("network");
        fs::create_dir(&project_dir)?;
        fs::create_dir(&project_dir.join("indexer"))?;
        project_dir.pop();
        project_dir.pop();

        project_dir.push("generators");
        project_dir.push("src");
        fs::create_dir_all(&project_dir)?;
        project_dir.pop();
        project_dir.pop();

        project_dir.push("schemas");
        project_dir.push("src");
        fs::create_dir_all(&project_dir)?;
        project_dir.pop();
        project_dir.push("mol");
        fs::create_dir(&project_dir)?;
        project_dir.pop();
        project_dir.pop();

        project_dir.push("scripts");
        fs::create_dir(&project_dir)?;
        project_dir.pop();

        let mut context = TeraContext::new();
        context.insert("PROJECT_NAME", &name);

        for path in TEMPLATES.get_template_names() {
            println!("PATH: {}", path);
            while !&project_dir.ends_with(&name) {
                project_dir.pop();
            }
            if path == "Dockerfile.template" {
                project_dir.push("Dockerfile");
            } else {
                project_dir.push(&path);
            }
            let content = TEMPLATES.render(path, &context)?;
            fs::write(&project_dir, content).unwrap_or_else(|_| {
                panic!(
                    "Error writing to {} with template {}",
                    &project_dir.to_str().unwrap(),
                    path
                )
            });
            project_dir.pop();
        }

        std::env::set_current_dir(&project_dir)?;
        TrampolineProject::load(project_dir)
    }

    

    
}

// TO DO: This requires that the ckb node is not running.
// Need to check it is shut down first
impl TrampolineProject {
    pub fn load_ckb_config(&self) -> ProjectResult<CKBAppConfig> {
        let ckb_toml_path = self.path_to_ckb_config()?;

        let raw_conf_str = fs::read_to_string(ckb_toml_path)?;
        CKBAppConfig::load_from_slice(raw_conf_str.as_bytes())
            .map_err(TrampolineProjectError::CkbAppConfig)
    }

    pub fn save_ckb_config(&self, c: CKBAppConfig) -> ProjectResult<()> {
        let config_toml = toml::Value::try_from(c)?;
        let config_string = toml::to_string(&config_toml)?;
        println!("CONFIG STRING: {}", config_string);

        let ckb_toml_path = self.path_to_ckb_config()?;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(ckb_toml_path)?;

        file.write_all(config_string.as_bytes())?;
        Ok(())
    }

    pub fn path_to_ckb_config(&self) -> ProjectResult<PathBuf> {
        let path_to_conf = fs::read_to_string(self.root_dir.join("trampoline-env.toml"))?;

        let env = toml::from_str::<TrampolineEnv>(path_to_conf.as_str())?;

        let ckb_toml_path = env.chain.local_binding.join("ckb.toml").canonicalize()?;
        Ok(ckb_toml_path)
    }    

}

fn find_ancestor(curr_path: &mut PathBuf, target: &str) -> Option<PathBuf> {
    let target_path = curr_path.join(target);
    if target_path.exists() {
        Some(target_path)
    } else if curr_path.pop() {
        find_ancestor(curr_path, target)
    } else {
        None
    }
}
