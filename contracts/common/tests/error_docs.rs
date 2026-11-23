//! Keeps `docs/developers/errors.md` in step with every contract's error enum.
//!
//! The page is the one place an integrator looks up `Error(Contract, #N)`, so
//! a variant added to the source without a row there (or a row with the wrong
//! code) fails this test.

const DOCS: &str = include_str!("../../../docs/developers/errors.md");

/// (section heading on the page, enum name, source file)
const ENUMS: &[(&str, &str, &str)] = &[
    (
        "## registry",
        "RegistryError",
        include_str!("../../registry/src/types.rs"),
    ),
    (
        "## voucher",
        "VoucherError",
        include_str!("../../voucher/src/types.rs"),
    ),
    (
        "## receipt",
        "ReceiptError",
        include_str!("../../receipt/src/lib.rs"),
    ),
    (
        "## faucet",
        "FaucetError",
        include_str!("../../faucet/src/lib.rs"),
    ),
    (
        "## sponsor",
        "SponsorError",
        include_str!("../../sponsor/src/types.rs"),
    ),
];

/// `(name, code)` for every variant of `pub enum <name> { ... }` in `source`.
fn variants(source: &str, name: &str) -> Vec<(String, u32)> {
    let start = source
        .find(&format!("pub enum {name} {{"))
        .unwrap_or_else(|| panic!("`pub enum {name}` not found in its source file"));
    let body = &source[start..];
    let body = &body[body.find('{').unwrap() + 1..body.find("\n}").unwrap()];
    body.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with("//") && !l.starts_with('#'))
        .map(|l| {
            let (variant, code) = l
                .trim_end_matches(',')
                .split_once('=')
                .unwrap_or_else(|| panic!("{name}: variant without an explicit code: {l}"));
            (
                variant.trim().to_string(),
                code.trim().parse().expect("error code is a number"),
            )
        })
        .collect()
}

/// The page's text from `heading` up to the next `## ` heading.
fn section<'a>(heading: &str) -> &'a str {
    let start = DOCS
        .lines()
        .position(|l| l == heading || l.starts_with(&format!("{heading} ")))
        .unwrap_or_else(|| panic!("docs/developers/errors.md has no `{heading}` section"));
    let offset: usize = DOCS.lines().take(start + 1).map(|l| l.len() + 1).sum();
    let rest = &DOCS[offset..];
    &rest[..rest.find("\n## ").unwrap_or(rest.len())]
}

/// `(name, code)` for every table row in `text`.
fn documented(text: &str) -> Vec<(String, u32)> {
    text.lines()
        .filter_map(|l| {
            let cells: Vec<&str> = l.split('|').map(str::trim).collect();
            let code = cells.get(1)?.parse().ok()?;
            let name = cells.get(2)?.trim_matches('`').to_string();
            Some((name, code))
        })
        .collect()
}

#[test]
fn every_error_variant_is_documented() {
    let mut problems = Vec::new();
    for (heading, name, source) in ENUMS {
        let in_source = variants(source, name);
        assert!(!in_source.is_empty(), "{name} has no variants");
        let on_page = documented(section(heading));
        for variant in &in_source {
            if !on_page.contains(variant) {
                problems.push(format!(
                    "{name}::{} = {} is missing from the `{heading}` table",
                    variant.0, variant.1
                ));
            }
        }
        for row in &on_page {
            if !in_source.contains(row) {
                problems.push(format!(
                    "`{heading}` lists `{}` = {}, which is not a {name} variant",
                    row.0, row.1
                ));
            }
        }
    }
    assert!(
        problems.is_empty(),
        "docs/developers/errors.md is out of date:\n{}",
        problems.join("\n")
    );
}
