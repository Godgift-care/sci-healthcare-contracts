---
title: FAQ clinique
nav_order: 9
language: fr
translations:
  en: /guides/clinics-faq.html
  sw: /sw/clinics-faq.html
---

# FAQ clinique

Questions courantes sur la gestion d'une clinique sur le protocole, chacune avec un lien vers la source. Les chiffres (frais, délais) correspondent aux constantes du contrat dans [la référence du contrat](../developers/contracts.md) et `scripts/deploy.sh`.

## Quand suis-je payé ?

Une fois la fenêtre de litige fermée. Dès qu'un attesteur confirme la prestation, la fenêtre court pendant 72 heures dans la configuration de production. À sa fermeture, n'importe qui — y compris vous — peut appeler `settle`, et vous recevez le montant du bon moins les 1 % de frais du protocole. Une consultation externe de 3,00 $ vous rapporte 2,97 $ ; le règlement est automatique et irréversible, et aucun opérateur ne peut le retenir.

*Sources : [Fonctionnement d'un bon — Attested et Settled](../protocol/lifecycle.md#attested), [Économie — les frais](../protocol/economics.md#the-fee), [Gérer une clinique](clinics.md)*

## Et si le patient ne vient jamais ?

Si le bon n'est jamais réclamé, il expire et vous ne pouvez pas être payé dessus — le financeur peut récupérer le montant total sans frais. Si la clinique est suspendue avant que le bon soit réclamé, le financeur peut être remboursé immédiatement au lieu d'attendre l'expiration. Si vous l'avez réclamé mais qu'aucun attesteur ne confirme jamais la visite, l'argent revient tout de même au financeur sept jours après l'expiration (le délai de grâce après réclamation) ; vous n'avez pas droit à un paiement sans attestation.

*Sources : [Fonctionnement d'un bon — Refunded](../protocol/lifecycle.md#refunded), [Financer des soins](funders.md)*

## Comment fonctionnent les litiges ?

Le financeur peut appuyer sur **Contester** tant que le bon est `Claimed` ou `Attested`, mais seulement avant la fermeture de la fenêtre de litige — 72 heures après l'attestation en production. Les fonds sont gelés sous séquestre et un administrateur tranche en faveur du financeur ou de la clinique. L'administrateur ne peut jamais envoyer l'argent à un tiers, et une fois la fenêtre fermée le paiement est automatique et ne peut pas être repris.

*Sources : [Fonctionnement d'un bon — Disputed](../protocol/lifecycle.md#disputed), [Modèle de confiance — l'administrateur](../protocol/trust.md#the-admin)*

## Quels frais s'appliquent ?

Les frais du protocole sont de 1 % (100 points de base), prélevés une fois au règlement. Ils sont plafonnés à 1000 points de base (10 %) à l'initialisation, donc un déploiement ne peut jamais être reconfiguré pour facturer davantage. Les remboursements sont gratuits et le financement est gratuit ; vous payez aussi les frais de réseau Stellar uniquement pour les transactions que vous soumettez (réclamation, et règlement si vous le déclenchez vous-même), soit une fraction de centime en XLM.

*Sources : [Économie — les frais](../protocol/economics.md#the-fee), [Économie — qui paie quoi](../protocol/economics.md#who-pays-what), [Référence du contrat — initialize](../developers/contracts.md#voucher)*

## Puis-je réclamer après l'expiration du bon ?

Non. Un bon ne peut pas être réclamé après son `expires_at`, et une clinique suspendue après le financement du bon ne peut pas le réclamer du tout. Si le patient se présente après l'expiration, le financeur récupère l'argent. Si la clinique est suspendue avant que le bon soit réclamé, le financeur peut être remboursé immédiatement.

*Sources : [Fonctionnement d'un bon — Claimed](../protocol/lifecycle.md#claimed), [Référence du contrat — claim](../developers/contracts.md#voucher)*

## Puis-je attester mon propre bon ?

Non. Le contrat rejette `attest` lorsque l'appelant est la clinique payée, même si cette adresse détient un rôle d'attesteur. Vous devez passer par un attesteur indépendant — un agent de santé communautaire, une infirmière superviseuse d'un autre établissement, ou équivalent — nommé par l'administrateur.

*Sources : [Modèle de confiance — ce que le contrat garantit](../protocol/trust.md#what-the-contract-does-guarantee), [Attester la prestation](attesters.md)*

## Et si ma clinique est suspendue ?

Une clinique suspendue ne peut recevoir de nouveaux bons ni modifier ses prix, mais les bons déjà financés se règlent normalement. Vous pouvez continuer à dispenser les soins sur les bons existants et être payé une fois qu'ils sont attestés et que la fenêtre de litige est fermée.

*Sources : [Gérer une clinique](clinics.md)*

## Qui paie les frais de réseau Stellar ?

Celui qui soumet la transaction : vous payez `claim`, l'attesteur paie `attest`, le financeur paie `create_voucher`, et celui qui déclenche `settle` ou `refund` paie pour ceux-ci. Comme `settle` et `refund` sont sans permission, un opérateur peut lancer un keeper qui règle les bons éligibles pour tout le monde et absorbe ces frais — mais le protocole ne dépend pas de son existence.

*Sources : [Économie — coûts réseau](../protocol/economics.md#network-costs), [Économie — qui paie quoi](../protocol/economics.md#who-pays-what)*

## Quelqu'un d'autre peut-il libérer mon paiement ?

Oui. `settle` ne demande aucune autorisation : une fois la fenêtre de litige fermée, n'importe quel appelant peut déclencher le règlement de votre bon attesté — vous n'avez jamais à relancer le financeur ou un opérateur pour être payé.

*Sources : [Économie — qui paie quoi](../protocol/economics.md#who-pays-what), [Référence du contrat — settle](../developers/contracts.md#voucher)*

## Dans quelle monnaie suis-je payé, et comment obtenir des espèces locales ?

Le règlement se fait en USDC. La conversion en naira, shillings ou espèces passe par un ancrage Stellar — Yellowcard, Cowrie, HoneyCoin ou MoneyGram — une étape distincte et réglementée avec ses propres vérifications d'identité.

*Sources : [Gérer une clinique](clinics.md)*

## Existe-t-il des frais maximaux que le protocole pourrait facturer ?

Oui. Les frais sont plafonnés à 1000 points de base (10 %) et ce plafond est appliqué à l'initialisation du contrat de bons, avant tout mouvement d'argent. La production est à 100 points de base (1 %).

*Sources : [Économie — les frais](../protocol/economics.md#the-fee), [Référence du contrat — initialize](../developers/contracts.md#voucher)*

## Combien de temps un financeur a-t-il pour contester un bon ?

Ce n'est pas une question de clinique, mais on vous la posera : le financeur peut contester tant que le bon est `Claimed`, ou tant qu'il est `Attested` et avant l'échéance — 72 heures après l'attestation en production. Une fois la fenêtre fermée, le règlement est automatique.

*Sources : [Fonctionnement d'un bon — Attested](../protocol/lifecycle.md#attested), [Financer des soins](funders.md)*

## Le protocole garantit-il que les soins étaient bons ?

Non. Il garantit qu'un attesteur indépendant a déclaré que les soins ont eu lieu, que vous avez eu une fenêtre pour vous y opposer, et que l'argent ne peut aller qu'à votre clinique ou revenir au financeur. La qualité des soins relève de vous, du patient et de vos autorités de régulation.

*Sources : [Modèle de confiance — ce que le protocole ne prétend délibérément pas](../protocol/trust.md#what-the-protocol-deliberately-does-not-claim)*
