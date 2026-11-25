//! Registration, the provider record, its lifecycle statuses and its capacity.

use super::*;

#[test]
fn provider_registers_as_pending() {
    let f = setup();
    let provider = Address::generate(&f.env);
    f.client.register_provider(
        &provider,
        &String::from_str(&f.env, "Nairobi Health Post"),
        &String::from_str(&f.env, "KE"),
        &ProviderCategory::Clinic,
    );

    let stored = f.client.get_provider(&provider);
    assert_eq!(stored.status, ProviderStatus::Pending);
    assert_eq!(stored.owner, provider);
    // Pending is not billable.
    assert!(!f.client.is_active_provider(&provider));
}

#[test]
fn duplicate_registration_rejected() {
    let f = setup();
    let provider = Address::generate(&f.env);
    let name = String::from_str(&f.env, "Clinic");
    let country = String::from_str(&f.env, "NG");
    f.client
        .register_provider(&provider, &name, &country, &ProviderCategory::Clinic);

    let err = f
        .client
        .try_register_provider(&provider, &name, &country, &ProviderCategory::Clinic)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, RegistryError::ProviderExists);
}

#[test]
fn bad_country_code_rejected() {
    let f = setup();
    let provider = Address::generate(&f.env);
    let err = f
        .client
        .try_register_provider(
            &provider,
            &String::from_str(&f.env, "Clinic"),
            &String::from_str(&f.env, "NGA"),
            &ProviderCategory::Clinic,
        )
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, RegistryError::InvalidCountry);
}

#[test]
fn empty_name_rejected() {
    let f = setup();
    let provider = Address::generate(&f.env);
    let err = f
        .client
        .try_register_provider(
            &provider,
            &String::from_str(&f.env, ""),
            &String::from_str(&f.env, "NG"),
            &ProviderCategory::Clinic,
        )
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, RegistryError::EmptyName);
}

#[test]
fn register_with_each_category() {
    let f = setup();
    for (category, name) in [
        (ProviderCategory::Clinic, "Clinic A"),
        (ProviderCategory::Pharmacy, "Pharmacy B"),
        (ProviderCategory::Laboratory, "Lab C"),
        (ProviderCategory::Maternity, "Maternity D"),
        (ProviderCategory::Other, "Other E"),
    ] {
        let provider = Address::generate(&f.env);
        f.client.register_provider(
            &provider,
            &String::from_str(&f.env, name),
            &String::from_str(&f.env, "NG"),
            &category,
        );
        let stored = f.client.get_provider(&provider);
        assert_eq!(stored.category, category);
    }
}

#[test]
fn update_provider_category_by_owner() {
    let f = setup();
    let provider = register_active_provider(&f);
    assert_eq!(
        f.client.get_provider(&provider).category,
        ProviderCategory::Clinic
    );

    f.client
        .update_provider_category(&provider, &ProviderCategory::Laboratory);
    assert_eq!(
        f.client.get_provider(&provider).category,
        ProviderCategory::Laboratory
    );

    f.client
        .update_provider_category(&provider, &ProviderCategory::Maternity);
    assert_eq!(
        f.client.get_provider(&provider).category,
        ProviderCategory::Maternity
    );

    f.client
        .update_provider_category(&provider, &ProviderCategory::Other);
    assert_eq!(
        f.client.get_provider(&provider).category,
        ProviderCategory::Other
    );

    f.client
        .update_provider_category(&provider, &ProviderCategory::Pharmacy);
    assert_eq!(
        f.client.get_provider(&provider).category,
        ProviderCategory::Pharmacy
    );
}

#[test]
fn update_category_unknown_provider() {
    let f = setup();
    let stranger = Address::generate(&f.env);

    let err = f
        .client
        .try_update_provider_category(&stranger, &ProviderCategory::Pharmacy)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, RegistryError::ProviderNotFound);
}

#[test]
fn set_verification_evidence_unknown_provider() {
    let f = setup();
    let stranger = Address::generate(&f.env);
    let hash = BytesN::from_array(&f.env, &[7u8; 32]);

    let err = f
        .client
        .try_set_verification_evidence(&f.admin, &stranger, &hash)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, RegistryError::ProviderNotFound);
}

#[test]
fn suspended_provider_is_not_active() {
    let f = setup();
    let provider = register_active_provider(&f);
    f.client.set_provider_status(
        &f.admin,
        &provider,
        &ProviderStatus::Suspended,
        &REASON_VERIFIED,
    );
    assert!(!f.client.is_active_provider(&provider));
}

#[test]
fn unknown_provider_is_not_active() {
    let f = setup();
    let stranger = Address::generate(&f.env);
    assert!(!f.client.is_active_provider(&stranger));
}

#[test]
fn provider_counts_lifecycle() {
    let f = setup();
    assert_eq!(f.client.provider_counts(), (0, 0, 0));

    let provider = Address::generate(&f.env);
    f.client.register_provider(
        &provider,
        &String::from_str(&f.env, "A Clinic"),
        &String::from_str(&f.env, "NG"),
        &ProviderCategory::Clinic,
    );
    assert_eq!(f.client.provider_counts(), (1, 0, 0));

    // Pending -> Active
    f.client
        .set_provider_status(&f.admin, &provider, &ProviderStatus::Active, &1u32);
    assert_eq!(f.client.provider_counts(), (0, 1, 0));

    // Active -> Suspended
    f.client
        .set_provider_status(&f.admin, &provider, &ProviderStatus::Suspended, &2u32);
    assert_eq!(f.client.provider_counts(), (0, 0, 1));

    // Suspended -> Active
    f.client
        .set_provider_status(&f.admin, &provider, &ProviderStatus::Active, &1u32);
    assert_eq!(f.client.provider_counts(), (0, 1, 0));

    // Active -> Retired
    f.client.retire(&provider, &5u32);
    assert_eq!(f.client.provider_counts(), (0, 0, 0));

    // New provider Pending -> Suspended -> Retired
    let p2 = Address::generate(&f.env);
    f.client.register_provider(
        &p2,
        &String::from_str(&f.env, "B Clinic"),
        &String::from_str(&f.env, "KE"),
        &ProviderCategory::Pharmacy,
    );
    assert_eq!(f.client.provider_counts(), (1, 0, 0));

    f.client
        .set_provider_status(&f.admin, &p2, &ProviderStatus::Suspended, &3u32);
    assert_eq!(f.client.provider_counts(), (0, 0, 1));

    f.client.retire(&p2, &5u32);
    assert_eq!(f.client.provider_counts(), (0, 0, 0));

    // New provider Pending -> Retired directly
    let p3 = Address::generate(&f.env);
    f.client.register_provider(
        &p3,
        &String::from_str(&f.env, "C Clinic"),
        &String::from_str(&f.env, "GH"),
        &ProviderCategory::Laboratory,
    );
    assert_eq!(f.client.provider_counts(), (1, 0, 0));
    f.client.retire(&p3, &5u32);
    assert_eq!(f.client.provider_counts(), (0, 0, 0));

    // Trying to retire an already retired provider fails
    let err = f.client.try_retire(&p3, &5u32).err().unwrap().unwrap();
    assert_eq!(err, RegistryError::InvalidTransition);
}

#[test]
fn registry_reads_restore_a_lapsed_entry() {
    let f = setup();
    let provider = register_active_provider(&f);

    // Nothing touches the provider for 95 days: past the 90-day TTL the
    // contract set on it.
    advance_days(&f, 95);

    // The value is not lost. The host restores the lapsed entry on the first
    // read, and this contract's read path immediately extends it again.
    assert_eq!(f.client.get_provider(&provider).owner, provider);
    assert_eq!(provider_ttl(&f, &provider), NINETY_DAYS);

    // And it is still billable: a voucher for this provider can be funded.
    assert!(f.client.is_active_provider(&provider));
}

#[test]
fn not_initialized_errors() {
    let env = Env::default();
    env.mock_all_auths();
    let a = Address::generate(&env);
    let addr = env.register(Registry, ());
    let client = RegistryClient::new(&env, &addr);

    // Calls that read the admin before initialize should return NotInitialized.
    let err = client.try_get_admin().err().unwrap().unwrap();
    assert_eq!(err, RegistryError::NotInitialized);
    // get_provider returns ProviderNotFound because the provider doesn't exist,
    // not because the contract isn't initialized.
    let err = client.try_get_provider(&a).err().unwrap().unwrap();
    assert_eq!(err, RegistryError::ProviderNotFound);
}

#[test]
fn provider_not_found() {
    let f = setup();
    let unknown = Address::generate(&f.env);
    let err = f.client.try_get_provider(&unknown).err().unwrap().unwrap();
    assert_eq!(err, RegistryError::ProviderNotFound);
    // upsert_service returns ProviderNotFound when the provider doesn't exist,
    // because it calls read_provider first.
    let err = f
        .client
        .try_upsert_service(&unknown, &1u32, &String::from_str(&f.env, "X"), &100i128)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, RegistryError::ProviderNotFound);
    // set_service_available calls read_service directly (not read_provider),
    // so it returns ServiceNotFound.
    let err = f
        .client
        .try_set_service_available(&unknown, &1u32, &true)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, RegistryError::ServiceNotFound);
}

#[test]
fn capacity_defaults_to_unlimited() {
    let f = setup();
    let provider = register_active_provider(&f);
    assert_eq!(f.client.get_capacity(&provider), 0);
    // An address that never registered reads as unlimited too.
    assert_eq!(f.client.get_capacity(&Address::generate(&f.env)), 0);
}

#[test]
fn provider_sets_and_clears_capacity() {
    let f = setup();
    let provider = register_active_provider(&f);

    f.client.set_capacity(&provider, &3);
    assert_eq!(f.client.get_capacity(&provider), 3);

    f.client.set_capacity(&provider, &0);
    assert_eq!(f.client.get_capacity(&provider), 0);
}

#[test]
fn capacity_needs_a_registered_provider() {
    let f = setup();
    let stranger = Address::generate(&f.env);
    assert_eq!(
        f.client.try_set_capacity(&stranger, &3),
        Err(Ok(RegistryError::ProviderNotFound))
    );
}

#[test]
fn country_codes_need_two_uppercase_ascii_letters() {
    let f = setup();
    let name = String::from_str(&f.env, "Clinic");

    // Two-character codes that are not ISO alpha-2: lowercase, a digit,
    // whitespace, mixed case, and a two-byte non-ASCII character ("é" is
    // exactly two UTF-8 bytes, so byte length alone would let it through).
    for bad in ["ng", "N1", "  ", "nG", "é"] {
        let provider = Address::generate(&f.env);
        let country = String::from_str(&f.env, bad);
        let err = f
            .client
            .try_register_provider(&provider, &name, &country, &ProviderCategory::Clinic)
            .err()
            .unwrap()
            .unwrap();
        assert_eq!(
            err,
            RegistryError::InvalidCountry,
            "`{bad}` must be refused"
        );
        assert!(!f.client.is_active_provider(&provider));
    }

    // The right shape, whatever the full ISO list says: accepted.
    for good in ["NG", "KE", "GB"] {
        let provider = Address::generate(&f.env);
        let country = String::from_str(&f.env, good);
        f.client
            .register_provider(&provider, &name, &country, &ProviderCategory::Clinic);
        assert_eq!(f.client.get_provider(&provider).country, country);
    }
}

#[test]
fn pending_to_active_is_allowed() {
    assert_allowed(ProviderStatus::Pending, ProviderStatus::Active);
}

#[test]
fn pending_to_suspended_is_allowed() {
    assert_allowed(ProviderStatus::Pending, ProviderStatus::Suspended);
}

#[test]
fn active_to_suspended_is_allowed() {
    assert_allowed(ProviderStatus::Active, ProviderStatus::Suspended);
}

#[test]
fn suspended_to_active_is_allowed() {
    assert_allowed(ProviderStatus::Suspended, ProviderStatus::Active);
}

#[test]
fn pending_to_pending_is_rejected() {
    assert_rejected(ProviderStatus::Pending, ProviderStatus::Pending);
}

#[test]
fn active_to_active_is_rejected() {
    assert_rejected(ProviderStatus::Active, ProviderStatus::Active);
}

#[test]
fn suspended_to_suspended_is_rejected() {
    assert_rejected(ProviderStatus::Suspended, ProviderStatus::Suspended);
}

#[test]
fn retired_to_retired_is_rejected() {
    assert_rejected(ProviderStatus::Retired, ProviderStatus::Retired);
}

#[test]
fn active_to_pending_is_rejected() {
    assert_rejected(ProviderStatus::Active, ProviderStatus::Pending);
}

#[test]
fn suspended_to_pending_is_rejected() {
    assert_rejected(ProviderStatus::Suspended, ProviderStatus::Pending);
}

#[test]
fn pending_to_retired_is_rejected_for_the_admin() {
    // Only the clinic's own owner retires it, via `retire`.
    assert_rejected(ProviderStatus::Pending, ProviderStatus::Retired);
}

#[test]
fn active_to_retired_is_rejected_for_the_admin() {
    assert_rejected(ProviderStatus::Active, ProviderStatus::Retired);
}

#[test]
fn suspended_to_retired_is_rejected_for_the_admin() {
    assert_rejected(ProviderStatus::Suspended, ProviderStatus::Retired);
}

#[test]
fn retired_to_pending_is_rejected() {
    assert_rejected(ProviderStatus::Retired, ProviderStatus::Pending);
}

#[test]
fn retired_to_active_is_rejected() {
    assert_rejected(ProviderStatus::Retired, ProviderStatus::Active);
}

#[test]
fn retired_to_suspended_is_rejected() {
    assert_rejected(ProviderStatus::Retired, ProviderStatus::Suspended);
}

#[test]
fn reason_code_is_recorded_on_every_status_change() {
    let f = setup();
    let provider = Address::generate(&f.env);
    f.client.register_provider(
        &provider,
        &String::from_str(&f.env, "Clinic"),
        &String::from_str(&f.env, "NG"),
        &ProviderCategory::Clinic,
    );
    // No status change has happened yet.
    assert_eq!(f.client.get_provider(&provider).reason_code, 0);

    f.client.set_provider_status(
        &f.admin,
        &provider,
        &ProviderStatus::Active,
        &REASON_VERIFIED,
    );
    assert_eq!(
        f.client.get_provider(&provider).reason_code,
        REASON_VERIFIED
    );

    f.client.set_provider_status(
        &f.admin,
        &provider,
        &ProviderStatus::Suspended,
        &REASON_SUSPENDED_WHILE_TESTING,
    );
    assert_eq!(
        f.client.get_provider(&provider).reason_code,
        REASON_SUSPENDED_WHILE_TESTING
    );

    f.client.set_provider_status(
        &f.admin,
        &provider,
        &ProviderStatus::Active,
        &REASON_VERIFIED,
    );
    assert_eq!(
        f.client.get_provider(&provider).reason_code,
        REASON_VERIFIED
    );
}

#[test]
fn owner_retires_a_clinic_and_it_leaves_the_registry() {
    let f = setup();
    let provider = register_active_provider(&f);

    f.client.retire(&provider, &REASON_CLOSED);

    let stored = f.client.get_provider(&provider);
    assert_eq!(stored.status, ProviderStatus::Retired);
    assert_eq!(stored.reason_code, REASON_CLOSED);
    // Not billable: `voucher` refuses to fund a retired clinic.
    assert!(!f.client.is_active_provider(&provider));
}

#[test]
fn retire_needs_the_owners_signature() {
    let f = setup();
    let provider = register_active_provider(&f);

    // With no address authorized, the host refuses `owner.require_auth()`:
    // an unsigned retirement must not go through. (That the demanded signer
    // is the clinic's own address and nobody else's is asserted exactly by
    // the authorisation matrix, which records one signer for `retire`.)
    f.env.mock_auths(&[]);
    assert!(
        f.client.try_retire(&provider, &REASON_CLOSED).is_err(),
        "an unsigned retirement must not go through"
    );

    // And `retire` keys the clinic off the caller's own address, so an
    // impostor can only name themselves — they cannot reach someone else's
    // clinic record at all.
    let impostor = Address::generate(&f.env);
    f.env.mock_all_auths();
    let err = f
        .client
        .try_retire(&impostor, &REASON_CLOSED)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, RegistryError::ProviderNotFound);

    assert!(f.client.is_active_provider(&provider));
}

#[test]
fn retiring_twice_is_rejected() {
    let f = setup();
    let provider = register_active_provider(&f);
    f.client.retire(&provider, &REASON_CLOSED);

    let err = f
        .client
        .try_retire(&provider, &REASON_CLOSED)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, RegistryError::InvalidTransition);
    assert_eq!(
        f.client.get_provider(&provider).status,
        ProviderStatus::Retired
    );
}

#[test]
fn reinstating_with_reason_zero_is_rejected() {
    let f = setup();
    let provider = suspended_provider(&f);
    let err = f
        .client
        .try_set_provider_status(&f.admin, &provider, &ProviderStatus::Active, &0u32)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, RegistryError::MissingReasonCode);
    assert_still_suspended(&f, &provider);
}
