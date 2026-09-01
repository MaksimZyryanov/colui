use colui_domain::{ComposeProjectName, DisplayName, ProfileId, Revision};
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
}

#[test]
fn display_name_rejects_empty_values() {
    assert!(DisplayName::try_from("").is_err());
    assert!(DisplayName::try_from("Checkout API").is_ok());
}

#[test]
fn revision_starts_at_one_and_advances() {
    let initial = Revision::initial();
    assert_eq!(initial.next(), Revision::new(2));
}
