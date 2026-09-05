use crate::{dto::*, AppState};
use colui_app::{
    ConfigureAutoRegistration, IgnoreCandidate, JournalEventKind as K, ListDiscoveryCandidates,
    RegisterCandidate,
};
use tauri::State;

#[tauri::command]
pub async fn get_auto_registration_configuration(
    state: State<'_, AppState>,
) -> Result<AutoRegistrationConfigurationDto, AppErrorDto> {
    Ok(AutoRegistrationConfigurationDto {
        enabled: state.discovery.auto_registration_enabled().await,
    })
}

#[tauri::command]
pub async fn list_discovery_candidates(
    state: State<'_, AppState>,
) -> Result<DiscoveryListDto, AppErrorDto> {
    let candidates = ListDiscoveryCandidates::new(
        &state.discovery_inventory(),
        state.profiles.as_ref(),
        state.discovery.as_ref(),
    )
    .execute()
    .await
    .map_err(AppErrorDto::from)?;
    Ok(DiscoveryListDto {
        candidates: candidates.into_iter().map(Into::into).collect(),
        auto_registration_enabled: state.discovery.auto_registration_enabled().await,
    })
}

#[tauri::command]
pub async fn ignore_candidate(
    request: IgnoreCandidateRequestDto,
    state: State<'_, AppState>,
) -> Result<bool, AppErrorDto> {
    let changed = IgnoreCandidate::new(state.discovery.as_ref())
        .execute(decode_candidate_id(request.candidate_id).map_err(AppErrorDto::from)?)
        .await;
    if changed {
        state.events.publish(&[
            ApplicationStateScopeDto::Discovery,
            ApplicationStateScopeDto::Diagnostics,
        ]);
    }
    Ok(changed)
}

#[tauri::command]
pub async fn configure_auto_registration(
    request: ConfigureAutoRegistrationRequestDto,
    state: State<'_, AppState>,
) -> Result<AutoRegistrationConfigurationDto, AppErrorDto> {
    let previous = state.discovery.auto_registration_enabled().await;
    let enabled = ConfigureAutoRegistration::new(state.discovery.as_ref())
        .execute(request.enabled)
        .await;
    if previous != enabled {
        state.events.publish(&[
            ApplicationStateScopeDto::Discovery,
            ApplicationStateScopeDto::Diagnostics,
        ]);
    }
    Ok(AutoRegistrationConfigurationDto { enabled })
}

#[tauri::command]
pub async fn register_candidate(
    request: RegisterCandidateRequestDto,
    state: State<'_, AppState>,
) -> Result<ProfileSummaryDto, AppErrorDto> {
    let request = request.into_domain().map_err(AppErrorDto::from)?;
    use ApplicationStateScopeDto as S;
    state
        .journaled(
            [
                K::ManualRegistrationStarted,
                K::ManualRegistrationSucceeded,
                K::ManualRegistrationFailed,
            ],
            Some(colui_domain::AppErrorSubject::candidate(
                &request.candidate_id,
            )),
            &[S::Profiles, S::Discovery, S::Diagnostics],
            async {
                RegisterCandidate::new(
                    &state.discovery_inventory(),
                    state.profiles.as_ref(),
                    state.ids.as_ref(),
                    state.locks.as_ref(),
                )
                .execute(request)
                .await
                .map(Into::into)
            },
        )
        .await
}

#[tauri::command]
pub async fn auto_register_candidates(
    request: AutoRegistrationRequestDto,
    state: State<'_, AppState>,
) -> Result<AutoRegistrationResultDto, AppErrorDto> {
    let results = colui_app::AutoRegisterCandidates::new(
        &state.discovery_inventory(),
        state.profiles.as_ref(),
        state.ids.as_ref(),
        state.locks.as_ref(),
        state.discovery.as_ref(),
    )
    .execute(colui_app::AutoRegistrationSchedule {
        runtime_session_id: decode_session_id(request.runtime_session_id)
            .map_err(AppErrorDto::from)?,
        inventory_generation: request.inventory_generation,
    })
    .await;
    let mut response = AutoRegistrationResultDto {
        profiles: vec![],
        errors: vec![],
    };
    for result in results {
        match result {
            Ok(profile) => response.profiles.push(profile.into()),
            Err(error) => response.errors.push(error.into()),
        }
    }
    if !response.profiles.is_empty() || !response.errors.is_empty() {
        state.events.publish(&[
            ApplicationStateScopeDto::Profiles,
            ApplicationStateScopeDto::Discovery,
            ApplicationStateScopeDto::Diagnostics,
        ]);
    }
    Ok(response)
}
