use std::path::PathBuf;

use pole_protocol_draft::{
    canonical_process_name as canonical_game_process_name, infer_reward_game_mapping_from_roots,
    load_cached_reward_game_mapping, recognition_cache_path, store_cached_reward_game_mapping,
    RewardGameMapping,
};

fn temp_root(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("pole-steam-dir-{name}-{}", std::process::id()))
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
