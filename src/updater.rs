use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::{
    current_platform, load_release_manifest_for_channel, release_manifest_path,
    resolve_install_layout, verify_release_manifest_signature, version_is_newer, InstallMode,
    ManagedServiceStatus, ReleaseArtifact, ReleaseManifest,
};
use serde::{Deserialize, Serialize};

const INSTALLED_LAYOUT_ROOT_OVERRIDE_ENV: &str = "POLE_INSTALLED_LAYOUT_ROOT_OVERRIDE";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateOverview {
    pub current_version: String,
    pub channel: String,
    pub update_dir: PathBuf,
    pub release_manifest_dir: PathBuf,
    pub update_available: bool,
    pub latest_available_version: Option<String>,
    pub signing_status: String,
    pub rollback_status: String,
    pub latest_manifest_path: PathBuf,
    pub artifact_count: usize,
    pub pending_target_version: Option<String>,
    pub applied_target_version: Option<String>,
    pub selected_artifact_kind: Option<String>,
    pub selected_artifact_path: Option<String>,
    pub executed_artifact_path: Option<String>,
    pub planned_install_path: Option<String>,
    pub planned_backup_path: Option<String>,
    pub executed_install_path: Option<String>,
    pub install_target_mode: Option<String>,
    pub install_action_status: String,
    pub switch_execution_status: String,
    pub service_window_status: String,
}

pub fn collect_update_overview(
    current_version: &str,
    channel: &str,
    update_dir: impl AsRef<Path>,
    release_manifest_dir: impl AsRef<Path>,
) -> UpdateOverview {
    collect_update_overview_with_status(
        current_version,
        channel,
        update_dir,
        release_manifest_dir,
        None,
    )
}

pub fn collect_update_overview_with_status(
    current_version: &str,
    channel: &str,
    update_dir: impl AsRef<Path>,
    release_manifest_dir: impl AsRef<Path>,
    managed_status: Option<ManagedServiceStatus>,
) -> UpdateOverview {
    let update_dir = update_dir.as_ref().to_path_buf();
    let release_manifest_dir = release_manifest_dir.as_ref().to_path_buf();
    let latest_manifest_path = release_manifest_path(&release_manifest_dir, channel);
    let effective_current_version = installed_version_record_path(&update_dir)
        .pipe(load_installed_version_record)
        .ok()
        .map(|record| record.version)
        .unwrap_or_else(|| current_version.to_string());
    let pending_update_path = pending_update_plan_path(&update_dir);
    let pending_plan = load_pending_update_plan(&pending_update_path).ok();
    let pending_target_version = pending_plan
        .as_ref()
        .map(|plan| plan.target_version.clone());
    let applied_update_path = applied_update_record_path(&update_dir);
    let applied_target_version = load_applied_update_record(&applied_update_path)
        .ok()
        .map(|record| record.target_version);
    let switch_plan_path = switch_plan_path(&update_dir);
    let switch_plan = load_switch_plan(&switch_plan_path).ok();
    let switch_execution_path = switch_execution_record_path(&update_dir);
    let switch_execution = load_switch_execution_record(&switch_execution_path).ok();
    let install_action_path = install_action_plan_path(&update_dir);
    let install_action = load_install_action_plan(&install_action_path).ok();
    let install_execution_path = install_execution_record_path(&update_dir);
    let install_execution = load_install_execution_record(&install_execution_path).ok();
    let service_window_status = service_window_status(&update_dir, managed_status);

    let rollback_status = if update_dir.join("rollback.json").exists() {
        "available".to_string()
    } else {
        "planned".to_string()
    };

    match load_release_manifest_for_channel(&release_manifest_dir, channel) {
        Ok(manifest) => {
            let verification = verify_release_manifest_signature(&manifest)
                .unwrap_or(crate::ManifestSignatureVerification::Invalid);
            UpdateOverview {
                current_version: effective_current_version.clone(),
                channel: manifest.channel,
                update_dir,
                release_manifest_dir,
                update_available: verification.is_verified()
                    && version_is_newer(&manifest.version, &effective_current_version),
                latest_available_version: Some(manifest.version),
                signing_status: verification.status_label().to_string(),
                rollback_status,
                latest_manifest_path,
                artifact_count: manifest.artifacts.len(),
                pending_target_version,
                applied_target_version,
                selected_artifact_kind: switch_plan
                    .as_ref()
                    .map(|plan| plan.artifact_kind.clone())
                    .or_else(|| pending_plan.as_ref().map(|plan| plan.artifact_kind.clone())),
                selected_artifact_path: switch_plan
                    .as_ref()
                    .map(|plan| plan.artifact_path.clone())
                    .or_else(|| pending_plan.as_ref().map(|plan| plan.artifact_path.clone())),
                executed_artifact_path: switch_execution
                    .as_ref()
                    .map(|record| record.applied_artifact_path.clone()),
                planned_install_path: install_action
                    .as_ref()
                    .map(|plan| plan.target_install_path.clone()),
                planned_backup_path: install_action.as_ref().map(|plan| plan.backup_path.clone()),
                executed_install_path: install_execution
                    .as_ref()
                    .map(|record| record.target_install_path.clone()),
                install_target_mode: install_execution
                    .as_ref()
                    .map(|record| record.target_mode.clone())
                    .or_else(|| install_action.as_ref().map(|plan| plan.target_mode.clone())),
                install_action_status: if install_execution.is_some() {
                    "install_action_executed".to_string()
                } else if install_action.is_some() {
                    "install_action_planned".to_string()
                } else {
                    "not_planned".to_string()
                },
                switch_execution_status: if switch_execution.is_some() {
                    "artifact_switched".to_string()
                } else {
                    "not_executed".to_string()
                },
                service_window_status,
            }
        }
        Err(_) => UpdateOverview {
            current_version: effective_current_version,
            channel: channel.to_string(),
            update_dir,
            release_manifest_dir,
            update_available: false,
            latest_available_version: None,
            signing_status: "manifest_missing".to_string(),
            rollback_status,
            latest_manifest_path,
            artifact_count: 0,
            pending_target_version,
            applied_target_version,
            selected_artifact_kind: switch_plan
                .as_ref()
                .map(|plan| plan.artifact_kind.clone())
                .or_else(|| pending_plan.as_ref().map(|plan| plan.artifact_kind.clone())),
            selected_artifact_path: switch_plan.as_ref().map(|plan| plan.artifact_path.clone()),
            executed_artifact_path: switch_execution
                .as_ref()
                .map(|record| record.applied_artifact_path.clone()),
            planned_install_path: install_action
                .as_ref()
                .map(|plan| plan.target_install_path.clone()),
            planned_backup_path: install_action.as_ref().map(|plan| plan.backup_path.clone()),
            executed_install_path: install_execution
                .as_ref()
                .map(|record| record.target_install_path.clone()),
            install_target_mode: install_execution
                .as_ref()
                .map(|record| record.target_mode.clone())
                .or_else(|| install_action.as_ref().map(|plan| plan.target_mode.clone())),
            install_action_status: if install_execution.is_some() {
                "install_action_executed".to_string()
            } else if install_action.is_some() {
                "install_action_planned".to_string()
            } else {
                "not_planned".to_string()
            },
            switch_execution_status: if switch_execution.is_some() {
                "artifact_switched".to_string()
            } else {
                "not_executed".to_string()
            },
            service_window_status,
        },
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingUpdatePlan {
    pub channel: String,
    pub target_version: String,
    pub manifest_path: String,
    pub artifact_count: usize,
    pub artifact_kind: String,
    pub artifact_path: String,
    pub staged_artifact_path: String,
    pub staged_at_millis: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RollbackMetadata {
    pub previous_version: String,
    pub channel: String,
    pub manifest_path: String,
    pub recorded_at_millis: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppliedUpdateRecord {
    pub channel: String,
    pub target_version: String,
    pub manifest_path: String,
    pub artifact_kind: String,
    pub artifact_path: String,
    pub applied_at_millis: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstalledVersionRecord {
    pub version: String,
    pub channel: String,
    pub recorded_at_millis: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SwitchPlanRecord {
    pub channel: String,
    pub target_version: String,
    pub artifact_kind: String,
    pub artifact_path: String,
    pub target_install_root: String,
    pub service_window_status: String,
    pub planned_at_millis: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SwitchExecutionRecord {
    pub channel: String,
    pub target_version: String,
    pub artifact_kind: String,
    pub source_artifact_path: String,
    pub applied_artifact_path: String,
    pub target_install_root: String,
    pub executed_at_millis: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstallActionPlanRecord {
    pub channel: String,
    pub target_version: String,
    pub artifact_kind: String,
    pub target_mode: String,
    pub staged_artifact_path: String,
    pub target_install_path: String,
    pub backup_path: String,
    pub strategy: String,
    pub planned_at_millis: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstallExecutionRecord {
    pub channel: String,
    pub target_version: String,
    pub target_mode: String,
    pub target_install_path: String,
    pub backup_path: String,
    pub staged_artifact_path: String,
    pub executed_at_millis: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateExecutionResult {
    pub action: String,
    pub channel: String,
    pub status: String,
    pub target_version: Option<String>,
    pub pending_update_path: PathBuf,
    pub rollback_path: PathBuf,
    pub switch_plan_path: PathBuf,
    pub switch_execution_path: PathBuf,
    pub install_action_path: PathBuf,
    pub install_execution_path: PathBuf,
    pub installed_version_path: PathBuf,
}

pub fn pending_update_plan_path(update_dir: impl AsRef<Path>) -> PathBuf {
    update_dir.as_ref().join("pending-update.json")
}

pub fn rollback_metadata_path(update_dir: impl AsRef<Path>) -> PathBuf {
    update_dir.as_ref().join("rollback.json")
}

pub fn applied_update_record_path(update_dir: impl AsRef<Path>) -> PathBuf {
    update_dir.as_ref().join("applied-update.json")
}

pub fn installed_version_record_path(update_dir: impl AsRef<Path>) -> PathBuf {
    update_dir.as_ref().join("installed-version.json")
}

pub fn switch_plan_path(update_dir: impl AsRef<Path>) -> PathBuf {
    update_dir.as_ref().join("switch-plan.json")
}

pub fn switch_execution_record_path(update_dir: impl AsRef<Path>) -> PathBuf {
    update_dir.as_ref().join("switch-executed.json")
}

pub fn install_action_plan_path(update_dir: impl AsRef<Path>) -> PathBuf {
    update_dir.as_ref().join("install-action.json")
}

pub fn install_execution_record_path(update_dir: impl AsRef<Path>) -> PathBuf {
    update_dir.as_ref().join("install-executed.json")
}

pub fn applied_artifact_dir(update_dir: impl AsRef<Path>) -> PathBuf {
    update_dir.as_ref().join("current")
}

pub fn staged_artifact_dir(update_dir: impl AsRef<Path>) -> PathBuf {
    update_dir.as_ref().join("staged")
}

pub fn load_pending_update_plan(
    path: impl AsRef<Path>,
) -> Result<PendingUpdatePlan, Box<dyn std::error::Error>> {
    let content = fs::read_to_string(path)?;
    Ok(serde_json::from_str(&content)?)
}

pub fn load_rollback_metadata(
    path: impl AsRef<Path>,
) -> Result<RollbackMetadata, Box<dyn std::error::Error>> {
    let content = fs::read_to_string(path)?;
    Ok(serde_json::from_str(&content)?)
}

pub fn load_applied_update_record(
    path: impl AsRef<Path>,
) -> Result<AppliedUpdateRecord, Box<dyn std::error::Error>> {
    let content = fs::read_to_string(path)?;
    Ok(serde_json::from_str(&content)?)
}

pub fn load_installed_version_record(
    path: impl AsRef<Path>,
) -> Result<InstalledVersionRecord, Box<dyn std::error::Error>> {
    let content = fs::read_to_string(path)?;
    Ok(serde_json::from_str(&content)?)
}

pub fn load_switch_plan(
    path: impl AsRef<Path>,
) -> Result<SwitchPlanRecord, Box<dyn std::error::Error>> {
    let content = fs::read_to_string(path)?;
    Ok(serde_json::from_str(&content)?)
}

pub fn load_switch_execution_record(
    path: impl AsRef<Path>,
) -> Result<SwitchExecutionRecord, Box<dyn std::error::Error>> {
    let content = fs::read_to_string(path)?;
    Ok(serde_json::from_str(&content)?)
}

pub fn load_install_action_plan(
    path: impl AsRef<Path>,
) -> Result<InstallActionPlanRecord, Box<dyn std::error::Error>> {
    let content = fs::read_to_string(path)?;
    Ok(serde_json::from_str(&content)?)
}

pub fn load_install_execution_record(
    path: impl AsRef<Path>,
) -> Result<InstallExecutionRecord, Box<dyn std::error::Error>> {
    let content = fs::read_to_string(path)?;
    Ok(serde_json::from_str(&content)?)
}

pub fn stage_update(
    current_version: &str,
    channel: &str,
    update_dir: impl AsRef<Path>,
    release_manifest_dir: impl AsRef<Path>,
) -> Result<UpdateExecutionResult, Box<dyn std::error::Error>> {
    let update_dir = update_dir.as_ref().to_path_buf();
    let effective_current_version = installed_version_record_path(&update_dir)
        .pipe(load_installed_version_record)
        .ok()
        .map(|record| record.version)
        .unwrap_or_else(|| current_version.to_string());
    let manifest_dir = release_manifest_dir.as_ref().to_path_buf();
    let manifest_path = release_manifest_path(&manifest_dir, channel);
    let manifest = load_release_manifest_for_channel(&manifest_dir, channel)?;
    let verification = verify_release_manifest_signature(&manifest)?;
    let pending_update_path = pending_update_plan_path(&update_dir);
    let rollback_path = rollback_metadata_path(&update_dir);
    let switch_plan_path = switch_plan_path(&update_dir);
    let switch_execution_path = switch_execution_record_path(&update_dir);
    let install_action_path = install_action_plan_path(&update_dir);
    let install_execution_path = install_execution_record_path(&update_dir);
    let installed_version_path = installed_version_record_path(&update_dir);

    if !verification.is_verified() {
        return Ok(UpdateExecutionResult {
            action: "stage".to_string(),
            channel: channel.to_string(),
            status: verification.status_label().to_string(),
            target_version: Some(manifest.version),
            pending_update_path,
            rollback_path,
            switch_plan_path,
            switch_execution_path,
            install_action_path,
            install_execution_path,
            installed_version_path,
        });
    }

    if !version_is_newer(&manifest.version, &effective_current_version) {
        return Ok(UpdateExecutionResult {
            action: "stage".to_string(),
            channel: channel.to_string(),
            status: "up_to_date".to_string(),
            target_version: Some(manifest.version),
            pending_update_path,
            rollback_path,
            switch_plan_path,
            switch_execution_path,
            install_action_path,
            install_execution_path,
            installed_version_path,
        });
    }

    fs::create_dir_all(&update_dir)?;
    let artifact = match select_release_artifact(&manifest) {
        Some(artifact) => artifact,
        None => {
            return Ok(UpdateExecutionResult {
                action: "stage".to_string(),
                channel: channel.to_string(),
                status: "no_compatible_artifact".to_string(),
                target_version: Some(manifest.version),
                pending_update_path,
                rollback_path,
                switch_plan_path,
                switch_execution_path,
                install_action_path,
                install_execution_path,
                installed_version_path,
            });
        }
    };
    let source_artifact_path =
        resolve_artifact_source_path(&manifest_path.to_string_lossy(), &artifact.path);
    if !source_artifact_path.exists() {
        return Ok(UpdateExecutionResult {
            action: "stage".to_string(),
            channel: channel.to_string(),
            status: "artifact_missing".to_string(),
            target_version: Some(manifest.version),
            pending_update_path,
            rollback_path,
            switch_plan_path,
            switch_execution_path,
            install_action_path,
            install_execution_path,
            installed_version_path,
        });
    }
    let staged_dir = staged_artifact_dir(&update_dir);
    fs::create_dir_all(&staged_dir)?;
    let staged_file_name = source_artifact_path
        .file_name()
        .map(|name| name.to_owned())
        .unwrap_or_else(|| std::ffi::OsString::from("artifact.bin"));
    let staged_artifact_path = staged_dir.join(staged_file_name);
    fs::copy(&source_artifact_path, &staged_artifact_path)?;
    let now = current_unix_millis()?;
    let pending = PendingUpdatePlan {
        channel: manifest.channel.clone(),
        target_version: manifest.version.clone(),
        manifest_path: manifest_path.to_string_lossy().into_owned(),
        artifact_count: manifest.artifacts.len(),
        artifact_kind: artifact.kind,
        artifact_path: artifact.path,
        staged_artifact_path: staged_artifact_path.to_string_lossy().into_owned(),
        staged_at_millis: now,
    };
    let rollback = RollbackMetadata {
        previous_version: current_version.to_string(),
        channel: channel.to_string(),
        manifest_path: manifest_path.to_string_lossy().into_owned(),
        recorded_at_millis: now,
    };
    fs::write(
        &pending_update_path,
        serde_json::to_string_pretty(&pending)?,
    )?;
    fs::write(&rollback_path, serde_json::to_string_pretty(&rollback)?)?;

    Ok(UpdateExecutionResult {
        action: "stage".to_string(),
        channel: pending.channel,
        status: "staged".to_string(),
        target_version: Some(pending.target_version),
        pending_update_path,
        rollback_path,
        switch_plan_path,
        switch_execution_path,
        install_action_path,
        install_execution_path,
        installed_version_path,
    })
}

pub fn apply_update(
    update_dir: impl AsRef<Path>,
) -> Result<UpdateExecutionResult, Box<dyn std::error::Error>> {
    apply_update_with_status(update_dir, None)
}

pub fn apply_update_with_status(
    update_dir: impl AsRef<Path>,
    managed_status: Option<ManagedServiceStatus>,
) -> Result<UpdateExecutionResult, Box<dyn std::error::Error>> {
    let update_dir = update_dir.as_ref().to_path_buf();
    let pending_update_path = pending_update_plan_path(&update_dir);
    let rollback_path = rollback_metadata_path(&update_dir);
    let applied_path = applied_update_record_path(&update_dir);
    let switch_plan_path = switch_plan_path(&update_dir);
    let switch_execution_path = switch_execution_record_path(&update_dir);
    let install_action_path = install_action_plan_path(&update_dir);
    let install_execution_path = install_execution_record_path(&update_dir);
    let installed_version_path = installed_version_record_path(&update_dir);

    let pending = match load_pending_update_plan(&pending_update_path) {
        Ok(pending) => pending,
        Err(_) => {
            return Ok(UpdateExecutionResult {
                action: "apply".to_string(),
                channel: "stable".to_string(),
                status: "no_pending_update".to_string(),
                target_version: None,
                pending_update_path,
                rollback_path,
                switch_plan_path,
                switch_execution_path,
                install_action_path,
                install_execution_path,
                installed_version_path,
            });
        }
    };

    let switch_plan = SwitchPlanRecord {
        channel: pending.channel.clone(),
        target_version: pending.target_version.clone(),
        artifact_kind: pending.artifact_kind.clone(),
        artifact_path: pending.artifact_path.clone(),
        target_install_root: install_root_for_current_platform().to_string(),
        service_window_status: service_window_status(&update_dir, managed_status),
        planned_at_millis: current_unix_millis()?,
    };
    fs::write(
        &switch_plan_path,
        serde_json::to_string_pretty(&switch_plan)?,
    )?;
    if switch_plan.service_window_status != "safe_now" {
        return Ok(UpdateExecutionResult {
            action: "apply".to_string(),
            channel: pending.channel,
            status: switch_plan.service_window_status,
            target_version: Some(pending.target_version),
            pending_update_path,
            rollback_path,
            switch_plan_path,
            switch_execution_path,
            install_action_path,
            install_execution_path,
            installed_version_path,
        });
    }

    let source_artifact_path = PathBuf::from(&pending.staged_artifact_path);
    if !source_artifact_path.exists() {
        return Ok(UpdateExecutionResult {
            action: "apply".to_string(),
            channel: pending.channel,
            status: "artifact_missing".to_string(),
            target_version: Some(pending.target_version),
            pending_update_path,
            rollback_path,
            switch_plan_path,
            switch_execution_path,
            install_action_path,
            install_execution_path,
            installed_version_path,
        });
    }

    let applied = AppliedUpdateRecord {
        channel: pending.channel.clone(),
        target_version: pending.target_version.clone(),
        manifest_path: pending.manifest_path,
        artifact_kind: pending.artifact_kind,
        artifact_path: pending.artifact_path,
        applied_at_millis: current_unix_millis()?,
    };
    let applied_artifact_dir = applied_artifact_dir(&update_dir);
    fs::create_dir_all(&applied_artifact_dir)?;
    let artifact_name = source_artifact_path
        .file_name()
        .map(|name| name.to_owned())
        .unwrap_or_else(|| std::ffi::OsString::from("artifact.bin"));
    let applied_artifact_path = applied_artifact_dir.join(artifact_name);
    fs::copy(&source_artifact_path, &applied_artifact_path)?;
    fs::write(&applied_path, serde_json::to_string_pretty(&applied)?)?;
    let switch_execution = SwitchExecutionRecord {
        channel: applied.channel.clone(),
        target_version: applied.target_version.clone(),
        artifact_kind: applied.artifact_kind.clone(),
        source_artifact_path: source_artifact_path.to_string_lossy().into_owned(),
        applied_artifact_path: applied_artifact_path.to_string_lossy().into_owned(),
        target_install_root: switch_plan.target_install_root.clone(),
        executed_at_millis: current_unix_millis()?,
    };
    fs::write(
        &switch_execution_path,
        serde_json::to_string_pretty(&switch_execution)?,
    )?;
    let install_action = build_install_action_plan(&switch_execution);
    fs::write(
        &install_action_path,
        serde_json::to_string_pretty(&install_action)?,
    )?;
    let installed = InstalledVersionRecord {
        version: applied.target_version.clone(),
        channel: applied.channel.clone(),
        recorded_at_millis: current_unix_millis()?,
    };
    fs::write(
        &installed_version_path,
        serde_json::to_string_pretty(&installed)?,
    )?;
    if pending_update_path.exists() {
        fs::remove_file(&pending_update_path)?;
    }

    Ok(UpdateExecutionResult {
        action: "apply".to_string(),
        channel: applied.channel,
        status: "applied".to_string(),
        target_version: Some(applied.target_version),
        pending_update_path,
        rollback_path,
        switch_plan_path,
        switch_execution_path,
        install_action_path,
        install_execution_path,
        installed_version_path,
    })
}

pub fn execute_install_action(
    update_dir: impl AsRef<Path>,
    install_root_override: Option<impl AsRef<Path>>,
    installed_layout_root_override: Option<impl AsRef<Path>>,
    use_installed_layout: bool,
    allow_system_install_write: bool,
) -> Result<UpdateExecutionResult, Box<dyn std::error::Error>> {
    let update_dir = update_dir.as_ref().to_path_buf();
    let pending_update_path = pending_update_plan_path(&update_dir);
    let rollback_path = rollback_metadata_path(&update_dir);
    let switch_plan_path = switch_plan_path(&update_dir);
    let switch_execution_path = switch_execution_record_path(&update_dir);
    let install_action_path = install_action_plan_path(&update_dir);
    let install_execution_path = install_execution_record_path(&update_dir);
    let installed_version_path = installed_version_record_path(&update_dir);

    let install_action = match load_install_action_plan(&install_action_path) {
        Ok(plan) => plan,
        Err(_) => {
            return Ok(UpdateExecutionResult {
                action: "commit-install".to_string(),
                channel: "stable".to_string(),
                status: "no_install_action".to_string(),
                target_version: None,
                pending_update_path,
                rollback_path,
                switch_plan_path,
                switch_execution_path,
                install_action_path,
                install_execution_path,
                installed_version_path,
            });
        }
    };

    let target_root = if let Some(override_root) =
        install_root_override.map(|path| path.as_ref().to_path_buf())
    {
        (override_root, "override_root".to_string())
    } else if use_installed_layout {
        if let Some(layout_root_override) =
            installed_layout_root_override.map(|path| path.as_ref().to_path_buf())
        {
            (
                resolve_install_layout(
                    current_platform(),
                    InstallMode::Installed,
                    layout_root_override,
                )
                .root_dir,
                "installed_layout_override".to_string(),
            )
        } else if !allow_system_install_write {
            return Ok(UpdateExecutionResult {
                action: "commit-install".to_string(),
                channel: install_action.channel.clone(),
                status: "installed_layout_requires_explicit_write".to_string(),
                target_version: Some(install_action.target_version.clone()),
                pending_update_path,
                rollback_path,
                switch_plan_path,
                switch_execution_path,
                install_action_path,
                install_execution_path,
                installed_version_path,
            });
        } else {
            (
                resolve_install_layout(
                    current_platform(),
                    InstallMode::Installed,
                    default_install_root_path(),
                )
                .root_dir,
                "installed_layout".to_string(),
            )
        }
    } else {
        return Ok(UpdateExecutionResult {
            action: "commit-install".to_string(),
            channel: install_action.channel.clone(),
            status: "install_root_required".to_string(),
            target_version: Some(install_action.target_version.clone()),
            pending_update_path,
            rollback_path,
            switch_plan_path,
            switch_execution_path,
            install_action_path,
            install_execution_path,
            installed_version_path,
        });
    };

    let target_install_path =
        override_target_path(&target_root.0, &install_action.target_install_path);
    let backup_path = PathBuf::from(format!("{}.bak", target_install_path.to_string_lossy()));
    if let Some(parent) = target_install_path.parent() {
        fs::create_dir_all(parent)?;
    }
    if target_install_path.exists() {
        fs::copy(&target_install_path, &backup_path)?;
    }
    fs::copy(&install_action.staged_artifact_path, &target_install_path)?;

    let execution = InstallExecutionRecord {
        channel: install_action.channel.clone(),
        target_version: install_action.target_version.clone(),
        target_mode: target_root.1,
        target_install_path: target_install_path.to_string_lossy().into_owned(),
        backup_path: backup_path.to_string_lossy().into_owned(),
        staged_artifact_path: install_action.staged_artifact_path.clone(),
        executed_at_millis: current_unix_millis()?,
    };
    fs::write(
        &install_execution_path,
        serde_json::to_string_pretty(&execution)?,
    )?;

    Ok(UpdateExecutionResult {
        action: "commit-install".to_string(),
        channel: install_action.channel,
        status: "install_executed".to_string(),
        target_version: Some(install_action.target_version),
        pending_update_path,
        rollback_path,
        switch_plan_path,
        switch_execution_path,
        install_action_path,
        install_execution_path,
        installed_version_path,
    })
}

pub fn rollback_update(
    update_dir: impl AsRef<Path>,
) -> Result<UpdateExecutionResult, Box<dyn std::error::Error>> {
    let update_dir = update_dir.as_ref().to_path_buf();
    let pending_update_path = pending_update_plan_path(&update_dir);
    let rollback_path = rollback_metadata_path(&update_dir);
    let applied_path = applied_update_record_path(&update_dir);
    let switch_plan_path = switch_plan_path(&update_dir);
    let switch_execution_path = switch_execution_record_path(&update_dir);
    let install_action_path = install_action_plan_path(&update_dir);
    let install_execution_path = install_execution_record_path(&update_dir);
    let installed_version_path = installed_version_record_path(&update_dir);

    let rollback = match load_rollback_metadata(&rollback_path) {
        Ok(rollback) => rollback,
        Err(_) => {
            return Ok(UpdateExecutionResult {
                action: "rollback".to_string(),
                channel: "stable".to_string(),
                status: "no_rollback_available".to_string(),
                target_version: None,
                pending_update_path,
                rollback_path,
                switch_plan_path,
                switch_execution_path,
                install_action_path,
                install_execution_path,
                installed_version_path,
            });
        }
    };

    if pending_update_path.exists() {
        fs::remove_file(&pending_update_path)?;
    }
    let staged_dir = staged_artifact_dir(&update_dir);
    if staged_dir.exists() {
        let _ = fs::remove_dir_all(&staged_dir);
    }
    if applied_path.exists() {
        fs::remove_file(&applied_path)?;
    }
    if rollback_path.exists() {
        fs::remove_file(&rollback_path)?;
    }
    if switch_plan_path.exists() {
        fs::remove_file(&switch_plan_path)?;
    }
    if switch_execution_path.exists() {
        if let Ok(record) = load_switch_execution_record(&switch_execution_path) {
            let applied_artifact_path = PathBuf::from(record.applied_artifact_path);
            if applied_artifact_path.exists() {
                let _ = fs::remove_file(&applied_artifact_path);
            }
        }
        fs::remove_file(&switch_execution_path)?;
    }
    if install_action_path.exists() {
        fs::remove_file(&install_action_path)?;
    }
    if install_execution_path.exists() {
        if let Ok(record) = load_install_execution_record(&install_execution_path) {
            let target_install_path = PathBuf::from(record.target_install_path);
            let backup_path = PathBuf::from(record.backup_path);
            if target_install_path.exists() {
                let _ = fs::remove_file(&target_install_path);
            }
            if backup_path.exists() {
                let _ = fs::copy(&backup_path, &target_install_path);
                let _ = fs::remove_file(&backup_path);
            }
        }
        fs::remove_file(&install_execution_path)?;
    }
    let installed = InstalledVersionRecord {
        version: rollback.previous_version.clone(),
        channel: rollback.channel.clone(),
        recorded_at_millis: current_unix_millis()?,
    };
    fs::write(
        &installed_version_path,
        serde_json::to_string_pretty(&installed)?,
    )?;

    Ok(UpdateExecutionResult {
        action: "rollback".to_string(),
        channel: rollback.channel,
        status: "rolled_back".to_string(),
        target_version: Some(rollback.previous_version),
        pending_update_path,
        rollback_path,
        switch_plan_path,
        switch_execution_path,
        install_action_path,
        install_execution_path,
        installed_version_path,
    })
}

fn current_unix_millis() -> Result<u64, Box<dyn std::error::Error>> {
    Ok(SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as u64)
}

fn select_release_artifact(manifest: &ReleaseManifest) -> Option<ReleaseArtifact> {
    let platform = current_platform_label();
    let preferred_kind = preferred_kind_for_current_platform();
    manifest
        .artifacts
        .iter()
        .find(|artifact| artifact.platform == platform && artifact.kind == preferred_kind)
        .cloned()
        .or_else(|| {
            manifest
                .artifacts
                .iter()
                .find(|artifact| artifact.platform == platform)
                .cloned()
        })
}

fn resolve_artifact_source_path(manifest_path: &str, artifact_path: &str) -> PathBuf {
    let artifact = PathBuf::from(artifact_path);
    if artifact.is_absolute() {
        artifact
    } else {
        Path::new(manifest_path)
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(artifact)
    }
}

fn build_install_action_plan(switch_execution: &SwitchExecutionRecord) -> InstallActionPlanRecord {
    let target_install_path = resolve_target_install_path(
        &switch_execution.target_install_root,
        &switch_execution.artifact_kind,
        &switch_execution.applied_artifact_path,
    );
    let backup_path = format!("{target_install_path}.bak");
    InstallActionPlanRecord {
        channel: switch_execution.channel.clone(),
        target_version: switch_execution.target_version.clone(),
        artifact_kind: switch_execution.artifact_kind.clone(),
        target_mode: "switch_execution".to_string(),
        staged_artifact_path: switch_execution.applied_artifact_path.clone(),
        target_install_path,
        backup_path,
        strategy: "copy_then_swap".to_string(),
        planned_at_millis: switch_execution.executed_at_millis,
    }
}

fn override_target_path(override_root: &Path, planned_target_path: &str) -> PathBuf {
    let file_name = Path::new(planned_target_path)
        .file_name()
        .map(|name| name.to_owned())
        .unwrap_or_else(|| std::ffi::OsString::from("artifact.bin"));
    override_root.join(file_name)
}

fn resolve_target_install_path(
    target_install_root: &str,
    artifact_kind: &str,
    applied_artifact_path: &str,
) -> String {
    let root = Path::new(target_install_root);
    match artifact_kind {
        "msi" => root.join("pole-node.exe").to_string_lossy().into_owned(),
        "deb" => root.join("pole-node").to_string_lossy().into_owned(),
        "zip" => {
            let file_name = Path::new(applied_artifact_path)
                .file_name()
                .map(|name| name.to_owned())
                .unwrap_or_else(|| std::ffi::OsString::from("artifact.bin"));
            root.join(file_name).to_string_lossy().into_owned()
        }
        _ => root.join("artifact.bin").to_string_lossy().into_owned(),
    }
}

fn current_platform_label() -> &'static str {
    if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    }
}

fn preferred_kind_for_current_platform() -> &'static str {
    if cfg!(windows) {
        "msi"
    } else if cfg!(target_os = "macos") {
        "zip"
    } else {
        "deb"
    }
}

fn install_root_for_current_platform() -> &'static str {
    if cfg!(windows) {
        "C:/Program Files/PoLE"
    } else if cfg!(target_os = "macos") {
        "/Applications/PoLE.app"
    } else {
        "/opt/pole"
    }
}

fn default_install_root_path() -> PathBuf {
    if let Ok(override_root) = std::env::var(INSTALLED_LAYOUT_ROOT_OVERRIDE_ENV) {
        if !override_root.trim().is_empty() {
            return PathBuf::from(override_root);
        }
    }
    PathBuf::from(install_root_for_current_platform())
}

fn service_window_status(
    update_dir: &Path,
    managed_status: Option<ManagedServiceStatus>,
) -> String {
    if let Some(status) = managed_status {
        match status {
            ManagedServiceStatus::Running { .. }
            | ManagedServiceStatus::Starting { .. }
            | ManagedServiceStatus::Failed { .. } => return "service_window_required".to_string(),
            ManagedServiceStatus::Stopped | ManagedServiceStatus::NotInstalled => {}
        }
    }
    let Some(data_dir) = update_dir.parent() else {
        return "safe_now".to_string();
    };
    let pid_path = data_dir.join("daemon.pid");
    let Some(pid) = fs::read_to_string(pid_path)
        .ok()
        .and_then(|content| content.lines().next().map(str::trim).map(str::to_string))
        .and_then(|content| content.parse::<u32>().ok())
    else {
        return "safe_now".to_string();
    };

    if process_is_running(pid) {
        "service_window_required".to_string()
    } else {
        "safe_now".to_string()
    }
}

trait Pipe: Sized {
    fn pipe<T>(self, f: impl FnOnce(Self) -> T) -> T {
        f(self)
    }
}

impl<T> Pipe for T {}

fn process_is_running(pid: u32) -> bool {
    #[cfg(windows)]
    {
        let script = format!(
            "$p = Get-Process -Id {pid} -ErrorAction SilentlyContinue; if ($p) {{ exit 0 }} else {{ exit 1 }}"
        );
        std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                &script,
            ])
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }

    #[cfg(not(windows))]
    {
        std::process::Command::new("sh")
            .args(["-c", &format!("kill -0 {pid} >/dev/null 2>&1")])
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }
}
