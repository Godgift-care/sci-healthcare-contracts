//! Admin handover, activation, status changes with a reason, and reinstatement.

use super::*;

#[test]
fn admin_activates_provider() {
    let f = setup();
    let provider = register_active_provider(&f);
    assert!(f.client.is_active_provider(&provider));
}

#[test]
fn admin_sets_verification_evidence() {
    let f = setup();
    let provider = register_active_provider(&f);
    let hash = BytesN::from_array(&f.env, &[7u8; 32]);

    f.client
        .set_verification_evidence(&f.admin, &provider, &hash);

    let stored = f.client.get_provider(&provider);
    assert_eq!(stored.verification_evidence, Some(hash));
}

#[test]
fn non_admin_cannot_set_verification_evidence() {
    let f = setup();
    let provider = register_active_provider(&f);
    let impostor = Address::generate(&f.env);
    let hash = BytesN::from_array(&f.env, &[7u8; 32]);

    let err = f
        .client
        .try_set_verification_evidence(&impostor, &provider, &hash)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, RegistryError::NotAuthorized);
}

#[test]
fn non_admin_cannot_change_status() {
    let f = setup();
    let provider = register_active_provider(&f);
    let impostor = Address::generate(&f.env);

    let err = f
        .client
        .try_set_provider_status(
            &impostor,
            &provider,
            &ProviderStatus::Suspended,
            &REASON_VERIFIED,
        )
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, RegistryError::NotAuthorized);
}

#[test]
fn admin_can_be_transferred() {
    let f = setup();
    let new_admin = Address::generate(&f.env);
    f.client.propose_admin(&f.admin, &new_admin);
    // Nothing changes until the nominee accepts.
    assert_eq!(f.client.get_admin(), f.admin);
    assert_eq!(f.client.get_pending_admin(), Some(new_admin.clone()));

    f.client.accept_admin(&new_admin);
    assert_eq!(f.client.get_admin(), new_admin);
    assert_eq!(f.client.get_pending_admin(), None);

    // Old admin loses authority.
    let attester = Address::generate(&f.env);
    let err = f
        .client
        .try_add_attester(&f.admin, &attester)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, RegistryError::NotAuthorized);
}

#[test]
fn only_the_nominee_can_accept_admin() {
    let f = setup();
    let nominee = Address::generate(&f.env);
    let intruder = Address::generate(&f.env);
    f.client.propose_admin(&f.admin, &nominee);

    let err = f.client.try_accept_admin(&intruder).err().unwrap().unwrap();
    assert_eq!(err, RegistryError::NotAuthorized);
    assert_eq!(f.client.get_admin(), f.admin);
}

#[test]
fn accept_admin_without_proposal_errors() {
    let f = setup();
    let someone = Address::generate(&f.env);
    let err = f.client.try_accept_admin(&someone).err().unwrap().unwrap();
    assert_eq!(err, RegistryError::NoPendingAdmin);
}

#[test]
fn non_admin_cannot_propose_admin() {
    let f = setup();
    let someone = Address::generate(&f.env);
    let err = f
        .client
        .try_propose_admin(&someone, &someone)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, RegistryError::NotAuthorized);
}

#[test]
fn reproposing_replaces_the_nominee() {
    let f = setup();
    let typo = Address::generate(&f.env);
    let intended = Address::generate(&f.env);
    f.client.propose_admin(&f.admin, &typo);
    f.client.propose_admin(&f.admin, &intended);

    let err = f.client.try_accept_admin(&typo).err().unwrap().unwrap();
    assert_eq!(err, RegistryError::NotAuthorized);
    f.client.accept_admin(&intended);
    assert_eq!(f.client.get_admin(), intended);
}

#[test]
fn a_retired_clinic_cannot_be_brought_back_by_the_admin() {
    let f = setup();
    let provider = register_active_provider(&f);
    f.client.retire(&provider, &REASON_CLOSED);

    // Every exit from `Retired` is refused: re-registration is the only
    // way back, so the admin cannot silently un-retire a clinic.
    for to in [
        ProviderStatus::Pending,
        ProviderStatus::Active,
        ProviderStatus::Suspended,
    ] {
        assert_eq!(
            try_transition(&f, &provider, &to),
            RegistryError::InvalidTransition
        );
    }
    assert_eq!(
        f.client.get_provider(&provider).status,
        ProviderStatus::Retired
    );
    assert!(!f.client.is_active_provider(&provider));
}

#[test]
fn admin_can_reinstate_a_suspended_provider_with_a_reason() {
    let f = setup();
    let provider = suspended_provider(&f);

    f.client.set_provider_status(
        &f.admin,
        &provider,
        &ProviderStatus::Active,
        &REASON_VERIFIED,
    );

    // Exactly the admin signed for it.
    let auths = f.env.auths();
    assert_eq!(auths.len(), 1);
    assert_eq!(auths[0].0, f.admin);

    let p = f.client.get_provider(&provider);
    assert_eq!(p.status, ProviderStatus::Active);
    assert_eq!(p.reason_code, REASON_VERIFIED);
    assert!(f.client.is_active_provider(&provider));
}

#[test]
fn provider_cannot_reinstate_itself() {
    let f = setup();
    let provider = suspended_provider(&f);
    assert_eq!(
        try_reinstate_as(&f, &provider, &provider),
        RegistryError::NotAuthorized
    );
    assert_still_suspended(&f, &provider);
}

#[test]
fn stranger_cannot_reinstate_a_provider() {
    let f = setup();
    let provider = suspended_provider(&f);
    let stranger = Address::generate(&f.env);
    assert_eq!(
        try_reinstate_as(&f, &stranger, &provider),
        RegistryError::NotAuthorized
    );
    assert_still_suspended(&f, &provider);
}

#[test]
fn another_active_provider_cannot_reinstate_a_provider() {
    let f = setup();
    let provider = suspended_provider(&f);
    let other = Address::generate(&f.env);
    activate(&f, &other);
    assert_eq!(
        try_reinstate_as(&f, &other, &provider),
        RegistryError::NotAuthorized
    );
    let p = f.client.get_provider(&provider);
    assert_eq!(p.status, ProviderStatus::Suspended);
    assert_eq!(p.reason_code, REASON_FRAUD);
}

#[test]
fn pending_admin_cannot_reinstate_before_accepting() {
    let f = setup();
    let provider = suspended_provider(&f);
    let nominee = Address::generate(&f.env);
    f.client.propose_admin(&f.admin, &nominee);
    assert_eq!(
        try_reinstate_as(&f, &nominee, &provider),
        RegistryError::NotAuthorized
    );
    assert_still_suspended(&f, &provider);
}

#[test]
fn previous_admin_cannot_reinstate_after_handover() {
    let f = setup();
    let provider = suspended_provider(&f);
    let new_admin = Address::generate(&f.env);
    f.client.propose_admin(&f.admin, &new_admin);
    f.client.accept_admin(&new_admin);

    assert_eq!(
        try_reinstate_as(&f, &f.admin, &provider),
        RegistryError::NotAuthorized
    );
    assert_still_suspended(&f, &provider);

    // The new admin can.
    f.client.set_provider_status(
        &new_admin,
        &provider,
        &ProviderStatus::Active,
        &REASON_VERIFIED,
    );
    assert!(f.client.is_active_provider(&provider));
}

#[test]
fn admin_address_without_its_signature_cannot_reinstate() {
    let f = setup();
    let provider = suspended_provider(&f);

    // The right address, but nobody signed: the host refuses.
    f.env.mock_auths(&[]);
    assert!(
        f.client
            .try_set_provider_status(
                &f.admin,
                &provider,
                &ProviderStatus::Active,
                &REASON_VERIFIED
            )
            .is_err(),
        "an unsigned reinstatement must not go through"
    );
    f.env.mock_all_auths();
    assert_still_suspended(&f, &provider);
}

#[test]
fn every_admin_status_change_requires_a_reason() {
    // Not only reinstatement: each allowed transition refuses code 0, which
    // is reserved for "no status change yet".
    for (from, to) in [
        (ProviderStatus::Pending, ProviderStatus::Active),
        (ProviderStatus::Pending, ProviderStatus::Suspended),
        (ProviderStatus::Active, ProviderStatus::Suspended),
        (ProviderStatus::Suspended, ProviderStatus::Active),
    ] {
        let f = setup();
        let provider = provider_in(&f, from);
        let before = f.client.get_provider(&provider);
        let err = f
            .client
            .try_set_provider_status(&f.admin, &provider, &to, &0u32)
            .err()
            .unwrap()
            .unwrap();
        assert_eq!(err, RegistryError::MissingReasonCode, "`{from:?} → {to:?}`");
        assert_eq!(f.client.get_provider(&provider), before);
    }
}

#[test]
fn non_admin_with_reason_zero_gets_not_authorized() {
    // Authorisation is checked first, so a stranger learns nothing about
    // the reason-code rule or the provider.
    let f = setup();
    let provider = suspended_provider(&f);
    let stranger = Address::generate(&f.env);
    let err = f
        .client
        .try_set_provider_status(&stranger, &provider, &ProviderStatus::Active, &0u32)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, RegistryError::NotAuthorized);
    assert_still_suspended(&f, &provider);
}
