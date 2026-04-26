use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use pole_protocol_draft::{
    applied_update_record_path, apply_update, collect_update_overview,
    collect_update_overview_with_status, development_manifest_signature, execute_install_action,
    install_action_plan_path, install_execution_record_path, installed_version_record_path,
    load_applied_update_record, load_install_action_plan, load_install_execution_record,
    load_installed_version_record, load_pending_update_plan, load_release_manifest,
    load_release_manifest_for_channel, load_rollback_metadata, load_switch_execution_record,
    release_manifest_path, rollback_metadata_path, rollback_update, stage_update,
    switch_execution_record_path, switch_plan_path, verify_release_manifest_signature,
    version_is_newer, AppliedUpdateRecord, InstallExecutionRecord, InstalledVersionRecord,
    ManagedServiceStatus, ManifestSignatureVerification, PendingUpdatePlan, ReleaseArtifact,
    ReleaseManifest,
};

fn temp_root(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("pole-update-flow-{name}-{}", std::process::id()))
}

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

#[test]
fn release_manifest_loader_reads_channel_and_artifacts() {
    let manifest = load_release_manifest("dist/release-manifests/stable.json").unwrap();
    assert_eq!(manifest.channel, "stable");
    assert_eq!(manifest.version, "0.1.0");
    assert_eq!(manifest.artifacts.len(), 2);
    assert_eq!(manifest.artifacts[0].platform, "windows");
}

#[test]
fn release_manifest_channel_loader_uses_channel_path() {
    let path = release_manifest_path("dist/release-manifests", "stable");
    assert!(path.to_string_lossy().ends_with("stable.json"));
    let manifest = load_release_manifest_for_channel("dist/release-manifests", "stable").unwrap();
    assert_eq!(manifest.channel, "stable");
}

#[test]
fn version_comparison_detects_newer_semver() {
    assert!(version_is_newer("1.2.0", "1.1.9"));
    assert!(version_is_newer("0.2.0", "0.1.9"));
    assert!(!version_is_newer("0.1.0", "0.1.0"));
    assert!(!version_is_newer("0.0.9", "0.1.0"));
}

#[test]
fn development_manifest_signature_verifies_against_payload() {
    let mut manifest = ReleaseManifest {
        channel: "stable".into(),
        version: "0.2.0".into(),
        artifacts: vec![ReleaseArtifact {
            platform: "windows".into(),
            kind: "msi".into(),
            path: "PoLE-0.2.0-x64.msi".into(),
            sha256: "abc".into(),
            size_bytes: 42,
        }],
        signature: String::new(),
    };

    manifest.signature = development_manifest_signature(&manifest).unwrap();
    let verification = verify_release_manifest_signature(&manifest).unwrap();
    assert_eq!(verification, ManifestSignatureVerification::Verified);
}

#[test]
fn update_overview_reads_manifest_and_signature_status() {
    let root = temp_root("overview");
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    let manifest_dir = root.join("release-manifests");
    let update_dir = root.join("updates");
    std::fs::create_dir_all(&manifest_dir).unwrap();
    std::fs::create_dir_all(&update_dir).unwrap();
    let mut manifest = ReleaseManifest {
        channel: "stable".into(),
        version: "0.2.0".into(),
        artifacts: vec![ReleaseArtifact {
            platform: "windows".into(),
            kind: "msi".into(),
            path: "PoLE-0.2.0-x64.msi".into(),
            sha256: "abc".into(),
            size_bytes: 42,
        }],
        signature: String::new(),
    };
    manifest.signature = development_manifest_signature(&manifest).unwrap();
    std::fs::write(
        manifest_dir.join("stable.json"),
        serde_json::to_string_pretty(&manifest).unwrap(),
    )
    .unwrap();

    let overview = collect_update_overview("0.1.0", "stable", &update_dir, &manifest_dir);
    assert!(overview.update_available);
    assert_eq!(overview.latest_available_version.as_deref(), Some("0.2.0"));
    assert_eq!(overview.signing_status, "manifest_signed");
    assert_eq!(overview.artifact_count, 1);
    assert_eq!(overview.pending_target_version, None);
    assert!(overview.latest_manifest_path.ends_with("stable.json"));

    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn stage_update_writes_pending_and_rollback_metadata() {
    let root = temp_root("stage");
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    let manifest_dir = root.join("release-manifests");
    let update_dir = root.join("updates");
    std::fs::create_dir_all(&manifest_dir).unwrap();
    std::fs::create_dir_all(&update_dir).unwrap();
    let mut manifest = ReleaseManifest {
        channel: "stable".into(),
        version: "0.2.0".into(),
        artifacts: vec![ReleaseArtifact {
            platform: "windows".into(),
            kind: "msi".into(),
            path: "PoLE-0.2.0-x64.msi".into(),
            sha256: "abc".into(),
            size_bytes: 42,
        }],
        signature: String::new(),
    };
    manifest.signature = development_manifest_signature(&manifest).unwrap();
    std::fs::write(
        manifest_dir.join("stable.json"),
        serde_json::to_string_pretty(&manifest).unwrap(),
    )
    .unwrap();
    std::fs::write(manifest_dir.join("PoLE-0.2.0-x64.msi"), b"artifact-bytes").unwrap();

    let result = stage_update("0.1.0", "stable", &update_dir, &manifest_dir).unwrap();
    assert_eq!(result.status, "staged");
    assert_eq!(result.target_version.as_deref(), Some("0.2.0"));

    let pending = load_pending_update_plan(&result.pending_update_path).unwrap();
    assert_eq!(
        pending,
        PendingUpdatePlan {
            channel: "stable".into(),
            target_version: "0.2.0".into(),
            manifest_path: manifest_dir
                .join("stable.json")
                .to_string_lossy()
                .into_owned(),
            artifact_count: 1,
            artifact_kind: "msi".into(),
            artifact_path: "PoLE-0.2.0-x64.msi".into(),
            staged_artifact_path: update_dir
                .join("staged")
                .join("PoLE-0.2.0-x64.msi")
                .to_string_lossy()
                .into_owned(),
            staged_at_millis: pending.staged_at_millis,
        }
    );
    let rollback = load_rollback_metadata(rollback_metadata_path(&update_dir)).unwrap();
    assert_eq!(rollback.previous_version, "0.1.0");
    assert_eq!(rollback.channel, "stable");

    let overview = collect_update_overview("0.1.0", "stable", &update_dir, &manifest_dir);
    assert_eq!(overview.pending_target_version.as_deref(), Some("0.2.0"));
    assert_eq!(overview.selected_artifact_kind.as_deref(), Some("msi"));
    assert_eq!(
        overview.selected_artifact_path.as_deref(),
        Some("PoLE-0.2.0-x64.msi")
    );
    assert_eq!(overview.rollback_status, "available");

    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn apply_update_promotes_pending_plan_to_applied_record() {
    let root = temp_root("apply");
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    let manifest_dir = root.join("release-manifests");
    let update_dir = root.join("updates");
    std::fs::create_dir_all(&manifest_dir).unwrap();
    std::fs::create_dir_all(&update_dir).unwrap();
    let mut manifest = ReleaseManifest {
        channel: "stable".into(),
        version: "0.2.0".into(),
        artifacts: vec![ReleaseArtifact {
            platform: "windows".into(),
            kind: "msi".into(),
            path: "PoLE-0.2.0-x64.msi".into(),
            sha256: "abc".into(),
            size_bytes: 42,
        }],
        signature: String::new(),
    };
    manifest.signature = development_manifest_signature(&manifest).unwrap();
    std::fs::write(
        manifest_dir.join("stable.json"),
        serde_json::to_string_pretty(&manifest).unwrap(),
    )
    .unwrap();
    std::fs::write(manifest_dir.join("PoLE-0.2.0-x64.msi"), b"artifact-bytes").unwrap();

    stage_update("0.1.0", "stable", &update_dir, &manifest_dir).unwrap();
    let result = apply_update(&update_dir).unwrap();
    assert_eq!(result.status, "applied");
    assert_eq!(result.target_version.as_deref(), Some("0.2.0"));
    assert!(!result.pending_update_path.exists());
    assert!(result.switch_plan_path.exists());

    let applied = load_applied_update_record(applied_update_record_path(&update_dir)).unwrap();
    assert_eq!(
        applied,
        AppliedUpdateRecord {
            channel: "stable".into(),
            target_version: "0.2.0".into(),
            manifest_path: manifest_dir
                .join("stable.json")
                .to_string_lossy()
                .into_owned(),
            artifact_kind: if cfg!(windows) {
                "msi".into()
            } else {
                "deb".into()
            },
            artifact_path: "PoLE-0.2.0-x64.msi".into(),
            applied_at_millis: applied.applied_at_millis,
        }
    );
    let installed =
        load_installed_version_record(installed_version_record_path(&update_dir)).unwrap();
    assert_eq!(
        installed,
        InstalledVersionRecord {
            version: "0.2.0".into(),
            channel: "stable".into(),
            recorded_at_millis: installed.recorded_at_millis,
        }
    );
    let switch_execution =
        load_switch_execution_record(switch_execution_record_path(&update_dir)).unwrap();
    assert_eq!(
        switch_execution.artifact_kind,
        if cfg!(windows) { "msi" } else { "deb" }
    );
    assert!(PathBuf::from(&switch_execution.source_artifact_path).exists());
    assert!(PathBuf::from(&switch_execution.applied_artifact_path).exists());
    let install_action = load_install_action_plan(install_action_plan_path(&update_dir)).unwrap();
    assert_eq!(install_action.strategy, "copy_then_swap");
    assert!(install_action.target_install_path.contains("pole-node"));
    assert!(install_action.backup_path.ends_with(".bak"));
    let overview = collect_update_overview("0.1.0", "stable", &update_dir, &manifest_dir);
    assert_eq!(overview.current_version, "0.2.0");
    assert_eq!(overview.pending_target_version, None);
    assert_eq!(overview.applied_target_version.as_deref(), Some("0.2.0"));
    assert_eq!(
        overview.selected_artifact_kind.as_deref(),
        Some(if cfg!(windows) { "msi" } else { "deb" })
    );
    assert!(overview.executed_artifact_path.is_some());
    assert!(overview.planned_install_path.is_some());
    assert!(overview.planned_backup_path.is_some());
    assert_eq!(overview.executed_install_path, None);
    assert_eq!(
        overview.install_target_mode.as_deref(),
        Some("switch_execution")
    );
    assert_eq!(overview.install_action_status, "install_action_planned");
    assert_eq!(overview.switch_execution_status, "artifact_switched");
    assert_eq!(overview.service_window_status, "safe_now");

    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn apply_update_requires_service_window_when_daemon_pid_is_running() {
    let root = temp_root("service-window");
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    let manifest_dir = root.join("release-manifests");
    let data_dir = root.join("pole-node-data");
    let update_dir = data_dir.join("updates");
    std::fs::create_dir_all(&manifest_dir).unwrap();
    std::fs::create_dir_all(&update_dir).unwrap();
    let mut manifest = ReleaseManifest {
        channel: "stable".into(),
        version: "0.2.0".into(),
        artifacts: vec![ReleaseArtifact {
            platform: if cfg!(windows) {
                "windows".into()
            } else {
                "linux".into()
            },
            kind: if cfg!(windows) {
                "msi".into()
            } else {
                "deb".into()
            },
            path: "artifact.bin".into(),
            sha256: "abc".into(),
            size_bytes: 42,
        }],
        signature: String::new(),
    };
    manifest.signature = development_manifest_signature(&manifest).unwrap();
    std::fs::write(
        manifest_dir.join("stable.json"),
        serde_json::to_string_pretty(&manifest).unwrap(),
    )
    .unwrap();
    std::fs::write(manifest_dir.join("artifact.bin"), b"artifact-bytes").unwrap();
    std::fs::write(data_dir.join("daemon.pid"), std::process::id().to_string()).unwrap();

    stage_update("0.1.0", "stable", &update_dir, &manifest_dir).unwrap();
    let result = apply_update(&update_dir).unwrap();
    assert_eq!(result.status, "service_window_required");
    assert!(switch_plan_path(&update_dir).exists());

    let overview = collect_update_overview("0.1.0", "stable", &update_dir, &manifest_dir);
    assert_eq!(overview.service_window_status, "service_window_required");
    assert_eq!(overview.pending_target_version.as_deref(), Some("0.2.0"));
    assert_eq!(overview.install_action_status, "not_planned");
    assert_eq!(overview.switch_execution_status, "not_executed");

    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn update_overview_uses_managed_service_status_for_window_check() {
    let root = temp_root("managed-status");
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    let manifest_dir = root.join("release-manifests");
    let update_dir = root.join("updates");
    std::fs::create_dir_all(&manifest_dir).unwrap();
    std::fs::create_dir_all(&update_dir).unwrap();
    let mut manifest = ReleaseManifest {
        channel: "stable".into(),
        version: "0.2.0".into(),
        artifacts: vec![ReleaseArtifact {
            platform: if cfg!(windows) {
                "windows".into()
            } else {
                "linux".into()
            },
            kind: if cfg!(windows) {
                "msi".into()
            } else {
                "deb".into()
            },
            path: "artifact.bin".into(),
            sha256: "abc".into(),
            size_bytes: 42,
        }],
        signature: String::new(),
    };
    manifest.signature = development_manifest_signature(&manifest).unwrap();
    std::fs::write(
        manifest_dir.join("stable.json"),
        serde_json::to_string_pretty(&manifest).unwrap(),
    )
    .unwrap();

    let overview = collect_update_overview_with_status(
        "0.1.0",
        "stable",
        &update_dir,
        &manifest_dir,
        Some(ManagedServiceStatus::Running { pid: None }),
    );
    assert_eq!(overview.service_window_status, "service_window_required");
    assert!(overview.update_available);

    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn execute_install_action_copies_into_override_root() {
    let root = temp_root("commit-install");
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    let manifest_dir = root.join("release-manifests");
    let update_dir = root.join("updates");
    let install_root = root.join("install-root");
    std::fs::create_dir_all(&manifest_dir).unwrap();
    std::fs::create_dir_all(&update_dir).unwrap();
    std::fs::create_dir_all(&install_root).unwrap();
    let mut manifest = ReleaseManifest {
        channel: "stable".into(),
        version: "0.2.0".into(),
        artifacts: vec![ReleaseArtifact {
            platform: "windows".into(),
            kind: "msi".into(),
            path: "PoLE-0.2.0-x64.msi".into(),
            sha256: "abc".into(),
            size_bytes: 42,
        }],
        signature: String::new(),
    };
    manifest.signature = development_manifest_signature(&manifest).unwrap();
    std::fs::write(
        manifest_dir.join("stable.json"),
        serde_json::to_string_pretty(&manifest).unwrap(),
    )
    .unwrap();
    std::fs::write(manifest_dir.join("PoLE-0.2.0-x64.msi"), b"artifact-bytes").unwrap();

    stage_update("0.1.0", "stable", &update_dir, &manifest_dir).unwrap();
    apply_update(&update_dir).unwrap();
    let result = execute_install_action(
        &update_dir,
        Some(&install_root),
        None::<&std::path::Path>,
        false,
        false,
    )
    .unwrap();
    assert_eq!(result.status, "install_executed");

    let execution =
        load_install_execution_record(install_execution_record_path(&update_dir)).unwrap();
    assert_eq!(
        execution,
        InstallExecutionRecord {
            channel: "stable".into(),
            target_version: "0.2.0".into(),
            target_mode: "override_root".into(),
            target_install_path: install_root
                .join("pole-node.exe")
                .to_string_lossy()
                .into_owned(),
            backup_path: format!(
                "{}.bak",
                install_root.join("pole-node.exe").to_string_lossy()
            ),
            staged_artifact_path: update_dir
                .join("current")
                .join("PoLE-0.2.0-x64.msi")
                .to_string_lossy()
                .into_owned(),
            executed_at_millis: execution.executed_at_millis,
        }
    );
    assert!(install_root.join("pole-node.exe").exists());

    let overview = collect_update_overview("0.1.0", "stable", &update_dir, &manifest_dir);
    assert_eq!(
        overview.executed_install_path.as_deref(),
        Some(
            install_root
                .join("pole-node.exe")
                .to_string_lossy()
                .as_ref()
        )
    );
    assert_eq!(
        overview.install_target_mode.as_deref(),
        Some("override_root")
    );
    assert_eq!(overview.install_action_status, "install_action_executed");

    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn execute_install_action_supports_installed_layout_strategy() {
    let root = temp_root("commit-installed-layout");
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    let manifest_dir = root.join("release-manifests");
    let update_dir = root.join("updates");
    std::fs::create_dir_all(&manifest_dir).unwrap();
    std::fs::create_dir_all(&update_dir).unwrap();
    let mut manifest = ReleaseManifest {
        channel: "stable".into(),
        version: "0.2.0".into(),
        artifacts: vec![ReleaseArtifact {
            platform: "windows".into(),
            kind: "msi".into(),
            path: "PoLE-0.2.0-x64.msi".into(),
            sha256: "abc".into(),
            size_bytes: 42,
        }],
        signature: String::new(),
    };
    manifest.signature = development_manifest_signature(&manifest).unwrap();
    std::fs::write(
        manifest_dir.join("stable.json"),
        serde_json::to_string_pretty(&manifest).unwrap(),
    )
    .unwrap();
    std::fs::write(manifest_dir.join("PoLE-0.2.0-x64.msi"), b"artifact-bytes").unwrap();

    stage_update("0.1.0", "stable", &update_dir, &manifest_dir).unwrap();
    apply_update(&update_dir).unwrap();
    let result = execute_install_action(
        &update_dir,
        None::<&std::path::Path>,
        None::<&std::path::Path>,
        true,
        false,
    )
    .unwrap();
    assert_eq!(result.status, "installed_layout_requires_explicit_write");
    assert_eq!(result.target_version.as_deref(), Some("0.2.0"));
}

#[test]
fn execute_install_action_supports_installed_layout_root_override() {
    let root = temp_root("commit-installed-layout-override");
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    let manifest_dir = root.join("release-manifests");
    let update_dir = root.join("updates");
    let installed_root = root.join("installed-root");
    std::fs::create_dir_all(&manifest_dir).unwrap();
    std::fs::create_dir_all(&update_dir).unwrap();
    std::fs::create_dir_all(&installed_root).unwrap();
    let mut manifest = ReleaseManifest {
        channel: "stable".into(),
        version: "0.2.0".into(),
        artifacts: vec![ReleaseArtifact {
            platform: "windows".into(),
            kind: "msi".into(),
            path: "PoLE-0.2.0-x64.msi".into(),
            sha256: "abc".into(),
            size_bytes: 42,
        }],
        signature: String::new(),
    };
    manifest.signature = development_manifest_signature(&manifest).unwrap();
    std::fs::write(
        manifest_dir.join("stable.json"),
        serde_json::to_string_pretty(&manifest).unwrap(),
    )
    .unwrap();
    std::fs::write(manifest_dir.join("PoLE-0.2.0-x64.msi"), b"artifact-bytes").unwrap();

    stage_update("0.1.0", "stable", &update_dir, &manifest_dir).unwrap();
    apply_update(&update_dir).unwrap();
    let result = execute_install_action(
        &update_dir,
        None::<&std::path::Path>,
        Some(&installed_root),
        true,
        false,
    )
    .unwrap();
    assert_eq!(result.status, "install_executed");
    assert!(installed_root.join("pole-node.exe").exists());

    let overview = collect_update_overview("0.1.0", "stable", &update_dir, &manifest_dir);
    assert_eq!(
        overview.executed_install_path.as_deref(),
        Some(
            installed_root
                .join("pole-node.exe")
                .to_string_lossy()
                .as_ref()
        )
    );
    assert_eq!(
        overview.install_target_mode.as_deref(),
        Some("installed_layout_override")
    );
    assert_eq!(overview.install_action_status, "install_action_executed");
}

#[test]
fn execute_install_action_supports_system_install_write_with_env_override() {
    let _guard = env_lock().lock().unwrap();

    let root = temp_root("commit-installed-layout-env");
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    let manifest_dir = root.join("release-manifests");
    let update_dir = root.join("updates");
    let installed_root = root.join("installed-root-env");
    std::fs::create_dir_all(&manifest_dir).unwrap();
    std::fs::create_dir_all(&update_dir).unwrap();
    std::fs::create_dir_all(&installed_root).unwrap();
    let mut manifest = ReleaseManifest {
        channel: "stable".into(),
        version: "0.2.0".into(),
        artifacts: vec![ReleaseArtifact {
            platform: "windows".into(),
            kind: "msi".into(),
            path: "PoLE-0.2.0-x64.msi".into(),
            sha256: "abc".into(),
            size_bytes: 42,
        }],
        signature: String::new(),
    };
    manifest.signature = development_manifest_signature(&manifest).unwrap();
    std::fs::write(
        manifest_dir.join("stable.json"),
        serde_json::to_string_pretty(&manifest).unwrap(),
    )
    .unwrap();
    std::fs::write(manifest_dir.join("PoLE-0.2.0-x64.msi"), b"artifact-bytes").unwrap();

    std::env::set_var("POLE_INSTALLED_LAYOUT_ROOT_OVERRIDE", &installed_root);
    stage_update("0.1.0", "stable", &update_dir, &manifest_dir).unwrap();
    apply_update(&update_dir).unwrap();
    let result = execute_install_action(
        &update_dir,
        None::<&std::path::Path>,
        None::<&std::path::Path>,
        true,
        true,
    )
    .unwrap();
    std::env::remove_var("POLE_INSTALLED_LAYOUT_ROOT_OVERRIDE");

    assert_eq!(result.status, "install_executed");
    assert!(installed_root.join("pole-node.exe").exists());

    let overview = collect_update_overview("0.1.0", "stable", &update_dir, &manifest_dir);
    assert_eq!(
        overview.install_target_mode.as_deref(),
        Some("installed_layout")
    );
    assert_eq!(
        overview.executed_install_path.as_deref(),
        Some(
            installed_root
                .join("pole-node.exe")
                .to_string_lossy()
                .as_ref()
        )
    );

    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn rollback_update_restores_committed_install_target_from_backup() {
    let root = temp_root("rollback-install-target");
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    let manifest_dir = root.join("release-manifests");
    let update_dir = root.join("updates");
    let install_root = root.join("install-root");
    std::fs::create_dir_all(&manifest_dir).unwrap();
    std::fs::create_dir_all(&update_dir).unwrap();
    std::fs::create_dir_all(&install_root).unwrap();
    let target_path = install_root.join("pole-node.exe");
    std::fs::write(&target_path, b"old-version").unwrap();

    let mut manifest = ReleaseManifest {
        channel: "stable".into(),
        version: "0.2.0".into(),
        artifacts: vec![ReleaseArtifact {
            platform: "windows".into(),
            kind: "msi".into(),
            path: "PoLE-0.2.0-x64.msi".into(),
            sha256: "abc".into(),
            size_bytes: 42,
        }],
        signature: String::new(),
    };
    manifest.signature = development_manifest_signature(&manifest).unwrap();
    std::fs::write(
        manifest_dir.join("stable.json"),
        serde_json::to_string_pretty(&manifest).unwrap(),
    )
    .unwrap();
    std::fs::write(manifest_dir.join("PoLE-0.2.0-x64.msi"), b"artifact-bytes").unwrap();

    stage_update("0.1.0", "stable", &update_dir, &manifest_dir).unwrap();
    apply_update(&update_dir).unwrap();
    execute_install_action(
        &update_dir,
        Some(&install_root),
        None::<&std::path::Path>,
        false,
        false,
    )
    .unwrap();

    assert_eq!(std::fs::read(&target_path).unwrap(), b"artifact-bytes");
    assert!(install_root.join("pole-node.exe.bak").exists());

    rollback_update(&update_dir).unwrap();

    assert_eq!(std::fs::read(&target_path).unwrap(), b"old-version");
    assert!(!install_root.join("pole-node.exe.bak").exists());

    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn rollback_update_clears_applied_and_rollback_state() {
    let root = temp_root("rollback");
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    let manifest_dir = root.join("release-manifests");
    let update_dir = root.join("updates");
    std::fs::create_dir_all(&manifest_dir).unwrap();
    std::fs::create_dir_all(&update_dir).unwrap();
    let mut manifest = ReleaseManifest {
        channel: "stable".into(),
        version: "0.2.0".into(),
        artifacts: vec![ReleaseArtifact {
            platform: "windows".into(),
            kind: "msi".into(),
            path: "PoLE-0.2.0-x64.msi".into(),
            sha256: "abc".into(),
            size_bytes: 42,
        }],
        signature: String::new(),
    };
    manifest.signature = development_manifest_signature(&manifest).unwrap();
    std::fs::write(
        manifest_dir.join("stable.json"),
        serde_json::to_string_pretty(&manifest).unwrap(),
    )
    .unwrap();
    std::fs::write(manifest_dir.join("PoLE-0.2.0-x64.msi"), b"artifact-bytes").unwrap();

    stage_update("0.1.0", "stable", &update_dir, &manifest_dir).unwrap();
    apply_update(&update_dir).unwrap();
    let result = rollback_update(&update_dir).unwrap();
    assert_eq!(result.status, "rolled_back");
    assert_eq!(result.target_version.as_deref(), Some("0.1.0"));
    assert!(!rollback_metadata_path(&update_dir).exists());
    assert!(!applied_update_record_path(&update_dir).exists());
    assert!(!switch_plan_path(&update_dir).exists());
    assert!(!switch_execution_record_path(&update_dir).exists());
    assert!(!install_action_plan_path(&update_dir).exists());
    let installed =
        load_installed_version_record(installed_version_record_path(&update_dir)).unwrap();
    assert_eq!(installed.version, "0.1.0");
    assert_eq!(installed.channel, "stable");

    let overview = collect_update_overview("0.1.0", "stable", &update_dir, &manifest_dir);
    assert_eq!(overview.current_version, "0.1.0");
    assert_eq!(overview.pending_target_version, None);
    assert_eq!(overview.applied_target_version, None);
    assert_eq!(overview.executed_artifact_path, None);
    assert_eq!(overview.planned_install_path, None);
    assert_eq!(overview.planned_backup_path, None);
    assert_eq!(overview.executed_install_path, None);
    assert_eq!(overview.install_target_mode, None);
    assert_eq!(overview.install_action_status, "not_planned");
    assert_eq!(overview.switch_execution_status, "not_executed");
    assert_eq!(overview.rollback_status, "planned");

    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn update_overview_rejects_invalid_manifest_signature() {
    let root = temp_root("invalid");
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    let manifest_dir = root.join("release-manifests");
    let update_dir = root.join("updates");
    std::fs::create_dir_all(&manifest_dir).unwrap();
    std::fs::create_dir_all(&update_dir).unwrap();
    std::fs::write(
        manifest_dir.join("stable.json"),
        r#"{
  "channel": "stable",
  "version": "0.2.0",
  "artifacts": [
    {"platform":"windows","kind":"msi","path":"PoLE-0.2.0-x64.msi","sha256":"abc","size_bytes":42}
  ],
  "signature": "tampered-signature"
}"#,
    )
    .unwrap();

    let overview = collect_update_overview("0.1.0", "stable", &update_dir, &manifest_dir);
    assert!(!overview.update_available);
    assert_eq!(overview.signing_status, "invalid_signature");
    assert_eq!(overview.latest_available_version.as_deref(), Some("0.2.0"));

    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn update_overview_reports_missing_manifest() {
    let root = temp_root("missing");
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    let manifest_dir = root.join("release-manifests");
    let update_dir = root.join("updates");
    std::fs::create_dir_all(&manifest_dir).unwrap();
    std::fs::create_dir_all(&update_dir).unwrap();

    let overview = collect_update_overview("0.1.0", "stable", &update_dir, &manifest_dir);
    assert!(!overview.update_available);
    assert_eq!(overview.latest_available_version, None);
    assert_eq!(overview.signing_status, "manifest_missing");
    assert_eq!(overview.artifact_count, 0);

    std::fs::remove_dir_all(&root).unwrap();
}
