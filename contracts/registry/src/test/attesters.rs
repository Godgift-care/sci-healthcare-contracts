//! Attester membership and the attester index.

use super::*;

#[test]
fn attester_lifecycle() {
    let f = setup();
    let attester = Address::generate(&f.env);
    assert!(!f.client.is_attester(&attester));

    f.client.add_attester(&f.admin, &attester);
    assert!(f.client.is_attester(&attester));

    f.client.remove_attester(&f.admin, &attester);
    assert!(!f.client.is_attester(&attester));
}

#[test]
fn list_attesters_lifecycle() {
    let f = setup();
    let a = Address::generate(&f.env);
    let b = Address::generate(&f.env);
    let c = Address::generate(&f.env);

    f.client.add_attester(&f.admin, &a);
    f.client.add_attester(&f.admin, &b);
    f.client.add_attester(&f.admin, &c);

    let list = f.client.list_attesters(&0u32, &10u32);
    assert_eq!(list.len(), 3);
    assert_eq!(list.get_unchecked(0), a);
    assert_eq!(list.get_unchecked(1), b);
    assert_eq!(list.get_unchecked(2), c);

    // Remove the middle one (b): swap-and-pop replaces it with c.
    f.client.remove_attester(&f.admin, &b);
    let list = f.client.list_attesters(&0u32, &10u32);
    assert_eq!(list.len(), 2);
    assert_eq!(list.get_unchecked(0), a);
    assert_eq!(list.get_unchecked(1), c);

    // Remove c: only a remains.
    f.client.remove_attester(&f.admin, &c);
    let list = f.client.list_attesters(&0u32, &10u32);
    assert_eq!(list.len(), 1);
    assert_eq!(list.get_unchecked(0), a);

    // Remove a: list is empty.
    f.client.remove_attester(&f.admin, &a);
    assert_eq!(f.client.list_attesters(&0u32, &10u32).len(), 0);
}

#[test]
fn list_attesters_pagination() {
    let f = setup();
    let mut addrs = std::vec::Vec::new();
    for _ in 0..5 {
        let addr = Address::generate(&f.env);
        f.client.add_attester(&f.admin, &addr);
        addrs.push(addr);
    }

    let p1 = f.client.list_attesters(&0u32, &2u32);
    assert_eq!(p1.len(), 2);
    assert_eq!(p1.get_unchecked(0), addrs[0]);
    assert_eq!(p1.get_unchecked(1), addrs[1]);

    let p2 = f.client.list_attesters(&2u32, &2u32);
    assert_eq!(p2.len(), 2);
    assert_eq!(p2.get_unchecked(0), addrs[2]);
    assert_eq!(p2.get_unchecked(1), addrs[3]);

    let p3 = f.client.list_attesters(&4u32, &2u32);
    assert_eq!(p3.len(), 1);
    assert_eq!(p3.get_unchecked(0), addrs[4]);

    let p_empty = f.client.list_attesters(&5u32, &2u32);
    assert_eq!(p_empty.len(), 0);

    let p_zero_limit = f.client.list_attesters(&0u32, &0u32);
    assert_eq!(p_zero_limit.len(), 0);

    let p_start_out_of_bounds = f.client.list_attesters(&100u32, &10u32);
    assert_eq!(p_start_out_of_bounds.len(), 0);

    let p_large_limit = f.client.list_attesters(&0u32, &100u32);
    assert_eq!(p_large_limit.len(), 5);

    // Removing an address that is not an attester does not fail and does not panic
    let stranger = Address::generate(&f.env);
    f.client.remove_attester(&f.admin, &stranger);
}

#[test]
fn list_attesters_duplicate_add_no_duplicate_index() {
    let f = setup();
    let a = Address::generate(&f.env);

    f.client.add_attester(&f.admin, &a);
    f.client.add_attester(&f.admin, &a);

    let list = f.client.list_attesters(&0u32, &10u32);
    assert_eq!(list.len(), 1);
    assert_eq!(list.get_unchecked(0), a);
}

#[test]
fn non_admin_cannot_add_attester() {
    let f = setup();
    let impostor = Address::generate(&f.env);
    let attester = Address::generate(&f.env);

    let err = f
        .client
        .try_add_attester(&impostor, &attester)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, RegistryError::NotAuthorized);
}

#[test]
fn attester_cannot_reinstate_a_provider() {
    let f = setup();
    let provider = suspended_provider(&f);
    let attester = Address::generate(&f.env);
    f.client.add_attester(&f.admin, &attester);
    assert_eq!(
        try_reinstate_as(&f, &attester, &provider),
        RegistryError::NotAuthorized
    );
    assert_still_suspended(&f, &provider);
}
