---
title: Gérer une clinique
nav_order: 7
---

# Gérer une clinique

🇬🇧 [Lire ce guide en anglais](../clinics.md)

Vous êtes une clinique, une pharmacie ou un laboratoire qui souhaite être payé via le protocole.

## Se faire vérifier

1. **Connectez le portefeuille de la clinique** sur la page Bureau de la clinique. Cette adresse recevra chaque paiement — utilisez-en une que vous contrôlez et pouvez garder sécurisée.
2. **Enregistrez-vous.** Saisissez le nom de la clinique et le code pays à deux lettres. Cela vous inscrit au registre comme `Pending`.
3. **Faites-vous vérifier.** Un administrateur examine la clinique — licence, localisation, contact — et vous fait passer à `Active`. Tant que ce n'est pas fait, vous ne pouvez pas lister de services ni recevoir de bons.

La vérification est un processus humain. S'enregistrer ne signifie pas être vérifié.

## Lister des services

Une fois active, listez ce que vous facturez. Chaque service nécessite :

- **Un code.** Un numéro identifiant la catégorie. Utilisez un schéma cohérent.
- **Un libellé.** Ce que les patients verront, par exemple "Consultation externe".
- **Un prix** en USDC.

> Gardez les libellés et les codes **grossiers**. "Consultation externe", et non "Séance de conseil VIH". Ils sont publiés sur une chaîne publique de façon permanente, et un code assez précis pour être cliniquement utile est assez précis pour exposer un patient.

Les prix sont un plancher. Un financeur peut payer plus, jamais moins. Mettez à jour un prix à tout moment — les bons déjà financés conservent le montant convenu à leur création.

## Être payé

1. **Le patient arrive.** Trouvez son bon sur le Bureau de la clinique et appuyez sur **Marquer le patient comme vu**. C'est `claim`.
2. **Dispenser les soins.**
3. **Un attesteur confirme.** Pas vous — le contrat rejette une attestation venant de la clinique qui est payée. C'est la principale sauvegarde du protocole et vous ne pouvez pas la contourner.
4. **La fenêtre de litige s'exécute.** 72 heures durant lesquelles le financeur peut s'opposer.
5. **Vous êtes payé.** Après la fenêtre, n'importe qui peut déclencher le règlement, y compris vous. Appuyez sur **Libérer le paiement**.

Vous recevez le montant du bon moins les 1 % de frais du protocole. Un bon de 3,00 $ vous rapporte 2,97 $.

## Ce qu'il vaut la peine de savoir

**Vous ne dépendez de personne pour être payé.** Le règlement est sans permission. Une fois la fenêtre fermée, les fonds vous appartiennent et aucun opérateur ne peut les retenir.

**Réclamez rapidement.** Un bon ne peut pas être réclamé après son expiration — le financeur le récupère.

**Suspension.** Un administrateur peut suspendre une clinique. Une clinique suspendue ne peut recevoir de nouveaux bons ni modifier ses prix. Les bons déjà financés se règlent normalement.

**Obtenir de la monnaie locale.** Le règlement est en USDC. La conversion en naira, shillings ou espèces passe par un ancrage Stellar — Yellowcard, Cowrie, HoneyCoin, MoneyGram. C'est une étape distincte et réglementée avec ses propres vérifications d'identité. L'intégration dans l'application est [un problème ouvert](https://github.com/Godgift-care/sci-healthcare-frontend/issues), pas encore construit.

## Plus de questions

Voir la [FAQ clinique](clinics-faq.md) pour les délais de paiement, les litiges, les frais et d'autres questions courantes, chacune avec un lien vers la source.
