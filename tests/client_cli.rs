use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

use pole_protocol_draft::{
    build_inmemory_simulation_network, epoch_preparation_artifact_path,
    epoch_settlement_artifact_path, open_local_protocol_state, retention_book_path,
    run_collect_tick_with_client, run_collect_tick_with_client_and_network, serve_control_api,
    AccountState, CapabilityConfig, CollectConfig, FilesystemP2pNetwork, HttpTextClient,
    LocalNodeProgress, NodeConfig, P2pNetwork, P2pTopic, RewardConfig, RuntimeConfig,
    SocketP2pNetwork, SteamCollectorError, StorageConfig,
};

fn temp_root(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("pole-client-{name}-{}", std::process::id()))
}

fn wait_for_file(path: &Path) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if path.exists() {
            return;
        }
        thread::sleep(Duration::from_millis(50));
    }
    panic!("timed out waiting for file {}", path.to_string_lossy());
}

fn free_local_addr() -> SocketAddr {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);
    addr
}

#[cfg(windows)]
fn terminate_process(pid: u32) {
    let output = Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            &format!(
                "$p = Get-Process -Id {pid} -ErrorAction SilentlyContinue; if ($p) {{ Stop-Process -Id {pid} -Force -ErrorAction Stop }}; exit 0"
            ),
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[cfg(not(windows))]
fn terminate_process(pid: u32) {
    let output = Command::new("sh")
        .args(["-c", &format!("kill -9 {pid}")])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

struct FixedHttpClient;

impl HttpTextClient for FixedHttpClient {
    fn get_text(&self, url: &str) -> Result<String, SteamCollectorError> {
        let app_id = url.split("appid=").nth(1).unwrap_or("0");
        let player_count = match app_id {
            "730" => 500_000,
            _ => 1_000,
        };
        Ok(format!(
            "{{\"response\":{{\"player_count\":{player_count},\"result\":1}}}}"
        ))
    }
}

#[test]
fn init_creates_player_profile_workspace() {
    let root = temp_root("init");
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    std::fs::create_dir_all(&root).unwrap();

    let config_path = root.join("client.json");
    let binary = env!("CARGO_BIN_EXE_pole-client");
    let output = Command::new(binary)
        .arg("init")
        .arg(&config_path)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let config: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&config_path).unwrap()).unwrap();
    assert_eq!(config["capabilities"]["collect"], true);
    assert_eq!(config["capabilities"]["store"], true);
    assert_eq!(config["capabilities"]["verify"], false);
    assert_eq!(config["capabilities"]["propose"], false);
    assert_eq!(config["reward"]["reward_source"], "tokenomics");
    assert_eq!(config["reward"]["emission_year"], 1);
    assert_eq!(config["runtime"]["low_impact_mode"], true);
    assert_eq!(config["runtime"]["os_background_priority"], true);
    assert_eq!(config["runtime"]["game_active_poll_interval_secs"], 900);
    assert_eq!(
        config["runtime"]["game_process_names"],
        serde_json::json!([])
    );
    assert_eq!(
        config["runtime"]["activity_sources"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        config["runtime"]["activity_sources"][0]["source_kind"],
        "Steam"
    );
    assert!(root.join("pole-node-data").exists());

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn player_start_bootstraps_player_mode_and_captures_foreground_game() {
    let root = temp_root("player-start");
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    std::fs::create_dir_all(&root).unwrap();

    let config_path = root.join("client.json");
    let app_data = root.join("appdata");
    let startup_dir = app_data
        .join("Microsoft")
        .join("Windows")
        .join("Start Menu")
        .join("Programs")
        .join("Startup");
    let binary = env!("CARGO_BIN_EXE_pole-client");
    let output = Command::new(binary)
        .env("APPDATA", &app_data)
        .env(
            "POLE_CLIENT_FOREGROUND_PROCESS_OVERRIDE",
            "BlackMythWukong.exe",
        )
        .env("POLE_CLIENT_SKIP_BACKGROUND_START", "1")
        .arg("player-start")
        .arg(&config_path)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("PoLE player mode started"));
    assert!(stdout.contains("captured_game_process=Some(\"BlackMythWukong.exe\")"));
    assert!(stdout.contains("autostart_enabled=true"));
    assert!(stdout.contains("background_mode=watch"));
    assert!(stdout.contains("background_start_skipped=true"));

    let config: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&config_path).unwrap()).unwrap();
    assert_eq!(config["capabilities"]["collect"], true);
    assert_eq!(config["capabilities"]["store"], true);
    assert_eq!(config["capabilities"]["verify"], false);
    assert_eq!(config["capabilities"]["propose"], false);
    assert_eq!(config["reward"]["reward_source"], "tokenomics");
    assert_eq!(config["reward"]["emission_year"], 1);
    assert_eq!(
        config["runtime"]["game_process_names"][0],
        "BlackMythWukong.exe"
    );
    assert_eq!(
        config["reward"]["game_mappings"][0]["process_name"],
        "BlackMythWukong.exe"
    );
    assert_eq!(config["reward"]["game_mappings"][0]["app_id"], 730);
    assert!(!config["runtime"]["activity_sources"]
        .as_array()
        .unwrap()
        .is_empty());
    assert_eq!(
        config["runtime"]["activity_sources"][0]["source_kind"],
        "Steam"
    );
    let launcher_paths = std::fs::read_dir(&startup_dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("vbs"))
        .collect::<Vec<_>>();
    assert_eq!(launcher_paths.len(), 1);
    let launcher_content = std::fs::read_to_string(&launcher_paths[0]).unwrap();
    assert!(launcher_content.contains("player-autostart"));
    assert!(launcher_content.contains(config_path.to_string_lossy().as_ref()));

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn activity_source_commands_manage_configured_sources() {
    let root = temp_root("activity-sources");
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    std::fs::create_dir_all(&root).unwrap();

    let config_path = root.join("client.json");
    let binary = env!("CARGO_BIN_EXE_pole-client");

    let init = Command::new(binary)
        .arg("init")
        .arg(&config_path)
        .output()
        .unwrap();
    assert!(init.status.success());

    let add = Command::new(binary)
        .arg("activity-sources-add")
        .arg(&config_path)
        .arg("730")
        .arg("epic")
        .arg("https://example.invalid/epic?appid=730")
        .output()
        .unwrap();
    assert!(
        add.status.success(),
        "stderr:\n{}",
        String::from_utf8_lossy(&add.stderr)
    );

    let list = Command::new(binary)
        .arg("activity-sources-list")
        .arg(&config_path)
        .output()
        .unwrap();
    assert!(list.status.success());
    let stdout = String::from_utf8_lossy(&list.stdout);
    assert!(stdout.contains("source_kind=Steam"));
    assert!(stdout.contains("source_kind=Epic"));

    let remove = Command::new(binary)
        .arg("activity-sources-remove")
        .arg(&config_path)
        .arg("730")
        .arg("epic")
        .output()
        .unwrap();
    assert!(remove.status.success());

    let config: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&config_path).unwrap()).unwrap();
    assert!(config["runtime"]["activity_sources"]
        .as_array()
        .unwrap()
        .iter()
        .all(|source| source["source_kind"] != "Epic"));

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn reward_config_commands_manage_tokenomics_settings() {
    let root = temp_root("reward-config");
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    std::fs::create_dir_all(&root).unwrap();

    let config_path = root.join("client.json");
    let binary = env!("CARGO_BIN_EXE_pole-client");

    let init = Command::new(binary)
        .arg("init")
        .arg(&config_path)
        .output()
        .unwrap();
    assert!(init.status.success());

    let show = Command::new(binary)
        .arg("reward-config-show")
        .arg(&config_path)
        .output()
        .unwrap();
    assert!(show.status.success());
    let stdout = String::from_utf8_lossy(&show.stdout);
    assert!(stdout.contains("reward_source=tokenomics"));
    assert!(stdout.contains("emission_year=1"));
    assert!(stdout.contains("tail_emission_start_year=4"));
    assert!(stdout.contains("tail_emission_rate_bps=200"));
    assert!(stdout.contains("collect_reward_bps=5000"));

    let set_year = Command::new(binary)
        .arg("reward-config-set")
        .arg(&config_path)
        .arg("emission-year")
        .arg("3")
        .output()
        .unwrap();
    assert!(set_year.status.success());
    let stdout = String::from_utf8_lossy(&set_year.stdout);
    assert!(stdout.contains("emission_year=3"));
    assert!(stdout.contains("effective_player_block_reward=9132"));

    let set_tail = Command::new(binary)
        .arg("reward-config-set")
        .arg(&config_path)
        .arg("tail-policy")
        .arg("5")
        .arg("180")
        .output()
        .unwrap();
    assert!(set_tail.status.success());
    let stdout = String::from_utf8_lossy(&set_tail.stdout);
    assert!(stdout.contains("tail_emission_start_year=5"));
    assert!(stdout.contains("tail_emission_rate_bps=180"));

    let set_split = Command::new(binary)
        .arg("reward-config-set")
        .arg(&config_path)
        .arg("service-split")
        .arg("4000")
        .arg("3000")
        .arg("2000")
        .arg("1000")
        .output()
        .unwrap();
    assert!(set_split.status.success());

    let config: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&config_path).unwrap()).unwrap();
    assert_eq!(config["reward"]["emission_year"], 3);
    assert_eq!(config["reward"]["tail_emission_start_year"], 5);
    assert_eq!(config["reward"]["tail_emission_rate_bps"], 180);
    assert_eq!(config["reward"]["collect_reward_bps"], 4000);
    assert_eq!(config["reward"]["store_reward_bps"], 3000);
    assert_eq!(config["reward"]["verify_reward_bps"], 2000);
    assert_eq!(config["reward"]["propose_reward_bps"], 1000);

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn reward_config_set_uses_default_config_when_subcommand_is_first_arg() {
    let root = temp_root("reward-config-default-path");
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    std::fs::create_dir_all(&root).unwrap();

    let config_path = root.join("node.json");
    let binary = env!("CARGO_BIN_EXE_pole-client");

    let init = Command::new(binary)
        .current_dir(&root)
        .arg("init")
        .arg(&config_path)
        .arg("player")
        .output()
        .unwrap();
    assert!(init.status.success());

    let set_mode = Command::new(binary)
        .current_dir(&root)
        .arg("reward-config-set")
        .arg("mode")
        .arg("static")
        .output()
        .unwrap();
    assert!(
        set_mode.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&set_mode.stdout),
        String::from_utf8_lossy(&set_mode.stderr)
    );
    let stdout = String::from_utf8_lossy(&set_mode.stdout);
    assert!(stdout.contains("reward_source=static"));
    assert!(stdout.contains("config_path="));

    let config: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&config_path).unwrap()).unwrap();
    assert_eq!(config["reward"]["reward_source"], "static");

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn governance_param_commands_propose_and_vote_future_update() {
    let root = temp_root("governance-params");
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    std::fs::create_dir_all(&root).unwrap();

    let config_path = root.join("client.json");
    let binary = env!("CARGO_BIN_EXE_pole-client");

    let init = Command::new(binary)
        .arg("init")
        .arg(&config_path)
        .arg("validator")
        .output()
        .unwrap();
    assert!(init.status.success());

    let (_, config) = NodeConfig::load_json_with_runtime_paths(&config_path).unwrap();
    let (runtime, mut state) =
        open_local_protocol_state(&config, config.runtime.challenge_window_blocks).unwrap();
    let reward_address = config.reward_address().unwrap();
    state.upsert_account(AccountState {
        address: reward_address,
        balance: 100_000,
        staked: 0,
        locked: 0,
        nonce: 0,
    });
    runtime
        .save_json(pole_protocol_draft::local_chain_runtime_path(&config))
        .unwrap();
    state.store.flush().unwrap();

    let proposal_id =
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string();
    let propose = Command::new(binary)
        .arg("governance-propose-params")
        .arg(&config_path)
        .arg(&proposal_id)
        .arg("2")
        .arg("4")
        .arg("9132")
        .output()
        .unwrap();
    assert!(
        propose.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&propose.stdout),
        String::from_utf8_lossy(&propose.stderr)
    );
    let proposal_artifact_path =
        pole_protocol_draft::governance_proposal_artifact_path(&config, &proposal_id);
    let index_artifact_path = pole_protocol_draft::governance_index_artifact_path(&config);
    assert!(proposal_artifact_path.exists());
    assert!(index_artifact_path.exists());

    let vote = Command::new(binary)
        .arg("governance-vote")
        .arg(&config_path)
        .arg(&proposal_id)
        .arg("yes")
        .arg("25000")
        .output()
        .unwrap();
    assert!(
        vote.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&vote.stdout),
        String::from_utf8_lossy(&vote.stderr)
    );
    let stdout = String::from_utf8_lossy(&vote.stdout);
    assert!(stdout.contains("scheduled_next_epoch=true"));
    let scheduled_artifact_path =
        pole_protocol_draft::governance_scheduled_artifact_path(&config, 2);
    assert!(scheduled_artifact_path.exists());

    let show_proposal = Command::new(binary)
        .arg("governance-show-proposal")
        .arg(&config_path)
        .arg(&proposal_id)
        .output()
        .unwrap();
    assert!(show_proposal.status.success());
    let stdout = String::from_utf8_lossy(&show_proposal.stdout);
    assert!(stdout.contains("proposal_state=Scheduled"));
    assert!(stdout.contains("bond_amount=10000"));
    assert!(stdout.contains("vote_record_count=1"));
    assert!(stdout.contains("yes_voting_power=25000"));
    assert!(stdout.contains("emission_year=4"));
    assert!(stdout.contains("tail_emission_start_year=4"));
    assert!(stdout.contains("tail_emission_rate_bps=200"));
    assert!(stdout.contains("artifact_path="));
    assert!(stdout.contains("artifact_index_path="));

    let show_scheduled = Command::new(binary)
        .arg("governance-show-scheduled")
        .arg(&config_path)
        .arg("2")
        .output()
        .unwrap();
    assert!(show_scheduled.status.success());
    let stdout = String::from_utf8_lossy(&show_scheduled.stdout);
    assert!(stdout.contains("scheduled=true"));
    assert!(stdout.contains("epoch_id=2"));
    assert!(stdout.contains("emission_year=4"));
    assert!(stdout.contains("artifact_path="));
    assert!(stdout.contains("artifact_index_path="));

    assert!(proposal_artifact_path.exists());
    assert!(scheduled_artifact_path.exists());
    assert!(index_artifact_path.exists());
    let index: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&index_artifact_path).unwrap()).unwrap();
    assert!(index["proposal_artifacts"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry["proposal_id_hex"] == proposal_id));
    assert!(index["scheduled_artifacts"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry["epoch_id"] == 2 && entry["scheduled"] == true));

    let show_index = Command::new(binary)
        .arg("governance-show-index")
        .arg(&config_path)
        .output()
        .unwrap();
    assert!(show_index.status.success());
    let stdout = String::from_utf8_lossy(&show_index.stdout);
    assert!(stdout.contains("proposal_artifact_count=1"));
    assert!(stdout.contains("scheduled_artifact_count=1"));
    assert!(stdout.contains("proposal_artifact proposal_id="));
    assert!(stdout.contains("scheduled_artifact epoch_id=2 scheduled=true"));

    let show_summary = Command::new(binary)
        .arg("governance-show-summary")
        .arg(&config_path)
        .output()
        .unwrap();
    assert!(show_summary.status.success());
    let stdout = String::from_utf8_lossy(&show_summary.stdout);
    assert!(stdout.contains("scheduled_proposal_count=1"));
    assert!(stdout.contains("proposal_artifact_count=1"));
    assert!(stdout.contains("scheduled_artifact_count=1"));
    assert!(stdout.contains("latest_effective_epoch=Some(2)"));

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn governance_commands_use_default_config_when_proposal_id_is_first_arg() {
    let root = temp_root("governance-default-path");
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    std::fs::create_dir_all(&root).unwrap();

    let config_path = root.join("node.json");
    let binary = env!("CARGO_BIN_EXE_pole-client");

    let init = Command::new(binary)
        .current_dir(&root)
        .arg("init")
        .arg(&config_path)
        .arg("validator")
        .output()
        .unwrap();
    assert!(init.status.success());

    let (_, config) = NodeConfig::load_json_with_runtime_paths(&config_path).unwrap();
    let (runtime, mut state) =
        open_local_protocol_state(&config, config.runtime.challenge_window_blocks).unwrap();
    let reward_address = config.reward_address().unwrap();
    state.upsert_account(AccountState {
        address: reward_address,
        balance: 100_000,
        staked: 0,
        locked: 0,
        nonce: 0,
    });
    runtime
        .save_json(pole_protocol_draft::local_chain_runtime_path(&config))
        .unwrap();
    state.store.flush().unwrap();

    let proposal_id =
        "acacacacacacacacacacacacacacacacacacacacacacacacacacacacacacacac".to_string();
    let propose = Command::new(binary)
        .current_dir(&root)
        .arg("governance-propose-service-split")
        .arg(&proposal_id)
        .arg("2")
        .arg("4000")
        .arg("3000")
        .arg("2000")
        .arg("1000")
        .output()
        .unwrap();
    assert!(
        propose.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&propose.stdout),
        String::from_utf8_lossy(&propose.stderr)
    );

    let vote = Command::new(binary)
        .current_dir(&root)
        .arg("governance-vote")
        .arg(&proposal_id)
        .arg("yes")
        .arg("25000")
        .output()
        .unwrap();
    assert!(
        vote.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&vote.stdout),
        String::from_utf8_lossy(&vote.stderr)
    );
    let stdout = String::from_utf8_lossy(&vote.stdout);
    assert!(stdout.contains("scheduled_next_epoch=true"));

    let show = Command::new(binary)
        .current_dir(&root)
        .arg("governance-show-proposal")
        .arg(&proposal_id)
        .output()
        .unwrap();
    assert!(
        show.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&show.stdout),
        String::from_utf8_lossy(&show.stderr)
    );
    let stdout = String::from_utf8_lossy(&show.stdout);
    assert!(stdout.contains("proposal_id="));
    assert!(stdout.contains("collect_reward_bps=4000"));
    assert!(stdout.contains("artifact_path="));

    let app_weight_id =
        "dadadadadadadadadadadadadadadadadadadadadadadadadadadadadadadada".to_string();
    let app_weight = Command::new(binary)
        .current_dir(&root)
        .arg("governance-propose-app-weight")
        .arg(&app_weight_id)
        .arg("3")
        .arg("730")
        .arg("850000")
        .output()
        .unwrap();
    assert!(
        app_weight.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&app_weight.stdout),
        String::from_utf8_lossy(&app_weight.stderr)
    );

    let show_app_weight = Command::new(binary)
        .current_dir(&root)
        .arg("governance-show-proposal")
        .arg(&app_weight_id)
        .output()
        .unwrap();
    assert!(
        show_app_weight.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&show_app_weight.stdout),
        String::from_utf8_lossy(&show_app_weight.stderr)
    );
    let stdout = String::from_utf8_lossy(&show_app_weight.stdout);
    assert!(stdout.contains("app_weight_override_count=1"));
    assert!(stdout.contains("app_weight_override app_id=730 game_coefficient_ppm=850000"));

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn governance_service_split_and_threshold_proposals_are_queryable() {
    let root = temp_root("governance-proposal-types");
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    std::fs::create_dir_all(&root).unwrap();

    let config_path = root.join("client.json");
    let binary = env!("CARGO_BIN_EXE_pole-client");

    let init = Command::new(binary)
        .arg("init")
        .arg(&config_path)
        .arg("validator")
        .output()
        .unwrap();
    assert!(init.status.success());

    let (_, config) = NodeConfig::load_json_with_runtime_paths(&config_path).unwrap();
    let (runtime, mut state) =
        open_local_protocol_state(&config, config.runtime.challenge_window_blocks).unwrap();
    let reward_address = config.reward_address().unwrap();
    state.upsert_account(AccountState {
        address: reward_address,
        balance: 100_000,
        staked: 0,
        locked: 0,
        nonce: 0,
    });
    runtime
        .save_json(pole_protocol_draft::local_chain_runtime_path(&config))
        .unwrap();
    state.store.flush().unwrap();

    let split_id = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_string();
    let split = Command::new(binary)
        .arg("governance-propose-service-split")
        .arg(&config_path)
        .arg(&split_id)
        .arg("2")
        .arg("4000")
        .arg("3000")
        .arg("2000")
        .arg("1000")
        .output()
        .unwrap();
    assert!(split.status.success());

    let show_split = Command::new(binary)
        .arg("governance-show-proposal")
        .arg(&config_path)
        .arg(&split_id)
        .output()
        .unwrap();
    assert!(show_split.status.success());
    let stdout = String::from_utf8_lossy(&show_split.stdout);
    assert!(stdout.contains("collect_reward_bps=4000"));
    assert!(stdout.contains("store_reward_bps=3000"));
    assert!(stdout.contains("verify_reward_bps=2000"));
    assert!(stdout.contains("propose_reward_bps=1000"));

    let threshold_id =
        "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc".to_string();
    let thresholds = Command::new(binary)
        .arg("governance-propose-thresholds")
        .arg(&config_path)
        .arg(&threshold_id)
        .arg("2")
        .arg("3000")
        .arg("7000")
        .output()
        .unwrap();
    assert!(thresholds.status.success());

    let show_thresholds = Command::new(binary)
        .arg("governance-show-proposal")
        .arg(&config_path)
        .arg(&threshold_id)
        .output()
        .unwrap();
    assert!(show_thresholds.status.success());
    let stdout = String::from_utf8_lossy(&show_thresholds.stdout);
    assert!(stdout.contains("params_update_quorum_bps=3000"));
    assert!(stdout.contains("params_update_approval_bps=7000"));

    let tuning_id = "edededededededededededededededededededededededededededededededed".to_string();
    let tuning = Command::new(binary)
        .arg("governance-propose-reward-tuning")
        .arg(&config_path)
        .arg(&tuning_id)
        .arg("2")
        .arg("1800000000000")
        .arg("1500")
        .arg("25")
        .arg("12000")
        .output()
        .unwrap();
    assert!(tuning.status.success());

    let show_tuning = Command::new(binary)
        .arg("governance-show-proposal")
        .arg(&config_path)
        .arg(&tuning_id)
        .output()
        .unwrap();
    assert!(show_tuning.status.success());
    let stdout = String::from_utf8_lossy(&show_tuning.stdout);
    assert!(stdout.contains("target_network_weight_units=1800000000000"));
    assert!(stdout.contains("reward_adjustment_cap_bps=1500"));
    assert!(stdout.contains("challenge_window_blocks=25"));
    assert!(stdout.contains("effective_player_block_reward=12000"));

    let slow_id = "abababababababababababababababababababababababababababababababab".to_string();
    let slow = Command::new(binary)
        .arg("governance-propose-slow-params")
        .arg(&config_path)
        .arg(&slow_id)
        .arg("3")
        .arg("7200")
        .arg("15000")
        .output()
        .unwrap();
    assert!(slow.status.success());

    let show_slow = Command::new(binary)
        .arg("governance-show-proposal")
        .arg(&config_path)
        .arg(&slow_id)
        .output()
        .unwrap();
    assert!(show_slow.status.success());
    let stdout = String::from_utf8_lossy(&show_slow.stdout);
    assert!(stdout.contains("reward_block_secs=7200"));
    assert!(stdout.contains("effective_player_block_reward=15000"));

    let retention_id =
        "adadadadadadadadadadadadadadadadadadadadadadadadadadadadadadadad".to_string();
    let retention = Command::new(binary)
        .arg("governance-propose-retention")
        .arg(&config_path)
        .arg(&retention_id)
        .arg("3")
        .arg("4")
        .arg("30")
        .output()
        .unwrap();
    assert!(retention.status.success());

    let show_retention = Command::new(binary)
        .arg("governance-show-proposal")
        .arg(&config_path)
        .arg(&retention_id)
        .output()
        .unwrap();
    assert!(show_retention.status.success());
    let stdout = String::from_utf8_lossy(&show_retention.stdout);
    assert!(stdout.contains("min_retention_epochs=4"));
    assert!(stdout.contains("challenge_window_blocks=30"));

    let app_weight_id =
        "aeaeaeaeaeaeaeaeaeaeaeaeaeaeaeaeaeaeaeaeaeaeaeaeaeaeaeaeaeaeaeae".to_string();
    let app_weight = Command::new(binary)
        .arg("governance-propose-app-weight")
        .arg(&config_path)
        .arg(&app_weight_id)
        .arg("3")
        .arg("730")
        .arg("850000")
        .output()
        .unwrap();
    assert!(app_weight.status.success());

    let show_app_weight = Command::new(binary)
        .arg("governance-show-proposal")
        .arg(&config_path)
        .arg(&app_weight_id)
        .output()
        .unwrap();
    assert!(show_app_weight.status.success());
    let stdout = String::from_utf8_lossy(&show_app_weight.stdout);
    assert!(stdout.contains("app_weight_override_count=1"));
    assert!(stdout.contains("app_weight_override app_id=730 game_coefficient_ppm=850000"));

    let tier_id = "bcbcbcbcbcbcbcbcbcbcbcbcbcbcbcbcbcbcbcbcbcbcbcbcbcbcbcbcbcbcbcbc".to_string();
    let tier = Command::new(binary)
        .arg("governance-propose-tier-weights")
        .arg(&config_path)
        .arg(&tier_id)
        .arg("2")
        .arg("950000")
        .arg("250000")
        .arg("550000")
        .arg("60000")
        .arg("160000")
        .output()
        .unwrap();
    assert!(tier.status.success());

    let show_tier = Command::new(binary)
        .arg("governance-show-proposal")
        .arg(&config_path)
        .arg(&tier_id)
        .output()
        .unwrap();
    assert!(show_tier.status.success());
    let stdout = String::from_utf8_lossy(&show_tier.stdout);
    assert!(stdout.contains("tier1_weight_ppm=950000"));
    assert!(stdout.contains("tier2_weight_min_ppm=250000"));
    assert!(stdout.contains("tier2_weight_max_ppm=550000"));
    assert!(stdout.contains("tier3_weight_min_ppm=60000"));
    assert!(stdout.contains("tier3_weight_max_ppm=160000"));

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn player_start_can_select_p2p_sim_background_mode() {
    let root = temp_root("player-start-p2p-mode");
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    std::fs::create_dir_all(&root).unwrap();

    let config_path = root.join("client.json");
    let app_data = root.join("appdata");
    let binary = env!("CARGO_BIN_EXE_pole-client");
    let output = Command::new(binary)
        .env("APPDATA", &app_data)
        .env("POLE_CLIENT_BACKGROUND_MODE", "watch-p2p-sim")
        .env("POLE_CLIENT_SKIP_BACKGROUND_START", "1")
        .arg("player-start")
        .arg(&config_path)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("background_mode=watch-p2p-sim"));
    assert!(stdout.contains("background_start_skipped=true"));

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn player_start_can_select_p2p_fs_background_mode() {
    let root = temp_root("player-start-p2p-fs-mode");
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    std::fs::create_dir_all(&root).unwrap();

    let config_path = root.join("client.json");
    let app_data = root.join("appdata");
    let binary = env!("CARGO_BIN_EXE_pole-client");
    let output = Command::new(binary)
        .env("APPDATA", &app_data)
        .env("POLE_CLIENT_BACKGROUND_MODE", "watch-p2p-fs")
        .env("POLE_CLIENT_SKIP_BACKGROUND_START", "1")
        .arg("player-start")
        .arg(&config_path)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("background_mode=watch-p2p-fs"));
    assert!(stdout.contains("background_start_skipped=true"));

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn player_start_can_select_p2p_socket_background_mode() {
    let root = temp_root("player-start-p2p-socket-mode");
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    std::fs::create_dir_all(&root).unwrap();

    let config_path = root.join("client.json");
    let app_data = root.join("appdata");
    let sink = [0x76; 32];
    let mut sink_network =
        SocketP2pNetwork::bind(sink, "127.0.0.1:0".parse().unwrap(), vec![]).unwrap();
    sink_network
        .bootstrap_peer(sink, &[P2pTopic::Batches, P2pTopic::Receipts])
        .unwrap();
    let sink_addr = sink_network.local_addr().unwrap();
    let binary = env!("CARGO_BIN_EXE_pole-client");
    let output = Command::new(binary)
        .env("APPDATA", &app_data)
        .env("POLE_CLIENT_BACKGROUND_MODE", "watch-p2p-socket")
        .env("POLE_CLIENT_SOCKET_BIND_ADDR", "127.0.0.1:0")
        .env(
            "POLE_CLIENT_SOCKET_PEER_SPECS",
            format!(
                "{}@{}@batches,receipts",
                pole_protocol_draft::hex_32(sink),
                sink_addr
            ),
        )
        .env("POLE_CLIENT_SKIP_BACKGROUND_START", "1")
        .arg("player-start")
        .arg(&config_path)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("background_mode=watch-p2p-socket"));
    assert!(stdout.contains("background_start_skipped=true"));

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn player_start_can_use_config_backed_p2p_socket_background_mode() {
    let root = temp_root("player-start-p2p-socket-config-mode");
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    std::fs::create_dir_all(&root).unwrap();

    let config_path = root.join("client.json");
    let app_data = root.join("appdata");
    let sink = [0x7a; 32];
    let mut sink_network =
        SocketP2pNetwork::bind(sink, "127.0.0.1:0".parse().unwrap(), vec![]).unwrap();
    sink_network
        .bootstrap_peer(sink, &[P2pTopic::Batches, P2pTopic::Receipts])
        .unwrap();
    let sink_addr = sink_network.local_addr().unwrap();

    let mut config = NodeConfig::default();
    config.runtime.data_dir = root.join("pole-node-data").to_string_lossy().into_owned();
    config.runtime.p2p_socket.bind_addr = "127.0.0.1:0".into();
    config.runtime.p2p_socket.peers = vec![pole_protocol_draft::P2pSocketPeerConfig {
        peer_id_hex: pole_protocol_draft::hex_32(sink),
        addr: sink_addr.to_string(),
        topics: vec!["batches".into(), "receipts".into()],
    }];
    config.save_json(&config_path).unwrap();

    let binary = env!("CARGO_BIN_EXE_pole-client");
    let output = Command::new(binary)
        .env("APPDATA", &app_data)
        .env("POLE_CLIENT_BACKGROUND_MODE", "watch-p2p-socket")
        .env("POLE_CLIENT_SKIP_BACKGROUND_START", "1")
        .arg("player-start")
        .arg(&config_path)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("background_mode=watch-p2p-socket"));
    assert!(stdout.contains("background_start_skipped=true"));

    std::fs::remove_dir_all(root).unwrap();
}

#[cfg(windows)]
#[test]
fn player_autostart_writes_background_metadata_and_reports_it() {
    let root = temp_root("player-autostart-meta");
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    std::fs::create_dir_all(&root).unwrap();

    let config_path = root.join("client.json");
    let binary = env!("CARGO_BIN_EXE_pole-client");
    let output = Command::new(binary)
        .env("POLE_CLIENT_BACKGROUND_MODE", "watch-p2p-sim")
        .env("POLE_CLIENT_BACKGROUND_TICKS", "1")
        .arg("player-autostart")
        .arg(&config_path)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("PoLE player autostart ensured"));
    assert!(stdout.contains("background_mode=watch-p2p-sim"));
    assert!(stdout.contains("background_started=true"));

    let config: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&config_path).unwrap()).unwrap();
    let data_dir = PathBuf::from(config["runtime"]["data_dir"].as_str().unwrap());
    let meta_path = data_dir.join("daemon.meta.json");
    wait_for_file(&meta_path);

    let metadata: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&meta_path).unwrap()).unwrap();
    let pid = metadata["pid"].as_u64().unwrap() as u32;

    assert_eq!(metadata["background_mode"], "watch-p2p-sim");
    assert_eq!(
        metadata["config_path"],
        config_path.to_string_lossy().as_ref()
    );
    assert_eq!(
        metadata["pid_file"],
        data_dir.join("daemon.pid").to_string_lossy().as_ref()
    );
    assert_eq!(
        metadata["stdout_log"],
        data_dir.join("daemon.out.log").to_string_lossy().as_ref()
    );
    assert_eq!(
        metadata["stderr_log"],
        data_dir.join("daemon.err.log").to_string_lossy().as_ref()
    );
    assert!(metadata["started_at_millis"].as_u64().unwrap() > 0);

    let status = Command::new(binary)
        .arg("status")
        .arg(&config_path)
        .output()
        .unwrap();
    assert!(
        status.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&status.stdout),
        String::from_utf8_lossy(&status.stderr)
    );
    let status_stdout = String::from_utf8_lossy(&status.stdout);
    assert!(status_stdout.contains("daemon_meta_exists=true"));
    assert!(status_stdout.contains("daemon_background_mode=watch-p2p-sim"));
    assert!(status_stdout.contains(&format!(
        "daemon_stdout_log={}",
        data_dir.join("daemon.out.log").to_string_lossy()
    )));

    let doctor = Command::new(binary)
        .arg("doctor")
        .arg(&config_path)
        .output()
        .unwrap();
    assert!(
        doctor.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&doctor.stdout),
        String::from_utf8_lossy(&doctor.stderr)
    );
    let doctor_stdout = String::from_utf8_lossy(&doctor.stdout);
    assert!(doctor_stdout.contains("daemon_meta_exists=true"));
    assert!(doctor_stdout.contains("daemon_background_mode=watch-p2p-sim"));
    assert!(doctor_stdout.contains(&format!(
        "daemon_pid_file={}",
        data_dir.join("daemon.pid").to_string_lossy()
    )));

    terminate_process(pid);
    thread::sleep(Duration::from_millis(100));
    std::fs::remove_dir_all(root).unwrap();
}

#[cfg(windows)]
#[test]
fn install_script_copies_binary_and_bootstraps_player_mode() {
    let root = temp_root("install-player");
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    std::fs::create_dir_all(&root).unwrap();

    let install_root = root.join("install-root");
    let config_path = root
        .join("localappdata")
        .join("PoLE")
        .join("player")
        .join("node.json");
    let app_data = root.join("appdata");
    let local_app_data = root.join("localappdata");
    let startup_dir = app_data
        .join("Microsoft")
        .join("Windows")
        .join("Start Menu")
        .join("Programs")
        .join("Startup");
    let script_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("scripts")
        .join("install-pole-player.ps1");
    let binary = env!("CARGO_BIN_EXE_pole-client");

    let output = Command::new("powershell")
        .env("APPDATA", &app_data)
        .env("LOCALAPPDATA", &local_app_data)
        .env(
            "POLE_CLIENT_FOREGROUND_PROCESS_OVERRIDE",
            "MonsterHunterWilds.exe",
        )
        .env("POLE_CLIENT_SKIP_BACKGROUND_START", "1")
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
            script_path.to_str().unwrap(),
            "-SourceBinaryPath",
            binary,
            "-InstallRoot",
            install_root.to_str().unwrap(),
            "-ConfigPath",
            config_path.to_str().unwrap(),
        ])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("PoLE player mode started"));
    assert!(stdout.contains("player_install_completed=true"));
    assert!(stdout.contains("autostart_enabled=true"));
    assert!(install_root.join("pole-client.exe").exists());
    assert!(install_root.join("README.txt").exists());
    assert!(config_path.exists());

    let launcher_paths = std::fs::read_dir(&startup_dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("vbs"))
        .collect::<Vec<_>>();
    assert_eq!(launcher_paths.len(), 1);
    let launcher_content = std::fs::read_to_string(&launcher_paths[0]).unwrap();
    assert!(launcher_content.contains("player-autostart"));
    assert!(launcher_content.contains(config_path.to_string_lossy().as_ref()));

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn status_reports_fresh_workspace() {
    let root = temp_root("status");
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    std::fs::create_dir_all(&root).unwrap();

    let config_path = root.join("client.json");
    let binary = env!("CARGO_BIN_EXE_pole-client");
    let init = Command::new(binary)
        .arg("init")
        .arg(&config_path)
        .arg("minimal")
        .output()
        .unwrap();
    assert!(init.status.success());

    let status = Command::new(binary)
        .arg("status")
        .arg(&config_path)
        .output()
        .unwrap();
    assert!(
        status.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&status.stdout),
        String::from_utf8_lossy(&status.stderr)
    );

    let stdout = String::from_utf8_lossy(&status.stdout);
    assert!(stdout.contains("PoLE client status"));
    assert!(stdout.contains("ticks_completed=0"));
    assert!(stdout.contains("next_epoch=1"));
    assert!(stdout.contains("next_slot=1"));
    assert!(stdout.contains("stored_payloads=0"));
    assert!(stdout.contains("challenge_window_blocks=20"));
    assert!(stdout.contains("reward_adjustment_cap_bps=2000"));
    assert!(stdout.contains("configured_p2p_batch_listeners=1"));
    assert!(stdout.contains("configured_p2p_receipt_listeners=1"));
    assert!(stdout.contains("configured_p2p_dual_listeners=1"));
    assert!(stdout.contains("daemon_meta_exists=false"));
    assert!(stdout.contains("last_p2p_retrieval_ok=None"));
    assert!(stdout.contains("last_retention_all_retrievable=None"));

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn tokenomics_reports_supply_allocations_and_schedule() {
    let binary = env!("CARGO_BIN_EXE_pole-client");
    let output = Command::new(binary)
        .arg("tokenomics")
        .arg("4")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("PoLE tokenomics"));
    assert!(stdout.contains("total_supply=1000000000"));
    assert!(stdout.contains("player_rewards_allocation=800000000"));
    assert!(stdout.contains("service_rewards_allocation=100000000"));
    assert!(stdout.contains(
        "year=1 nominal_rate_bps=2000 annual_emission=200000000 cumulative_emission=200000000"
    ));
    assert!(stdout.contains(
        "year=3 nominal_rate_bps=1000 annual_emission=100000000 cumulative_emission=500000000"
    ));
}

#[test]
fn doctor_reports_initialized_workspace_is_healthy() {
    let root = temp_root("doctor");
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    std::fs::create_dir_all(&root).unwrap();

    let config_path = root.join("client.json");
    let binary = env!("CARGO_BIN_EXE_pole-client");
    let init = Command::new(binary)
        .arg("init")
        .arg(&config_path)
        .arg("validator")
        .output()
        .unwrap();
    assert!(init.status.success());

    let doctor = Command::new(binary)
        .arg("doctor")
        .arg(&config_path)
        .output()
        .unwrap();
    assert!(
        doctor.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&doctor.stdout),
        String::from_utf8_lossy(&doctor.stderr)
    );

    let stdout = String::from_utf8_lossy(&doctor.stdout);
    assert!(stdout.contains("PoLE client doctor"));
    assert!(stdout.contains("overall_ok=true"));
    assert!(stdout.contains("node_id_ok=true"));
    assert!(stdout.contains("reward_address_ok=true"));
    assert!(stdout.contains("target_app_ids_ok=true"));
    assert!(stdout.contains("reward_source=tokenomics"));
    assert!(stdout.contains("emission_year=1"));
    assert!(stdout.contains("game_safe_mode=false"));
    assert!(stdout.contains("game_process_awareness_configured=false"));
    assert!(stdout.contains("daemon_meta_exists=false"));
    assert!(stdout.contains("last_p2p_retrieval_ok=None"));
    assert!(stdout.contains("last_retention_all_retrievable=None"));

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn status_reports_attached_network_and_retention_health() {
    let root = temp_root("status-attached-network");
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    std::fs::create_dir_all(&root).unwrap();

    let config_path = root.join("client.json");
    let data_dir = root.join("pole-node-data");
    let config = NodeConfig {
        chain_id: "pole-local".into(),
        node_id_hex: pole_protocol_draft::hex_32([0x31; 32]),
        reward_address_hex: pole_protocol_draft::hex_32([0x41; 32]),
        capabilities: CapabilityConfig {
            collect: true,
            store: true,
            verify: false,
            propose: false,
            archive: false,
        },
        collect: CollectConfig {
            enabled: true,
            default_epoch_id: 1,
            default_slot_id: 1,
        },
        runtime: RuntimeConfig {
            data_dir: data_dir.to_string_lossy().into_owned(),
            poll_interval_secs: 0,
            slots_per_epoch: 10,
            challenge_window_blocks: 20,
            low_impact_mode: true,
            os_background_priority: true,
            game_active_poll_interval_secs: 900,
            game_process_names: Vec::new(),
            target_app_ids: vec![730],
            p2p_simulation: pole_protocol_draft::P2pSimulationConfig::default(),
            p2p_socket: pole_protocol_draft::P2pSocketConfig::default(),
            p2p_libp2p: pole_protocol_draft::P2pLibp2pConfig::default(),
            activity_sources: Vec::new(),
        },
        storage: StorageConfig {
            quota_gb: 1,
            retention_epochs: 2,
        },
        reward: RewardConfig::default(),
    };
    config.save_json(&config_path).unwrap();

    let client = FixedHttpClient;
    let mut progress = LocalNodeProgress::default_from_config(&config);
    let mut network = build_inmemory_simulation_network(config.runtime.p2p_simulation);
    run_collect_tick_with_client_and_network(&config, &mut progress, &client, &mut network)
        .unwrap();

    let binary = env!("CARGO_BIN_EXE_pole-client");
    let status = Command::new(binary)
        .arg("status")
        .arg(&config_path)
        .output()
        .unwrap();
    assert!(
        status.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&status.stdout),
        String::from_utf8_lossy(&status.stderr)
    );

    let stdout = String::from_utf8_lossy(&status.stdout);
    assert!(stdout.contains("configured_p2p_batch_listeners=1"));
    assert!(stdout.contains("configured_p2p_receipt_listeners=1"));
    assert!(stdout.contains("configured_p2p_dual_listeners=1"));
    assert!(stdout.contains("last_p2p_batch_recipients=Some(2)"));
    assert!(stdout.contains("last_p2p_receipt_recipients=Some(2)"));
    assert!(stdout.contains("last_p2p_retrieval_ok=Some(true)"));
    assert!(stdout.contains("last_p2p_retrieval_error=None"));
    assert!(stdout.contains("last_p2p_challenge_recipients=None"));
    assert!(stdout.contains("last_p2p_challenge_kind=None"));
    assert!(stdout.contains("last_p2p_challenge_epoch_id=None"));
    assert!(stdout.contains("last_p2p_challenge_payload_cid=None"));
    assert!(stdout.contains("p2p_challenge_events_total=Some(0)"));
    assert!(stdout.contains("p2p_bad_batch_challenge_events=Some(0)"));
    assert!(stdout.contains("p2p_omission_challenge_events=Some(0)"));
    assert!(stdout.contains("p2p_bad_aggregate_challenge_events=Some(0)"));
    assert!(stdout.contains("p2p_bad_reward_challenge_events=Some(0)"));
    assert!(stdout.contains("p2p_bad_storage_challenge_events=Some(0)"));
    assert!(stdout.contains("recent_p2p_challenge_events=Some(0)"));
    assert!(stdout.contains("recent_p2p_bad_batch_challenge_events=Some(0)"));
    assert!(stdout.contains("recent_p2p_omission_challenge_events=Some(0)"));
    assert!(stdout.contains("recent_p2p_bad_aggregate_challenge_events=Some(0)"));
    assert!(stdout.contains("recent_p2p_bad_reward_challenge_events=Some(0)"));
    assert!(stdout.contains("recent_p2p_bad_storage_challenge_events=Some(0)"));
    assert!(stdout.contains("p2p_challenge_delivered_events_total=Some(0)"));
    assert!(stdout.contains("p2p_challenge_zero_recipient_events_total=Some(0)"));
    assert!(stdout.contains("recent_p2p_challenge_delivered_events=Some(0)"));
    assert!(stdout.contains("recent_p2p_challenge_zero_recipient_events=Some(0)"));
    assert!(stdout.contains("recent_p2p_challenge_recipient_sum=Some(0)"));
    assert!(stdout.contains("last_retention_all_retrievable=Some(true)"));
    assert!(stdout.contains("last_retention_retained_payload_count=Some(1)"));
    assert!(stdout.contains("last_storage_challenge_all_passed=Some(true)"));
    assert!(stdout.contains("last_storage_challenge_checked_payload_count=Some(1)"));
    assert!(stdout.contains("last_storage_challenge_failed_payload_count=Some(0)"));
    assert!(stdout.contains("last_storage_challenge_error=None"));

    let doctor = Command::new(binary)
        .arg("doctor")
        .arg(&config_path)
        .output()
        .unwrap();
    assert!(
        doctor.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&doctor.stdout),
        String::from_utf8_lossy(&doctor.stderr)
    );

    let doctor_stdout = String::from_utf8_lossy(&doctor.stdout);
    assert!(doctor_stdout.contains("last_p2p_retrieval_ok=Some(true)"));
    assert!(doctor_stdout.contains("last_retention_all_retrievable=Some(true)"));
    assert!(doctor_stdout.contains("last_storage_challenge_all_passed=Some(true)"));
    assert!(doctor_stdout.contains("last_p2p_challenge_recipients=None"));
    assert!(doctor_stdout.contains("last_p2p_challenge_kind=None"));
    assert!(doctor_stdout.contains("p2p_challenge_events_total=Some(0)"));
    assert!(doctor_stdout.contains("p2p_bad_batch_challenge_events=Some(0)"));
    assert!(doctor_stdout.contains("recent_p2p_challenge_events=Some(0)"));
    assert!(doctor_stdout.contains("p2p_challenge_delivered_events_total=Some(0)"));

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn status_and_doctor_report_storage_challenge_failure() {
    let root = temp_root("status-storage-challenge-failure");
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    std::fs::create_dir_all(&root).unwrap();

    let config_path = root.join("client.json");
    let data_dir = root.join("pole-node-data");
    let config = NodeConfig {
        chain_id: "pole-local".into(),
        node_id_hex: pole_protocol_draft::hex_32([0x31; 32]),
        reward_address_hex: pole_protocol_draft::hex_32([0x41; 32]),
        capabilities: CapabilityConfig {
            collect: true,
            store: true,
            verify: false,
            propose: false,
            archive: false,
        },
        collect: CollectConfig {
            enabled: true,
            default_epoch_id: 1,
            default_slot_id: 1,
        },
        runtime: RuntimeConfig {
            data_dir: data_dir.to_string_lossy().into_owned(),
            poll_interval_secs: 0,
            slots_per_epoch: 10,
            challenge_window_blocks: 20,
            low_impact_mode: true,
            os_background_priority: false,
            game_active_poll_interval_secs: 0,
            game_process_names: Vec::new(),
            target_app_ids: vec![730],
            p2p_simulation: pole_protocol_draft::P2pSimulationConfig::default(),
            p2p_socket: pole_protocol_draft::P2pSocketConfig::default(),
            p2p_libp2p: pole_protocol_draft::P2pLibp2pConfig::default(),
            activity_sources: Vec::new(),
        },
        storage: StorageConfig {
            quota_gb: 1,
            retention_epochs: 3,
        },
        reward: RewardConfig::default(),
    };
    config.save_json(&config_path).unwrap();

    let client = FixedHttpClient;
    let mut progress = LocalNodeProgress::default_from_config(&config);
    let first = run_collect_tick_with_client(&config, &mut progress, &client).unwrap();
    let ledger_path = pole_protocol_draft::retention_book_path(&config);
    let mut retention_book =
        pole_protocol_draft::LocalRetentionBook::load_or_default_json(&ledger_path, 1).unwrap();
    let record = retention_book
        .payloads
        .get_mut(&first.artifact.payload_cid)
        .unwrap();
    record.receipt.receipt_signature = vec![0u8; 32];
    retention_book.save_json(&ledger_path).unwrap();
    run_collect_tick_with_client(&config, &mut progress, &client).unwrap();

    let binary = env!("CARGO_BIN_EXE_pole-client");
    let status = Command::new(binary)
        .arg("status")
        .arg(&config_path)
        .output()
        .unwrap();
    assert!(
        status.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&status.stdout),
        String::from_utf8_lossy(&status.stderr)
    );
    let status_stdout = String::from_utf8_lossy(&status.stdout);
    assert!(status_stdout.contains("last_storage_challenge_all_passed=Some(false)"));
    assert!(status_stdout.contains("last_storage_challenge_checked_payload_count=Some(2)"));
    assert!(status_stdout.contains("last_storage_challenge_failed_payload_count=Some(1)"));
    assert!(status_stdout.contains("last_storage_challenge_error=None"));
    assert!(status_stdout.contains("last_p2p_challenge_recipients=None"));
    assert!(status_stdout.contains("last_p2p_challenge_kind=None"));

    let doctor = Command::new(binary)
        .arg("doctor")
        .arg(&config_path)
        .output()
        .unwrap();
    assert!(
        doctor.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&doctor.stdout),
        String::from_utf8_lossy(&doctor.stderr)
    );
    let doctor_stdout = String::from_utf8_lossy(&doctor.stdout);
    assert!(doctor_stdout.contains("last_storage_challenge_all_passed=Some(false)"));
    assert!(doctor_stdout.contains("last_storage_challenge_failed_payload_count=Some(1)"));
    assert!(doctor_stdout.contains("last_p2p_challenge_kind=None"));

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn status_and_doctor_report_attached_network_challenge_activity_summary() {
    let root = temp_root("status-challenge-activity");
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    std::fs::create_dir_all(&root).unwrap();

    let config_path = root.join("client.json");
    let data_dir = root.join("pole-node-data");
    let config = NodeConfig {
        chain_id: "pole-local".into(),
        node_id_hex: pole_protocol_draft::hex_32([0x31; 32]),
        reward_address_hex: pole_protocol_draft::hex_32([0x41; 32]),
        capabilities: CapabilityConfig {
            collect: true,
            store: true,
            verify: false,
            propose: false,
            archive: false,
        },
        collect: CollectConfig {
            enabled: true,
            default_epoch_id: 1,
            default_slot_id: 1,
        },
        runtime: RuntimeConfig {
            data_dir: data_dir.to_string_lossy().into_owned(),
            poll_interval_secs: 0,
            slots_per_epoch: 10,
            challenge_window_blocks: 20,
            low_impact_mode: true,
            os_background_priority: false,
            game_active_poll_interval_secs: 0,
            game_process_names: Vec::new(),
            target_app_ids: vec![730],
            p2p_simulation: pole_protocol_draft::P2pSimulationConfig::default(),
            p2p_socket: pole_protocol_draft::P2pSocketConfig::default(),
            p2p_libp2p: pole_protocol_draft::P2pLibp2pConfig::default(),
            activity_sources: Vec::new(),
        },
        storage: StorageConfig {
            quota_gb: 1,
            retention_epochs: 3,
        },
        reward: RewardConfig::default(),
    };
    config.save_json(&config_path).unwrap();

    let client = FixedHttpClient;
    let mut progress = LocalNodeProgress::default_from_config(&config);
    let mut network = build_inmemory_simulation_network(config.runtime.p2p_simulation);
    let challenge_listener = [0x88; 32];
    network.register_peer(challenge_listener);
    network
        .subscribe(challenge_listener, P2pTopic::Challenges)
        .unwrap();
    let first =
        run_collect_tick_with_client_and_network(&config, &mut progress, &client, &mut network)
            .unwrap();
    let ledger_path = retention_book_path(&config);
    let mut retention_book =
        pole_protocol_draft::LocalRetentionBook::load_or_default_json(&ledger_path, 1).unwrap();
    let record = retention_book
        .payloads
        .get_mut(&first.artifact.payload_cid)
        .unwrap();
    record.receipt.receipt_signature = vec![0u8; 32];
    retention_book.save_json(&ledger_path).unwrap();
    run_collect_tick_with_client_and_network(&config, &mut progress, &client, &mut network)
        .unwrap();

    let binary = env!("CARGO_BIN_EXE_pole-client");
    let status = Command::new(binary)
        .arg("status")
        .arg(&config_path)
        .output()
        .unwrap();
    assert!(
        status.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&status.stdout),
        String::from_utf8_lossy(&status.stderr)
    );
    let status_stdout = String::from_utf8_lossy(&status.stdout);
    assert!(status_stdout.contains("last_p2p_challenge_recipients=Some(1)"));
    assert!(status_stdout.contains("last_p2p_challenge_kind=Some(\"BadStorage\")"));
    assert!(status_stdout.contains("last_p2p_challenge_epoch_id=Some(1)"));
    assert!(status_stdout.contains(&format!(
        "last_p2p_challenge_payload_cid=Some(\"{}\")",
        first.artifact.payload_cid
    )));
    assert!(status_stdout.contains("p2p_challenge_events_total=Some(1)"));
    assert!(status_stdout.contains("p2p_bad_batch_challenge_events=Some(0)"));
    assert!(status_stdout.contains("p2p_omission_challenge_events=Some(0)"));
    assert!(status_stdout.contains("p2p_bad_aggregate_challenge_events=Some(0)"));
    assert!(status_stdout.contains("p2p_bad_reward_challenge_events=Some(0)"));
    assert!(status_stdout.contains("p2p_bad_storage_challenge_events=Some(1)"));
    assert!(status_stdout.contains("recent_p2p_challenge_events=Some(1)"));
    assert!(status_stdout.contains("recent_p2p_bad_batch_challenge_events=Some(0)"));
    assert!(status_stdout.contains("recent_p2p_omission_challenge_events=Some(0)"));
    assert!(status_stdout.contains("recent_p2p_bad_aggregate_challenge_events=Some(0)"));
    assert!(status_stdout.contains("recent_p2p_bad_reward_challenge_events=Some(0)"));
    assert!(status_stdout.contains("recent_p2p_bad_storage_challenge_events=Some(1)"));
    assert!(status_stdout.contains("p2p_challenge_delivered_events_total=Some(1)"));
    assert!(status_stdout.contains("p2p_challenge_zero_recipient_events_total=Some(0)"));
    assert!(status_stdout.contains("recent_p2p_challenge_delivered_events=Some(1)"));
    assert!(status_stdout.contains("recent_p2p_challenge_zero_recipient_events=Some(0)"));
    assert!(status_stdout.contains("recent_p2p_challenge_recipient_sum=Some(1)"));

    let doctor = Command::new(binary)
        .arg("doctor")
        .arg(&config_path)
        .output()
        .unwrap();
    assert!(
        doctor.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&doctor.stdout),
        String::from_utf8_lossy(&doctor.stderr)
    );
    let doctor_stdout = String::from_utf8_lossy(&doctor.stdout);
    assert!(doctor_stdout.contains("last_p2p_challenge_kind=Some(\"BadStorage\")"));
    assert!(doctor_stdout.contains("last_p2p_challenge_epoch_id=Some(1)"));
    assert!(doctor_stdout.contains("p2p_challenge_events_total=Some(1)"));
    assert!(doctor_stdout.contains("recent_p2p_challenge_events=Some(1)"));
    assert!(doctor_stdout.contains("p2p_challenge_delivered_events_total=Some(1)"));

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn status_and_doctor_report_last_auto_settlement_summary() {
    let root = temp_root("status-auto-settlement");
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    std::fs::create_dir_all(&root).unwrap();

    let config_path = root.join("client.json");
    let data_dir = root.join("pole-node-data");
    let config = NodeConfig {
        chain_id: "pole-local".into(),
        node_id_hex: pole_protocol_draft::hex_32([0x31; 32]),
        reward_address_hex: pole_protocol_draft::hex_32([0x41; 32]),
        capabilities: CapabilityConfig {
            collect: true,
            store: true,
            verify: true,
            propose: true,
            archive: false,
        },
        collect: CollectConfig {
            enabled: true,
            default_epoch_id: 1,
            default_slot_id: 1,
        },
        runtime: RuntimeConfig {
            data_dir: data_dir.to_string_lossy().into_owned(),
            poll_interval_secs: 0,
            slots_per_epoch: 2,
            challenge_window_blocks: 20,
            low_impact_mode: false,
            os_background_priority: false,
            game_active_poll_interval_secs: 0,
            game_process_names: Vec::new(),
            target_app_ids: vec![730],
            p2p_simulation: pole_protocol_draft::P2pSimulationConfig::default(),
            p2p_socket: pole_protocol_draft::P2pSocketConfig::default(),
            p2p_libp2p: pole_protocol_draft::P2pLibp2pConfig::default(),
            activity_sources: Vec::new(),
        },
        storage: StorageConfig {
            quota_gb: 1,
            retention_epochs: 2,
        },
        reward: RewardConfig::default(),
    };
    config.save_json(&config_path).unwrap();

    let client = FixedHttpClient;
    let mut progress = LocalNodeProgress::default_from_config(&config);
    run_collect_tick_with_client(&config, &mut progress, &client).unwrap();
    run_collect_tick_with_client(&config, &mut progress, &client).unwrap();

    let binary = env!("CARGO_BIN_EXE_pole-client");
    let status = Command::new(binary)
        .arg("status")
        .arg(&config_path)
        .output()
        .unwrap();
    assert!(
        status.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&status.stdout),
        String::from_utf8_lossy(&status.stderr)
    );
    let status_stdout = String::from_utf8_lossy(&status.stdout);
    assert!(status_stdout.contains("last_auto_settlement_pending_epoch_count=Some(0)"));
    assert!(status_stdout.contains("last_auto_settled_epoch=Some(1)"));
    assert!(status_stdout.contains("last_auto_settlement_reward_claimed=Some(true)"));

    let doctor = Command::new(binary)
        .arg("doctor")
        .arg(&config_path)
        .output()
        .unwrap();
    assert!(
        doctor.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&doctor.stdout),
        String::from_utf8_lossy(&doctor.stderr)
    );
    let doctor_stdout = String::from_utf8_lossy(&doctor.stdout);
    assert!(doctor_stdout.contains("last_auto_settlement_pending_epoch_count=Some(0)"));
    assert!(doctor_stdout.contains("last_auto_settled_epoch=Some(1)"));
    assert!(doctor_stdout.contains("last_auto_settlement_reward_claimed=Some(true)"));

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn status_and_doctor_report_last_auto_settlement_error() {
    let root = temp_root("status-auto-settlement-error");
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    std::fs::create_dir_all(&root).unwrap();

    let config_path = root.join("client.json");
    let data_dir = root.join("pole-node-data");
    let config = NodeConfig {
        chain_id: "pole-local".into(),
        node_id_hex: pole_protocol_draft::hex_32([0x31; 32]),
        reward_address_hex: pole_protocol_draft::hex_32([0x41; 32]),
        capabilities: CapabilityConfig {
            collect: true,
            store: true,
            verify: true,
            propose: true,
            archive: false,
        },
        collect: CollectConfig {
            enabled: true,
            default_epoch_id: 1,
            default_slot_id: 1,
        },
        runtime: RuntimeConfig {
            data_dir: data_dir.to_string_lossy().into_owned(),
            poll_interval_secs: 0,
            slots_per_epoch: 2,
            challenge_window_blocks: 20,
            low_impact_mode: false,
            os_background_priority: false,
            game_active_poll_interval_secs: 0,
            game_process_names: Vec::new(),
            target_app_ids: vec![730],
            p2p_simulation: pole_protocol_draft::P2pSimulationConfig::default(),
            p2p_socket: pole_protocol_draft::P2pSocketConfig::default(),
            p2p_libp2p: pole_protocol_draft::P2pLibp2pConfig::default(),
            activity_sources: Vec::new(),
        },
        storage: StorageConfig {
            quota_gb: 1,
            retention_epochs: 2,
        },
        reward: RewardConfig::default(),
    };
    config.save_json(&config_path).unwrap();

    let client = FixedHttpClient;
    let mut progress = LocalNodeProgress::default_from_config(&config);
    run_collect_tick_with_client(&config, &mut progress, &client).unwrap();
    std::fs::create_dir_all(
        pole_protocol_draft::local_chain_runtime_path(&config)
            .parent()
            .unwrap(),
    )
    .unwrap();
    std::fs::write(
        pole_protocol_draft::local_chain_runtime_path(&config),
        "{not-json",
    )
    .unwrap();
    run_collect_tick_with_client(&config, &mut progress, &client).unwrap();

    let binary = env!("CARGO_BIN_EXE_pole-client");
    let status = Command::new(binary)
        .arg("status")
        .arg(&config_path)
        .output()
        .unwrap();
    assert!(
        status.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&status.stdout),
        String::from_utf8_lossy(&status.stderr)
    );
    let status_stdout = String::from_utf8_lossy(&status.stdout);
    assert!(status_stdout.contains("last_auto_settled_epoch=None"));
    assert!(status_stdout.contains("last_auto_settlement_error=Some(\"epoch 1: json error:"));

    let doctor = Command::new(binary)
        .arg("doctor")
        .arg(&config_path)
        .output()
        .unwrap();
    assert!(
        doctor.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&doctor.stdout),
        String::from_utf8_lossy(&doctor.stderr)
    );
    let doctor_stdout = String::from_utf8_lossy(&doctor.stdout);
    assert!(doctor_stdout.contains("last_auto_settled_epoch=None"));
    assert!(doctor_stdout.contains("last_auto_settlement_error=Some(\"epoch 1: json error:"));

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn watch_p2p_sim_reports_attached_network_health() {
    let root = temp_root("watch-p2p-sim");
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    std::fs::create_dir_all(&root).unwrap();

    let config_path = root.join("client.json");
    let data_dir = root.join("pole-node-data");
    let config = NodeConfig {
        chain_id: "pole-local".into(),
        node_id_hex: pole_protocol_draft::hex_32([0x31; 32]),
        reward_address_hex: pole_protocol_draft::hex_32([0x41; 32]),
        capabilities: CapabilityConfig {
            collect: true,
            store: true,
            verify: false,
            propose: false,
            archive: false,
        },
        collect: CollectConfig {
            enabled: true,
            default_epoch_id: 1,
            default_slot_id: 1,
        },
        runtime: RuntimeConfig {
            data_dir: data_dir.to_string_lossy().into_owned(),
            poll_interval_secs: 0,
            slots_per_epoch: 10,
            challenge_window_blocks: 20,
            low_impact_mode: true,
            os_background_priority: true,
            game_active_poll_interval_secs: 900,
            game_process_names: Vec::new(),
            target_app_ids: vec![730],
            p2p_simulation: pole_protocol_draft::P2pSimulationConfig::default(),
            p2p_socket: pole_protocol_draft::P2pSocketConfig::default(),
            p2p_libp2p: pole_protocol_draft::P2pLibp2pConfig::default(),
            activity_sources: Vec::new(),
        },
        storage: StorageConfig {
            quota_gb: 1,
            retention_epochs: 2,
        },
        reward: RewardConfig::default(),
    };
    config.save_json(&config_path).unwrap();

    let binary = env!("CARGO_BIN_EXE_pole-client");
    let output = Command::new(binary)
        .arg("watch-p2p-sim")
        .arg(&config_path)
        .arg("1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("PoLE client watch"));
    assert!(stdout.contains("p2p_simulation_enabled=true"));
    assert!(stdout.contains("ticks_run=1"));
    assert!(stdout.contains("last_p2p_batch_recipients=Some(2)"));
    assert!(stdout.contains("last_p2p_receipt_recipients=Some(2)"));
    assert!(stdout.contains("last_p2p_retrieval_ok=Some(true)"));
    assert!(stdout.contains("last_retention_all_retrievable=Some(true)"));

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn watch_p2p_fs_reports_attached_network_health() {
    let root = temp_root("watch-p2p-fs");
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    std::fs::create_dir_all(&root).unwrap();

    let config_path = root.join("client.json");
    let data_dir = root.join("pole-node-data");
    let config = NodeConfig {
        chain_id: "pole-local".into(),
        node_id_hex: pole_protocol_draft::hex_32([0x31; 32]),
        reward_address_hex: pole_protocol_draft::hex_32([0x41; 32]),
        capabilities: CapabilityConfig {
            collect: true,
            store: true,
            verify: false,
            propose: false,
            archive: false,
        },
        collect: CollectConfig {
            enabled: true,
            default_epoch_id: 1,
            default_slot_id: 1,
        },
        runtime: RuntimeConfig {
            data_dir: data_dir.to_string_lossy().into_owned(),
            poll_interval_secs: 0,
            slots_per_epoch: 10,
            challenge_window_blocks: 20,
            low_impact_mode: true,
            os_background_priority: true,
            game_active_poll_interval_secs: 900,
            game_process_names: Vec::new(),
            target_app_ids: vec![730],
            p2p_simulation: pole_protocol_draft::P2pSimulationConfig::default(),
            p2p_socket: pole_protocol_draft::P2pSocketConfig::default(),
            p2p_libp2p: pole_protocol_draft::P2pLibp2pConfig::default(),
            activity_sources: Vec::new(),
        },
        storage: StorageConfig {
            quota_gb: 1,
            retention_epochs: 2,
        },
        reward: RewardConfig::default(),
    };
    config.save_json(&config_path).unwrap();

    let network_dir = data_dir.join("p2p-fs-network");
    let mut seed = FilesystemP2pNetwork::new(&network_dir);
    seed.bootstrap_peer([0x90; 32], &[P2pTopic::Batches, P2pTopic::Receipts])
        .unwrap();

    let binary = env!("CARGO_BIN_EXE_pole-client");
    let output = Command::new(binary)
        .arg("watch-p2p-fs")
        .arg(&config_path)
        .arg("1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("PoLE client watch"));
    assert!(stdout.contains("p2p_transport=filesystem"));
    assert!(stdout.contains("p2p_simulation_enabled=false"));
    assert!(stdout.contains("ticks_run=1"));
    assert!(stdout.contains("last_p2p_transport=Some(\"filesystem\")"));
    assert!(stdout.contains("last_p2p_known_peer_count=Some(2)"));
    assert!(stdout.contains("last_p2p_learned_remote_peer_count=Some(0)"));
    assert!(stdout.contains("last_p2p_batch_listener_count=Some(1)"));
    assert!(stdout.contains("last_p2p_receipt_listener_count=Some(1)"));
    assert!(stdout.contains("last_p2p_challenge_listener_count=Some(0)"));
    assert!(stdout.contains("last_p2p_coordination_sent_count=Some(0)"));
    assert!(stdout.contains("last_p2p_coordination_received_count=Some(0)"));
    assert!(stdout.contains("last_p2p_hello_sent_count=Some(0)"));
    assert!(stdout.contains("last_p2p_hint_sent_count=Some(0)"));
    assert!(stdout.contains("last_p2p_goodbye_sent_count=Some(0)"));
    assert!(stdout.contains("last_p2p_batch_recipients=Some(1)"));
    assert!(stdout.contains("last_p2p_receipt_recipients=Some(1)"));
    assert!(stdout.contains("last_p2p_retrieval_ok=Some(true)"));
    assert!(stdout.contains("last_retention_all_retrievable=Some(true)"));

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn watch_p2p_socket_reports_attached_network_health() {
    let root = temp_root("watch-p2p-socket");
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    std::fs::create_dir_all(&root).unwrap();

    let config_path = root.join("client.json");
    let data_dir = root.join("pole-node-data");
    let config = NodeConfig {
        chain_id: "pole-local".into(),
        node_id_hex: pole_protocol_draft::hex_32([0x31; 32]),
        reward_address_hex: pole_protocol_draft::hex_32([0x41; 32]),
        capabilities: CapabilityConfig {
            collect: true,
            store: true,
            verify: false,
            propose: false,
            archive: false,
        },
        collect: CollectConfig {
            enabled: true,
            default_epoch_id: 1,
            default_slot_id: 1,
        },
        runtime: RuntimeConfig {
            data_dir: data_dir.to_string_lossy().into_owned(),
            poll_interval_secs: 0,
            slots_per_epoch: 10,
            challenge_window_blocks: 20,
            low_impact_mode: true,
            os_background_priority: true,
            game_active_poll_interval_secs: 900,
            game_process_names: Vec::new(),
            target_app_ids: vec![730],
            p2p_simulation: pole_protocol_draft::P2pSimulationConfig::default(),
            p2p_socket: pole_protocol_draft::P2pSocketConfig::default(),
            p2p_libp2p: pole_protocol_draft::P2pLibp2pConfig::default(),
            activity_sources: Vec::new(),
        },
        storage: StorageConfig {
            quota_gb: 1,
            retention_epochs: 2,
        },
        reward: RewardConfig::default(),
    };
    config.save_json(&config_path).unwrap();

    let sink = [0x77; 32];
    let mut sink_network =
        SocketP2pNetwork::bind(sink, "127.0.0.1:0".parse().unwrap(), vec![]).unwrap();
    sink_network
        .bootstrap_peer(sink, &[P2pTopic::Batches, P2pTopic::Receipts])
        .unwrap();
    let sink_addr: SocketAddr = sink_network.local_addr().unwrap();

    let binary = env!("CARGO_BIN_EXE_pole-client");
    let output = Command::new(binary)
        .env("POLE_CLIENT_SOCKET_BIND_ADDR", "127.0.0.1:0")
        .env(
            "POLE_CLIENT_SOCKET_PEER_SPECS",
            format!(
                "{}@{}@batches,receipts",
                pole_protocol_draft::hex_32(sink),
                sink_addr
            ),
        )
        .arg("watch-p2p-socket")
        .arg(&config_path)
        .arg("1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("PoLE client watch"));
    assert!(stdout.contains("p2p_transport=socket"));
    assert!(stdout.contains("p2p_simulation_enabled=false"));
    assert!(stdout.contains("ticks_run=1"));
    assert!(stdout.contains("last_p2p_transport=Some(\"socket\")"));
    assert!(stdout.contains("last_p2p_known_peer_count=Some(2)"));
    assert!(stdout.contains("last_p2p_learned_remote_peer_count=Some(0)"));
    assert!(stdout.contains("last_p2p_batch_listener_count=Some(2)"));
    assert!(stdout.contains("last_p2p_receipt_listener_count=Some(2)"));
    assert!(stdout.contains("last_p2p_challenge_listener_count=Some(0)"));
    assert!(stdout.contains("last_p2p_coordination_sent_count=Some(1)"));
    assert!(stdout.contains("last_p2p_coordination_received_count=Some(0)"));
    assert!(stdout.contains("last_p2p_hello_sent_count=Some(1)"));
    assert!(stdout.contains("last_p2p_hint_sent_count=Some(0)"));
    assert!(stdout.contains("last_p2p_goodbye_sent_count=Some(0)"));
    assert!(stdout.contains("last_p2p_batch_recipients=Some(1)"));
    assert!(stdout.contains("last_p2p_receipt_recipients=Some(1)"));
    assert!(stdout.contains("last_p2p_retrieval_ok=Some(true)"));
    assert!(stdout.contains("last_retention_all_retrievable=Some(true)"));

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn watch_p2p_socket_can_use_config_backed_peer_specs() {
    let root = temp_root("watch-p2p-socket-config");
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    std::fs::create_dir_all(&root).unwrap();

    let config_path = root.join("client.json");
    let data_dir = root.join("pole-node-data");
    let sink = [0x79; 32];
    let mut sink_network =
        SocketP2pNetwork::bind(sink, "127.0.0.1:0".parse().unwrap(), vec![]).unwrap();
    sink_network
        .bootstrap_peer(sink, &[P2pTopic::Batches, P2pTopic::Receipts])
        .unwrap();
    let sink_addr: SocketAddr = sink_network.local_addr().unwrap();

    let config = NodeConfig {
        chain_id: "pole-local".into(),
        node_id_hex: pole_protocol_draft::hex_32([0x31; 32]),
        reward_address_hex: pole_protocol_draft::hex_32([0x41; 32]),
        capabilities: CapabilityConfig {
            collect: true,
            store: true,
            verify: false,
            propose: false,
            archive: false,
        },
        collect: CollectConfig {
            enabled: true,
            default_epoch_id: 1,
            default_slot_id: 1,
        },
        runtime: RuntimeConfig {
            data_dir: data_dir.to_string_lossy().into_owned(),
            poll_interval_secs: 0,
            slots_per_epoch: 10,
            challenge_window_blocks: 20,
            low_impact_mode: true,
            os_background_priority: true,
            game_active_poll_interval_secs: 900,
            game_process_names: Vec::new(),
            target_app_ids: vec![730],
            p2p_simulation: pole_protocol_draft::P2pSimulationConfig::default(),
            p2p_socket: pole_protocol_draft::P2pSocketConfig {
                bind_addr: "127.0.0.1:0".into(),
                peers: vec![pole_protocol_draft::P2pSocketPeerConfig {
                    peer_id_hex: pole_protocol_draft::hex_32(sink),
                    addr: sink_addr.to_string(),
                    topics: vec!["batches".into(), "receipts".into()],
                }],
            },
            p2p_libp2p: pole_protocol_draft::P2pLibp2pConfig::default(),
            activity_sources: Vec::new(),
        },
        storage: StorageConfig {
            quota_gb: 1,
            retention_epochs: 2,
        },
        reward: RewardConfig::default(),
    };
    config.save_json(&config_path).unwrap();

    let binary = env!("CARGO_BIN_EXE_pole-client");
    let output = Command::new(binary)
        .arg("watch-p2p-socket")
        .arg(&config_path)
        .arg("1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("p2p_transport=socket"));
    assert!(stdout.contains("last_p2p_transport=Some(\"socket\")"));
    assert!(stdout.contains("last_p2p_known_peer_count=Some(2)"));
    assert!(stdout.contains("last_p2p_learned_remote_peer_count=Some(0)"));
    assert!(stdout.contains("last_p2p_coordination_sent_count=Some(1)"));
    assert!(stdout.contains("last_p2p_hello_sent_count=Some(1)"));
    assert!(stdout.contains("last_p2p_hint_sent_count=Some(0)"));
    assert!(stdout.contains("last_p2p_goodbye_sent_count=Some(0)"));
    assert!(stdout.contains("last_p2p_batch_listener_count=Some(2)"));
    assert!(stdout.contains("last_p2p_receipt_listener_count=Some(2)"));
    assert!(stdout.contains("last_p2p_batch_recipients=Some(1)"));
    assert!(stdout.contains("last_p2p_retrieval_ok=Some(true)"));

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn prepare_epoch_builds_local_epoch_artifact() {
    let root = temp_root("prepare-epoch");
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    std::fs::create_dir_all(&root).unwrap();

    let config_path = root.join("client.json");
    let data_dir = root.join("pole-node-data");
    let config = NodeConfig {
        chain_id: "pole-local".into(),
        node_id_hex: pole_protocol_draft::hex_32([0x31; 32]),
        reward_address_hex: pole_protocol_draft::hex_32([0x41; 32]),
        capabilities: CapabilityConfig {
            collect: true,
            store: true,
            verify: true,
            propose: true,
            archive: false,
        },
        collect: CollectConfig {
            enabled: true,
            default_epoch_id: 1,
            default_slot_id: 1,
        },
        runtime: RuntimeConfig {
            data_dir: data_dir.to_string_lossy().into_owned(),
            poll_interval_secs: 0,
            slots_per_epoch: 10,
            challenge_window_blocks: 20,
            low_impact_mode: false,
            os_background_priority: false,
            game_active_poll_interval_secs: 0,
            game_process_names: Vec::new(),
            target_app_ids: vec![730],
            p2p_simulation: pole_protocol_draft::P2pSimulationConfig::default(),
            p2p_socket: pole_protocol_draft::P2pSocketConfig::default(),
            p2p_libp2p: pole_protocol_draft::P2pLibp2pConfig::default(),
            activity_sources: Vec::new(),
        },
        storage: StorageConfig {
            quota_gb: 1,
            retention_epochs: 2,
        },
        reward: RewardConfig::default(),
    };
    config.save_json(&config_path).unwrap();

    let client = FixedHttpClient;
    let mut progress = LocalNodeProgress::default_from_config(&config);
    run_collect_tick_with_client(&config, &mut progress, &client).unwrap();

    let binary = env!("CARGO_BIN_EXE_pole-client");
    let output = Command::new(binary)
        .arg("prepare-epoch")
        .arg(&config_path)
        .arg("1")
        .arg("50")
        .arg("20")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("PoLE client prepare-epoch"));
    assert!(stdout.contains("verification_all_valid=true"));
    assert!(stdout.contains("ready_for_submission=true"));
    assert!(epoch_preparation_artifact_path(&config, 1).exists());

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn settle_epoch_executes_local_submission_finalization_and_claim() {
    let root = temp_root("settle-epoch");
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    std::fs::create_dir_all(&root).unwrap();

    let config_path = root.join("client.json");
    let data_dir = root.join("pole-node-data");
    let config = NodeConfig {
        chain_id: "pole-local".into(),
        node_id_hex: pole_protocol_draft::hex_32([0x31; 32]),
        reward_address_hex: pole_protocol_draft::hex_32([0x41; 32]),
        capabilities: CapabilityConfig {
            collect: true,
            store: true,
            verify: true,
            propose: true,
            archive: false,
        },
        collect: CollectConfig {
            enabled: true,
            default_epoch_id: 1,
            default_slot_id: 1,
        },
        runtime: RuntimeConfig {
            data_dir: data_dir.to_string_lossy().into_owned(),
            poll_interval_secs: 0,
            slots_per_epoch: 10,
            challenge_window_blocks: 20,
            low_impact_mode: false,
            os_background_priority: false,
            game_active_poll_interval_secs: 0,
            game_process_names: Vec::new(),
            target_app_ids: vec![730],
            p2p_simulation: pole_protocol_draft::P2pSimulationConfig::default(),
            p2p_socket: pole_protocol_draft::P2pSocketConfig::default(),
            p2p_libp2p: pole_protocol_draft::P2pLibp2pConfig::default(),
            activity_sources: Vec::new(),
        },
        storage: StorageConfig {
            quota_gb: 1,
            retention_epochs: 2,
        },
        reward: RewardConfig::default(),
    };
    config.save_json(&config_path).unwrap();

    let client = FixedHttpClient;
    let mut progress = LocalNodeProgress::default_from_config(&config);
    run_collect_tick_with_client(&config, &mut progress, &client).unwrap();

    let binary = env!("CARGO_BIN_EXE_pole-client");
    let output = Command::new(binary)
        .arg("settle-epoch")
        .arg(&config_path)
        .arg("1")
        .arg("50")
        .arg("20")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("PoLE client settle-epoch"));
    assert!(stdout.contains("epoch_finalized=true"));
    assert!(stdout.contains("local_reward_claimed=true"));
    assert!(stdout.contains("accepted_batches_root="));
    assert!(stdout.contains("observations_root="));
    assert!(stdout.contains("availability_root="));
    assert!(stdout.contains("aggregates_root="));
    assert!(stdout.contains("rewards_root="));
    assert!(epoch_preparation_artifact_path(&config, 1).exists());
    assert!(epoch_settlement_artifact_path(&config, 1).exists());

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn status_reports_pending_hour_reward_block_progress() {
    let root = temp_root("status-pending-reward-block");
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    std::fs::create_dir_all(&root).unwrap();

    let config_path = root.join("client.json");
    let data_dir = root.join("pole-node-data");
    let mut config = NodeConfig {
        chain_id: "pole-local".into(),
        node_id_hex: pole_protocol_draft::hex_32([0x31; 32]),
        reward_address_hex: pole_protocol_draft::hex_32([0x41; 32]),
        capabilities: CapabilityConfig {
            collect: true,
            store: true,
            verify: false,
            propose: false,
            archive: false,
        },
        collect: CollectConfig {
            enabled: true,
            default_epoch_id: 1,
            default_slot_id: 1,
        },
        runtime: RuntimeConfig {
            data_dir: data_dir.to_string_lossy().into_owned(),
            poll_interval_secs: 300,
            slots_per_epoch: 24,
            challenge_window_blocks: 20,
            low_impact_mode: false,
            os_background_priority: false,
            game_active_poll_interval_secs: 0,
            game_process_names: vec!["cs2.exe".into()],
            target_app_ids: vec![730],
            p2p_simulation: pole_protocol_draft::P2pSimulationConfig::default(),
            p2p_socket: pole_protocol_draft::P2pSocketConfig::default(),
            p2p_libp2p: pole_protocol_draft::P2pLibp2pConfig::default(),
            activity_sources: Vec::new(),
        },
        storage: StorageConfig {
            quota_gb: 1,
            retention_epochs: 2,
        },
        reward: RewardConfig::default(),
    };
    config.reward.target_network_weight_units = 1_800_000_000_000;
    config.reward.reward_adjustment_cap_bps = 2_000;
    config.reward.game_mappings = vec![pole_protocol_draft::RewardGameMapping {
        process_name: "cs2.exe".into(),
        app_id: 730,
        game_coefficient_ppm: 1_000_000,
    }];
    config.save_json(&config_path).unwrap();

    let client = FixedHttpClient;
    let mut progress = LocalNodeProgress::default_from_config(&config);
    std::env::set_var("POLE_CLIENT_FOREGROUND_PROCESS_OVERRIDE", "cs2.exe");
    run_collect_tick_with_client(&config, &mut progress, &client).unwrap();
    run_collect_tick_with_client(&config, &mut progress, &client).unwrap();
    std::env::remove_var("POLE_CLIENT_FOREGROUND_PROCESS_OVERRIDE");

    let binary = env!("CARGO_BIN_EXE_pole-client");
    let output = Command::new(binary)
        .env("POLE_CLIENT_FOREGROUND_PROCESS_OVERRIDE", "cs2.exe")
        .arg("status")
        .arg(&config_path)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("reward_block_secs=3600"));
    assert!(stdout.contains("adjustment_cycle_blocks=288"));
    assert!(stdout.contains("configured_fixed_player_reward=1000"));
    assert!(stdout.contains("effective_fixed_player_reward=1000"));
    assert!(stdout.contains("reward_blocks_completed=0"));
    assert!(stdout.contains("current_fixed_player_reward=1000"));
    assert!(stdout.contains("current_fixed_player_reward_basis_cycle_index=0"));
    assert!(stdout.contains("current_fixed_player_reward_basis_total_network_weight_units=0"));
    assert!(stdout.contains("current_reward_adjustment_artifact_exists=true"));
    assert!(stdout.contains("current_reward_adjustment_artifact_path="));
    assert!(stdout.contains("reward_adjustment_artifact_index_path="));
    assert!(stdout.contains("reward_adjustment_artifact_summary_path="));
    assert!(stdout.contains("adjustment_cycle_artifact_count=1"));
    assert!(stdout.contains("previous_adjustment_cycle_total_network_weight_units=0"));
    assert!(stdout.contains("current_adjustment_cycle_total_network_weight_units=0"));
    assert!(stdout.contains("pending_reward_block_seconds=600"));
    assert!(stdout.contains("pending_reward_block_ticks=2"));
    assert!(stdout.contains("pending_reward_block_remaining_seconds=3000"));
    assert!(stdout.contains("pending_reward_block_entry_count=1"));

    let show_index = Command::new(binary)
        .arg("reward-adjustment-show-index")
        .arg(&config_path)
        .output()
        .unwrap();
    assert!(show_index.status.success());
    let stdout = String::from_utf8_lossy(&show_index.stdout);
    assert!(stdout.contains("adjustment_artifact_count=1"));
    assert!(stdout.contains("adjustment_cycle_artifact_count=1"));
    assert!(stdout.contains("adjustment_artifact period_index=0"));
    assert!(stdout.contains("adjustment_cycle cycle_index=0"));

    let show_summary = Command::new(binary)
        .arg("reward-adjustment-show-summary")
        .arg(&config_path)
        .output()
        .unwrap();
    assert!(show_summary.status.success());
    let stdout = String::from_utf8_lossy(&show_summary.stdout);
    assert!(stdout.contains("adjustment_artifact_count=1"));
    assert!(stdout.contains("adjustment_cycle_artifact_count=1"));
    assert!(stdout.contains("latest_period_index=Some(0)"));
    assert!(stdout.contains("latest_adjustment_cycle_index=Some(0)"));
    assert!(stdout.contains("latest_adjusted_player_block_reward=Some(1000)"));
    assert!(stdout.contains("latest_fixed_player_reward=Some(1000)"));

    let alias_summary = Command::new(binary)
        .arg("adjustment-cycle-show-summary")
        .arg(&config_path)
        .output()
        .unwrap();
    assert!(alias_summary.status.success());
    let stdout = String::from_utf8_lossy(&alias_summary.stdout);
    assert!(stdout.contains("latest_adjustment_cycle_index=Some(0)"));

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn capture_foreground_process_adds_detected_process_to_config() {
    let root = temp_root("capture-foreground-process");
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    std::fs::create_dir_all(&root).unwrap();

    let config_path = root.join("client.json");
    let binary = env!("CARGO_BIN_EXE_pole-client");
    let init = Command::new(binary)
        .arg("init")
        .arg(&config_path)
        .arg("player")
        .output()
        .unwrap();
    assert!(init.status.success());

    let capture = Command::new(binary)
        .env(
            "POLE_CLIENT_FOREGROUND_PROCESS_OVERRIDE",
            "MonsterHunterWilds.exe",
        )
        .arg("capture-foreground-process")
        .arg(&config_path)
        .output()
        .unwrap();
    assert!(
        capture.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&capture.stdout),
        String::from_utf8_lossy(&capture.stderr)
    );

    let config: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&config_path).unwrap()).unwrap();
    assert_eq!(
        config["runtime"]["game_process_names"][0],
        "MonsterHunterWilds.exe"
    );
    assert_eq!(
        config["reward"]["game_mappings"][0]["process_name"],
        "MonsterHunterWilds.exe"
    );

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn set_game_processes_updates_config_for_game_aware_mode() {
    let root = temp_root("set-game-processes");
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    std::fs::create_dir_all(&root).unwrap();

    let config_path = root.join("client.json");
    let binary = env!("CARGO_BIN_EXE_pole-client");
    let init = Command::new(binary)
        .arg("init")
        .arg(&config_path)
        .arg("player")
        .output()
        .unwrap();
    assert!(init.status.success());

    let update = Command::new(binary)
        .arg("set-game-processes")
        .arg(&config_path)
        .arg("cs2.exe")
        .arg("eldenring")
        .output()
        .unwrap();
    assert!(
        update.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&update.stdout),
        String::from_utf8_lossy(&update.stderr)
    );

    let config: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&config_path).unwrap()).unwrap();
    assert_eq!(config["runtime"]["low_impact_mode"], true);
    assert_eq!(config["runtime"]["os_background_priority"], true);
    assert_eq!(config["runtime"]["game_process_names"][0], "cs2.exe");
    assert_eq!(config["runtime"]["game_process_names"][1], "eldenring.exe");
    assert_eq!(config["runtime"]["target_app_ids"][0], 730);
    assert_eq!(config["runtime"]["target_app_ids"][1], 1245620);
    assert_eq!(
        config["reward"]["game_mappings"][0]["process_name"],
        "cs2.exe"
    );
    assert_eq!(config["reward"]["game_mappings"][0]["app_id"], 730);
    assert_eq!(
        config["reward"]["game_mappings"][1]["process_name"],
        "eldenring.exe"
    );
    assert_eq!(config["reward"]["game_mappings"][1]["app_id"], 1245620);
    assert!(root
        .join("pole-node-data")
        .join("recognition-cache.json")
        .exists());

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn status_resolves_relative_data_dir_against_config_directory() {
    let root = temp_root("status-relative-data-dir");
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    let config_dir = root.join("workspace");
    std::fs::create_dir_all(&config_dir).unwrap();

    let config_path = config_dir.join("client.json");
    let relative_data_dir = "./pole-node-data".to_string();
    let absolute_data_dir = config_dir.join("pole-node-data");

    let config = NodeConfig {
        chain_id: "pole-local".into(),
        node_id_hex: pole_protocol_draft::hex_32([0x31; 32]),
        reward_address_hex: pole_protocol_draft::hex_32([0x41; 32]),
        capabilities: CapabilityConfig {
            collect: true,
            store: true,
            verify: false,
            propose: false,
            archive: false,
        },
        collect: CollectConfig {
            enabled: true,
            default_epoch_id: 1,
            default_slot_id: 1,
        },
        runtime: RuntimeConfig {
            data_dir: relative_data_dir.clone(),
            poll_interval_secs: 0,
            slots_per_epoch: 10,
            challenge_window_blocks: 20,
            low_impact_mode: false,
            os_background_priority: false,
            game_active_poll_interval_secs: 0,
            game_process_names: Vec::new(),
            target_app_ids: vec![730],
            p2p_simulation: pole_protocol_draft::P2pSimulationConfig::default(),
            p2p_socket: pole_protocol_draft::P2pSocketConfig::default(),
            p2p_libp2p: pole_protocol_draft::P2pLibp2pConfig::default(),
            activity_sources: Vec::new(),
        },
        storage: StorageConfig {
            quota_gb: 1,
            retention_epochs: 2,
        },
        reward: RewardConfig::default(),
    };
    config.save_json(&config_path).unwrap();

    let mut runtime_config = config.clone();
    runtime_config.runtime.data_dir = absolute_data_dir.to_string_lossy().into_owned();

    let client = FixedHttpClient;
    let mut progress = LocalNodeProgress::default_from_config(&runtime_config);
    run_collect_tick_with_client(&runtime_config, &mut progress, &client).unwrap();

    let binary = env!("CARGO_BIN_EXE_pole-client");
    let status = Command::new(binary)
        .env("POLE_CLIENT_FOREGROUND_PROCESS_OVERRIDE", "powershell.exe")
        .current_dir(std::env::temp_dir())
        .arg("status")
        .arg(&config_path)
        .output()
        .unwrap();

    assert!(
        status.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&status.stdout),
        String::from_utf8_lossy(&status.stderr)
    );

    let stdout = String::from_utf8_lossy(&status.stdout);
    assert!(stdout.contains("ticks_completed=1"));
    assert!(stdout.contains("stored_payloads=1"));
    assert!(stdout.contains(&format!("data_dir={}", absolute_data_dir.to_string_lossy())));
    assert!(stdout.contains("foreground_process=Some(\"powershell.exe\")"));

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn paths_reports_install_layout_directories() {
    let root = temp_root("paths-layout");
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    let config_dir = root.join("workspace");
    std::fs::create_dir_all(&config_dir).unwrap();

    let config_path = config_dir.join("client.json");
    let binary = env!("CARGO_BIN_EXE_pole-client");
    let init = Command::new(binary)
        .arg("init")
        .arg(&config_path)
        .arg("player")
        .output()
        .unwrap();
    assert!(init.status.success());

    let paths = Command::new(binary)
        .arg("paths")
        .arg(&config_path)
        .output()
        .unwrap();
    assert!(
        paths.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&paths.stdout),
        String::from_utf8_lossy(&paths.stderr)
    );

    let stdout = String::from_utf8_lossy(&paths.stdout);
    assert!(stdout.contains(&format!("config_dir={}", config_dir.to_string_lossy())));
    assert!(stdout.contains(&format!(
        "data_dir={}",
        config_dir.join("pole-node-data").to_string_lossy()
    )));
    assert!(stdout.contains(&format!(
        "logs_dir={}",
        config_dir
            .join("pole-node-data")
            .join("logs")
            .to_string_lossy()
    )));
    assert!(stdout.contains(&format!(
        "updates_dir={}",
        config_dir
            .join("pole-node-data")
            .join("updates")
            .to_string_lossy()
    )));

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn control_api_open_starts_dashboard_server_and_uses_browser_opener() {
    let root = temp_root("control-api-open");
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    std::fs::create_dir_all(&root).unwrap();

    let config_path = root.join("client.json");
    let binary = env!("CARGO_BIN_EXE_pole-client");
    let init = Command::new(binary)
        .arg("init")
        .arg(&config_path)
        .arg("player")
        .output()
        .unwrap();
    assert!(init.status.success());

    let opener_log = root.join("opened-url.txt");
    #[cfg(windows)]
    let opener_path = {
        let path = root.join("browser.cmd");
        std::fs::write(
            &path,
            format!(
                "@echo off\r\necho %~1>{}\r\nexit /b 0\r\n",
                opener_log.to_string_lossy()
            ),
        )
        .unwrap();
        path
    };
    #[cfg(not(windows))]
    let opener_path = {
        let path = root.join("browser.sh");
        std::fs::write(
            &path,
            format!(
                "#!/bin/sh\nprintf '%s' \"$1\" > '{}'\n",
                opener_log.to_string_lossy()
            ),
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&path).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&path, perms).unwrap();
        path
    };

    let listener = std::net::TcpListener::bind(free_local_addr()).unwrap();
    let addr = listener.local_addr().unwrap();
    let config_path_for_thread = config_path.clone();
    let handle = thread::spawn(move || {
        serve_control_api(listener, config_path_for_thread, Some(1)).unwrap();
    });

    let output = Command::new(binary)
        .env("POLE_CLIENT_BROWSER_OPENER", &opener_path)
        .arg("control-api-open")
        .arg(&config_path)
        .arg(addr.to_string())
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    wait_for_file(&opener_log);
    let opened_url = std::fs::read_to_string(&opener_log).unwrap();
    assert_eq!(opened_url.trim(), format!("http://{addr}/"));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("control_api_already_running=true"));
    handle.join().unwrap();

    std::fs::remove_dir_all(root).unwrap();
}
