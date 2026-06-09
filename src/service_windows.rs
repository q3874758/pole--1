use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::{ManagedServiceStatus, ServiceManager, ServiceManagerError};
use serde_json::json;

pub const WINDOWS_SERVICE_NAME: &str = "PoLENode";
pub const WINDOWS_SERVICE_DISPLAY_NAME: &str = "PoLE Node Service";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsServiceDefinition {
    pub service_name: String,
    pub display_name: String,
    pub description: String,
    pub executable_path: PathBuf,
    pub config_path: PathBuf,
    pub service_root: PathBuf,
    pub sc_binary: PathBuf,
}

impl WindowsServiceDefinition {
    pub fn new(executable_path: impl AsRef<Path>, config_path: impl AsRef<Path>) -> Self {
        Self {
            service_name: WINDOWS_SERVICE_NAME.to_string(),
            display_name: WINDOWS_SERVICE_DISPLAY_NAME.to_string(),
            description: "PoLE node background service".to_string(),
            executable_path: executable_path.as_ref().to_path_buf(),
            config_path: config_path.as_ref().to_path_buf(),
            service_root: PathBuf::from("C:/ProgramData/PoLE/services"),
            sc_binary: PathBuf::from("sc.exe"),
        }
    }

    pub fn with_service_root(mut self, service_root: impl AsRef<Path>) -> Self {
        self.service_root = service_root.as_ref().to_path_buf();
        self
    }

    pub fn with_sc_binary(mut self, sc_binary: impl AsRef<Path>) -> Self {
        self.sc_binary = sc_binary.as_ref().to_path_buf();
        self
    }

    pub fn binary_path(&self) -> String {
        format!(
            "\"{}\" service-run \"{}\"",
            self.executable_path.display(),
            self.config_path.display()
        )
    }

    pub fn registration_path(&self) -> PathBuf {
        self.service_root
            .join(format!("{}.service.json", self.service_name))
    }

    pub fn render_registration_payload(&self) -> String {
        json!({
            "service_name": self.service_name,
            "display_name": self.display_name,
            "binary_path": self.binary_path(),
        })
        .to_string()
    }

    pub fn sc_create_command(&self) -> String {
        format!(
            "sc.exe create {} binPath= {} DisplayName= {}",
            self.service_name,
            self.binary_path(),
            self.display_name
        )
    }

    pub fn sc_start_command(&self) -> String {
        format!("sc.exe start {}", self.service_name)
    }

    pub fn sc_stop_command(&self) -> String {
        format!("sc.exe stop {}", self.service_name)
    }

    pub fn sc_query_command(&self) -> String {
        format!("sc.exe query {}", self.service_name)
    }
}

#[derive(Debug, Clone)]
pub struct WindowsServiceManager {
    definition: WindowsServiceDefinition,
}

impl WindowsServiceManager {
    pub fn new(definition: WindowsServiceDefinition) -> Self {
        Self { definition }
    }
}

impl ServiceManager for WindowsServiceManager {
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
        let registration_path = self.definition.registration_path();
        if let Some(parent) = registration_path.parent() {
            fs::create_dir_all(parent).map_err(|err| ServiceManagerError::Io(err.to_string()))?;
        }
        fs::write(
            &registration_path,
            self.definition.render_registration_payload(),
        )
        .map_err(|err| ServiceManagerError::Io(err.to_string()))?;
        Ok(())
    }

    fn uninstall(&self) -> Result<(), ServiceManagerError> {
        let registration_path = self.definition.registration_path();
        if registration_path.exists() {
            fs::remove_file(&registration_path)
                .map_err(|err| ServiceManagerError::Io(err.to_string()))?;
        }
        Ok(())
    }

    fn start(&self) -> Result<(), ServiceManagerError> {
        if !self.definition.registration_path().exists() {
            return Err(ServiceManagerError::Io(format!(
                "service {} is not installed. click install first.",
                self.definition.service_name
            )));
        }
        let output = Command::new(&self.definition.sc_binary)
            .args(["start", &self.definition.service_name])
            .output();

        match output {
            Ok(output) if output.status.success() => Ok(()),
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr).to_lowercase();
                let stdout = String::from_utf8_lossy(&output.stdout).to_lowercase();
                let msg = if stdout.contains("access is denied")
                    || stderr.contains("access is denied")
                {
                    "requires administrator privileges. right-click pole-client and choose 'run as administrator', or install the service from an elevated command prompt."
                } else if stdout.contains("service has not been started")
                    || stderr.contains("service has not been started")
                {
                    "service failed to start. check windows event viewer for details."
                } else {
                    "failed to start service. ensure you have administrator privileges."
                };
                Err(ServiceManagerError::Io(msg.to_string()))
            }
            Err(e) => Err(ServiceManagerError::Io(format!(
                "failed to execute sc start: {}. ensure you have administrator privileges.",
                e
            ))),
        }
    }

    fn stop(&self) -> Result<(), ServiceManagerError> {
        if !self.definition.registration_path().exists() {
            return Ok(());
        }
        let output = Command::new(&self.definition.sc_binary)
            .args(["stop", &self.definition.service_name])
            .output();

        match output {
            Ok(output) if output.status.success() => Ok(()),
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr).to_lowercase();
                let stdout = String::from_utf8_lossy(&output.stdout).to_lowercase();
                let msg =
                    if stdout.contains("access is denied") || stderr.contains("access is denied") {
                        "requires administrator privileges"
                    } else {
                        "failed to stop service"
                    };
                Err(ServiceManagerError::Io(msg.to_string()))
            }
            Err(e) => Err(ServiceManagerError::Io(format!(
                "failed to execute sc stop: {}",
                e
            ))),
        }
    }

    fn status(&self) -> Result<ManagedServiceStatus, ServiceManagerError> {
        if !self.definition.registration_path().exists() {
            return Ok(ManagedServiceStatus::NotInstalled);
        }

        let output = Command::new(&self.definition.sc_binary)
            .args(["query", &self.definition.service_name])
            .output();

        let Ok(output) = output else {
            return Ok(ManagedServiceStatus::Stopped);
        };
        if !output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout).to_ascii_uppercase();
            let stderr = String::from_utf8_lossy(&output.stderr).to_ascii_uppercase();
            let state = format!("{stdout}\n{stderr}");
            if state.contains("FAILED 1060") || state.contains("DOES NOT EXIST") {
                return Ok(ManagedServiceStatus::NotInstalled);
            }
            return Ok(ManagedServiceStatus::Stopped);
        }

        let stdout = String::from_utf8_lossy(&output.stdout).to_ascii_uppercase();
        let stderr = String::from_utf8_lossy(&output.stderr).to_ascii_uppercase();
        let state = if !stdout.is_empty() { stdout } else { stderr };

        if state.contains("RUNNING") {
            Ok(ManagedServiceStatus::Running { pid: None })
        } else if state.contains("START_PENDING") || state.contains("START PENDING") {
            Ok(ManagedServiceStatus::Starting { pid: None })
        } else if state.contains("STOPPED") {
            Ok(ManagedServiceStatus::Stopped)
        } else if state.contains("FAILED") {
            Ok(ManagedServiceStatus::Failed {
                last_error: "windows service manager reported failed".to_string(),
            })
        } else {
            Ok(ManagedServiceStatus::Stopped)
        }
    }
}
