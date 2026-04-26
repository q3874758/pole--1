#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServiceState {
    Stopped,
    Starting { pid: u32 },
    Running { pid: u32 },
    Failed { last_error: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceSnapshot {
    pub state_label: &'static str,
    pub pid: Option<u32>,
    pub stale: bool,
    pub recoverable_without_manual_cleanup: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManagedServiceStatus {
    NotInstalled,
    Stopped,
    Starting { pid: Option<u32> },
    Running { pid: Option<u32> },
    Failed { last_error: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServiceManagerError {
    Unsupported {
        action: &'static str,
        platform: &'static str,
    },
    InvalidDefinition(String),
    Io(String),
}

impl std::fmt::Display for ServiceManagerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unsupported { action, platform } => {
                write!(f, "{action} is not supported on {platform}")
            }
            Self::InvalidDefinition(message) => write!(f, "invalid service definition: {message}"),
            Self::Io(message) => write!(f, "service io error: {message}"),
        }
    }
}

impl std::error::Error for ServiceManagerError {}

pub trait ServiceManager {
    fn service_name(&self) -> &str;
    fn install(&self) -> Result<(), ServiceManagerError>;
    fn uninstall(&self) -> Result<(), ServiceManagerError>;
    fn start(&self) -> Result<(), ServiceManagerError>;
    fn stop(&self) -> Result<(), ServiceManagerError>;
    fn status(&self) -> Result<ManagedServiceStatus, ServiceManagerError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceRuntime {
    state: ServiceState,
    stale: bool,
}

impl Default for ServiceRuntime {
    fn default() -> Self {
        Self {
            state: ServiceState::Stopped,
            stale: false,
        }
    }
}

impl ServiceRuntime {
    pub fn from_observed_process(pid: Option<u32>, is_running: bool) -> Self {
        let mut runtime = Self::default();
        runtime.observe_process(pid, is_running);
        runtime
    }

    pub fn from_persisted_observation(
        observed_pid: Option<u32>,
        persisted_pid: Option<u32>,
        is_running: bool,
    ) -> Self {
        let mut runtime = Self::default();
        runtime.observe_persisted_process(observed_pid, persisted_pid, is_running);
        runtime
    }

    pub fn state(&self) -> &ServiceState {
        &self.state
    }

    pub fn snapshot(&self) -> ServiceSnapshot {
        ServiceSnapshot {
            state_label: self.state_label(),
            pid: self.current_pid(),
            stale: self.is_stale(),
            recoverable_without_manual_cleanup: self.can_recover_without_manual_cleanup(),
        }
    }

    pub fn is_stale(&self) -> bool {
        self.stale
    }

    pub fn is_running(&self) -> bool {
        matches!(self.state, ServiceState::Running { .. })
    }

    pub fn current_pid(&self) -> Option<u32> {
        match self.state {
            ServiceState::Starting { pid } | ServiceState::Running { pid } => Some(pid),
            ServiceState::Stopped | ServiceState::Failed { .. } => None,
        }
    }

    pub fn state_label(&self) -> &'static str {
        match self.state {
            ServiceState::Stopped => "stopped",
            ServiceState::Starting { .. } => "starting",
            ServiceState::Running { .. } => "running",
            ServiceState::Failed { .. } => "failed",
        }
    }

    pub fn observe_process(&mut self, pid: Option<u32>, is_running: bool) {
        match (pid, is_running) {
            (Some(pid), true) => self.mark_running(pid),
            (Some(pid), false) => {
                self.mark_starting(pid);
                self.mark_stale();
            }
            (None, _) => self.mark_stopped(),
        }
    }

    pub fn observe_persisted_process(
        &mut self,
        observed_pid: Option<u32>,
        persisted_pid: Option<u32>,
        is_running: bool,
    ) {
        self.observe_process(observed_pid.or(persisted_pid), is_running);
    }

    pub fn mark_starting(&mut self, pid: u32) {
        self.state = ServiceState::Starting { pid };
        self.stale = false;
    }

    pub fn mark_running(&mut self, pid: u32) {
        self.state = ServiceState::Running { pid };
        self.stale = false;
    }

    pub fn mark_failed(&mut self, last_error: impl Into<String>) {
        self.state = ServiceState::Failed {
            last_error: last_error.into(),
        };
        self.stale = false;
    }

    pub fn mark_stopped(&mut self) {
        self.state = ServiceState::Stopped;
        self.stale = false;
    }

    pub fn mark_stale(&mut self) {
        self.stale = true;
    }

    pub fn can_recover_without_manual_cleanup(&self) -> bool {
        self.stale
            && matches!(
                self.state,
                ServiceState::Starting { .. } | ServiceState::Running { .. }
            )
    }

    pub fn managed_status(&self) -> ManagedServiceStatus {
        match &self.state {
            ServiceState::Stopped => ManagedServiceStatus::Stopped,
            ServiceState::Starting { pid } => ManagedServiceStatus::Starting { pid: Some(*pid) },
            ServiceState::Running { pid } => ManagedServiceStatus::Running { pid: Some(*pid) },
            ServiceState::Failed { last_error } => ManagedServiceStatus::Failed {
                last_error: last_error.clone(),
            },
        }
    }
}
