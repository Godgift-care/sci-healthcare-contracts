---
title: Maswali ya kliniki
nav_order: 9
language: sw
translations:
  en: /guides/clinics-faq.html
  fr: /fr/clinics-faq.html
---

# Maswali ya kliniki

Maswali ya kawaida kuhusu kuendesha kliniki kupitia itifaki, kila moja likiwa na kiungo cha chanzo. Namba (ada, muda) zinalingana na viwango vya mkataba katika [marejeo ya mkataba](../developers/contracts.md) na `scripts/deploy.sh`.

## Ninalipwa lini?

Baada ya kipindi cha mgogoro kufungwa. Mthibitishaji anapothibitisha utoaji, kipindi hicho huendelea kwa saa 72 katika usanidi wa uzalishaji. Kikifungwa, mtu yeyote — ikiwemo wewe — anaweza kuita `settle`, na unapokea kiasi cha vocha ukiondoa ada ya itifaki ya 1%. Mashauriano ya nje ya $3.00 yanakulipa $2.97; malipo ni ya kiotomatiki na hayarudishwi, na hakuna mwendeshaji anayeweza kuyazuia.

*Vyanzo: [Vocha inavyofanya kazi — Attested na Settled](protocol/lifecycle.md#attested), [Uchumi — ada](protocol/economics.md#ada), [Kuendesha kliniki](clinics.md)*

## Mgonjwa asipofika kabisa?

Vocha isipodaiwa kamwe, inaisha muda na huwezi kulipwa kwayo — mfadhili anaweza kurejeshewa kiasi chote bila ada. Kliniki ikisimamishwa kabla vocha haijadaiwa, mfadhili anaweza kurejeshewa mara moja badala ya kusubiri muda uishe. Ukiidai lakini hakuna mthibitishaji anayethibitisha ziara, pesa bado zinamrudia mfadhili siku saba baada ya muda kuisha (kipindi cha rehema cha dai); huna haki ya malipo bila uthibitisho.

*Vyanzo: [Vocha inavyofanya kazi — Refunded](protocol/lifecycle.md#refunded), [Kulipia matibabu](funders.md)*

## Migogoro inafanyaje kazi?

Mfadhili anaweza kubonyeza **Dispute** wakati vocha iko `Claimed` au `Attested`, lakini tu kabla kipindi cha mgogoro kufungwa — saa 72 baada ya uthibitisho katika uzalishaji. Pesa zinaganda kwenye escrow na msimamizi anaitatua kwa upande wa mfadhili au wa kliniki. Msimamizi hawezi kamwe kutuma pesa kwa mtu wa tatu, na kipindi kikifungwa malipo ni ya kiotomatiki na hayawezi kurudishwa.

*Vyanzo: [Vocha inavyofanya kazi — Disputed](protocol/lifecycle.md#disputed), [Mfumo wa uaminifu — msimamizi](protocol/trust.md#msimamizi)*

## Ni ada gani zinatumika?

Ada ya itifaki ni 1% (basis points 100), inachukuliwa mara moja wakati wa malipo. Ina kikomo cha 1000 bps (10%) wakati wa uanzishaji, kwa hiyo usambazaji hauwezi kusanidiwa upya kutoza zaidi. Marejesho hayana malipo na ufadhili ni bure; pia unalipa ada ya mtandao wa Stellar kwa miamala unayowasilisha pekee (claim, na settle ikiwa unaianzisha mwenyewe), ambayo ni sehemu ya senti kwa XLM.

*Vyanzo: [Uchumi — ada](protocol/economics.md#ada), [Uchumi — nani analipa nini](protocol/economics.md#nani-analipa-nini), [Marejeo ya mkataba — initialize](../developers/contracts.md#initialize)*

## Naweza kudai baada ya vocha kuisha muda?

Hapana. Vocha haiwezi kudaiwa baada ya `expires_at` yake, na kliniki iliyosimamishwa baada ya vocha kufadhiliwa haiwezi kuidai kabisa. Mgonjwa akifika baada ya muda kuisha, mfadhili anarudishiwa pesa. Kliniki ikisimamishwa kabla vocha haijadaiwa, mfadhili anaweza kurejeshewa mara moja.

*Vyanzo: [Vocha inavyofanya kazi — Claimed](protocol/lifecycle.md#claimed), [Marejeo ya mkataba — claim](../developers/contracts.md#claim)*

## Naweza kuthibitisha vocha yangu mwenyewe?

Hapana. Mkataba unakataa `attest` ikiwa mpigaji ni kliniki inayolipwa, hata kama anwani hiyo ina jukumu la mthibitishaji. Lazima utumie mthibitishaji huru — mfanyakazi wa afya ya jamii, muuguzi msimamizi katika kituo kingine, au kama hivyo — aliyeteuliwa na msimamizi.

*Vyanzo: [Mfumo wa uaminifu — mkataba unahakikisha nini](protocol/trust.md#mkataba-unahakikisha-nini), [Kuthibitisha utoaji](attesters.md)*

## Kliniki yangu ikisimamishwa?

Kliniki iliyosimamishwa haiwezi kupokea vocha mpya wala kubadilisha bei, lakini vocha zilizokwisha fadhiliwa bado zinalipwa kawaida. Unaweza kuendelea kutoa huduma kwa vocha zilizopo na kulipwa zikithibitishwa na kipindi cha mgogoro kikifungwa.

*Vyanzo: [Kuendesha kliniki](clinics.md)*

## Nani analipa ada ya mtandao wa Stellar?

Anayewasilisha muamala: wewe unalipa `claim`, mthibitishaji analipa `attest`, mfadhili analipa `create_voucher`, na anayeanzisha `settle` au `refund` analipa hizo. Kwa sababu `settle` na `refund` hazihitaji idhini, mwendeshaji anaweza kuendesha keeper inayolipa vocha zinazostahili kwa kila mtu na kuchukua ada hizo — lakini itifaki haitegemei keeper huyo kuwepo.

*Vyanzo: [Uchumi — gharama za mtandao](protocol/economics.md#gharama-za-mtandao), [Uchumi — nani analipa nini](protocol/economics.md#nani-analipa-nini)*

## Mtu mwingine anaweza kutoa malipo yangu?

Ndiyo. `settle` haihitaji idhini, kwa hiyo kipindi cha mgogoro kikishafungwa mpigaji yeyote anaweza kuanzisha malipo ya vocha yako iliyothibitishwa — huna budi kumfuata mfadhili au mwendeshaji ili ulipwe.

*Vyanzo: [Uchumi — nani analipa nini](protocol/economics.md#nani-analipa-nini), [Marejeo ya mkataba — settle](../developers/contracts.md#settle)*

## Ninalipwa kwa sarafu gani, na nitapataje pesa taslimu ya ndani?

Malipo ni kwa USDC. Kubadilisha kuwa naira, shilingi au pesa taslimu kunapitia anchor ya Stellar — Yellowcard, Cowrie, HoneyCoin au MoneyGram — hatua tofauti, inayodhibitiwa, yenye ukaguzi wake wa utambulisho.

*Vyanzo: [Kuendesha kliniki](clinics.md)*

## Kuna ada ya juu zaidi ambayo itifaki inaweza kutoza?

Ndiyo. Ada ina kikomo cha basis points 1000 (10%) na kikomo hiki kinasimamiwa wakati wa uanzishaji wa mkataba wa vocha, kabla pesa yoyote kuhamishwa. Uzalishaji ni 100 bps (1%).

*Vyanzo: [Uchumi — ada](protocol/economics.md#ada), [Marejeo ya mkataba — initialize](../developers/contracts.md#initialize)*

## Mfadhili ana muda gani wa kupinga vocha?

Si swali la kliniki, lakini kliniki huulizwa: mfadhili anaweza kupinga wakati vocha iko `Claimed`, au wakati iko `Attested` na kabla ya tarehe ya mwisho — saa 72 baada ya uthibitisho katika uzalishaji. Kipindi kikifungwa, malipo ni ya kiotomatiki.

*Vyanzo: [Vocha inavyofanya kazi — Attested](protocol/lifecycle.md#attested), [Kulipia matibabu](funders.md)*

## Itifaki inahakikisha huduma ilikuwa nzuri?

Hapana. Inahakikisha kuwa mthibitishaji huru alisema huduma ilitokea, kwamba ulikuwa na kipindi cha kupinga, na kwamba pesa zinaweza kwenda kwa kliniki yako tu au kurudi kwa mfadhili. Ubora wa huduma ni kati yako, mgonjwa na wadhibiti wako.

*Vyanzo: [Mfumo wa uaminifu — kile itifaki haidai kwa makusudi](protocol/trust.md#kile-itifaki-haidai-kwa-makusudi)*
