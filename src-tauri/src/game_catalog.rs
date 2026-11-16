use serde::Deserialize;
use std::{
    collections::HashSet,
    fs,
    path::{Component, Path, PathBuf},
};
use winreg::{
    enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_32KEY, KEY_WOW64_64KEY},
    RegKey,
};

#[derive(Clone)]
pub struct DetectedGame {
    pub id: String,
    pub name: String,
    pub source: &'static str,
    pub path: PathBuf,
}

struct Installed {
    id: String,
    name: String,
    source: &'static str,
    root: PathBuf,
    relative: Option<PathBuf>,
}

impl Installed {
    fn game(&self, path: PathBuf) -> DetectedGame {
        DetectedGame {
            id: self.id.clone(),
            name: self.name.clone(),
            source: self.source,
            path,
        }
    }
}

#[derive(Default)]
pub struct Catalog {
    installed: Vec<Installed>,
}

pub fn path_key(path: &Path) -> String {
    let value = path.to_string_lossy().replace('/', "\\");
    value.strip_prefix(r"\\?\").unwrap_or(&value).to_lowercase()
}

fn normal_relative(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && path
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
}

fn safe_read(path: &Path) -> Option<String> {
    let file = fs::File::open(path).ok()?;
    if file.metadata().ok()?.len() > 16 * 1024 * 1024 {
        return None;
    }
    use std::io::Read;
    let mut text = String::new();
    file.take(16 * 1024 * 1024).read_to_string(&mut text).ok()?;
    Some(text)
}

fn manifest_files(root: &Path, extension: &'static str) -> impl Iterator<Item = PathBuf> {
    fs::read_dir(root)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .map(|item| item.path())
        .filter(move |path| path.extension().and_then(|part| part.to_str()) == Some(extension))
}

fn vdf_pairs(input: &str) -> Vec<(String, String)> {
    let mut chars = input.chars().peekable();
    let mut key = None;
    let mut result = Vec::new();
    while let Some(ch) = chars.next() {
        match ch {
            '{' | '}' => {
                key = None;
            }
            '/' if chars.peek() == Some(&'/') => {
                for next in chars.by_ref() {
                    if next == '\n' {
                        break;
                    }
                }
            }
            '"' => {
                let mut token = String::new();
                while let Some(next) = chars.next() {
                    match next {
                        '"' => break,
                        '\\' if matches!(chars.peek(), Some('\\' | '"')) => {
                            token.push(chars.next().unwrap())
                        }
                        other => token.push(other),
                    }
                }
                if let Some(previous) = key.take() {
                    result.push((previous, token));
                } else {
                    key = Some(token);
                }
            }
            _ => {}
        }
    }
    result
}

fn field<'a>(pairs: &'a [(String, String)], name: &str) -> Option<&'a str> {
    pairs
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.as_str())
}

pub(super) fn steam_root() -> Option<PathBuf> {
    let user = RegKey::predef(HKEY_CURRENT_USER);
    let configured = user
        .open_subkey(r"Software\Valve\Steam")
        .ok()
        .and_then(|key| key.get_value::<String, _>("SteamPath").ok())
        .map(PathBuf::from);
    let default =
        std::env::var_os("ProgramFiles(x86)").map(|path| PathBuf::from(path).join("Steam"));
    configured.or(default)
}

fn steam(installed: &mut Vec<Installed>) {
    let Some(steam) = steam_root() else {
        return;
    };
    let mut libraries = vec![steam.clone()];
    if let Some(text) = safe_read(&steam.join("steamapps/libraryfolders.vdf")) {
        libraries.extend(
            vdf_pairs(&text)
                .into_iter()
                .filter(|(key, _)| key == "path")
                .map(|(_, value)| PathBuf::from(value)),
        );
    }
    let mut seen = HashSet::new();
    for library in libraries
        .into_iter()
        .filter(|path| path.is_absolute() && seen.insert(path_key(path)))
    {
        let apps = library.join("steamapps");
        for path in manifest_files(&apps, "acf") {
            if !path
                .file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with("appmanifest_"))
            {
                continue;
            }
            let Some(text) = safe_read(&path) else {
                continue;
            };
            let pairs = vdf_pairs(&text);
            let (Some(id), Some(name), Some(dir)) = (
                field(&pairs, "appid"),
                field(&pairs, "name"),
                field(&pairs, "installdir"),
            ) else {
                continue;
            };
            if id.parse::<u64>().is_err()
                || !normal_relative(Path::new(dir))
                || name.is_empty()
                || name.to_lowercase().contains("dedicated server")
                || name == "Steamworks Common Redistributables"
            {
                continue;
            }
            installed.push(Installed {
                id: format!("steam:{id}"),
                name: name.into(),
                source: "Steam",
                root: apps.join("common").join(dir),
                relative: None,
            });
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct EpicItem {
    app_name: String,
    display_name: String,
    install_location: PathBuf,
    launch_executable: PathBuf,
    #[serde(rename = "bIsIncompleteInstall", default)]
    incomplete: bool,
    #[serde(default)]
    main_game_app_name: String,
}

fn epic(installed: &mut Vec<Installed>, program_data: &Path) {
    for file in manifest_files(
        &program_data.join("Epic/EpicGamesLauncher/Data/Manifests"),
        "item",
    ) {
        let Some(item) =
            safe_read(&file).and_then(|text| serde_json::from_str::<EpicItem>(&text).ok())
        else {
            continue;
        };
        if item.incomplete
            || item.display_name.is_empty()
            || item.app_name.is_empty()
            || (!item.main_game_app_name.is_empty() && item.main_game_app_name != item.app_name)
            || !normal_relative(&item.launch_executable)
        {
            continue;
        }
        installed.push(Installed {
            id: format!("epic:{}", item.app_name),
            name: item.display_name,
            source: "Epic Games",
            root: item.install_location,
            relative: Some(item.launch_executable),
        });
    }
}

fn riot(installed: &mut Vec<Installed>, program_data: &Path) {
    for (product, name, executable) in [
        (
            "league_of_legends.live",
            "League of Legends",
            "Game/League of Legends.exe",
        ),
        (
            "valorant.live",
            "VALORANT",
            "ShooterGame/Binaries/Win64/VALORANT-Win64-Shipping.exe",
        ),
    ] {
        let metadata = program_data.join("Riot Games/Metadata").join(product);
        let configured = safe_read(&metadata.join(format!("{product}.product_settings.yaml")))
            .and_then(|text| {
                text.lines().find_map(|line| {
                    let (key, value) = line.trim().split_once(':')?;
                    (key == "product_install_full_path")
                        .then(|| PathBuf::from(value.trim().trim_matches(['\'', '"'])))
                })
            });
        let default = PathBuf::from(r"C:\Riot Games").join(if product.starts_with("valorant") {
            "VALORANT/live"
        } else {
            name
        });
        if let Some(root) = configured
            .into_iter()
            .chain([default])
            .find(|root| root.join(executable).is_file())
        {
            installed.push(Installed {
                id: format!("riot:{product}"),
                name: name.into(),
                source: "Riot Games",
                root,
                relative: Some(executable.into()),
            });
        }
    }
}

fn registry_games(installed: &mut Vec<Installed>) {
    let machine = RegKey::predef(HKEY_LOCAL_MACHINE);
    for view in [KEY_WOW64_32KEY, KEY_WOW64_64KEY] {
        for (branch, path_name, source, prefix) in [
            (r"SOFTWARE\GOG.com\Games", "path", "GOG", "gog"),
            (
                r"SOFTWARE\Ubisoft\Launcher\Installs",
                "InstallDir",
                "Ubisoft Connect",
                "ubisoft",
            ),
        ] {
            let Ok(games) = machine.open_subkey_with_flags(branch, KEY_READ | view) else {
                continue;
            };
            for id in games.enum_keys().flatten() {
                let Ok(game) = games.open_subkey(&id) else {
                    continue;
                };
                let Ok(root) = game.get_value::<String, _>(path_name).map(PathBuf::from) else {
                    continue;
                };
                let name = game.get_value::<String, _>("gameName").ok().or_else(|| {
                    root.file_name()
                        .map(|name| name.to_string_lossy().into_owned())
                });
                let Some(name) = name else { continue };
                let relative = game
                    .get_value::<String, _>("exe")
                    .ok()
                    .map(PathBuf::from)
                    .and_then(|path| {
                        if path.is_absolute() {
                            path.strip_prefix(&root).ok().map(Path::to_owned)
                        } else {
                            Some(path)
                        }
                    })
                    .filter(|path| normal_relative(path));
                installed.push(Installed {
                    id: format!("{prefix}:{id}"),
                    name,
                    source,
                    root,
                    relative,
                });
            }
        }
    }
}

fn gameplay_binary(path: &Path) -> bool {
    let Some(stem) = path.file_stem().and_then(|name| name.to_str()) else {
        return false;
    };
    let stem = stem.to_ascii_lowercase();
    path.extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("exe"))
        && ![
            "launcher",
            "setup",
            "unins",
            "crash",
            "anti-cheat",
            "anticheat",
            "webhelper",
            "bootstrap",
            "updater",
            "redist",
            "dedicated",
            "helper",
            "cefsubprocess",
            "reporter",
            "prereq",
            "-server",
            "_server",
            "editor",
            "benchmark",
            "config",
            "trainer",
            "installer",
        ]
        .iter()
        .any(|part| stem.contains(part))
        && ![
            "steam",
            "leagueclient",
            "leagueclientux",
            "riotclientservices",
            "riotclientux",
            "epicgameslauncher",
            "galaxyclient",
            "ubisoftconnect",
            "upc",
            "eadesktop",
            "battle.net",
            "agent",
            "rundll32",
            "java",
            "javaw",
        ]
        .contains(&stem.as_str())
}

fn engine_game(path: &Path) -> Option<String> {
    let stem = path.file_stem()?.to_str()?;
    let lower = stem.to_ascii_lowercase();
    let folder = path.parent()?;
    if let Some(name) = lower.strip_suffix("-win64-shipping").filter(|_| {
        folder.ends_with("Binaries/Win64") || path_key(folder).ends_with(r"\binaries\win64")
    }) {
        return Some(stem[..name.len()].to_owned());
    }
    let unity = folder.join("UnityPlayer.dll").is_file()
        && folder
            .join(format!("{stem}_Data"))
            .join("globalgamemanagers")
            .is_file();
    unity.then(|| stem.to_owned())
}

fn title_key(title: &str) -> String {
    title
        .chars()
        .filter(|ch| ch.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn probable_target(item: &Installed) -> Option<PathBuf> {
    let title = title_key(&item.name);
    let mut roots = vec![item.root.clone(), item.root.join("bin/win64")];
    let children = fs::read_dir(&item.root)
        .ok()?
        .filter_map(Result::ok)
        .take(64);
    roots.extend(
        children
            .filter(|child| child.file_type().is_ok_and(|kind| kind.is_dir()))
            .map(|child| child.path().join("Binaries/Win64")),
    );
    let mut candidates = roots
        .iter()
        .flat_map(|root| manifest_files(root, "exe").take(32))
        .filter(|path| {
            gameplay_binary(path)
                && (engine_game(path).is_some()
                    || title.len() >= 4
                        && path.parent() == Some(item.root.as_path())
                        && path
                            .file_stem()
                            .and_then(|stem| stem.to_str())
                            .is_some_and(|stem| title_key(stem) == title))
        });
    let candidate = candidates.next()?;
    candidates.next().is_none().then_some(candidate)
}

impl Catalog {
    pub fn discover() -> Self {
        let mut installed = Vec::new();
        steam(&mut installed);
        registry_games(&mut installed);
        if let Some(program_data) = std::env::var_os("ProgramData").map(PathBuf::from) {
            epic(&mut installed, &program_data);
            riot(&mut installed, &program_data);
        }
        let mut seen = HashSet::new();
        installed.retain_mut(|item| {
            if !item.root.is_absolute() || !item.root.is_dir() {
                return false;
            }
            let Ok(root) = fs::canonicalize(&item.root) else {
                return false;
            };
            if root.parent().is_none() {
                return false;
            }
            item.root = root;
            seen.insert((item.id.clone(), path_key(&item.root)))
        });
        installed.sort_by_key(|item| std::cmp::Reverse(item.root.components().count()));
        Self { installed }
    }

    pub fn launch_targets(&self) -> Vec<DetectedGame> {
        self.installed
            .iter()
            .filter_map(|item| {
                let candidate = item
                    .relative
                    .as_ref()
                    .map(|path| item.root.join(path))
                    .filter(|path| gameplay_binary(path) && path.is_file())
                    .or_else(|| probable_target(item))?;
                let path = fs::canonicalize(candidate).ok()?;
                (under(&path, &item.root) && path.is_file() && gameplay_binary(&path))
                    .then(|| item.game(path))
            })
            .collect()
    }

    pub fn identify(&self, path: &Path, foreground: bool) -> Option<DetectedGame> {
        if !gameplay_binary(path) {
            return None;
        }
        let engine = engine_game(path);
        if let Some(item) = self.installed.iter().find(|item| under(path, &item.root)) {
            let explicit = item
                .relative
                .as_ref()
                .is_some_and(|relative| path_key(&item.root.join(relative)) == path_key(path));
            if explicit || engine.is_some() || (foreground && item.source != "Riot Games") {
                return Some(item.game(path.to_owned()));
            }
        }
        let name = engine?;
        Some(DetectedGame {
            id: format!("local:{}", path_key(path)),
            name,
            source: "Напрямую",
            path: path.to_owned(),
        })
    }
}

fn under(path: &Path, root: &Path) -> bool {
    let path = PathBuf::from(path_key(path));
    path.starts_with(path_key(root)) && path != PathBuf::from(path_key(root))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_steam_libraries_and_ignores_helpers() {
        let pairs = vdf_pairs(
            r#""libraryfolders" { "0" { "path" "D:\\SteamLibrary" // comment
            "apps" { "123" "42" } } }"#,
        );
        assert_eq!(field(&pairs, "path"), Some(r"D:\SteamLibrary"));
        assert!(!gameplay_binary(Path::new("EpicGamesLauncher.exe")));
        assert!(!gameplay_binary(Path::new("LeagueClientUx.exe")));
        assert!(gameplay_binary(Path::new("League of Legends.exe")));
    }
}
