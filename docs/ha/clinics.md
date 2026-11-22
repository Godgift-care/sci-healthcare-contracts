---
title: Gudanar da asibiti
nav_order: 7
language: ha
translations:
  en: /guides/clinics.html
  fr: /fr/clinics.html
  sw: /sw/clinics.html
  ar: /ar/clinics.html
  am: /am/clinics.html
  pt: /pt/clinics.html
  yo: /yo/clinics.html
  ig: /ig/clinics.html
---

# Gudanar da asibiti

Ku asibiti ne, kantin magani ko ɗakin gwaje-gwaje (lab) da ke son a biya ku ta hanyar wannan tsarin.

Ma'anar kalmomin fasaha tana cikin [ƙamus ɗin](glossary.md).

## Samun tantancewa

1. **Ku haɗa walat ɗin asibitin** a shafin Clinic desk. Wannan adireshi ne zai karɓi kowane biya — ku yi amfani da wanda kuke iko da shi kuma kuna iya kiyaye shi.
2. **Ku yi rajista.** Ku shigar da sunan asibitin da lambar ƙasa mai haruffa biyu. Wannan na saka ku a cikin rajista (registry) a matsayin `Pending`.
3. **Ku sami tantancewa.** Mai gudanarwa zai duba asibitin — lasisi, wuri, hanyar tuntuɓa — sannan ya mayar da ku `Active`. Kafin nan ba za ku iya lissafa ayyuka ko karɓar takardun jinya ba.

Tantancewa aiki ne na mutane. Yin rajista ba ya nufin an tantance ku.

## Lissafa ayyuka

Da zarar kun zama `Active`, ku lissafa abubuwan da kuke karɓar kuɗi a kansu. Kowane aiki yana buƙatar:

- **Lamba (code).** Lambar da ke gane rukunin. Ku yi amfani da tsari guda ɗaya.
- **Suna (label).** Abin da majiyyata za su gani, misali "Outpatient consult".
- **Farashi** a USDC.

> Ku sa sunaye da lambobi su kasance **na gama-gari**. "Outpatient consult", ba "HIV counselling session" ba. Ana wallafa su a kan blockchain na jama'a har abada, kuma lambar da ta yi takamaimai har ta amfani wajen jinya ta yi takamaimai har ta iya bayyana majiyyaci.

Farashi shi ne mafi ƙanƙanta. Mai biya na iya biyan fiye da shi, ba ƙasa ba. Kuna iya sabunta farashi a kowane lokaci — takardun da aka riga aka biya suna riƙe adadin da aka amince da shi lokacin da aka ƙirƙira su.

## Karɓar biya

1. **Majiyyacin ya iso.** Ku nemo takardarsa a Clinic desk sannan ku danna **Mark patient seen**. Wannan shi ne `claim`.
2. **Ku ba da jinyar.**
3. **Mai tabbatarwa (attester) ya tabbatar.** Ba ku ba — kwangilar na ƙin tabbatarwa daga asibitin da ake biya. Wannan shi ne babban kariyar tsarin kuma ba abu ne da za ku iya kauce masa ba.
4. **Lokacin ƙorafi yana gudana.** Awanni 72 da mai biya zai iya yin ƙin yarda.
5. **Ana biyan ku.** Bayan lokacin, kowa na iya fara biyan, har da ku. Ku danna **Release payment**.

Kuna karɓar adadin takardar ban da kuɗin sabis na tsarin na 1%. Takardar $3.00 na biyan ku $2.97.

## Abubuwan da ya kamata ku sani

**Ba ku dogara da kowa don a biya ku ba.** Biyan (settlement) ba ya buƙatar izini. Da zarar lokacin ya rufe, kuɗin naku ne ku karɓa kuma babu mai gudanarwa da zai iya hana su.

**Ku karɓi da wuri.** Ba za a iya karɓar takarda bayan ta ƙare ba — mai biya zai dawo da ita.

**Dakatarwa.** Mai gudanarwa na iya dakatar da asibiti. Asibitin da aka dakatar ba zai iya karɓar sababbin takardu ko canza farashi ba. Takardun da aka riga aka biya suna ci gaba da biyan kamar yadda aka saba.

**Samun kuɗin gida.** Biyan yana cikin USDC. Canza shi zuwa naira, CFA ko tsabar kuɗi na wucewa ta hanyar Stellar anchor — Yellowcard, Cowrie, HoneyCoin, MoneyGram. Wannan mataki ne daban, mai ka'ida, tare da nasa binciken shaida. Haɗin kai a cikin manhajar [batu ne a buɗe](https://github.com/Godgift-care/sci-healthcare-frontend/issues), ba a gina shi ba tukuna.

## Ƙarin tambayoyi

Duba [Clinic FAQ](../guides/clinics-faq.md) (da Turanci) don lokacin biya, ƙorafi, kuɗin sabis da sauran tambayoyin da aka saba yi, kowanne da hanyoyin zuwa tushensa.
