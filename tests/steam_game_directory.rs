use std::path::PathBuf;

use pole_protocol_draft::{
    canonical_process_name as canonical_game_process_name, infer_reward_game_mapping_from_roots,
    load_cached_reward_game_mapping, recognition_cache_path, store_cached_reward_game_mapping,
    RewardGameMapping,
};

static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn temp_root(name: &str) -> PathBuf {
    let id = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("tmp");
    let _ = std::fs::create_dir_all(&base);
    base.join(format!(
        "pole-steam-dir-{name}-{}-{id}-{nanos}",
        std::process::id()
    ))
}

#[test]
fn built_in_catalog_maps_common_process_to_app_id() {
    let mapping = infer_reward_game_mapping_from_roots("cs2.exe", &[]).unwrap();
    assert_eq!(mapping.process_name, "cs2.exe");
    assert_eq!(mapping.app_id, 730);
    assert_eq!(mapping.game_coefficient_ppm, 1_000_000);
}

#[test]
fn built_in_catalog_maps_epic_ea_and_gog_processes() {
    let epic = infer_reward_game_mapping_from_roots("rocketleague.exe", &[]).unwrap();
    assert_eq!(epic.app_id, 252_950);

    let ea = infer_reward_game_mapping_from_roots("masseffectlauncher.exe", &[]).unwrap();
    assert_eq!(ea.app_id, 1_328_670);

    let gog = infer_reward_game_mapping_from_roots("cyberpunk2077.exe", &[]).unwrap();
    assert_eq!(gog.app_id, 1_091_500);
}

#[test]
fn steam_library_scan_can_resolve_process_to_manifest_app_id() {
    let root = temp_root("manifest-scan");
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }

    let steamapps = root.join("steamapps");
    let install_dir = steamapps.join("common").join("ELDEN RING").join("Game");
    std::fs::create_dir_all(&install_dir).unwrap();
    std::fs::write(
        steamapps.join("appmanifest_1245620.acf"),
        "\"AppState\"\n{\n    \"appid\"    \"1245620\"\n    \"installdir\"    \"ELDEN RING\"\n}\n",
    )
    .unwrap();
    std::fs::write(install_dir.join("eldenring.exe"), b"stub").unwrap();

    let mapping =
        infer_reward_game_mapping_from_roots("eldenring.exe", std::slice::from_ref(&root)).unwrap();
    assert_eq!(mapping.process_name, "eldenring.exe");
    assert_eq!(mapping.app_id, 1_245_620);

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn canonical_process_name_appends_exe_suffix() {
    assert_eq!(canonical_game_process_name("dota2"), "dota2.exe");
    assert_eq!(canonical_game_process_name("CS2.EXE"), "CS2.exe");
}

#[test]
fn recognition_cache_roundtrip_returns_cached_mapping() {
    let root = temp_root("cache-roundtrip");
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    std::fs::create_dir_all(&root).unwrap();

    let cache_path = recognition_cache_path(&root);
    let mapping = RewardGameMapping {
        process_name: "Control_DX12.exe".into(),
        app_id: 870_780,
        game_coefficient_ppm: 850_000,
    };
    store_cached_reward_game_mapping(&cache_path, &mapping).unwrap();

    let cached = load_cached_reward_game_mapping(&cache_path, "control_dx12.exe").unwrap();
    assert_eq!(cached.process_name, "Control_DX12.exe");
    assert_eq!(cached.app_id, 870_780);
    assert_eq!(cached.game_coefficient_ppm, 850_000);

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn crash_handlers_and_helpers_are_never_recognized_even_inside_game_dir() {
    let root = temp_root("crash-handler-exclusion");
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }

    let steamapps = root.join("steamapps");
    let install_dir = steamapps.join("common").join("War of Genesis").join("Game");
    std::fs::create_dir_all(&install_dir).unwrap();
    std::fs::write(
        steamapps.join("appmanifest_4891320.acf"),
        "\"AppState\"\n{\n    \"appid\"    \"4891320\"\n    \"installdir\"    \"War of Genesis\"\n}\n",
    )
    .unwrap();
    std::fs::write(install_dir.join("Genesis.exe"), b"game").unwrap();
    std::fs::write(install_dir.join("crashpad_handler.exe"), b"crashpad").unwrap();
    std::fs::write(install_dir.join("UnityCrashHandler64.exe"), b"unitycrash").unwrap();
    std::fs::write(install_dir.join("SteamWebHelper.exe"), b"helper").unwrap();
    std::fs::write(install_dir.join("Antigravity.exe"), b"ide").unwrap();

    // Legitimate game executable is recognized
    let mapping =
        infer_reward_game_mapping_from_roots("Genesis.exe", std::slice::from_ref(&root)).unwrap();
    assert_eq!(mapping.process_name, "Genesis.exe");
    assert_eq!(mapping.app_id, 4_891_320);

    // Helpers and crash daemons are strictly rejected despite physical presence in game dir
    assert!(infer_reward_game_mapping_from_roots(
        "crashpad_handler.exe",
        std::slice::from_ref(&root)
    )
    .is_none());
    assert!(infer_reward_game_mapping_from_roots(
        "UnityCrashHandler64.exe",
        std::slice::from_ref(&root)
    )
    .is_none());
    assert!(infer_reward_game_mapping_from_roots(
        "SteamWebHelper.exe",
        std::slice::from_ref(&root)
    )
    .is_none());
    assert!(
        infer_reward_game_mapping_from_roots("Antigravity.exe", std::slice::from_ref(&root))
            .is_none()
    );

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn recognition_cache_ignores_and_purges_non_game_entries() {
    let root = temp_root("cache-purge");
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    std::fs::create_dir_all(&root).unwrap();

    let cache_path = recognition_cache_path(&root);
    // Write cache with contaminated entries
    let raw_json = r#"{
        "entries": [
            { "process_name": "crashpad_handler.exe", "app_id": 4891320, "game_coefficient_ppm": 1000000 },
            { "process_name": "Genesis.exe", "app_id": 4891320, "game_coefficient_ppm": 1000000 },
            { "process_name": "UnityCrashHandler64.exe", "app_id": 3678970, "game_coefficient_ppm": 1000000 }
        ]
    }"#;
    std::fs::write(&cache_path, raw_json).unwrap();

    // Querying non-game processes from cache returns None
    assert!(load_cached_reward_game_mapping(&cache_path, "crashpad_handler.exe").is_none());
    assert!(load_cached_reward_game_mapping(&cache_path, "UnityCrashHandler64.exe").is_none());

    // Valid game still loads
    let genesis = load_cached_reward_game_mapping(&cache_path, "Genesis.exe").unwrap();
    assert_eq!(genesis.process_name, "Genesis.exe");
    assert_eq!(genesis.app_id, 4_891_320);

    // Storing non-game mapping is ignored
    let bogus = RewardGameMapping {
        process_name: "crashpad_handler.exe".into(),
        app_id: 4891320,
        game_coefficient_ppm: 1_000_000,
    };
    store_cached_reward_game_mapping(&cache_path, &bogus).unwrap();
    assert!(load_cached_reward_game_mapping(&cache_path, "crashpad_handler.exe").is_none());

    std::fs::remove_dir_all(root).unwrap();
}
