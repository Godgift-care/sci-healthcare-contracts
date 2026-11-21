---
title: Financer des soins pour quelqu'un
nav_order: 6
---

# Financer des soins pour quelqu'un

🇬🇧 [Lire ce guide en anglais](../guides/funders.md)

Vous payez un traitement précis dans une clinique précise, pour vous-même ou pour quelqu'un d'autre.

## Ce dont vous avez besoin

- Un portefeuille Stellar. [Freighter](https://freighter.app) est le choix habituel.
- Votre portefeuille configuré sur **Testnet** tant que le protocole est en phase de test.
- Quelques USDC dans ce portefeuille, et une ligne de confiance pour cet actif (voir ci-dessous). Sur testnet, obtenez des USDC de test gratuits depuis l'application : **Mes bons → Obtenir des USDC de test** (une fois par jour et par portefeuille).
- La référence clinique du patient et une clé secrète sur laquelle vous vous êtes mis d'accord.

## À propos de la ligne de confiance

Stellar exige que vous acceptiez explicitement de détenir un actif avant de pouvoir le recevoir. C'est ce qu'on appelle une ligne de confiance, et sans ligne de confiance pour l'USDC votre portefeuille ne peut pas le détenir et le financement échouera.

Sur testnet, ajoutez-la manuellement dans votre portefeuille. Dans Freighter : **Gérer les actifs → Ajouter un actif**, code `USDC`, émetteur `GCKX5KAFVF7GLFZ3JWQGJ4F62RYYZU67PI3TACBGKVPJ73JELMI3L4KR`.

## Financement

1. **Connectez votre portefeuille.** En haut à droite. Rien n'est stocké ; vous pouvez vous déconnecter à tout moment.
2. **Choisissez la clinique.** L'annuaire affiche uniquement les cliniques vérifiées, avec leurs prix affichés.
3. **Choisissez le service.** Les prix sont fixés par la clinique et publiés sur la chaîne.
4. **Saisissez la référence du patient et la clé secrète.**

   La référence est ce qui identifie le patient dans cette clinique — un numéro de carte, un numéro d'inscription. La clé est un secret partagé d'au moins 32 caractères.

   Ces deux éléments sont combinés *dans votre navigateur* en une valeur opaque. Rien ne quitte votre appareil. Rien ne nous est envoyé. Rien ne va sur la chaîne.

   **Utilisez toujours la même référence et la même clé pour une même personne.** Ce sont elles qui lient ensemble leurs épisodes de soins. Si vous perdez la clé, les reçus passés ne peuvent plus être retrouvés. Il n'y a pas de récupération — c'est le prix de ce que nous ne détenons pas.

   La page [Confidentialité](protocol/privacy.md) explique ce qui reste hors de la chaîne et ce que la référence révèle.

5. **Confirmez le montant.** Vous pouvez payer plus que le prix affiché, jamais moins.
6. **Signez.** Votre portefeuille affiche la transaction. En l'approuvant, les USDC sont transférés dans le contrat.

Votre argent est maintenant sous séquestre. Il n'est pas encore celui de la clinique.

## Après le financement

Suivez-le sous **Mes bons**.

| Statut | Ce que cela signifie |
| --- | --- |
| Funded | Sous séquestre. Le patient ne s'est pas encore présenté. |
| Claimed | La clinique déclare que le patient s'est présenté. |
| Attested | Un attesteur a confirmé la prestation. La fenêtre de litige est ouverte. |
| Settled | La clinique a été payée. |
| Refunded | L'argent vous est retourné. |

Chaque étape est détaillée dans [Fonctionnement d'un bon](protocol/lifecycle.md), et les frais, leur part pour l'attesteur et les montants nets dans [Économie](protocol/economics.md).

## Si quelque chose se passe mal

**Le patient ne s'est jamais présenté.** Attendez la date d'expiration, puis appuyez sur **Me rembourser**. Vous récupérez le montant total, sans frais. N'importe qui peut déclencher ce remboursement, vous ne dépendez donc pas de nous.

**La clinique a réclamé, mais personne n'a confirmé la visite.** Vous pouvez contester immédiatement. Si vous ne faites rien, le bon devient remboursable sept jours après sa date d'expiration et **Me rembourser** réapparaît.

**La clinique a réclamé mais n'a pas traité le patient.** Appuyez sur **Contester** tant que le statut est `Claimed` ou `Attested`. Les fonds sont gelés et un administrateur examine le dossier. Faites-le avant que la fenêtre de litige ne se ferme — 72 heures après l'attestation. Une fois fermée, le règlement est automatique et irréversible.

**La fenêtre est fermée et vous l'avez manquée.** L'argent est allé à la clinique. Le protocole ne peut pas le récupérer. Réglez la question directement avec la clinique.

## Ce que cela ne fait pas

Cela ne garantit pas que les soins étaient *bons*. Cela garantit qu'une partie indépendante a attesté que la prestation a eu lieu, et que vous avez eu une fenêtre pour vous y opposer.

Le [modèle de confiance](protocol/trust.md) détaille ce que le contrat garantit, ce à quoi vous faites confiance et ce que le protocole ne prétend pas.
