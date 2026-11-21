---
title: Confidentialité
nav_order: 5
---

# Confidentialité

🇬🇧 [Lire cette page en anglais](../../protocol/privacy.md) · 🇰🇪🇹🇿 [Soma ukurasa huu kwa Kiswahili](../../sw/protocol/privacy.md)

La règle la plus difficile de ce projet : **aucune information de santé protégée n'atteint le registre, jamais.**

Une blockchain publique est permanente et mondiale. Tout ce qui y est écrit l'est pour de bon. Les données de santé n'y ont pas leur place, et la plupart des tentatives de les y mettre — chiffrées, hachées ou autrement — finissent par fuir via les métadonnées ou la compromission d'une clé.

## Ce qui est sur la chaîne

Pour chaque bon :

| Champ | Exemple | Ce que cela révèle |
| --- | --- | --- |
| `funder` | `GDUP…JG45` | Une adresse Stellar qui a payé |
| `provider` | `GDOO…ZKBK` | Quelle clinique |
| `service_code` | `101` | Une catégorie grossière, par ex. « consultation externe » |
| `amount` | `30000000` | 3,00 $ |
| `beneficiary_ref` | `72676a6f…094745` | 32 octets opaques |
| horodatages | | Quand le bon a été financé, réclamé, attesté, réglé |

## Ce qui ne l'est pas, et ne le sera jamais

- Noms, dates de naissance, numéros de téléphone, adresses
- Diagnostics, symptômes, résultats d'analyses, ordonnances
- Tout champ de texte libre qu'un clinicien pourrait remplir

Il n'y a pas de champ pour cela. En ajouter un serait rejeté en revue.

## Les codes de prestation restent grossiers à dessein

`101` signifie « consultation externe ». Cela ne signifie pas « charge virale VIH ».

C'est une vraie tension de conception. Des codes plus fins rendraient le produit plus utile — de meilleures analyses, de meilleurs prix, une meilleure détection de la fraude. Ils transformeraient aussi le registre en un dossier public permanent des traitements des gens. Un code assez précis pour être cliniquement utile est assez précis pour identifier quelqu'un.

Alors les codes restent grossiers, et le protocole accepte d'être moins utile pour cela.

## Les références de bénéficiaire

La référence est un HMAC-SHA256 d'un identifiant de patient sous une clé secrète :

```
beneficiary_ref = HMAC-SHA256(key, lowercase(trim(identifier)))
```

Calculée dans le navigateur du patient via Web Crypto. L'identifiant et la clé n'atteignent jamais l'API, ne sont jamais journalisés et ne touchent jamais le registre.

**Pourquoi HMAC et pas un simple hachage.** Un `SHA256(phone_number)` simple est trivialement réversible par force brute — il n'y a qu'un nombre fini de numéros de téléphone, et un attaquant peut tous les hacher. Un HMAC sous une clé secrète n'est pas recherchable sans cette clé.

**Ce que cela vous apporte.** Deux bons pour la même personne produisent la même référence, donc un historique de soins peut être reconstitué. Quelqu'un qui ne détient que des données on-chain ne peut pas savoir à qui appartient cet historique, ni énumérer les patients.

**Ce que cela ne vous apporte pas.** C'est du pseudonymat, pas de l'anonymat. Quiconque connaît déjà l'identifiant et la clé peut confirmer une correspondance. Quelqu'un qui voit un patient précis entrer dans une clinique précise à un moment précis peut corréler cela avec un événement on-chain. La réutilisation d'adresse par un financeur relie ses bons entre eux.

Quiconque affirme qu'une blockchain publique garantit l'anonymat des patients se trompe ou vend quelque chose.

## La base de données

La base de données de l'indexeur ne stocke rien qui ne soit déjà public sur la chaîne. C'est une contrainte délibérée, et cela signifie qu'une violation de cette base n'est pas une violation de données de santé. C'est une violation d'un cache d'informations publiques.
