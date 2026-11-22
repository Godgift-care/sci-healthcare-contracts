---
title: "Jíjẹ́rìí pé a ṣe ìtọ́jú"
nav_order: 8
language: yo
translations:
  en: /guides/attesters.html
  fr: /fr/attesters.html
  sw: /sw/attesters.html
  pt: /pt/attesters.html
  ig: /ig/attesters.html
  ha: /ha/attesters.html
  ar: /ar/attesters.html
  am: /am/attesters.html
---

# Jíjẹ́rìí pé a ṣe ìtọ́jú

Attester (ẹlẹ́rìí) ń fìdí rẹ̀ múlẹ̀ pé ìtọ́jú ṣẹlẹ̀ lóòótọ́. Èyí ni ipa tí ó ṣe pàtàkì jùlọ nínú protocol náà, òun sì ni ó gbé ẹrù-iṣẹ́ tí ó pọ̀ jùlọ.

## Ohun tí ẹ ń ṣe

Nígbà tí ẹ bá jẹ́rìí, ẹ ń bẹ̀rẹ̀ aago wákàtí 72. Nígbà tí ó bá parí, a ó san owó fún ilé-ìwòsàn fúnra rẹ̀ láìṣeé yí padà, ẹ ó sì gba ìpín yín tí a ti ṣètò láti inú owó ìṣẹ́ protocol.

Ẹ kò fọwọ́ sí ìbéèrè ìsanwó. Ẹ ń sọ òtítọ́ kan nípa ayé: aláìsàn yìí gba iṣẹ́ ìtọ́jú yìí ní ilé-ìwòsàn yìí.

## Ta ni ó ń ṣe èyí

Alábòójútó (administrator) ni ó ń yan àwọn attester. Ní ìṣe, wọ́n jẹ́ ènìyàn tí ó ti wà nínú ọ̀nà ìtọ́jú náà tẹ́lẹ̀, tí kì í ṣe òṣìṣẹ́ ilé-ìwòsàn tí a ń sanwó fún — òṣìṣẹ́ ìlera àdúgbò, nọ́ọ̀sì alábòójútó ní ilé-ìwòsàn míràn, òṣìṣẹ́ pápá NGO, òṣìṣẹ́ ìlera ẹkùn.

Kókó rẹ̀ ni òmìnira. Tí owó-oṣù attester bá gbára lé sísanwó fún ilé-ìwòsàn, ààbò náà kò ní ṣiṣẹ́.

## Owó tí ẹ ń gbà

Protocol ń san ìpín tí a ti ṣètò láti inú owó ìṣẹ́ rẹ̀ fún attester tí ó fìdí ìtọ́jú múlẹ̀. Ìfilọ́lẹ̀ (deployment) náà ń ṣètò `attester_share_bps` nígbà ìbẹ̀rẹ̀ (50 bps, ìdajì owó ìṣẹ́ 1%, nínú ètò lọ́wọ́lọ́wọ́). A ń gé e láti inú owó ìṣẹ́ protocol, kì í ṣe láti inú owó àpapọ̀ ilé-ìwòsàn, contract (àdéhùn oní-kóòdù) náà sì ń san án nígbà settlement (ìsanwó ìkẹyìn).

Ètò àti contract ni ó ń pinnu ìsanwó yẹn. Ilé-ìwòsàn kò lè fi lélẹ̀, kò lè yí i padà, tàbí fi kún un fún ìpinnu tí ó tẹ́ ẹ lọ́rùn. Ẹ kà á sí ẹ̀san protocol fún ìjẹ́rìí náà, kì í ṣe ìsanwó láti ọ̀dọ̀ ilé-ìwòsàn, aláìsàn tàbí olùsanwó láé.

## Bí a ṣe ń ṣe é

1. **Ẹ so wallet (àpò owó oní-nọ́ńbà) yín pọ̀.** Ojú-ìwé Attest ń fi gbogbo voucher (ìwé-ẹ̀rí ìsanwó) tí ó ń dúró hàn.
2. **Ẹ rí i dájú pé ìtọ́jú náà ṣẹlẹ̀.** Bí ètò yín ṣe ń ṣe é — wíwà níbẹ̀, ṣíṣàyẹ̀wò ìwé ìforúkọsílẹ̀, bíbá aláìsàn sọ̀rọ̀.
3. **Ẹ tẹ Confirm care delivered.**

## Ohun tí contract náà ń mú ṣẹ

- **Ẹ kò lè jẹ́rìí fún ilé-ìwòsàn tí ó jẹ́ ẹ̀yin fúnra yín.** Tí àdírẹ́sì attester bá bá àdírẹ́sì olùpèsè (provider) mu, a ó kọ ìpè náà. Nígbà gbogbo, láìsí àyàfi.
- **Ẹ gbọdọ̀ jẹ́ attester tí a fún ní àṣẹ.** A ń kọ àwọn attester tí a ti gba àṣẹ wọn padà.
- **Voucher náà gbọdọ̀ wà ní `Claimed`.** Ẹ kò lè jẹ́rìí kí ilé-ìwòsàn tó sàmì sí i pé ó ti rí aláìsàn, tàbí lẹ́ẹ̀mejì.

## Ohun tí kò mú ṣẹ

Pé ẹ ń sọ òtítọ́. Èyí ni ìgbàgbọ́ ìgbẹ́kẹ̀lé pàtàkì protocol náà, tí a kọ sílẹ̀ ní kedere nínú [àwòṣe ìgbẹ́kẹ̀lé (trust model)](../protocol/trust.md).

Ohun tí ó ń ṣe: àdírẹ́sì yín ni ó buwọ́lu ìjẹ́rìí yín, a sì kọ ọ́ sílẹ̀ títí láé lórí blockchain. Gbogbo voucher tí ẹ ti fìdí rẹ̀ múlẹ̀ rí wà ní gbangba, ó sì ṣeé tọ́ka sí yín. Kò sí ìjẹ́rìí aláìlórúkọ.

## Kí ẹ tó jẹ́rìí

Ẹ máa lọ nípasẹ̀ [àkójọ ẹ̀rí attester àti ìlànà ìwà](../guides/attester-checklist.md) ní gbogbo ìgbà: ohun tí ẹ gbọdọ̀ fìdí rẹ̀ múlẹ̀ kí ẹ tó tẹ **Confirm care delivered**, àti ìgbà tí ẹ gbọdọ̀ yọwọ́ kúrò.

## Tí kò bá dá yín lójú

Ẹ má ṣe jẹ́rìí. Voucher tí a kò jẹ́rìí sí kò sọnù — ó wà ní ìpamọ́ (escrow), a ó sì dá owó rẹ̀ padà fún olùsanwó ní ọjọ́ ìparí, tàbí lẹ́sẹ̀kẹsẹ̀ tí ilé-ìwòsàn náà kò bá ṣiṣẹ́ mọ́. Àbájáde àìjẹ́rìí ni ìdádúró. Àbájáde jíjẹ́rìí lọ́nà òdì ni pé owó aláìsàn yóò lọ sọ́dọ̀ ilé-ìwòsàn tí kò ṣe nǹkan kan, láìsí ọ̀nà láti gbà á padà lẹ́yìn wákàtí 72.

Tí ẹ bá gbàgbọ́ pé ilé-ìwòsàn kan ń béèrè owó fún ìtọ́jú tí kò ṣe, ẹ sọ fún alábòójútó kí a lè dá a dúró.

---

*Ìtumọ̀ yìí ń dúró de àyẹ̀wò láti ọwọ́ ẹni tí Yorùbá jẹ́ èdè abínibí rẹ̀.*
