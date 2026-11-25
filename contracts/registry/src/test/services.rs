//! The provider catalogue: listing, repricing, pausing and scheduled prices.

use super::*;

#[test]
fn active_provider_manages_catalog() {
    let f = setup();
    let provider = register_active_provider(&f);

    f.client.upsert_service(
        &provider,
        &101u32,
        &String::from_str(&f.env, "Outpatient consult"),
        &30_000_000i128,
    );

    let item = f.client.get_service(&provider, &101u32);
    assert_eq!(item.price, 30_000_000i128);
    assert!(item.active);
}

#[test]
fn upsert_lowers_price_immediately() {
    let f = setup();
    let provider = register_active_provider(&f);
    let label = String::from_str(&f.env, "Malaria RDT");

    f.client
        .upsert_service(&provider, &202u32, &label, &12_500_000i128);
    f.client
        .upsert_service(&provider, &202u32, &label, &10_000_000i128);

    assert_eq!(
        f.client.get_service(&provider, &202u32).price,
        10_000_000i128
    );
}

#[test]
fn upsert_cannot_raise_price_without_notice() {
    let f = setup();
    let provider = register_active_provider(&f);
    let label = String::from_str(&f.env, "Malaria RDT");
    f.client
        .upsert_service(&provider, &202u32, &label, &10_000_000i128);

    let err = f
        .client
        .try_upsert_service(&provider, &202u32, &label, &12_500_000i128)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, RegistryError::PriceIncreaseNeedsNotice);
    assert_eq!(f.client.get_service_price(&provider, &202u32), 10_000_000);
}

#[test]
fn scheduled_price_applies_at_the_effective_time() {
    let f = setup();
    let provider = priced_service(&f);
    let effective_at = START + MIN_PRICE_NOTICE_SECS;

    f.client
        .schedule_service_price(&provider, &CONSULT, &(PRICE * 2), &effective_at, &false);

    // Before: the current price stands and the upcoming one is visible.
    assert_eq!(f.client.get_service_price(&provider, &CONSULT), PRICE);
    assert_eq!(
        f.client.get_upcoming_price(&provider, &CONSULT),
        (PRICE * 2, effective_at)
    );
    f.env
        .ledger()
        .with_mut(|li| li.timestamp = effective_at - 1);
    assert_eq!(f.client.get_service_price(&provider, &CONSULT), PRICE);

    // At the effective time it applies, with nobody calling in.
    f.env.ledger().with_mut(|li| li.timestamp = effective_at);
    assert_eq!(f.client.get_service_price(&provider, &CONSULT), PRICE * 2);
    assert_eq!(f.client.get_service(&provider, &CONSULT).price, PRICE * 2);
    assert_eq!(f.client.get_upcoming_price(&provider, &CONSULT), (0, 0));
    let codes = Vec::from_array(&f.env, [CONSULT]);
    assert_eq!(
        f.client
            .get_services(&provider, &codes)
            .get_unchecked(0)
            .price,
        PRICE * 2
    );
}

#[test]
fn scheduled_price_needs_the_minimum_notice() {
    let f = setup();
    let provider = priced_service(&f);

    for effective_at in [START, START + MIN_PRICE_NOTICE_SECS - 1] {
        let err = f
            .client
            .try_schedule_service_price(&provider, &CONSULT, &(PRICE * 2), &effective_at, &false)
            .err()
            .unwrap()
            .unwrap();
        assert_eq!(err, RegistryError::PriceNoticeTooShort);
    }
    assert_eq!(f.client.get_upcoming_price(&provider, &CONSULT), (0, 0));
}

#[test]
fn large_price_jump_needs_explicit_confirmation() {
    let f = setup();
    let provider = priced_service(&f);
    let effective_at = START + MIN_PRICE_NOTICE_SECS;
    let five_times = PRICE * MAX_PRICE_JUMP as i128;

    // A 5x rise is rejected unless the clinic acknowledges it.
    let err = f
        .client
        .try_schedule_service_price(&provider, &CONSULT, &five_times, &effective_at, &false)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, RegistryError::LargePriceJumpNeedsConfirmation);
    assert_eq!(f.client.get_upcoming_price(&provider, &CONSULT), (0, 0));

    // A smaller rise still needs no flag.
    let four_times = PRICE * 4;
    f.client
        .schedule_service_price(&provider, &CONSULT, &four_times, &effective_at, &false);
    assert_eq!(
        f.client.get_upcoming_price(&provider, &CONSULT),
        (four_times, effective_at)
    );

    // With the flag the same 5x rise is accepted.
    f.client
        .schedule_service_price(&provider, &CONSULT, &five_times, &effective_at, &true);
    assert_eq!(
        f.client.get_upcoming_price(&provider, &CONSULT),
        (five_times, effective_at)
    );
}

#[test]
fn large_price_jump_confirmation_cannot_skip_the_notice() {
    let f = setup();
    let provider = priced_service(&f);
    let five_times = PRICE * MAX_PRICE_JUMP as i128;

    let err = f
        .client
        .try_schedule_service_price(&provider, &CONSULT, &five_times, &START, &true)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, RegistryError::PriceNoticeTooShort);
}

#[test]
fn price_jump_threshold_handles_overflow_as_a_large_jump() {
    let f = setup();
    let provider = priced_service(&f);
    let effective_at = START + MIN_PRICE_NOTICE_SECS;

    let err = f
        .client
        .try_schedule_service_price(&provider, &CONSULT, &i128::MAX, &effective_at, &false)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, RegistryError::LargePriceJumpNeedsConfirmation);
}

#[test]
fn rescheduling_replaces_the_earlier_schedule() {
    let f = setup();
    let provider = priced_service(&f);
    let first = START + MIN_PRICE_NOTICE_SECS;
    let second = START + 2 * MIN_PRICE_NOTICE_SECS;

    f.client
        .schedule_service_price(&provider, &CONSULT, &(PRICE * 2), &first, &false);
    // Scheduling the current price is how a clinic calls a change off.
    f.client
        .schedule_service_price(&provider, &CONSULT, &PRICE, &second, &false);

    f.env.ledger().with_mut(|li| li.timestamp = first);
    assert_eq!(f.client.get_service_price(&provider, &CONSULT), PRICE);
    assert_eq!(
        f.client.get_upcoming_price(&provider, &CONSULT),
        (PRICE, second)
    );
}

#[test]
fn upsert_keeps_a_scheduled_price() {
    let f = setup();
    let provider = priced_service(&f);
    let effective_at = START + MIN_PRICE_NOTICE_SECS;
    f.client
        .schedule_service_price(&provider, &CONSULT, &(PRICE * 2), &effective_at, &false);

    f.client.upsert_service(
        &provider,
        &CONSULT,
        &String::from_str(&f.env, "Consult (renamed)"),
        &PRICE,
    );

    assert_eq!(
        f.client.get_upcoming_price(&provider, &CONSULT),
        (PRICE * 2, effective_at)
    );
}

#[test]
fn schedule_rejects_bad_input() {
    let f = setup();
    let provider = priced_service(&f);
    let effective_at = START + MIN_PRICE_NOTICE_SECS;

    let err = f
        .client
        .try_schedule_service_price(&provider, &CONSULT, &0i128, &effective_at, &false)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, RegistryError::InvalidPrice);

    let err = f
        .client
        .try_schedule_service_price(&provider, &999u32, &PRICE, &effective_at, &false)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, RegistryError::ServiceNotFound);

    let err = f
        .client
        .try_get_upcoming_price(&provider, &999u32)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, RegistryError::ServiceNotFound);

    f.client.set_provider_status(
        &f.admin,
        &provider,
        &ProviderStatus::Suspended,
        &REASON_VERIFIED,
    );
    let err = f
        .client
        .try_schedule_service_price(&provider, &CONSULT, &PRICE, &effective_at, &false)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, RegistryError::ProviderNotActive);
}

#[test]
fn pending_provider_cannot_add_services() {
    let f = setup();
    let provider = Address::generate(&f.env);
    f.client.register_provider(
        &provider,
        &String::from_str(&f.env, "Unverified Clinic"),
        &String::from_str(&f.env, "NG"),
        &ProviderCategory::Clinic,
    );

    let err = f
        .client
        .try_upsert_service(
            &provider,
            &101u32,
            &String::from_str(&f.env, "Consult"),
            &30_000_000i128,
        )
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, RegistryError::ProviderNotActive);
}

#[test]
fn suspended_provider_cannot_reprice() {
    let f = setup();
    let provider = register_active_provider(&f);
    f.client.upsert_service(
        &provider,
        &101u32,
        &String::from_str(&f.env, "Consult"),
        &30_000_000i128,
    );
    f.client.set_provider_status(
        &f.admin,
        &provider,
        &ProviderStatus::Suspended,
        &REASON_VERIFIED,
    );

    let err = f
        .client
        .try_upsert_service(
            &provider,
            &101u32,
            &String::from_str(&f.env, "Consult"),
            &1i128,
        )
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, RegistryError::ProviderNotActive);
}

#[test]
fn non_positive_price_rejected() {
    let f = setup();
    let provider = register_active_provider(&f);
    let label = String::from_str(&f.env, "Consult");

    for bad in [0i128, -1i128] {
        let err = f
            .client
            .try_upsert_service(&provider, &101u32, &label, &bad)
            .err()
            .unwrap()
            .unwrap();
        assert_eq!(err, RegistryError::InvalidPrice);
    }
}

#[test]
fn removing_absent_service_errors() {
    let f = setup();
    let provider = register_active_provider(&f);
    let err = f
        .client
        .try_remove_service(&provider, &999u32)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, RegistryError::ServiceNotFound);
}

#[test]
fn remove_service_clears_it() {
    let f = setup();
    let provider = register_active_provider(&f);
    f.client.upsert_service(
        &provider,
        &101u32,
        &String::from_str(&f.env, "Consult"),
        &30_000_000i128,
    );
    f.client.remove_service(&provider, &101u32);

    let err = f
        .client
        .try_get_service(&provider, &101u32)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, RegistryError::ServiceNotFound);
}

#[test]
fn get_services_batches_and_skips_missing() {
    let f = setup();
    let provider = register_active_provider(&f);
    f.client.upsert_service(
        &provider,
        &101u32,
        &String::from_str(&f.env, "Consult"),
        &30_000_000i128,
    );
    f.client.upsert_service(
        &provider,
        &202u32,
        &String::from_str(&f.env, "Malaria RDT"),
        &10_000_000i128,
    );

    let codes = Vec::from_array(&f.env, [101u32, 202u32, 999u32]);
    let items = f.client.get_services(&provider, &codes);
    assert_eq!(items.len(), 2);
}

#[test]
fn new_service_is_available() {
    let f = setup();
    let provider = register_active_provider(&f);
    f.client.upsert_service(
        &provider,
        &202u32,
        &String::from_str(&f.env, "Malaria RDT"),
        &10_000_000i128,
    );

    assert!(f.client.get_service(&provider, &202u32).available);
    assert!(f.client.is_service_available(&provider, &202u32));
}

#[test]
fn paused_service_stays_listed_and_priced() {
    let f = setup();
    let provider = register_active_provider(&f);
    f.client.upsert_service(
        &provider,
        &202u32,
        &String::from_str(&f.env, "Malaria RDT"),
        &10_000_000i128,
    );

    f.client.set_service_available(&provider, &202u32, &false);

    let item = f.client.get_service(&provider, &202u32);
    assert!(!item.available);
    assert!(item.active);
    assert!(!f.client.is_service_available(&provider, &202u32));
    // Still priced, so `voucher` can say "unavailable" rather than "not offered".
    assert_eq!(
        f.client.get_service_price(&provider, &202u32),
        10_000_000i128
    );
    let codes = Vec::from_array(&f.env, [202u32]);
    assert_eq!(f.client.get_services(&provider, &codes).len(), 1);

    f.client.set_service_available(&provider, &202u32, &true);
    assert!(f.client.is_service_available(&provider, &202u32));
}

#[test]
fn repricing_a_paused_service_keeps_it_paused() {
    let f = setup();
    let provider = register_active_provider(&f);
    let label = String::from_str(&f.env, "Malaria RDT");
    f.client
        .upsert_service(&provider, &202u32, &label, &10_000_000i128);
    f.client.set_service_available(&provider, &202u32, &false);

    f.client
        .upsert_service(&provider, &202u32, &label, &9_000_000i128);

    assert!(!f.client.get_service(&provider, &202u32).available);
}

#[test]
fn pausing_absent_service_errors() {
    let f = setup();
    let provider = register_active_provider(&f);
    let err = f
        .client
        .try_set_service_available(&provider, &999u32, &false)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, RegistryError::ServiceNotFound);
    assert!(!f.client.is_service_available(&provider, &999u32));
}

#[test]
fn service_price_view_returns_price() {
    let f = setup();
    let provider = register_active_provider(&f);
    f.client.upsert_service(
        &provider,
        &101u32,
        &String::from_str(&f.env, "Consult"),
        &30_000_000i128,
    );
    assert_eq!(
        f.client.get_service_price(&provider, &101u32),
        30_000_000i128
    );
}

#[test]
fn service_price_view_returns_zero_when_absent() {
    let f = setup();
    let provider = register_active_provider(&f);
    assert_eq!(f.client.get_service_price(&provider, &999u32), 0);

    let stranger = Address::generate(&f.env);
    assert_eq!(f.client.get_service_price(&stranger, &101u32), 0);
}

#[test]
fn provider_service_and_attester_survive_sixty_days_of_activity() {
    let f = setup();
    let provider = register_active_provider(&f);
    f.client.upsert_service(
        &provider,
        &CONSULT,
        &String::from_str(&f.env, "Outpatient consult"),
        &PRICE,
    );
    let attester = Address::generate(&f.env);
    f.client.add_attester(&f.admin, &attester);

    // Ten days at a time, the way an indexer and the app poll the registry.
    for day in 1..=6 {
        advance_days(&f, 10);
        assert_eq!(
            f.client.get_provider(&provider).status,
            ProviderStatus::Active,
            "provider was archived on day {}",
            day * 10,
        );
        assert_eq!(f.client.get_service(&provider, &CONSULT).price, PRICE);
        assert!(f.client.is_attester(&attester));

        if day == 3 {
            // A suspension and reinstatement mid-way: normal admin writes
            // extend the whole contract instance, not just one entry.
            f.client.set_provider_status(
                &f.admin,
                &provider,
                &ProviderStatus::Suspended,
                &REASON_VERIFIED,
            );
            f.client.set_provider_status(
                &f.admin,
                &provider,
                &ProviderStatus::Active,
                &REASON_VERIFIED,
            );
        }
    }

    // Day 60. The registration written on day 0 had a 90-day TTL, so the
    // read above still found it live, and by now it has crossed the 30-day
    // threshold and been pushed back out to a full 90 days.
    assert_eq!(provider_ttl(&f, &provider), NINETY_DAYS);
}

#[test]
fn a_retired_clinic_cannot_manage_its_catalog() {
    let f = setup();
    let provider = register_active_provider(&f);
    f.client.retire(&provider, &REASON_CLOSED);

    let err = f
        .client
        .try_upsert_service(
            &provider,
            &101u32,
            &String::from_str(&f.env, "Consult"),
            &30_000_000i128,
        )
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, RegistryError::ProviderNotActive);
}
