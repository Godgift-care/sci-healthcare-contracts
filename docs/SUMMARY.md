# Table of contents

* [What this is](README.md)
* [Glossary of protocol terms](glossary.md)

## Protocol

* [How a voucher works](protocol/lifecycle.md)
* [Economics](protocol/economics.md)
* [Storage and rent costs](protocol/costs.md)
* [Trust model](protocol/trust.md)
* [Privacy](protocol/privacy.md)

## Architecture decisions

* [Architecture decision records](adr/README.md)
* [0001 — A provider cannot attest its own voucher](adr/0001-provider-cannot-attest-its-own-voucher.md)
* [0002 — Settle and refund are permissionless](adr/0002-settle-and-refund-are-permissionless.md)
* [0003 — Beneficiary references are HMACs, not hashes](adr/0003-beneficiary-references-are-hmacs.md)
* [0004 — The contract holds no general balance](adr/0004-no-general-balance.md)

## Guides

* [Funding care for someone](guides/funders.md)
* [Funding from an organisation wallet](guides/organisation-funders.md)
* [Running a clinic](guides/clinics.md)
* [Clinic FAQ](guides/clinics-faq.md)
* [Clinic onboarding sheet (printable)](guides/clinic-onboarding-sheet.html)
* [Attesting delivery](guides/attesters.md)
* [Attester checklist and code of conduct](guides/attester-checklist.md)
* [Attester dispute examples](guides/attester-dispute-examples.md)

## Translated guides

* [Português — Financiar cuidados para alguém](pt/funders.md)
* [Português — Gerir uma clínica](pt/clinics.md)
* [Português — Atestar a prestação de cuidados](pt/attesters.md)
* [Yorùbá — Sísanwó ìtọ́jú fún ẹlòmíràn](yo/funders.md)
* [Yorùbá — Ṣíṣe àkóso ilé-ìwòsàn](yo/clinics.md)
* [Yorùbá — Jíjẹ́rìí pé a ṣe ìtọ́jú](yo/attesters.md)
* [Igbo — Ịkwụ ụgwọ ọgwụgwọ maka mmadụ](ig/funders.md)
* [Igbo — Ịgba ụlọ ọgwụ](ig/clinics.md)
* [Igbo — Ịgba akaebe na e nyere ọgwụgwọ](ig/attesters.md)

## Developers

* [Local setup](developers/setup.md)
* [Contract reference](developers/contracts.md)
* [Error code reference](developers/errors.md)
* [Reading contract events](developers/events.md)
* [Integrating from another app](developers/integration.md)
* [Running a settlement keeper](developers/keepers.md)
* [Trying it by hand in Stellar Lab](developers/stellar-lab.md)
* [Using a multisig admin](developers/multisig-admin.md)
* [Mainnet launch checklist](developers/mainnet-checklist.md)
* [Security audit scope](developers/audit-scope.md)
* [Incident response runbook](developers/incident-response.md)
* [API reference](developers/api.md)
* [Contributing](developers/contributing.md)
