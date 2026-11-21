---
title: Modèle de confiance
nav_order: 4
language: fr
translations:
  en: /protocol/trust.html
  sw: /sw/protocol/trust.html
---

# Modèle de confiance

Ce protocole est **minimisant la confiance, pas sans confiance**. Il vaut la peine d'être précis sur ce que cela signifie, car le survendre serait la chose la plus facile à faire et la plus dommageable.

## Le problème inévitable

Savoir si une infirmière a réellement examiné un patient est un fait du monde physique. Aucun contrat intelligent ne peut l'observer. Quelque chose hors chaîne doit l'affirmer, et cette affirmation peut être fausse.

Tout ce qui suit vise à réduire la marge de mensonge, pas à l'éliminer.

## Ce que le contrat garantit

Ces points tiennent quel que soit le comportement des acteurs :

1. **Les fonds ne peuvent atteindre que la clinique nommée ou revenir au financeur.** Il n'y a pas de troisième destination. Ni l'administrateur, ni le protocole, ni un attesteur.
2. **La clinique payée ne peut pas attester sa propre prestation.** Le contrat le vérifie explicitement et le rejette, même si cette adresse de clinique s'est vu attribuer un rôle d'attesteur.
3. **Une clinique ne peut pas être payée sans attestation.** Aucun chemin ne mène de `Funded` ou `Claimed` directement à `Settled`.
4. **L'argent ne peut pas être libéré trop tôt.** `settle` échoue tant que la fenêtre de litige est ouverte.
5. **Un bon non attesté revient toujours au financeur.** Non réclamé, il est remboursable à l'expiration — ou immédiatement si la clinique n'est plus active. Réclamé mais jamais attesté, sept jours après l'expiration. Personne ne peut l'empêcher — `refund` ne demande aucune autorisation.
6. **Le montant total est remboursé.** Aucun frais n'est prélevé sur des soins qui n'ont pas eu lieu.
7. **Un bon réglé ne peut plus être réglé ni remboursé.** Les états terminaux sont terminaux.
8. **Les droits d'administrateur ne peuvent pas être remis à une adresse mal saisie.** Chaque contrat transfère l'administration en deux étapes — `propose_admin`, puis `accept_admin` signé par la nouvelle adresse.

## Ce à quoi vous faites confiance

Dit clairement, parce que c'est réel :

### Les attesteurs

Un attesteur peut confirmer des soins qui n'ont jamais eu lieu. Si un attesteur et une clinique s'entendent, ils peuvent vider des bons pour des visites fictives.

Ce qui limite : les attesteurs sont nommés par l'administrateur et peuvent être révoqués ; le prestataire ne peut pas attester pour lui-même ; le financeur dispose d'une fenêtre de 72 heures pour contester avant tout mouvement d'argent ; et chaque attestation est attribuable de façon permanente sur la chaîne, donc un attesteur malhonnête laisse un dossier complet.

Ce qui l'améliorerait : un quorum d'attesteurs pour les bons de grande valeur, pour qu'une seule partie compromise ne suffise pas. C'est [un problème ouvert](https://github.com/Godgift-care/sci-healthcare-contracts/issues), pas quelque chose de déjà construit.

### L'administrateur

L'administrateur peut activer et suspendre des cliniques, nommer et révoquer des attesteurs, et trancher les litiges.

Ce qui limite : sur un bon contesté, le seul choix de l'administrateur est *laquelle des deux parties* reçoit les fonds sous séquestre. Aucun appel ne lui permet de déplacer l'argent vers une troisième adresse, d'émettre des bons ou de prendre des fonds sur un bon réglé. L'administrateur ne peut pas toucher un bon qui n'est pas `Disputed`.

La clé de l'administrateur est un point unique de défaillance aujourd'hui. La déplacer vers un multisig est une décision de déploiement, et le mainnet ne devrait pas être lancé sans cela.

### Le registre

Si une clinique qui n'aurait pas dû être vérifiée l'est, les financeurs peuvent lui envoyer de l'argent. La vérification est un processus hors chaîne — contrôle d'une licence, visite d'un site — et le registre n'enregistre que son résultat.

Chaque changement de statut dans le registre enregistre **pourquoi**, sous forme de `reason_code` stocké sur le prestataire et émis dans `provider_status_changed`. Une clinique suspendue lit son code et sait quoi corriger ; un financeur qui lit l'historique du registre peut distinguer une suspension pour fraude d'une paperasse expirée. Le registre est délibérément muet sur les valeurs — il stocke et publie le code, il ne l'interprète pas — donc les applications doivent mapper les codes elles-mêmes :

#### Codes de raison

| Code | Signification |
| --- | --- |
| 0 | Aucun changement de statut pour l'instant (défini à l'enregistrement ; `set_provider_status` le refuse avec `MissingReasonCode`) |
| 1 | Vérifiée |
| 2 | Documents expirés |
| 3 | Plainte en cours d'examen |
| 4 | Fraude |
| 5 | Fermée (donné quand une clinique se retire) |

Les codes sont **ajoutés, jamais renumérotés ni réutilisés**. Un code retiré conserve sa signification pour chaque déploiement et chaque événement historique, donc un audit des suspensions de l'année dernière se lit encore correctement. Les nouvelles significations reçoivent de nouveaux codes ajoutés ici dans la même pull request qui les émet pour la première fois.

L'autre chose que le registre enregistre à propos du statut est ce qu'il refuse : `set_provider_status` n'accepte que `Pending → Active`, `Pending → Suspended`, `Active → Suspended` et `Suspended → Active` (voir le [cycle de vie des statuts](../../developers/contracts.md#status-lifecycle), en anglais), donc l'historique des événements ne contient aucun changement sans effet ni retour en arrière inexplicable. Une clinique qui ferme se retire elle-même ; rien — pas même l'administrateur — ne peut ramener une clinique retirée, donc aucun code de raison ne sort jamais de `Retired`.

Réintégrer une clinique suspendue (`Suspended → Active`) la rouvre au financement, donc cette opération est soumise à la même règle que tout autre changement : seul l'administrateur stocké, avec sa propre signature, et toujours avec un code de raison non nul. La clinique ne peut pas se réintégrer elle-même, et ni un attesteur, ni une autre clinique, ni un administrateur nommé qui n'a pas accepté, ni un administrateur qui a remis ses droits ne le peuvent. Chacun de ces cas a un test.

### L'émetteur du jeton de règlement

L'USDC est un stablecoin régulé, et son émetteur peut **récupérer** des fonds de n'importe quel solde et **en geler** (révoquer) un. Les bons sous séquestre se trouvent dans le solde de jetons du contrat de bons lui-même, donc l'émetteur peut les atteindre ; rien dans le contrat ne peut l'en empêcher. Ce que le contrat fait, c'est se comporter de manière prévisible quand cela arrive :

- **Récupération (clawback).** Avant de payer quoi que ce soit, `settle`, `refund`, `resolve_dispute`, `resolve_split` et `reduce_amount` vérifient que le contrat détient toujours le montant du bon. Sinon, ils échouent avec `EscrowShortfall` (#23) au lieu de paniquer dans le jeton. Le séquestre est un solde unique partagé par tous les bons ouverts, donc une récupération est supportée par les bons qui puisent dedans en dernier : les premiers se règlent ou se remboursent intégralement, et les autres échouent avec `EscrowShortfall` jusqu'à ce que le solde soit restauré.
- **Gel (freeze).** Si l'émetteur a gelé le contrat, la clinique ou le financeur, le jeton refuse le transfert et l'appel échoue avec `TokenTransferFailed` (#24).
- **Jetons facturant des frais ou avec hooks.** Le contrat suppose que le jeton de règlement déplace exactement le montant demandé, ce que fait le contrat d'actif Stellar de l'USDC. Un jeton qui prélève des frais de transfert, débite davantage l'expéditeur ou exécute un hook qui modifie les soldes briserait l'invariant du séquestre sans que personne ne le remarque. Le contrat mesure donc les soldes autour de chaque transfert. Le financement vérifie que le séquestre a augmenté exactement du montant du bon. Chaque paiement vérifie que le séquestre a diminué exactement du montant envoyé et que le destinataire a augmenté exactement de ce montant. Toute différence échoue avec `TokenAmountMismatch` (#34). Les tests `nonstandard_token` dans `contracts/voucher/src/test/nonstandard_token.rs` exécutent le cycle de vie contre un jeton simulé dans chacun de ces modes.
- **Rien n'est payé à moitié.** L'une de ces erreurs annule tout l'appel : le bon conserve son statut, aucun reçu n'est émis et aucun frais n'est prélevé. Un bon bloqué par un gel se règle ou se rembourse normalement une fois le gel levé.

Ces cas sont couverts par les tests `clawback_*`, `partial_clawback_*` et `frozen_*` dans `contracts/voucher/src/test.rs`, qui s'exécutent contre un contrat d'actif Stellar avec récupération et révocation activées.

## Ce que le protocole ne prétend délibérément pas

- Il ne vérifie pas que les soins étaient *bons*, seulement que quelqu'un a attesté qu'ils ont eu lieu.
- Il ne vérifie pas l'identité. La référence du bénéficiaire est un pseudonyme.
- Il n'empêche pas une clinique de surfacturer. Il impose un plancher au prix affiché, pas un plafond sur ce qu'une clinique peut afficher.
