//! Paginated views: providers in registration order, and providers by country.

use super::*;

/// Clinics are listed in registration order, and pagination pages through them.
#[test]
fn list_providers_in_registration_order() {
    let f = setup();

    let a = Address::generate(&f.env);
    let b = Address::generate(&f.env);
    let c = Address::generate(&f.env);

    f.client.register_provider(
        &a,
        &String::from_str(&f.env, "A Clinic"),
        &String::from_str(&f.env, "NG"),
        &ProviderCategory::Clinic,
    );
    f.client.register_provider(
        &b,
        &String::from_str(&f.env, "B Clinic"),
        &String::from_str(&f.env, "KE"),
        &ProviderCategory::Clinic,
    );
    f.client.register_provider(
        &c,
        &String::from_str(&f.env, "C Clinic"),
        &String::from_str(&f.env, "GH"),
        &ProviderCategory::Clinic,
    );

    let first = f.client.list_providers(&0u32, &2u32);
    assert_eq!(first.len(), 2);
    assert_eq!(first.get_unchecked(0).owner, a);
    assert_eq!(
        first.get_unchecked(0).country,
        String::from_str(&f.env, "NG")
    );
    assert_eq!(first.get_unchecked(1).owner, b);
    assert_eq!(
        first.get_unchecked(1).country,
        String::from_str(&f.env, "KE")
    );

    let second = f.client.list_providers(&2u32, &2u32);
    assert_eq!(second.len(), 1);
    assert_eq!(second.get_unchecked(0).owner, c);
    assert_eq!(
        second.get_unchecked(0).country,
        String::from_str(&f.env, "GH")
    );

    let all = f.client.list_providers(&0u32, &10u32);
    assert_eq!(all.len(), 3);
    assert_eq!(all.get_unchecked(0).owner, a);
    assert_eq!(all.get_unchecked(1).owner, b);
    assert_eq!(all.get_unchecked(2).owner, c);
}

/// A `start` beyond the end returns an empty list.
#[test]
fn list_providers_start_beyond_end_is_empty() {
    let f = setup();
    let provider = Address::generate(&f.env);
    f.client.register_provider(
        &provider,
        &String::from_str(&f.env, "A Clinic"),
        &String::from_str(&f.env, "NG"),
        &ProviderCategory::Clinic,
    );

    assert_eq!(f.client.list_providers(&1u32, &10u32).len(), 0);
    assert_eq!(f.client.list_providers(&999u32, &10u32).len(), 0);
}

/// A limit above `MAX_PROVIDERS_PAGE` is clamped to the cap.
#[test]
fn list_providers_limit_is_capped() {
    let f = setup();

    for _ in 0..60 {
        let provider = Address::generate(&f.env);
        f.client.register_provider(
            &provider,
            &String::from_str(&f.env, "Clinic"),
            &String::from_str(&f.env, "NG"),
            &ProviderCategory::Clinic,
        );
    }

    let first = f.client.list_providers(&0u32, &u32::MAX);
    assert_eq!(first.len(), MAX_PROVIDERS_PAGE);

    let second = f.client.list_providers(&MAX_PROVIDERS_PAGE, &u32::MAX);
    assert_eq!(second.len(), 10);
}

/// An empty registry lists nothing.
#[test]
fn list_providers_empty_registry() {
    let f = setup();
    assert_eq!(f.client.list_providers(&0u32, &10u32).len(), 0);
}

/// `providers_in` returns only `Active` clinics in one country: pending and
/// suspended clinics stay out, suspension removes a clinic from the country
/// index, and reactivation re-adds it once.
#[test]
fn providers_in_tracks_active_status() {
    let f = setup();

    let a = Address::generate(&f.env);
    let b = Address::generate(&f.env);
    let c = Address::generate(&f.env);

    f.client.register_provider(
        &a,
        &String::from_str(&f.env, "A Clinic"),
        &String::from_str(&f.env, "NG"),
        &ProviderCategory::Clinic,
    );
    f.client.register_provider(
        &b,
        &String::from_str(&f.env, "B Clinic"),
        &String::from_str(&f.env, "NG"),
        &ProviderCategory::Clinic,
    );
    f.client.register_provider(
        &c,
        &String::from_str(&f.env, "C Clinic"),
        &String::from_str(&f.env, "NG"),
        &ProviderCategory::Clinic,
    );
    f.client
        .set_provider_status(&f.admin, &a, &ProviderStatus::Active, &REASON_VERIFIED);
    f.client
        .set_provider_status(&f.admin, &b, &ProviderStatus::Active, &REASON_VERIFIED);
    // c stays Pending.

    let ng = f
        .client
        .providers_in(&String::from_str(&f.env, "NG"), &0u32, &10u32);
    assert_eq!(ng.len(), 2);
    assert_eq!(ng.get_unchecked(0).owner, a);
    assert_eq!(ng.get_unchecked(1).owner, b);

    // Suspending b removes it; only a remains.
    f.client.set_provider_status(
        &f.admin,
        &b,
        &ProviderStatus::Suspended,
        &REASON_SUSPENDED_WHILE_TESTING,
    );
    let ng = f
        .client
        .providers_in(&String::from_str(&f.env, "NG"), &0u32, &10u32);
    assert_eq!(ng.len(), 1);
    assert_eq!(ng.get_unchecked(0).owner, a);

    // Reactivating b re-adds it once.
    f.client
        .set_provider_status(&f.admin, &b, &ProviderStatus::Active, &REASON_VERIFIED);
    let ng = f
        .client
        .providers_in(&String::from_str(&f.env, "NG"), &0u32, &10u32);
    assert_eq!(ng.len(), 2);
    assert_eq!(ng.get_unchecked(0).owner, a);
    assert_eq!(ng.get_unchecked(1).owner, b);

    // Other countries are untouched.
    assert_eq!(
        f.client
            .providers_in(&String::from_str(&f.env, "KE"), &0u32, &10u32)
            .len(),
        0
    );
}

/// `providers_in` pages through a country's active clinics, capping `limit`
/// at `MAX_PROVIDERS_PAGE`.
#[test]
fn providers_in_pages() {
    let f = setup();

    let mut providers = Vec::new(&f.env);
    for _ in 0..7 {
        let provider = Address::generate(&f.env);
        f.client.register_provider(
            &provider,
            &String::from_str(&f.env, "Clinic"),
            &String::from_str(&f.env, "NG"),
            &ProviderCategory::Clinic,
        );
        f.client.set_provider_status(
            &f.admin,
            &provider,
            &ProviderStatus::Active,
            &REASON_VERIFIED,
        );
        providers.push_back(provider);
    }

    let first = f
        .client
        .providers_in(&String::from_str(&f.env, "NG"), &0u32, &3u32);
    assert_eq!(first.len(), 3);
    assert_eq!(first.get_unchecked(0).owner, providers.get_unchecked(0));
    assert_eq!(first.get_unchecked(1).owner, providers.get_unchecked(1));
    assert_eq!(first.get_unchecked(2).owner, providers.get_unchecked(2));

    let second = f
        .client
        .providers_in(&String::from_str(&f.env, "NG"), &3u32, &3u32);
    assert_eq!(second.len(), 3);
    assert_eq!(second.get_unchecked(0).owner, providers.get_unchecked(3));
    assert_eq!(second.get_unchecked(1).owner, providers.get_unchecked(4));
    assert_eq!(second.get_unchecked(2).owner, providers.get_unchecked(5));

    let last = f
        .client
        .providers_in(&String::from_str(&f.env, "NG"), &6u32, &3u32);
    assert_eq!(last.len(), 1);
    assert_eq!(last.get_unchecked(0).owner, providers.get_unchecked(6));

    // start beyond the end returns nothing.
    assert_eq!(
        f.client
            .providers_in(&String::from_str(&f.env, "NG"), &7u32, &10u32)
            .len(),
        0
    );
}

/// The counter and index entries get their TTL extended on read, like every
/// other persistent entry.
#[test]
fn list_providers_extends_index_ttl() {
    let f = setup();
    let a = Address::generate(&f.env);
    let b = Address::generate(&f.env);
    f.client.register_provider(
        &a,
        &String::from_str(&f.env, "A Clinic"),
        &String::from_str(&f.env, "NG"),
        &ProviderCategory::Clinic,
    );
    f.client.register_provider(
        &b,
        &String::from_str(&f.env, "B Clinic"),
        &String::from_str(&f.env, "KE"),
        &ProviderCategory::Clinic,
    );

    // Jump far enough that the 90-day TTL would lapse on every entry.
    advance_days(&f, 95);

    // Reading the index restores each lapsed entry and pushes it back out.
    let providers = f.client.list_providers(&0u32, &10u32);
    assert_eq!(providers.len(), 2);
    assert_eq!(providers.get_unchecked(0).owner, a);
    assert_eq!(providers.get_unchecked(1).owner, b);

    let contract = f.client.address.clone();
    let count_key = DataKey::ProviderCount;
    let count_ttl = f.env.as_contract(&contract, || {
        f.env.storage().persistent().get_ttl(&count_key)
    });
    assert_eq!(count_ttl, NINETY_DAYS);

    let index_key = DataKey::ProviderAt(0);
    let index_ttl = f.env.as_contract(&contract, || {
        f.env.storage().persistent().get_ttl(&index_key)
    });
    assert_eq!(index_ttl, NINETY_DAYS);

    // And each provider record itself is still live.
    assert_eq!(f.client.get_provider(&a).owner, a);
    assert_eq!(f.client.get_provider(&b).owner, b);
}
