use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::node_config::RewardGameMapping;
use crate::primitives::AppId;

const DEFAULT_GAME_COEFFICIENT_PPM: u32 = 1_000_000;

const KNOWN_STEAM_GAMES: &[(&str, AppId, u32)] = &[
    ("cs2.exe", 730, 1_000_000),
    ("dota2.exe", 570, 1_000_000),
    ("eldenring.exe", 1_245_620, 1_000_000),
    ("r5apex.exe", 1_172_470, 1_000_000),
    ("helldivers2.exe", 553_850, 1_000_000),
    ("tslgame.exe", 578_080, 1_000_000),
];

const KNOWN_EPIC_GAMES: &[(&str, AppId, u32)] = &[
    ("rocketleague.exe", 252_950, 1_000_000),
    ("borderlands3.exe", 397_540, 1_000_000),
    ("control_dx11.exe", 870_780, 1_000_000),
    ("control_dx12.exe", 870_780, 1_000_000),
];

const KNOWN_EA_GAMES: &[(&str, AppId, u32)] = &[
    ("masseffectlauncher.exe", 1_328_670, 1_000_000),
    ("ittakestwo.exe", 1_426_210, 1_000_000),
    ("starwarsjedisurvivor.exe", 1_774_580, 1_000_000),
];

const KNOWN_GOG_GAMES: &[(&str, AppId, u32)] = &[
    ("cyberpunk2077.exe", 1_091_500, 1_000_000),
    ("witcher3.exe", 292_030, 1_000_000),
    ("baldursgate3.exe", 1_080_940, 1_000_000),
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RecognitionCache {
    #[serde(default)]
    pub entries: Vec<RecognitionCacheEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecognitionCacheEntry {
    pub process_name: String,
    pub app_id: AppId,
    pub game_coefficient_ppm: u32,
}

pub fn infer_reward_game_mapping(process_name: &str) -> Option<RewardGameMapping> {
    let process_name = canonical_process_name(process_name);
    infer_reward_game_mapping_from_roots(&process_name, &discover_steam_library_roots())
}

pub fn infer_reward_game_mapping_from_roots(
    process_name: &str,
    roots: &[PathBuf],
) -> Option<RewardGameMapping> {
    let process_name = canonical_process_name(process_name);
    if process_name.is_empty() {
        return None;
    }

    if let Some(mapping) = known_catalog_mapping(&process_name) {
        return Some(mapping);
    }

    scan_steam_library_roots(&process_name, roots)
}

pub fn canonical_process_name(input: &str) -> String {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let base = if trimmed.len() >= 4 && trimmed[trimmed.len() - 4..].eq_ignore_ascii_case(".exe") {
        &trimmed[..trimmed.len() - 4]
    } else {
        trimmed
    };
    format!("{base}.exe")
}

pub fn recognition_cache_path(data_dir: &Path) -> PathBuf {
    data_dir.join("recognition-cache.json")
}

pub fn load_cached_reward_game_mapping(
    cache_path: &Path,
    process_name: &str,
) -> Option<RewardGameMapping> {
    let cache = RecognitionCache::load_json(cache_path).ok()?;
    let normalized = normalize_process_name(process_name);
    cache.entries.into_iter().find_map(|entry| {
        (normalize_process_name(&entry.process_name) == normalized).then(|| RewardGameMapping {
            process_name: canonical_process_name(&entry.process_name),
            app_id: entry.app_id,
            game_coefficient_ppm: entry.game_coefficient_ppm,
        })
    })
}

pub fn store_cached_reward_game_mapping(
    cache_path: &Path,
    mapping: &RewardGameMapping,
) -> Result<(), io::Error> {
    let mut cache = RecognitionCache::load_json(cache_path).unwrap_or_default();
    let normalized = normalize_process_name(&mapping.process_name);
    cache
        .entries
        .retain(|entry| normalize_process_name(&entry.process_name) != normalized);
    cache.entries.push(RecognitionCacheEntry {
        process_name: canonical_process_name(&mapping.process_name),
        app_id: mapping.app_id,
        game_coefficient_ppm: mapping.game_coefficient_ppm,
    });
    cache
        .entries
        .sort_by(|left, right| left.process_name.cmp(&right.process_name));
    cache.save_json(cache_path)
}

impl RecognitionCache {
    pub fn load_json(path: &Path) -> Result<Self, io::Error> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = fs::read_to_string(path)?;
        serde_json::from_str(&content).map_err(|err| io::Error::other(err.to_string()))
    }

    pub fn save_json(&self, path: &Path) -> Result<(), io::Error> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }
        let content =
            serde_json::to_string_pretty(self).map_err(|err| io::Error::other(err.to_string()))?;
        fs::write(path, content)
    }
}

fn known_catalog_mapping(process_name: &str) -> Option<RewardGameMapping> {
    let normalized = normalize_process_name(process_name);
    KNOWN_STEAM_GAMES
        .iter()
        .chain(KNOWN_EPIC_GAMES.iter())
        .chain(KNOWN_EA_GAMES.iter())
        .chain(KNOWN_GOG_GAMES.iter())
        .find_map(|(name, app_id, coefficient)| {
            (normalize_process_name(name) == normalized).then(|| RewardGameMapping {
                process_name: canonical_process_name(name),
                app_id: *app_id,
                game_coefficient_ppm: *coefficient,
            })
        })
}

fn scan_steam_library_roots(process_name: &str, roots: &[PathBuf]) -> Option<RewardGameMapping> {
    let normalized = normalize_process_name(process_name);
    for root in roots {
        let steamapps_dir = root.join("steamapps");
        if !steamapps_dir.exists() {
            continue;
        }

        let mut manifests = match fs::read_dir(&steamapps_dir) {
            Ok(entries) => match entries.collect::<Result<Vec<_>, _>>() {
                Ok(entries) => entries,
                Err(_) => continue,
            },
            Err(_) => continue,
        };
        manifests.sort_by_key(|entry| entry.path());

        for manifest in manifests {
            let path = manifest.path();
            if !path.is_file() {
                continue;
            }
            let file_name = path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("");
            if !file_name.starts_with("appmanifest_") || !file_name.ends_with(".acf") {
                continue;
            }

            let Some(app_id) = parse_app_id_from_manifest_name(file_name) else {
                continue;
            };
            let Ok(manifest_text) = fs::read_to_string(&path) else {
                continue;
            };
            let Some(install_dir_name) = parse_manifest_string_field(&manifest_text, "installdir")
            else {
                continue;
            };
            let install_dir = steamapps_dir.join("common").join(install_dir_name);
            if process_exists_in_install_dir(&install_dir, &normalized) {
                return Some(RewardGameMapping {
                    process_name: process_name.to_string(),
                    app_id,
                    game_coefficient_ppm: DEFAULT_GAME_COEFFICIENT_PPM,
                });
            }
        }
    }

    None
}

fn parse_app_id_from_manifest_name(file_name: &str) -> Option<AppId> {
    file_name
        .strip_prefix("appmanifest_")?
        .strip_suffix(".acf")?
        .parse::<AppId>()
        .ok()
}

fn parse_manifest_string_field(content: &str, field: &str) -> Option<String> {
    content.lines().find_map(|line| {
        let tokens = quoted_tokens(line);
        if tokens.len() >= 2 && tokens[0].eq_ignore_ascii_case(field) {
            Some(tokens[1].replace("\\\\", "\\"))
        } else {
            None
        }
    })
}

fn quoted_tokens(line: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut in_quote = false;
    let mut current = String::new();

    for ch in line.chars() {
        if in_quote {
            if ch == '"' {
                tokens.push(current.clone());
                current.clear();
                in_quote = false;
            } else {
                current.push(ch);
            }
        } else if ch == '"' {
            in_quote = true;
        }
    }

    tokens
}

fn process_exists_in_install_dir(install_dir: &Path, normalized_process_name: &str) -> bool {
    if !install_dir.exists() {
        return false;
    }

    let mut stack = vec![(install_dir.to_path_buf(), 0usize)];
    while let Some((dir, depth)) = stack.pop() {
        if depth > 3 {
            continue;
        }

        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(_) => continue,
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push((path, depth + 1));
                continue;
            }

            let file_name = match path.file_name().and_then(|value| value.to_str()) {
                Some(value) => value,
                None => continue,
            };
            if normalize_process_name(file_name) == normalized_process_name {
                return true;
            }
        }
    }

    false
}

fn discover_steam_library_roots() -> Vec<PathBuf> {
    let mut roots = BTreeSet::new();

    if let Ok(override_value) = env::var("POLE_STEAM_ROOTS") {
        for item in override_value.split(';') {
            let trimmed = item.trim();
            if !trimmed.is_empty() {
                roots.insert(PathBuf::from(trimmed));
            }
        }
    }

    if let Some(program_files_x86) = env::var_os("ProgramFiles(x86)") {
        roots.insert(PathBuf::from(program_files_x86).join("Steam"));
    }
    if let Some(program_files) = env::var_os("ProgramFiles") {
        roots.insert(PathBuf::from(program_files).join("Steam"));
    }

    let mut discovered = Vec::new();
    for root in roots {
        if !root.join("steamapps").exists() {
            continue;
        }
        discovered.push(root.clone());
        let extra_libraries = parse_libraryfolders(&root);
        for library in extra_libraries {
            discovered.push(library);
        }
    }

    let mut deduped = Vec::new();
    let mut seen = BTreeSet::new();
    for root in discovered {
        let normalized = normalize_root(&root);
        if seen.insert(normalized) {
            deduped.push(root);
        }
    }
    deduped
}

fn parse_libraryfolders(root: &Path) -> Vec<PathBuf> {
    let path = root.join("steamapps").join("libraryfolders.vdf");
    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(_) => return Vec::new(),
    };

    let mut libraries = Vec::new();
    for line in content.lines() {
        let tokens = quoted_tokens(line);
        if tokens.len() >= 2 && tokens[0].eq_ignore_ascii_case("path") {
            libraries.push(PathBuf::from(tokens[1].replace("\\\\", "\\")));
        }
    }
    libraries
}

fn normalize_root(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "/")
        .to_ascii_lowercase()
}

fn normalize_process_name(input: &str) -> String {
    input
        .trim()
        .to_ascii_lowercase()
        .trim_end_matches(".exe")
        .to_string()
}
