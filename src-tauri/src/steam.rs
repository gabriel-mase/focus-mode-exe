use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Game {
    pub app_id: String,
    pub name: String,
    pub install_dir: String,
    pub exe_name: Option<String>,
    pub exe_path: Option<String>,
    #[serde(default)]
    pub is_custom: bool,
}

pub fn find_steam_games() -> Vec<Game> {
    let steam_path = match find_steam_path() {
        Some(p) => p,
        None => return vec![],
    };

    // Start with the primary Steam path; VDF also lists it as entry "0",
    // so dedup after collecting to avoid scanning it twice.
    let mut library_paths = vec![steam_path.clone()];

    let vdf_path = steam_path.join("steamapps").join("libraryfolders.vdf");
    if vdf_path.exists() {
        library_paths.extend(parse_library_folders(&vdf_path));
    }

    // Dedup while preserving order.
    // Normalize: lowercase + forward→back slashes + strip trailing slash,
    // because the registry path uses '/' while the VDF uses '\'.
    let mut seen: HashSet<String> = HashSet::new();
    library_paths.retain(|p| {
        let key = p.to_string_lossy()
            .to_lowercase()
            .replace('/', "\\")
            .trim_end_matches('\\')
            .to_string();
        seen.insert(key)
    });

    let mut games = Vec::new();

    for lib_path in &library_paths {
        let steamapps = lib_path.join("steamapps");
        if !steamapps.exists() {
            continue;
        }

        let entries = match fs::read_dir(&steamapps) {
            Ok(e) => e,
            Err(_) => continue,
        };

        for entry in entries.flatten() {
            let path = entry.path();
            let fname = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("");

            if fname.starts_with("appmanifest_") && fname.ends_with(".acf") {
                if let Some(game) = parse_acf(&path, &steamapps) {
                    games.push(game);
                }
            }
        }
    }

    games.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    games
}

fn find_steam_path() -> Option<PathBuf> {
    // 1. Windows Registry — Steam always writes its install path here.
    #[cfg(windows)]
    {
        use winreg::enums::*;
        use winreg::RegKey;

        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        if let Ok(key) = hkcu.open_subkey(r"Software\Valve\Steam") {
            if let Ok(path_str) = key.get_value::<String, _>("SteamPath") {
                let p = PathBuf::from(path_str);
                if p.exists() {
                    return Some(p);
                }
            }
        }

        // HKLM fallback (some installs register here instead)
        let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
        for subkey in &[
            r"SOFTWARE\Valve\Steam",
            r"SOFTWARE\WOW6432Node\Valve\Steam",
        ] {
            if let Ok(key) = hklm.open_subkey(subkey) {
                if let Ok(path_str) = key.get_value::<String, _>("InstallPath") {
                    let p = PathBuf::from(path_str);
                    if p.exists() {
                        return Some(p);
                    }
                }
            }
        }
    }

    // 2. Fallback: enumerate all available drive letters dynamically.
    let drive_letters = get_available_drives();
    let common_steam_dirs = ["Steam", "SteamLibrary"];

    for drive in &drive_letters {
        for dir in &common_steam_dirs {
            let p = PathBuf::from(format!(r"{}:\{}", drive, dir));
            if p.exists() {
                return Some(p);
            }
        }
        // Also check Program Files on each drive
        for pf in &["Program Files (x86)", "Program Files"] {
            let p = PathBuf::from(format!(r"{}:\{}\Steam", drive, pf));
            if p.exists() {
                return Some(p);
            }
        }
    }

    None
}

#[cfg(windows)]
fn get_available_drives() -> Vec<char> {
    use winapi::um::fileapi::GetLogicalDrives;
    let mut drives = Vec::new();
    let mask = unsafe { GetLogicalDrives() };
    for i in 0u32..26 {
        if mask & (1 << i) != 0 {
            drives.push((b'A' + i as u8) as char);
        }
    }
    drives
}

#[cfg(not(windows))]
fn get_available_drives() -> Vec<char> {
    vec![]
}

fn parse_library_folders(vdf_path: &Path) -> Vec<PathBuf> {
    let content = match fs::read_to_string(vdf_path) {
        Ok(c) => c,
        Err(_) => return vec![],
    };

    let mut paths = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if let Some((key, value)) = parse_kv_line(trimmed) {
            if key == "path" && !value.is_empty() {
                // VDF stores paths with escaped backslashes: \\ → \
                let path_str = value.replace(r"\\", r"\");
                let p = PathBuf::from(&path_str);
                if p.exists() {
                    paths.push(p);
                }
            }
        }
    }

    paths
}

/// Known non-game Steam app IDs to always filter out.
const NON_GAME_APP_IDS: &[&str] = &[
    "431960", // Wallpaper Engine
    "1420170", // Wallpaper Engine (beta)
    "413080", // 3DMark
    "228980", // Steamworks Common Redistributables (also caught by name)
];

/// Returns false for Steam tool/redistributable entries that aren't real games.
fn is_real_game(name: &str, app_id: &str) -> bool {
    if NON_GAME_APP_IDS.contains(&app_id) {
        return false;
    }
    let lower = name.to_lowercase();
    !lower.contains("redistributable")
        && !lower.contains("dedicated server")
        && !lower.contains("linux runtime")
        && !lower.contains("steamworks sdk")
        && !lower.contains("proton experimental")
        && !lower.eq("steam")
}

fn parse_acf(acf_path: &Path, steamapps_dir: &Path) -> Option<Game> {
    let content = fs::read_to_string(acf_path).ok()?;

    let mut app_id = String::new();
    let mut name = String::new();
    let mut install_dir = String::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if let Some((key, value)) = parse_kv_line(trimmed) {
            match key {
                "appid" => app_id = value.to_string(),
                "name" => name = value.to_string(),
                "installdir" => install_dir = value.to_string(),
                _ => {}
            }
        }
    }

    if app_id.is_empty() || name.is_empty() || install_dir.is_empty() {
        return None;
    }

    if !is_real_game(&name, &app_id) {
        return None;
    }

    let full_install_path = steamapps_dir.join("common").join(&install_dir);

    let (exe_name, exe_path) = match find_main_exe(&full_install_path) {
        Some(p) => {
            let n = p
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();
            let path_str = p.to_string_lossy().to_string();
            (Some(n), Some(path_str))
        }
        None => (None, None),
    };

    Some(Game {
        app_id,
        name,
        install_dir,
        exe_name,
        exe_path,
        is_custom: false,
    })
}

fn parse_kv_line(line: &str) -> Option<(&str, &str)> {
    if !line.starts_with('"') {
        return None;
    }

    let rest = &line[1..];
    let key_end = rest.find('"')?;
    let key = &rest[..key_end];

    let after_key = &rest[key_end + 1..];
    let value_start = after_key.find('"')?;
    let after_open = &after_key[value_start + 1..];
    let value_end = after_open.find('"')?;
    let value = &after_open[..value_end];

    Some((key, value))
}

fn find_main_exe(install_dir: &Path) -> Option<PathBuf> {
    if !install_dir.exists() {
        return None;
    }

    // (depth, size, path) — depth relative to install_dir root
    let mut candidates: Vec<(u32, u64, PathBuf)> = Vec::new();
    collect_exe_candidates(install_dir, &mut candidates, 0);

    if candidates.is_empty() {
        return None;
    }

    // Primary: shallowest depth (closer to root = more likely the real game exe).
    // Secondary: largest size (filters out tiny helper exes at the same level).
    // Example: CS2 has cs2.exe AND vconsole2.exe at the same depth;
    // vconsole2 is larger but "vconsole" is in the skip list so it never reaches here.
    candidates.sort_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)));

    Some(candidates.into_iter().next()?.2)
}

fn collect_exe_candidates(dir: &Path, candidates: &mut Vec<(u32, u64, PathBuf)>, depth: u32) {
    // Scan up to 5 levels deep (0..=4) to handle:
    //   cs2.exe        → game/bin/win64/          (depth 3)
    //   UE5 games      → GameName/Binaries/Win64/ (depth 3)
    //   simple games   → game.exe at root         (depth 0)
    if depth > 4 {
        return;
    }

    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let name_lower = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_lowercase();

        if path.is_dir() {
            let skip_dirs = [
                "redist",
                "directx",
                "vcredist",
                "support",
                "installer",
                "_commonredist",
                "commonredist",
                "__installer",
                "crashpad",
                "easyanticheat",
                "battleye",
            ];
            if !skip_dirs.iter().any(|s| name_lower.contains(s)) {
                collect_exe_candidates(&path, candidates, depth + 1);
            }
        } else if name_lower.ends_with(".exe") {
            let skip_exes = [
                "unins",
                "setup",
                "install",
                "crashreport",
                "crash_report",
                "crashpad",
                "dotnet",
                "vcredist",
                "vc_redist",
                "dxsetup",
                "oalinst",
                "physxupdater",
                "ue4prereq",
                "directx",
                "redist",
                // Valve developer tools (present in CS2, Dota 2, etc.)
                "vconsole",
                // Generic developer/editor tools
                "editor",
                "dedicated",
            ];
            if !skip_exes.iter().any(|s| name_lower.contains(s)) {
                if let Ok(metadata) = entry.metadata() {
                    candidates.push((depth, metadata.len(), path));
                }
            }
        }
    }
}
