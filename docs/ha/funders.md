---
title: Biyan kuɗin jinyar wani
nav_order: 6
language: ha
translations:
  en: /guides/funders.html
  fr: /fr/funders.html
  sw: /sw/funders.html
  ar: /ar/funders.html
  am: /am/funders.html
  pt: /pt/funders.html
  yo: /yo/funders.html
  ig: /ig/funders.html
---

# Biyan kuɗin jinyar wani

Kuna biyan wata takamaiman jinya a wani takamaiman asibiti, don kanku ko don wani.

Idan kuna biyan takardun jinya (vouchers) da yawa daga walat ɗin haɗin gwiwa na ƙungiya ko
asusun multisig, ku karanta [Biya daga walat ɗin ƙungiya](../guides/organisation-funders.md)
(da Turanci) maimakon wannan: ya yi bayani kan amincewa, mayar da kuɗi zuwa asusun haɗin
gwiwa da adana bayanai.

Ma'anar kalmomin fasaha tana cikin [ƙamus ɗin](glossary.md).

## Abin da kuke buƙata

- Walat ɗin Stellar. [Freighter](https://freighter.app) shi ne wanda aka fi amfani da shi.
- A saita walat ɗinku zuwa **Testnet** yayin da ake gwajin tsarin (protocol).
- Ɗan USDC a cikin walat ɗin, tare da trustline ɗinsa (duba ƙasa). A testnet, ku samo USDC na gwaji kyauta daga manhajar: **My vouchers → Get test USDC** (sau ɗaya a rana ga kowane walat).
- Lambar shaidar majiyyaci a asibitin, da kalmar sirri (secret key) da ku biyu kuka amince da ita.

## Game da trustline

Stellar na buƙatar ku amince a fili da riƙe wata kadara kafin ku iya karɓar ta. Ana kiran wannan trustline, kuma idan babu shi don USDC, walat ɗinku ba zai iya riƙe shi ba kuma biyan zai gaza.

A testnet, ku ƙara shi da hannu a walat ɗinku. A Freighter: **Manage assets → Add an asset**, lamba `USDC`, mai bayarwa (issuer) `GCKX5KAFVF7GLFZ3JWQGJ4F62RYYZU67PI3TACBGKVPJ73JELMI3L4KR`.

## Biya

1. **Ku haɗa walat ɗinku.** A saman dama. Ba a adana komai; kuna iya cire haɗin a kowane lokaci.
2. **Ku zaɓi asibitin.** Jerin yana nuna asibitocin da aka tantance kawai, tare da farashinsu.
3. **Ku zaɓi aikin (service).** Asibitin ne ke saita farashi, kuma ana wallafa su a kan blockchain.
4. **Ku shigar da lambar majiyyaci da kalmar sirri.**

   Lambar ita ce duk abin da ke gane majiyyacin a asibitin — lambar kati, lambar rajista. Kalmar sirri sirri ne da kuka raba, aƙalla haruffa 32.

   Ana haɗa waɗannan biyun *a cikin burauzarku* zuwa wata ƙima da ba a iya karantawa. Babu ɗayansu da ke barin na'urarku. Ba a aiko mana da ko ɗaya. Babu wanda ke shiga blockchain.

   **Ku yi amfani da lamba da kalmar sirri iri ɗaya a kowane lokaci don mutum ɗaya.** Su ne ke haɗa tarihin jinyarsa wuri ɗaya. Idan kalmar sirri ta ɓace, ba za a iya sake duba rasit na baya ba. Babu hanyar dawo da ita — wannan shi ne farashin rashin riƙe ta a hannunmu.

5. **Ku tabbatar da adadin.** Kuna iya biyan fiye da farashin da aka lissafa, amma ba ƙasa da shi ba.
6. **Ku sa hannu.** Walat ɗinku zai nuna ma'amalar (transaction). Amincewa na tura USDC zuwa cikin kwangilar (contract).

Yanzu kuɗinku na cikin ajiyar amana (escrow). Har yanzu ba na asibitin ba ne.

## Bayan biya

Ku bi diddigin sa a ƙarƙashin **My vouchers**.

| Matsayi | Ma'anarsa |
| --- | --- |
| Funded | Yana cikin ajiyar amana. Majiyyacin bai halarta ba tukuna. |
| Claimed | Asibitin ya ce majiyyacin ya zo. |
| Attested | Mai tabbatarwa ya tabbatar an ba da jinyar. Lokacin ƙorafi ya buɗe. |
| Settled | An biya asibitin. |
| Refunded | An mayar muku da kuɗin. |

## Idan wani abu ya lalace

**Kun biya fiye da farashin bisa kuskure.** Yayin da takardar jinyar har yanzu `Funded` ce, kuna iya karɓar ƙarin kuɗin ba tare da rasa takardar ba: `reduce_amount` na rage adadin da ke cikin ajiya zuwa farashin da asibitin ya lissafa kuma ya mayar muku da bambancin. Ba ya taɓa sauka ƙasa da farashin, kuma yana rufe da zarar asibitin ya karɓi takardar. Manhajar yanar gizo ba ta da wannan maɓalli tukuna — walat ko mai haɗa manhaja (integrator) na iya kiran kwangilar kai tsaye.

**Majiyyacin bai je ba.** Ku jira ranar ƙarewa, sannan ku danna **Refund me**. Za ku karɓi cikakken adadin, ba tare da kuɗin sabis ba. Kowa na iya fara wannan, don haka ba ku dogara da mu ba. Idan aka dakatar da asibitin kafin a karɓi takardar, kuna iya karɓar kuɗinku nan take ba tare da jiran ranar ƙarewa ba.

**Asibitin ya karɓa, amma babu wanda ya tabbatar da ziyarar.** Kuna iya yin ƙorafi nan take. Idan ba ku yi komai ba, takardar za ta zama mai yiwuwar mayarwa kwanaki bakwai bayan ranar ƙarewarta kuma **Refund me** zai sake bayyana.

**Asibitin ya karɓa amma bai yi masa jinya ba.** Ku danna **Dispute** yayin da matsayin yake `Claimed` ko `Attested`. Kuɗin zai daskare kuma mai gudanarwa (administrator) zai duba shi. Ku yi haka kafin lokacin ƙorafi ya rufe — awanni 72 bayan tabbatarwa. Da zarar ya rufe, biyan asibitin na atomatik ne kuma ba a iya juyawa.

**Lokacin ya rufe kuma kun rasa shi.** Kuɗin ya tafi wajen asibitin. Tsarin ba zai iya ƙwace shi ba. Ku tattauna da asibitin kai tsaye.

## Abin da wannan ba ya yi

Ba ya tabbatar da cewa jinyar ta yi *kyau*. Yana tabbatar da cewa wani mai zaman kansa ya tabbatar ta faru, kuma kuna da lokacin yin ƙin yarda.
