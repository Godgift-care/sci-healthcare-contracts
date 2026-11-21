---
title: Économie
nav_order: 3
language: fr
translations:
  en: /protocol/economics.html
  sw: /sw/protocol/economics.html
---

# Économie

## Les frais

Le protocole prélève des frais en points de base, uniquement au règlement. La configuration de production est de **100 points de base — 1 %**. Le contrat les plafonne à 1000 bps (10 %) et ce plafond est appliqué à l'initialisation, donc un déploiement mal configuré ne peut pas facturer davantage.

```
fee = amount * fee_bps / 10_000
net = amount - fee
```

Tout est en arithmétique entière. Il n'y a aucun nombre à virgule flottante nulle part dans ce protocole. La division tronque, et la troncature favorise toujours la clinique plutôt que le protocole.

### La part de l'attesteur

Les attesteurs font le travail qui rend le protocole digne de confiance, donc le règlement leur verse une part configurable des frais. `attester_share_bps` est fixé une fois à l'initialisation avec `fee_bps`, et le contrat refuse toute valeur supérieure à `fee_bps` : la part est prélevée sur les frais, jamais ajoutée par-dessus.

```
attester_share = amount * attester_share_bps / 10_000
fee_account    = fee - attester_share
```

Le `net` de la clinique ne change pas. Les trois paiements totalisent toujours exactement le montant du bon :

```
net + attester_share + fee_account = amount
```

Le déploiement de production fixe `fee_bps = 100` et `attester_share_bps = 50`, donc la moitié des frais va à l'attesteur qui a confirmé la prestation et l'autre moitié au compte de frais. `attester_share_bps = 0` verse la totalité des frais au compte de frais, exactement comme le contrat se comportait avant l'existence de la part de l'attesteur. Une contestation que l'administrateur tranche en faveur de la clinique sans attestation n'a pas d'attesteur enregistré, donc ce règlement verse aussi la totalité des frais au compte de frais.

### Chiffres calculés à 1 %

| Bon | Frais | La clinique reçoit |
| --- | --- | --- |
| 1,00 $ (test rapide du paludisme) | 0,01 $ | 0,99 $ |
| 3,00 $ (consultation externe) | 0,03 $ | 2,97 $ |
| 5,00 $ (visite prénatale) | 0,05 $ | 4,95 $ |
| 25,00 $ (petite intervention) | 0,25 $ | 24,75 $ |

Avant l'introduction de la part de l'attesteur, un bon de 3,00 $ réglait exactement `29700000` stroops à la clinique et `300000` au compte de frais. Avec la répartition de production ci-dessus, ces `300000` de frais deviennent `150000` pour l'attesteur et `150000` pour le compte de frais ; la clinique reçoit toujours `29700000`.

### Exemples de règlement en stroops

Ce que `quote` renvoie, et ce qu'un règlement paie, aux frais de production de 100 bps. L'USDC sur Stellar a 7 décimales, donc 1 USDC vaut `10,000,000` stroops ; le contrat travaille en stroops partout.

| Bon | Montant (stroops) | Frais (stroops) | La clinique reçoit (stroops) |
| --- | --- | --- | --- |
| 1 USDC | 10,000,000 | 100,000 | 9,900,000 |
| 3 USDC | 30,000,000 | 300,000 | 29,700,000 |
| 10 USDC | 100,000,000 | 1,000,000 | 99,000,000 |
| 100 USDC | 1,000,000,000 | 10,000,000 | 990,000,000 |
| 1.2345678 USDC | 12,345,678 | 123,456 | 12,222,222 |
| 0.0000099 USDC | 99 | 0 | 99 |

Les deux dernières lignes montrent la troncature : `12,345,678 × 100 / 10,000` donne `123,456.78`, et le `.78` reste à la clinique ; sur un bon de 99 stroops, les frais s'arrondissent à zéro. Un financeur qui paie plus que le prix affiché est coté sur ce qu'il paie réellement.

### Part de l'attesteur à 50 bps

Comment les frais de production se répartissent au règlement : la clinique reçoit toujours `amount - fee`, l'attesteur reçoit sa part du montant, et le compte de frais reçoit le reste. La part de l'attesteur fait partie des frais, ce n'est pas une déduction supplémentaire.

| Bon | Montant (stroops) | L'attesteur reçoit (stroops) | Le compte de frais reçoit (stroops) |
| --- | --- | --- | --- |
| 1 USDC | 10,000,000 | 50,000 | 50,000 |
| 3 USDC | 30,000,000 | 150,000 | 150,000 |
| 10 USDC | 100,000,000 | 500,000 | 500,000 |
| 100 USDC | 1,000,000,000 | 5,000,000 | 5,000,000 |
| 1.2345678 USDC | 12,345,678 | 61,728 | 61,728 |
| 0.0000099 USDC | 99 | 0 | 0 |

La dernière ligne est la même troncature en miniature : un bon de 99 stroops n'a pas de frais à partager, donc ni l'attesteur ni le compte de frais ne reçoivent quoi que ce soit.

Les trois tableaux de cette page sont vérifiés contre le contrat : `settlement_examples_table_matches_quote`, `worked_numbers_table_matches_quote` et `attester_share_table_matches_the_settlement_split` dans `contracts/voucher/src/economics_test.rs` recalculent chaque ligne et échouent si un chiffre ici est faux. Pour en vérifier un à la main sur un déploiement :

```bash
stellar contract invoke --id "$VOUCHER_CONTRACT_ID" --source-account sci-admin \
  --network testnet --send=no -- quote --amount 30000000
# ["300000","29700000"]
```

### Ce qui n'est pas facturé

- **Les remboursements sont gratuits.** Un bon expiré ou contesté avec succès rend le montant total. Aucun frais n'est prélevé sur des soins qui n'ont pas eu lieu.
- **Réduire un trop-perçu est gratuit.** `reduce_amount` rend au financeur la différence entre ce qui a été mis sous séquestre et le prix affiché de la clinique. Aucun frais n'est prélevé sur de l'argent qui n'est jamais devenu des soins.
- **Le financement est gratuit.** Les frais sont prélevés une seule fois, au règlement.
- **La part de l'attesteur fait partie des frais.** Elle sort des frais du protocole, pas du net de la clinique, donc la configurer ne réduit jamais ce que reçoit la clinique.
- **Pas de spread.** Le protocole ne convertit pas de devise et ne prend aucune marge de change.

## Coûts réseau

Les frais de transaction Stellar sont payés en XLM et représentent une fraction de centime. Au moment de la rédaction, les frais de base sont de 100 stroops — 0,00001 XLM — par opération.

Cela compte plus qu'il n'y paraît. Le cycle de vie complet d'un bon représente quatre transactions : création, réclamation, attestation, règlement. Sur une chaîne où une transaction coûte 0,50 $, cela fait 2,00 $ de frais généraux sur une consultation de 3,00 $, et le produit n'existe pas. Ici, c'est une erreur d'arrondi.

L'autre coût récurrent est la location de stockage : chaque bon, prestataire, service et reçu est une entrée de registre qui loue de l'espace pour sa durée de vie (TTL). [Coûts de stockage et de location](../../protocol/costs.md) (en anglais) liste la taille mesurée et la location de chaque objet et montre comment reproduire les chiffres.

## Qui paie quoi

| Action | Qui soumet | Qui paie les frais réseau |
| --- | --- | --- |
| `create_voucher` | Financeur | Financeur |
| `reduce_amount` | Financeur | Financeur |
| `claim` | Clinique | Clinique |
| `attest` | Attesteur | Attesteur |
| `settle` | N'importe qui | Celui qui soumet |
| `refund` | N'importe qui | Celui qui soumet |

Comme `settle` et `refund` sont sans permission, un opérateur peut lancer un keeper qui règle les bons éligibles pour tout le monde et absorbe ces frais, y compris la location du reçu que `settle` émet ([Exploiter un keeper de règlement](../../developers/keepers.md#what-it-costs), en anglais). Rien dans le protocole ne dépend de l'existence de ce keeper — c'est une commodité, pas une dépendance.

## Hypothèse : le jeton déplace des montants exacts

Chaque chiffre de cette page suppose qu'un transfert de `x` déplace exactement `x` : l'expéditeur perd `x` et le destinataire gagne `x`. Le contrat d'actif Stellar de l'USDC se comporte ainsi. Un jeton avec des frais de transfert, une destruction au transfert ou des hooks qui modifient les soldes ne se comporte pas ainsi, et une mise sous séquestre construite dessus enregistrerait plus qu'elle ne détient.

Le contrat de bons ne fait pas confiance aveuglément à cette hypothèse. Il vérifie les soldes autour de chaque transfert et échoue en se fermant avec `TokenAmountMismatch` (#34) si les montants diffèrent, donc un tel jeton ne peut pas être utilisé. Il n'en prend pas un en charge non plus. Ne déployez que contre un contrat d'actif Stellar standard ; le point 3.11 de la [liste de contrôle du mainnet](../../developers/mainnet-checklist.md#3-parameters) (en anglais) en fait une condition de lancement.

## Pourquoi il n'y a pas de flottant

Le contrat ne détient aucun solde et n'a pas de fonction de dépôt. L'argent n'entre que sous forme de bon déjà lié à une clinique, une prestation et une expiration, et il ne peut sortir que vers cette clinique ou revenir au financeur.

C'est une contrainte de conception délibérée, pas un oubli, et c'est la différence entre un bon prépayé et une valeur stockée. Un solde rechargeable général est de la monnaie électronique dans la plupart des juridictions et entraîne une licence. Un bon prépayé pour un prestataire identifié et une prestation identifiée relève généralement des dérogations de réseau limité.

Le protocole ne **porte aucun risque** non plus. Il ne met pas en commun des primes, ne couvre pas de risque et ne paie pas sur un événement de santé. Dès qu'un système prend de l'argent et promet de payer pour un besoin médical futur et incertain, c'est de l'assurance et il lui faut une licence. Ce protocole ne déplace que de l'argent qu'un financeur a déjà engagé pour un achat précis.
