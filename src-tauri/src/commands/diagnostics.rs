use crate::{dto::*, AppState};
use colui_app::{JournalEventKind as K, RegistryRecovery, RestoreRegistryBackup};
use tauri::State;

#[tauri::command]
pub async fn get_diagnostics(
    state: State<'_, AppState>,
) -> Result<DiagnosticsSnapshotDto, AppErrorDto> {
    state
        .diagnostics()
        .await
        .map(Into::into)
        .map_err(Into::into)
}

#[tauri::command]
pub async fn create_registry_backup(
    state: State<'_, AppState>,
) -> Result<RegistrySnapshotIdentityDto, AppErrorDto> {
    state
        .journaled(
            [K::BackupStarted, K::BackupSucceeded, K::BackupFailed],
            Some(colui_domain::AppErrorSubject::registry("registry")),
            &[ApplicationStateScopeDto::Diagnostics],
            async {
                state
                    .registry_diagnostics
                    .create_registry_backup()
                    .await
                    .map(Into::into)
            },
        )
        .await
}

#[tauri::command]
pub async fn restore_registry_backup(
    state: State<'_, AppState>,
) -> Result<Vec<ProfileSummaryDto>, AppErrorDto> {
    use ApplicationStateScopeDto as S;
    state
        .journaled(
            [K::RestoreStarted, K::RestoreSucceeded, K::RestoreFailed],
            Some(colui_domain::AppErrorSubject::registry("registry")),
            &[S::Profiles, S::Discovery, S::Definitions, S::Diagnostics],
            async {
                let snapshot = RestoreRegistryBackup::new(
                    state.registry_diagnostics.as_ref(),
                    state.locks.as_ref(),
                    state.definitions.as_ref(),
                )
                .execute()
                .await?;
                Ok(snapshot.profiles.into_iter().map(Into::into).collect())
            },
        )
        .await
}
