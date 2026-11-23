---
title: Liste de contrôle de l'attesteur et code de conduite
nav_order: 9
language: fr
translations:
  en: /guides/attester-checklist.html
  sw: /sw/attester-checklist.html
---

# Liste de contrôle des preuves et code de conduite de l'attesteur

Votre attestation libère l'argent d'un financeur vers une clinique, de façon irréversible, 72 heures après votre signature. Le contrat peut vérifier *qui* vous êtes ; il ne peut pas vérifier *ce que vous avez vu*. Cette page fixe la norme de ce que vous devez voir avant d'attester, et des cas où vous ne devez pas.

Elle est écrite contre les règles que le contrat applique (voir [Attester la prestation](attesters.md) et le [modèle de confiance](../protocol/trust.md)). Là où le contrat bloque déjà quelque chose, c'est précisé, pour que vous sachiez quelles règles reposent sur vous seul.

## La liste de contrôle des preuves

Confirmez **chaque** point avant d'appuyer sur **Confirmer que les soins ont été délivrés**. Si un point ne peut pas être confirmé, n'attestez pas.

| # | Vérification | Comment le savoir | Le contrat l'applique ? |
| --- | --- | --- | --- |
| 1 | **Le patient était présent.** La personne sur le bon était physiquement à la clinique. | Vous l'avez vue, ou une trace prise lors de la visite — l'entrée du registre de la clinique, un reçu signé par le patient, le patient qui le confirme quand vous l'appelez. La parole de la clinique seule ne suffit pas. | Non — c'est pourquoi les attesteurs existent. |
| 2 | **La prestation correspond.** Ce qui a été délivré est la prestation nommée par le bon (son code et son libellé), pas une moins chère ni une autre. | L'entrée du registre, le registre de délivrance, le reçu de résultat d'analyse. | Non. Le contrat ne connaît que le code payé par le financeur. |
| 3 | **Cela s'est passé dans cette clinique.** Les soins ont été délivrés par la clinique nommée sur le bon, pas orientés ailleurs. | Le registre est celui de cette clinique. | Non. |
| 4 | **Cela s'est passé une seule fois.** Cette visite n'a pas déjà servi pour un autre bon. | Chaque entrée du registre correspond à un bon. | En partie : le contrat refuse d'attester deux fois le même bon, mais ne peut pas distinguer deux bons pour une seule visite. |
| 5 | **Le bon est `Claimed`.** La clinique a marqué le patient comme vu. | La page Attester ne liste que les bons réclamés. | Oui — attester autre chose est rejeté. |
| 6 | **Vous n'avez pas de conflit d'intérêts** (voir ci-dessous). | Vous. | En partie : l'adresse de la clinique elle-même est toujours rejetée. Rien d'autre ne l'est. |

Gardez une note des preuves sur lesquelles vous vous êtes appuyé pour chaque bon — date, ce que vous avez vérifié, à qui vous avez parlé. Chaque attestation est publique et attribuable à votre adresse pour toujours ; en cas de litige, cette note est votre version. N'enregistrez rien qui identifie le patient au-delà de ce que votre programme détient déjà ; le bon lui-même ne porte qu'une référence opaque.

## Code de conduite

**N'attestez jamais pour votre propre clinique.** Le contrat rejette une attestation venant de l'adresse de la clinique payée, mais il ne peut pas voir que vous y travaillez, que vous en possédez une part ou que vous êtes payé par elle. Si le revenu de la clinique est votre revenu, vous ne devez pas attester ses bons.

**Récusez-vous lorsque vous êtes lié.** Écartez-vous et prévenez l'administrateur pour qu'un autre attesteur prenne le bon lorsque :

- le patient est un parent ou un membre de votre foyer ;
- une personne de la clinique est un parent, un partenaire d'affaires, ou vous paie de quelque manière que ce soit ;
- vous, ou quelqu'un de proche, avez financé le bon ;
- vous avez toute autre raison pour laquelle quelqu'un pourrait raisonnablement douter de votre indépendance.

**N'attestez jamais sur la parole d'autrui.** « La clinique me l'a dit » et « le parent du patient me l'a dit » ne sont pas des preuves. Si vous n'avez pas pu confirmer la liste vous-même, la réponse est d'attendre.

**N'acceptez jamais quoi que ce soit pour une attestation.** Aucun paiement, cadeau ou faveur d'une clinique, d'un patient ou d'un financeur, avant comme après. La part d'attesteur configurée par le protocole est la seule rémunération que vous pouvez recevoir pour attester. Signalez toute offre à un administrateur.

**Ne partagez jamais votre clé.** Votre clé d'attesteur est votre signature. Si quelqu'un d'autre la détient, il peut libérer de l'argent en votre nom — la limite quotidienne d'attestations ne fait que limiter le montant avant que l'administrateur ne la retire. Si vous pensez que votre clé est exposée, prévenez immédiatement un administrateur pour qu'elle soit retirée du registre.

**Ne vous précipitez pas.** Un bon non attesté n'est jamais perdu : il reste sous séquestre et revient au financeur à l'expiration. Une attestation erronée envoie l'argent d'un patient à une clinique sans moyen de retour après 72 heures. En cas de doute, n'attestez pas.

**Signalez ce que vous voyez.** Si une clinique réclame des soins qui n'ont pas eu lieu, ou vous presse d'attester, prévenez un administrateur pour que la clinique puisse être suspendue. Signaler fait partie du rôle, ce n'est pas une trahison de la confiance de la clinique.

## Ce qui se passe si ces règles sont enfreintes

Un administrateur peut retirer un attesteur du registre à tout moment (`remove_attester`), après quoi chaque attestation que cet attesteur tente est rejetée. Les attestations déjà faites restent sur la chaîne, signées par votre adresse, et constituent la trace sur laquelle un litige est tranché.
