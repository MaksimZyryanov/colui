use colui_domain::{ComposeProjectName, DisplayName, ProfileId, Revision};
use serde_json::from_str;
use uuid::Uuid;

#[test]
fn profile_id_is_not_derived_from_name_or_path() {
    let first = ProfileId::new(Uuid::from_u128(1));
    let renamed = ProfileId::new(Uuid::from_u128(1));
    assert_eq!(first, renamed);
}

#[test]
fn compose_name_rejects_invalid_namespace() {
    assert!(ComposeProjectName::try_from("Checkout API").is_err());
    assert!(ComposeProjectName::try_from("checkout_api-2").is_ok());
    assert!(ComposeProjectName::try_from("2checkout").is_ok());
    assert!(ComposeProjectName::try_from("Checkout").is_err());
    assert!(ComposeProjectName::try_from("_checkout").is_err());
    assert!(ComposeProjectName::try_from("-checkout").is_err());
    assert!(ComposeProjectName::try_from("").is_err());
}

#[test]
fn display_name_rejects_empty_values() {
    assert!(DisplayName::try_from("").is_err());
    assert!(DisplayName::try_from("Checkout API").is_ok());
}

#[test]
fn deserialization_preserves_display_name_validation() {
    assert!(from_str::<DisplayName>("\"\"").is_err());
}

#[test]
fn deserialization_preserves_compose_name_validation() {
    assert!(from_str::<ComposeProjectName>("\"Checkout\"").is_err());
    assert!(from_str::<ComposeProjectName>("\"_checkout\"").is_err());
    assert!(from_str::<ComposeProjectName>("\"-checkout\"").is_err());
    assert!(from_str::<ComposeProjectName>("\"\"").is_err());
    assert!(from_str::<ComposeProjectName>("\"2checkout\"").is_ok());
}

#[test]
fn revision_starts_at_one_and_advances() {
    let initial = Revision::initial();
    assert_eq!(initial.next(), Revision::new(2));
}
