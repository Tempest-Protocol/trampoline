use crate::docker;
use crate::project::VirtualEnv;
use std::collections::HashMap;
use std::fmt::Formatter;
use std::io::Write;
use std::marker::PhantomData;
use std::process::Command;

use std::path::{Path, PathBuf};
use std::time::Duration;

use thiserror::Error;

use std::process::Stdio;

use anyhow::anyhow;
use std::string::ToString;

pub const DOCKER_BIN: &str = "docker";
pub const IMAGE_NAME: &str = "iamm/trampoline-env:latest";

#[derive(Debug, Error)]
pub enum DockerError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Any(#[from] anyhow::Error),
    #[error("No image set")]
    NoImage,
}

type DockerResult<T> = std::result::Result<T, DockerError>;

#[derive(Hash, Eq, PartialEq, Debug, Clone)]
pub struct Port(usize);

impl From<usize> for Port {
    fn from(port_num: usize) -> Self {
        Self(port_num)
    }
}

#[derive(Hash, Eq, PartialEq, Debug, Clone)]
pub struct VolumePath(String);

impl From<String> for VolumePath {
    fn from(path: String) -> Self {
        Self(path)
    }
}

impl From<PathBuf> for VolumePath {
    fn from(buf: PathBuf) -> Self {
        Self(buf.to_str().unwrap().to_string())
    }
}

impl std::fmt::Display for VolumePath {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::fmt::Display for Port {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone)]
pub struct Volume<'a> {
    pub host: &'a Path,
    pub container: &'a Path,
}

impl std::fmt::Display for Volume<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}:{}",
            self.host
                .canonicalize()
                .unwrap()
                .as_os_str()
                .to_str()
                .unwrap(),
            self.container.as_os_str().to_str().unwrap()
        )
    }
}

#[derive(Debug, Clone)]
pub struct DockerPort {
    pub host: usize,
    pub container: usize,
}

impl std::fmt::Display for DockerPort {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.host, self.container)
    }
}

#[derive(Debug, Clone, Default)]
pub struct DockerContainer<'a> {
    pub name: String,
    pub port_bindings: Vec<DockerPort>,
    pub volumes: Vec<Volume<'a>>,
    pub env_vars: HashMap<String, String>,
    pub image: DockerImage,
}

impl std::fmt::Display for DockerContainer<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let port_bindings_string = self
            .port_bindings
            .iter()
            .map(|port| format!("-p{}", port))
            .collect::<Vec<String>>()
            .join(" ");
        let image_string = {
            if let Some(tag) = self.image.tag.as_ref() {
                format!("{}:{}", self.image.name, tag)
            } else {
                self.image.name.clone()
            }
        };
        if self.volumes.is_empty() {
            write!(
                f,
                "{} --name {} {}",
                port_bindings_string, self.name, image_string
            )
        } else {
            let volumes_string = self
                .volumes
                .iter()
                .map(|vol| format!("-v{}", vol))
                .collect::<Vec<String>>()
                .join(" ");
            write!(
                f,
                "{} --name {} {} {}",
                port_bindings_string, self.name, volumes_string, image_string
            )
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct DockerImage {
    pub name: String,
    pub tag: Option<String>,
    pub file_path: Option<String>,
    pub host_mappings: Vec<Port>,
    pub build_args: HashMap<String, String>,
}

impl std::fmt::Display for DockerImage {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut fmt_string = "".to_string();
        if self.file_path.is_some() {
            fmt_string.push_str(self.file_path.as_ref().unwrap());
        }
        if self.tag.is_some() {
            fmt_string.push_str(&format!(" -t {}:{}", self.name, self.tag.as_ref().unwrap()));
        } else {
            fmt_string.push_str(&self.name.to_string());
        }

        write!(f, "{}", fmt_string)
    }
}

#[derive(Debug, Default)]
pub struct DockerCommand<C> {
    _docker: PhantomData<C>,
    pub command_string: Option<String>,
}

impl<T> From<DockerCommand<T>> for String {
    fn from(command: DockerCommand<T>) -> String {
        command.command_string.unwrap_or_default()
    }
}

impl<T> DockerCommand<T> {
    pub fn execute(&self, args: Option<Vec<String>>) -> DockerResult<()> {
        if let Some(cmd_str) = &self.command_string {
            let mut cmd = Command::new("docker");
            cmd_str.split(' ').for_each(|arg| {
                cmd.arg(arg);
            });
            if let Some(args) = args {
                cmd.args(args);
            }

            let _cmd_dis = cmd.get_program().to_str().unwrap();
            let mut child = cmd
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .stdin(Stdio::null())
                .spawn()?;
            let res = loop {
                match child.try_wait() {
                    Ok(Some(status)) => {
                        if status.success() {
                            break Ok(());
                        } else {
                            let err = anyhow!("Docker exited with status {:?}", status.code());
                            break Err(docker::DockerError::Any(err));
                        }
                    }
                    Ok(None) => {
                        println!("Waiting 500ms");
                        std::thread::sleep(Duration::from_millis(500));
                        continue;
                    }
                    Err(e) => {
                        panic!("Error executing docker command: {}", e);
                    }
                }
            };

            res
        } else {
            Err(DockerError::NoImage)
        }
    }
}

impl DockerCommand<DockerImage> {
    fn format_command(
        &self,
        image: &DockerImage,
        command_string: &str,
        flags: Vec<String>,
    ) -> String {
        let flags_string = flags
            .iter()
            .map(|flag| format!("--{}", flag))
            .collect::<Vec<String>>()
            .join(" ");

        if !(&flags).is_empty() {
            format!("image {} {} {}", command_string, flags_string, image)
        } else {
            format!("image {} {}", command_string, image)
        }
    }

    pub fn build(&self, image: &DockerImage, rm: bool) -> DockerResult<DockerCommand<DockerImage>> {
        let mut flags = vec![];
        if rm {
            flags.push("rm".to_string());
        }
        let build_command_string = self.format_command(image, "build", flags);
        Ok(DockerCommand::<DockerImage> {
            command_string: Some(build_command_string),
            _docker: PhantomData::<DockerImage>,
        })
    }

    pub fn remove(&self, image: &DockerImage) -> DockerResult<DockerCommand<DockerImage>> {
        let build_command_string = self.format_command(image, "rm", vec![]);
        Ok(DockerCommand::<DockerImage> {
            command_string: Some(build_command_string),
            _docker: PhantomData::<DockerImage>,
        })
    }

    pub fn prune() -> DockerResult<()> {
        Ok(())
    }
}

impl DockerCommand<DockerContainer<'_>> {
    fn format_command(
        &self,
        container: &DockerContainer,
        command_string: &str,
        flags: Vec<String>,
    ) -> String {
        let flags_string = flags
            .iter()
            .map(|flag| format!("--{}", flag))
            .collect::<Vec<String>>()
            .join(" ");

        if !(&flags).is_empty() {
            format!(
                "container {} {} {}",
                command_string, flags_string, container
            )
        } else {
            format!("container {} {}", command_string, container)
        }
    }
    pub fn run<'a>(
        self,
        container: &'a DockerContainer,
        rm: bool,
        detach: bool,
    ) -> DockerResult<DockerCommand<DockerContainer<'a>>> {
        let mut flags = vec![];
        if rm {
            flags.push("rm".into());
        }
        if detach {
            flags.push("detach".into());
        }
        let run_cmd_str = self.format_command(container, "run", flags);

        Ok(DockerCommand::<DockerContainer> {
            command_string: Some(run_cmd_str),
            _docker: PhantomData::<DockerContainer>,
        })
    }

    pub fn exec(
        &self,
        container: &DockerContainer,
        flags: Vec<String>,
    ) -> DockerResult<DockerCommand<DockerContainer>> {
        let cmd = self.format_command(container, "exec", flags);
        Ok(DockerCommand::<DockerContainer> {
            command_string: Some(cmd),
            _docker: PhantomData::<DockerContainer>,
        })
    }

    pub fn cp_from<'a>(
        container: &'a DockerContainer,
        file_path: &Path,
        dest_path: &Path,
    ) -> DockerResult<DockerCommand<DockerContainer<'a>>> {
        let cmd = format!(
            "container cp {}:{} {}",
            &container.name,
            &file_path.to_str().unwrap(),
            &dest_path.to_str().unwrap()
        );
        Ok(DockerCommand::<DockerContainer> {
            command_string: Some(cmd),
            _docker: PhantomData::<DockerContainer>,
        })
    }

    pub fn cp_to<'a>(
        container: &'a DockerContainer,
        file_path: &Path,
        dest_path: &Path,
    ) -> DockerResult<DockerCommand<DockerContainer<'a>>> {
        let cmd = format!(
            "container cp {} {}:{}",
            &file_path.to_str().unwrap(),
            &container.name,
            &dest_path.to_str().unwrap()
        );
        Ok(DockerCommand::<DockerContainer> {
            command_string: Some(cmd),
            _docker: PhantomData::<DockerContainer>,
        })
    }

    pub fn start<'a>(
        container: &'a DockerContainer,
    ) -> DockerResult<DockerCommand<DockerContainer<'a>>> {
        let cmd = format!("container start {}", &container.name);
        Ok(DockerCommand::<DockerContainer> {
            command_string: Some(cmd),
            _docker: PhantomData::<DockerContainer>,
        })
    }

    pub fn stop<'a>(
        container: &'a DockerContainer,
    ) -> DockerResult<DockerCommand<DockerContainer<'a>>> {
        let cmd = format!("container stop {}", &container.name);
        Ok(DockerCommand::<DockerContainer> {
            command_string: Some(cmd),
            _docker: PhantomData::<DockerContainer>,
        })
    }

    pub fn pause<'a>(
        container: &'a DockerContainer,
    ) -> DockerResult<DockerCommand<DockerContainer<'a>>> {
        let cmd = format!("container pause {}", &container.name);
        Ok(DockerCommand::<DockerContainer> {
            command_string: Some(cmd),
            _docker: PhantomData::<DockerContainer>,
        })
    }

    pub fn unpause<'a>(
        container: &'a DockerContainer,
    ) -> DockerResult<DockerCommand<DockerContainer<'a>>> {
        let cmd = format!("container unpause {}", &container.name);
        Ok(DockerCommand::<DockerContainer> {
            command_string: Some(cmd),
            _docker: PhantomData::<DockerContainer>,
        })
    }

    pub fn restart<'a>(
        container: &'a DockerContainer,
    ) -> DockerResult<DockerCommand<DockerContainer<'a>>> {
        let cmd = format!("container restart {}", &container.name);
        Ok(DockerCommand::<DockerContainer> {
            command_string: Some(cmd),
            _docker: PhantomData::<DockerContainer>,
        })
    }
}

#[derive(Debug, Default)]
pub struct Docker {
    name: Option<String>,
    port_bindings: Vec<(Port, Port)>,
    env_vars: HashMap<String, String>,
    volumes: HashMap<VolumePath, VolumePath>,
    virtuals: Vec<VirtualEnv>,
}

impl Docker {
    pub fn add_service(mut self, env: VirtualEnv) -> DockerResult<Self> {
        let VirtualEnv {
            host: _,
            container_port,
            local_binding,
            container_mount,
            host_port,
        } = env.clone();
        self.port_bindings
            .push((host_port.into(), container_port.into()));
        let local_binding = local_binding.canonicalize()?;
        self.volumes
            .insert(local_binding.into(), container_mount.into());
        self.virtuals.push(env);

        Ok(self)
    }

    pub fn build(&self) -> DockerResult<()> {
        let mut cmd = Command::new(DOCKER_BIN);
        cmd.arg("build");
        cmd.arg(".");
        cmd.arg("-t");
        cmd.arg(IMAGE_NAME);
        let output = cmd
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .output()?;
        std::io::stdout().write_all(&output.stdout)?;
        std::io::stdout().write_all(&output.stderr)?;

        Ok(())
    }

    pub fn env_var(mut self, key: String, val: String) -> DockerResult<Self> {
        self.env_vars.insert(key, val);
        Ok(self)
    }

    pub fn name(mut self, name: &str) -> Self {
        self.name = Some(name.to_string());
        self
    }

    pub fn exec(container_name: &str, exec_args: Vec<&str>, _work_dir: &str) -> DockerResult<()> {
        let mut cmd = Command::new(DOCKER_BIN);
        cmd.args(&["exec", "-d", container_name, "bash", "-c"]);

        let args_string = exec_args.join(" ");
        let args_string = args_string;
        println!("Args string: {}", args_string);
        cmd.arg(args_string.as_str());

        println!("{:?}", cmd);
        cmd.stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .stdin(Stdio::null())
            .spawn()?;

        Ok(())
    }

    pub fn restart(&self) -> DockerResult<()> {
        let mut cmd = Command::new(DOCKER_BIN);
        cmd.args(&["restart", self.name.as_ref().unwrap().as_str()]);

        let _child = cmd.stdout(Stdio::null()).stderr(Stdio::inherit()).spawn()?;

        Ok(())
    }
    pub fn run(
        &self,
        exec_args: Option<Vec<String>>,
        name_mod: Option<&str>,
        additional_ports: Vec<(Port, Port)>,
    ) -> DockerResult<()> {
        let mut cmd = Command::new(DOCKER_BIN);
        cmd.args(&["run", "--rm", "-d", "-eCKB_CHAIN:dev"]);
        let mut container_name = None;

        if let Some(name) = self.name.as_ref() {
            if let Some(name_mod) = name_mod {
                let display_name = format!("{}-{}", &name, name_mod);
                container_name = Some(display_name);
                cmd.arg("--name").arg(container_name.as_ref().unwrap());
            } else {
                container_name = Some(name.clone());
                cmd.arg("--name").arg(name);
            }
        }
        self.port_bindings.iter().for_each(|bind| {
            cmd.arg(format!("-p{}:{}", bind.0, bind.1).as_str());
        });
        additional_ports.iter().for_each(|bind| {
            cmd.arg(format!("-p{}:{}", bind.0, bind.1).as_str());
        });
        self.volumes.iter().for_each(|bind| {
            cmd.arg(format!("-v{}:{}", bind.0, bind.1).as_str());
        });

        cmd.arg(IMAGE_NAME);
        if exec_args.is_some() {
            cmd.args(exec_args.unwrap().as_slice());
        }

        let _child = cmd.stdout(Stdio::null()).stderr(Stdio::null()).spawn()?;

        println!("Successfully started network!\n");
        if container_name.is_some() {
            println!("Running in container: {}", container_name.unwrap());
        }
        self.virtuals.iter().for_each(|e| println!("{}", e));

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn image() -> DockerImage {
        DockerImage {
            name: "trampoline".to_string(),
            tag: Some("latest".to_string()),
            file_path: Some("./docker".to_string()),
            host_mappings: vec![],
            build_args: HashMap::new(),
        }
    }

    fn image_2() -> DockerImage {
        DockerImage {
            name: "trampoline".to_string(),
            tag: None,
            file_path: None,
            host_mappings: vec![],
            build_args: HashMap::new(),
        }
    }

    fn dummy_container() -> DockerContainer<'static> {
        let image = image();

        let docker_volume = Volume {
            host: Path::new("/test/path/host"),
            container: Path::new("/test/path/container"),
        };

        let container = DockerContainer {
            name: String::from("test-container"),
            port_bindings: vec![DockerPort {
                host: 7357,
                container: 7357,
            }],
            volumes: vec![docker_volume],
            env_vars: HashMap::default(),
            image,
        };

        container
    }

    #[test]
    fn test_container_cp_to() {
        let container = dummy_container();
        let file_path = Path::new("./file.test");
        let dest_path = Path::new("/var/lib/ckb");
        let cmd =
            DockerCommand::<DockerContainer>::cp_to(&container, file_path, dest_path).unwrap();
        assert_eq!(
            cmd.command_string.as_ref().unwrap().as_str(),
            format!("container cp ./file.test {}:/var/lib/ckb", container.name)
        );
    }

    fn test_container_cp_from() {
        let container = dummy_container();
        let file_path = Path::new("/var/lib/ckb/file.test");
        let dest_path = Path::new(".");
        let cmd =
            DockerCommand::<DockerContainer>::cp_from(&container, file_path, dest_path).unwrap();
        assert_eq!(
            cmd.command_string.as_ref().unwrap().as_str(),
            format!("container cp {}:/var/lib/ckb/file.test .", container.name)
        );
    }

    #[test]
    fn test_container_start() {
        let container = dummy_container();
        let cmd = DockerCommand::<DockerContainer>::start(&container).unwrap();
        assert_eq!(
            cmd.command_string.as_ref().unwrap().as_str(),
            format!("container start {}", container.name)
        );
    }

    #[test]
    fn test_container_stop() {
        let container = dummy_container();
        let cmd = DockerCommand::<DockerContainer>::stop(&container).unwrap();
        assert_eq!(
            cmd.command_string.as_ref().unwrap().as_str(),
            format!("container stop {}", container.name)
        );
    }

    #[test]
    fn test_container_pause() {
        let container = dummy_container();
        let cmd = DockerCommand::<DockerContainer>::pause(&container).unwrap();
        assert_eq!(
            cmd.command_string.as_ref().unwrap().as_str(),
            format!("container pause {}", container.name)
        );
    }

    #[test]
    fn test_container_unpause() {
        let container = dummy_container();
        let cmd = DockerCommand::<DockerContainer>::unpause(&container).unwrap();
        assert_eq!(
            cmd.command_string.as_ref().unwrap().as_str(),
            format!("container unpause {}", container.name)
        );
    }

    #[test]
    fn test_container_restart() {
        let container = dummy_container();
        let cmd = DockerCommand::<DockerContainer>::restart(&container).unwrap();
        assert_eq!(
            cmd.command_string.as_ref().unwrap().as_str(),
            format!("container restart {}", container.name)
        );
    }

    #[test]
    fn test_build_format_command() {
        let image = image();
        let command = DockerCommand::default().build(&image, true).unwrap();
        assert_eq!(
            command.command_string.as_ref().unwrap().as_str(),
            "image build --rm ./docker -t trampoline:latest"
        );
    }

    #[test]
    fn test_rm_format_command() {
        let image = image_2();
        let command = DockerCommand::default().remove(&image).unwrap();
        assert_eq!(
            command.command_string.as_ref().unwrap().as_str(),
            "image rm trampoline"
        );
    }

    #[test]
    fn test_run_format_command() {
        let image = image_2();
        let host_path = Path::new("./").canonicalize().unwrap();
        let cwd = std::env::current_dir().unwrap();
        let container = DockerContainer {
            name: "test-container".to_string(),
            port_bindings: vec![DockerPort {
                host: 80,
                container: 80,
            }],
            volumes: vec![Volume {
                host: &host_path,
                container: Path::new("/data"),
            }],
            env_vars: HashMap::new(),
            image,
        };
        let expected_host_path_string = cwd.display();
        let expected_docker_command = format!(
            "container run --rm --detach -p80:80 --name test-container -v{}:/data trampoline",
            expected_host_path_string
        );
        let command = DockerCommand::default()
            .run(&container, true, true)
            .unwrap();
        assert_eq!(
            command.command_string.as_ref().unwrap().as_str(),
            expected_docker_command.as_str()
        )
    }
}
