use colui_tauri_lib::dto::{LifecycleResultDto, UpdateProfileRequestDto};

#[test]
fn update_request_serializes_only_camel_case_metadata_and_patch() {
    let request = UpdateProfileRequestDto::fixture();
    let value = serde_json::to_value(request).unwrap();
    assert_eq!(value["profileId"], "00000000-0000-0000-0000-000000000001");
    assert_eq!(value["expectedRevision"], 3);
    assert!(value["patch"].get("id").is_none());
}

#[test]
fn lifecycle_result_contains_profile_id_and_success() {
    let value = serde_json::to_value(LifecycleResultDto::success("id-1")).unwrap();
    assert_eq!(
        value,
        serde_json::json!({"profileId":"id-1","success":true})
    );
}
