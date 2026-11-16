use crate::{model, CommandResult, Engine};
use serde::Serialize;
use std::{sync::Mutex, time::Duration};
use tauri::{AppHandle, Manager};
use tauri_plugin_updater::{Update, UpdaterExt};

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
enum Phase {
    Disabled,
    Idle,
    Checking,
    Current,
    Available,
    Downloading,
    Ready,
    Installing,
}

struct Operation {
    phase: Phase,
    update: Option<Update>,
    package: Option<Vec<u8>>,
    received: u64,
    total: Option<u64>,
    error: Option<String>,
}

pub struct Updates(Mutex<Operation>);

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatus {
    current_version: String,
    auto_update: bool,
    phase: Phase,
    version: Option<String>,
    received: u64,
    total: Option<u64>,
    error: Option<String>,
}

impl Updates {
    pub fn new(app: &AppHandle, isolated: bool) -> Self {
        let configured = app
            .config()
            .plugins
            .0
            .get("updater")
            .and_then(|config| config["pubkey"].as_str())
            .is_some_and(|key| !key.trim().is_empty());
        Self(Mutex::new(Operation {
            phase: if isolated || !configured {
                Phase::Disabled
            } else {
                Phase::Idle
            },
            update: None,
            package: None,
            received: 0,
            total: None,
            error: None,
        }))
    }
}

#[tauri::command]
pub fn update_status(app: AppHandle) -> CommandResult<UpdateStatus> {
    let auto_update = app
        .state::<Mutex<Engine>>()
        .lock()
        .map_err(crate::storage_error)?
        .data
        .auto_update;
    let updates = app.state::<Updates>();
    let operation = updates.0.lock().unwrap();
    Ok(UpdateStatus {
        current_version: app.package_info().version.to_string(),
        auto_update,
        phase: operation.phase,
        version: operation
            .update
            .as_ref()
            .map(|update| update.version.clone()),
        received: operation.received,
        total: operation.total,
        error: operation.error.clone(),
    })
}

#[tauri::command]
pub fn set_auto_update(app: AppHandle, enabled: bool) -> CommandResult<()> {
    app.state::<Mutex<Engine>>()
        .lock()
        .map_err(crate::storage_error)?
        .update(|data| {
            data.auto_update = enabled;
            Ok(())
        })?;
    if enabled {
        start_download(&app, true);
    }
    Ok(())
}

#[tauri::command]
pub fn check_update(app: AppHandle) {
    start_check(&app);
}

pub fn start_check(app: &AppHandle) {
    let updates = app.state::<Updates>();
    {
        let mut operation = updates.0.lock().unwrap();
        if matches!(
            operation.phase,
            Phase::Disabled
                | Phase::Checking
                | Phase::Downloading
                | Phase::Ready
                | Phase::Installing
        ) {
            return;
        }
        operation.phase = Phase::Checking;
        operation.update = None;
        operation.error = None;
        operation.received = 0;
        operation.total = None;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let result = async {
            app.updater_builder()
                .timeout(Duration::from_secs(20))
                // Tauri's default hook hides the window before ShellExecute. Keep
                // it visible so a failed installer launch can be reported and retried.
                .on_before_exit(|| {})
                .build()?
                .check()
                .await
        }
        .await;
        {
            let updates = app.state::<Updates>();
            let mut operation = updates.0.lock().unwrap();
            match result {
                Ok(Some(mut update)) => {
                    update.timeout = Some(Duration::from_secs(600));
                    operation.update = Some(update);
                    operation.phase = Phase::Available;
                }
                Ok(None) => operation.phase = Phase::Current,
                Err(error) => {
                    eprintln!("Afterwork updater check: {error}");
                    operation.phase = Phase::Idle;
                    operation.error = Some("Не удалось проверить обновления. Проверьте подключение к интернету и доступность GitHub Releases.".into());
                }
            }
        }
        start_download(&app, true);
    });
}

#[tauri::command]
pub fn download_update(app: AppHandle) {
    start_download(&app, false);
}

fn start_download(app: &AppHandle, automatic: bool) {
    let engine = app.state::<Mutex<Engine>>();
    let engine_guard = if automatic {
        let Ok(engine) = engine.lock() else { return };
        if !engine.data.auto_update {
            return;
        }
        Some(engine)
    } else {
        None
    };
    let updates = app.state::<Updates>();
    let update = {
        let mut operation = updates.0.lock().unwrap();
        if !matches!(operation.phase, Phase::Available) {
            return;
        }
        let Some(update) = operation.update.clone() else {
            return;
        };
        operation.phase = Phase::Downloading;
        operation.error = None;
        operation.received = 0;
        operation.total = None;
        update
    };
    drop(engine_guard);
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let updates = app.state::<Updates>();
        let result = update
            .download(
                |chunk, total| {
                    let mut operation = updates.0.lock().unwrap();
                    operation.received += chunk as u64;
                    operation.total = total;
                },
                || {},
            )
            .await;
        let mut operation = updates.0.lock().unwrap();
        match result {
            Ok(bytes) => {
                operation.package = Some(bytes);
                operation.phase = Phase::Ready;
            }
            Err(error) => {
                eprintln!("Afterwork updater download: {error}");
                operation.phase = Phase::Available;
                operation.error = Some(
                    "Не удалось скачать обновление или проверить его подпись. Попробуйте ещё раз."
                        .into(),
                );
            }
        }
    });
}

#[tauri::command]
pub fn install_update(app: AppHandle, version: String) -> CommandResult<()> {
    let updates = app.state::<Updates>();
    let (update, bytes) = {
        let mut operation = updates.0.lock().unwrap();
        if !matches!(operation.phase, Phase::Ready) {
            return Err("Обновление ещё не готово к установке.".into());
        }
        let update = operation
            .update
            .clone()
            .filter(|update| update.version == version)
            .ok_or("Версия обновления изменилась. Проверьте её перед установкой.")?;
        let bytes = operation
            .package
            .take()
            .ok_or("Пакет обновления недоступен.")?;
        operation.phase = Phase::Installing;
        operation.error = None;
        (update, bytes)
    };
    tauri::async_runtime::spawn_blocking(move || {
        let result = (|| {
            // Activity changes are already saved atomically. Save live game history
            // too, and release the lock before launching the installer.
            app.state::<Mutex<Engine>>()
                .lock()
                .map_err(crate::storage_error)?
                .checkpoint_games(model::now())?;
            update.install(&bytes).map_err(|error| {
                eprintln!("Afterwork updater install: {error}");
                "Не удалось запустить установщик. Попробуйте ещё раз.".into()
            })
        })();
        // On Windows a successful handoff to NSIS exits the process.
        if let Err(error) = result {
            let updates = app.state::<Updates>();
            let mut operation = updates.0.lock().unwrap();
            operation.package = Some(bytes);
            operation.phase = Phase::Ready;
            operation.error = Some(error);
        }
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn progress_matches_shared_fixture() {
        let status = UpdateStatus {
            current_version: "0.6.0".into(),
            auto_update: true,
            phase: Phase::Downloading,
            version: Some("0.7.0".into()),
            received: 512,
            total: Some(1024),
            error: None,
        };
        let expected: Value =
            serde_json::from_str(include_str!("../../fixtures/ipc-update-status.json")).unwrap();
        assert_eq!(serde_json::to_value(status).unwrap(), expected);
    }
}
