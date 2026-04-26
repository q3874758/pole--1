use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::{ManagedServiceStatus, ServiceManager, ServiceManagerError};

pub const SYSTEMD_SERVICE_NAME: &str = "pole-node";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemdUnitDefinition {
    pub service_name: String,
    pub description: String,
    pub executable_path: PathBuf,
    pub config_path: PathBuf,
    pub working_directory: PathBuf,
    pub unit_root: PathBuf,
    pub systemctl_binary: PathBuf,
}

impl SystemdUnitDefinition {
    pub fn new(
        executable_path: impl AsRef<Path>,
        config_path: impl AsRef<Path>,
        working_directory: impl AsRef<Path>,
    ) -> Self {
        Self {
            service_name: SYSTEMD_SERVICE_NAME.to_string(),
            description: "PoLE node service".to_string(),
            executable_path: executable_path.as_ref().to_path_buf(),
            config_path: config_path.as_ref().to_path_buf(),
            working_directory: working_directory.as_ref().to_path_buf(),
            unit_root: PathBuf::from("/etc/systemd/system"),
            systemctl_binary: PathBuf::from("systemctl"),
        }
    }

    pub fn with_unit_root(mut self, unit_root: impl AsRef<Path>) -> Self {
        self.unit_root = unit_root.as_ref().to_path_buf();
        self
    }

    pub fn with_systemctl_binary(mut self, systemctl_binary: impl AsRef<Path>) -> Self {
        self.systemctl_binary = systemctl_binary.as_ref().to_path_buf();
        self
    }

    pub fn render(&self) -> String {
        format!(
            "[Unit]\nDescription={}\nAfter=network-online.target\nWants=network-online.target\n\n[Service]\nType=simple\nExecStart={} service-run {}\nWorkingDirectory={}\nRestart=on-failure\nRestartSec=5\n\n[Install]\nWantedBy=multi-user.target\n",
            self.description,
            self.executable_path.display(),
            self.config_path.display(),
            self.working_directory.display()
        )
    }

    pub fn unit_path(&self) -> PathBuf {
        self.unit_root
            .join(format!("{}.service", self.service_name))
    }

    pub fn systemctl_start_command(&self) -> String {
        format!("systemctl start {}.service", self.service_name)
    }

    pub fn systemctl_stop_command(&self) -> String {
        format!("systemctl stop {}.service", self.service_name)
    }

    pub fn systemctl_status_command(&self) -> String {
        format!("systemctl is-active {}.service", self.service_name)
    }
}

#[derive(Debug, Clone)]
pub struct SystemdServiceManager {
    definition: SystemdUnitDefinition,
}

impl SystemdServiceManager {
    pub fn new(definition: SystemdUnitDefinition) -> Self {
        Self { definition }
    }
}

impl ServiceManager for SystemdServiceManager {
    fn service_name(&self) -> &str {
        &self.definition.service_name
    }

    fn install(&self) -> Result<(), ServiceManagerError> {
        if self.definition.executable_path.as_os_str().is_empty() {
            return Err(ServiceManagerError::InvalidDefinition(
                "missing executable path".to_string(),
            ));
        }
        if self.definition.config_path.as_os_str().is_empty() {
            return Err(ServiceManagerError::InvalidDefinition(
                "missing config path".to_string(),
            ));
        }
        let unit_path = self.definition.unit_path();
        if let Some(parent) = unit_path.parent() {
            fs::create_dir_all(parent).map_err(|err| ServiceManagerError::Io(err.to_string()))?;
        }
        fs::write(&unit_path, self.definition.render())
            .map_err(|err| ServiceManagerError::Io(err.to_string()))?;
        Ok(())
    }

    fn uninstall(&self) -> Result<(), ServiceManagerError> {
        let unit_path = self.definition.unit_path();
        if unit_path.exists() {
            fs::remove_file(&unit_path).map_err(|err| ServiceManagerError::Io(err.to_string()))?;
        }
        Ok(())
    }

    fn start(&self) -> Result<(), ServiceManagerError> {
        if !self.definition.unit_path().exists() {
            return Ok(());
        }
        let status = Command::new(&self.definition.systemctl_binary)
            .args([
                "start",
                &format!("{}.service", self.definition.service_name),
            ])
            .status()
            .map_err(|err| ServiceManagerError::Io(err.to_string()))?;
        if status.success() {
            Ok(())
        } else {
            Err(ServiceManagerError::Io(format!(
                "failed to start service {}",
                self.definition.service_name
            )))
        }
    }

    fn stop(&self) -> Result<(), ServiceManagerError> {
        if !self.definition.unit_path().exists() {
            return Ok(());
        }
        let status = Command::new(&self.definition.systemctl_binary)
            .args(["stop", &format!("{}.service", self.definition.service_name)])
            .status()
            .map_err(|err| ServiceManagerError::Io(err.to_string()))?;
        if status.success() {
            Ok(())
        } else {
            Err(ServiceManagerError::Io(format!(
                "failed to stop service {}",
                self.definition.service_name
            )))
        }
    }

    fn status(&self) -> Result<ManagedServiceStatus, ServiceManagerError> {
        if !self.definition.unit_path().exists() {
            return Ok(ManagedServiceStatus::NotInstalled);
        }

        let output = Command::new(&self.definition.systemctl_binary)
            .args([
                "is-active",
                &format!("{}.service", self.definition.service_name),
            ])
            .output();

        let Ok(output) = output else {
            return Ok(ManagedServiceStatus::Stopped);
        };

        let stdout = String::from_utf8_lossy(&output.stdout)
            .trim()
            .to_ascii_lowercase();
        let stderr = String::from_utf8_lossy(&output.stderr)
            .trim()
            .to_ascii_lowercase();
        let state = if !stdout.is_empty() { stdout } else { stderr };

        if state.contains("active") {
            Ok(ManagedServiceStatus::Running { pid: None })
        } else if state.contains("activating") {
            Ok(ManagedServiceStatus::Starting { pid: None })
        } else if state.contains("failed") {
            Ok(ManagedServiceStatus::Failed {
                last_error: "systemd reported failed".to_string(),
            })
        } else {
            Ok(ManagedServiceStatus::Stopped)
        }
    }
}
