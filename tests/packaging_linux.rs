#[test]
fn deb_control_contains_required_package_metadata() {
    let control = include_str!("../packaging/linux/deb/control");

    assert!(control.contains("Package: pole-node"));
    assert!(control.contains("Version: 0.1.0"));
    assert!(control.contains("Architecture: amd64"));
    assert!(control.contains("Depends: systemd"));
    assert!(control.contains("Description: PoLE node service"));
}

#[test]
fn deb_postinst_sets_up_service_and_runtime_directories() {
    let postinst = include_str!("../packaging/linux/deb/postinst");

    assert!(postinst.contains("mkdir -p \"$CONFIG_DIR\" \"$DATA_DIR\" \"$LOG_DIR\""));
    assert!(postinst.contains("systemctl daemon-reload || true"));
    assert!(postinst.contains("systemctl enable \"$SERVICE_NAME\" || true"));
    assert!(postinst.contains("systemctl restart \"$SERVICE_NAME\" || true"));
}

#[test]
fn deb_prerm_stops_and_disables_service() {
    let prerm = include_str!("../packaging/linux/deb/prerm");

    assert!(prerm.contains("systemctl stop \"$SERVICE_NAME\" || true"));
    assert!(prerm.contains("systemctl disable \"$SERVICE_NAME\" || true"));
    assert!(prerm.contains("systemctl daemon-reload || true"));
}

#[test]
fn deb_scripts_reference_packaged_systemd_service_name() {
    let service = include_str!("../packaging/linux/deb/pole-node.service");
    let postinst = include_str!("../packaging/linux/deb/postinst");
    let prerm = include_str!("../packaging/linux/deb/prerm");

    assert!(service.contains("Description=PoLE node service"));
    assert!(postinst.contains("SERVICE_NAME=\"pole-node.service\""));
    assert!(prerm.contains("SERVICE_NAME=\"pole-node.service\""));
}

#[test]
fn deb_conffiles_registers_primary_node_config() {
    let conffiles = include_str!("../packaging/linux/deb/conffiles");

    assert!(conffiles.contains("/etc/pole/node.json"));
}

#[test]
fn deb_dirs_provisions_required_runtime_directories() {
    let dirs = include_str!("../packaging/linux/deb/dirs");

    assert!(dirs.contains("/etc/pole"));
    assert!(dirs.contains("/var/lib/pole"));
    assert!(dirs.contains("/var/log/pole"));
}

#[test]
fn linux_dashboard_launcher_targets_control_api_open() {
    let launcher = include_str!("../packaging/linux/deb/open-dashboard.sh");

    assert!(launcher.contains("/opt/pole/pole-client"));
    assert!(launcher.contains("control-api-open"));
    assert!(launcher.contains("/etc/pole/node.json"));
}
