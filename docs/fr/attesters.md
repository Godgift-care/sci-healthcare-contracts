---
title: Attester la prestation de soins
nav_order: 8
---

# Attester la prestation de soins

🇬🇧 [Lire ce guide en anglais](../attesters.md)

Un attesteur confirme que les soins ont bien eu lieu. C'est le rôle le plus important du protocole et celui qui porte le plus de responsabilité.

## Ce que vous faites

Quand vous attestez, vous déclenchez une horloge de 72 heures. Quand elle est écoulée, la clinique est payée automatiquement et de façon irréversible.

Vous n'approuvez pas une demande de paiement. Vous affirmez un fait sur le monde réel : ce patient a reçu ce service dans cette clinique.

## Qui fait cela

Les attesteurs sont nommés par un administrateur. Dans la pratique, ce sont des personnes déjà présentes dans le parcours de soins et n'étant pas employées par la clinique payée — agents de santé communautaires, infirmières superviseuses d'un autre établissement, personnel de terrain d'ONG, agents de santé de district.

Le but est l'indépendance. Si le revenu de l'attesteur dépend du paiement de la clinique, la sauvegarde ne fonctionne pas.

## Comment faire

1. **Connectez votre portefeuille.** La page Attester affiche tous les bons en attente.
2. **Vérifiez que les soins ont eu lieu.** Comme votre programme le fait — en étant présent, en consultant le registre, en parlant au patient.
3. **Appuyez sur « Confirmer que les soins ont été délivrés »**.

Avant d'attester, consultez toujours la [liste de contrôle des preuves et le code de conduite](attester-checklist.md).

## Ce que le contrat impose

- **Vous ne pouvez pas attester pour une clinique qui est la vôtre.** Si l'adresse de l'attesteur correspond à l'adresse du fournisseur, l'appel est rejeté. Toujours, sans exception.
- **Vous devez être un attesteur autorisé.** Les attesteurs révoqués sont rejetés.
- **Le bon doit être `Claimed`.** Vous ne pouvez pas attester avant que la clinique marque le patient comme vu, ni deux fois.

## Ce que le contrat n'impose pas

Que vous dites la vérité. C'est l'hypothèse de confiance centrale du protocole, documentée clairement dans le [modèle de confiance](../protocol/trust.md).

Ce qu'il fait : votre attestation est signée par votre adresse et enregistrée sur la chaîne de façon permanente. Chaque bon que vous avez jamais confirmé est public et attribuable. Il n'y a pas d'attestation anonyme.

## Si vous avez un doute

N'attestez pas. Un bon non attesté n'est pas perdu — il reste sous séquestre et est remboursé au financeur à l'expiration. Le mode d'échec de ne pas attester est un retard. Le mode d'échec d'une attestation erronée est l'argent d'un patient allant à une clinique qui n'a rien fait, sans moyen de le récupérer après 72 heures.

Si vous pensez qu'une clinique réclame des soins qu'elle n'a pas dispensés, informez un administrateur afin qu'il puisse la suspendre.
