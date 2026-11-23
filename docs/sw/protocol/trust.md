---
title: Mfumo wa uaminifu
nav_order: 4
language: sw
translations:
  en: /protocol/trust.html
  fr: /fr/protocol/trust.html
---

# Mfumo wa uaminifu

Itifaki hii ni **iliyopunguza utegemezi wa uaminifu, si isiyo na uaminifu**. Inafaa kuwa sahihi kuhusu maana ya hilo, kwa sababu kukuzidisha kungekuwa jambo rahisi zaidi kufanya na lenye madhara zaidi.

## Tatizo lisiloepukika

Je, muuguzi alimchunguza mgonjwa kweli ni ukweli kuhusu dunia halisi. Hakuna mkataba wa akili unaoweza kuuona. Kitu kingine nje ya mnyororo kinapaswa kuuthibitisha, na uthibitisho huo unaweza kuwa wa uongo.

Kila kitu kilicho hapa chini ni kuhusu kupunguza nafasi ya kusema uongo, si kuiondoa.

## Mkataba unahakikisha nini

Hivi hushikilia bila kujali nani anafanya vibaya:

1. **Pesa zinaweza kufikia kliniki iliyotajwa tu au kurudi kwa mfadhili.** Hakuna mwisho wa tatu. Si msimamizi, si itifaki, si mthibitishaji.
2. **Kliniki inayolipwa haiwezi kuthibitisha utoaji wake yenyewe.** Mkataba unakagua hili waziwazi na kulikataa, hata kama anwani hiyo ya kliniki imepewa jukumu la mthibitishaji.
3. **Kliniki haiwezi kulipwa bila uthibitisho.** Hakuna njia kutoka `Funded` au `Claimed` moja kwa moja hadi `Settled`.
4. **Pesa haiwezi kutolewa mapema.** `settle` inashindwa wakati kipindi cha mgogoro kiko wazi.
5. **Vocha isiyothibitishwa daima inarudi kwa mfadhili.** Bila kudaiwa, inaweza kurejeshwa wakati wa tarehe ya mwisho — au mara moja ikiwa kliniki haiko active tena. Ilidaiwa lakini haikuthibitishwa, siku saba baada ya tarehe ya mwisho. Hakuna anayeweza kuzuia hilo — `refund` haihitaji idhini.
6. **Kiasi chote kinarejeshwa.** Hakuna ada inayotozwa kwa huduma ambayo haikutokea.
7. **Vocha iliyolipwa haiwezi kulipwa au kurejeshwa tena.** Hali za mwisho ni za mwisho.
8. **Haki za msimamizi haziwezi kupewa anwani iliyokosewa.** Kila mkataba huhamisha uongozi kwa hatua mbili — `propose_admin`, kisha `accept_admin` iliyosainiwa na anwani mpya.

## Unamuamini nani

Imeelezwa waziwazi, kwa sababu haya ni ya kweli:

### Wathibitishaji

Mthibitishaji anaweza kuthibitisha huduma ambayo haikutokea kamwe. Ikiwa mthibitishaji na kliniki wanashirikiana, wanaweza kutoa vocha kwa ziara za kubuni.

Kinachopunguza: wathibitishaji huteuliwa na msimamizi na wanaweza kuondolewa; mtoa huduma hawezi kuthibitisha mwenyewe; mfadhili ana kipindi cha saa 72 cha kupinga kabla pesa yoyote kuhamishwa; na kila uthibitisho unahusishwa kwa kudumu kwenye mnyororo, kwa hiyo mthibitishaji asiye mwaminifu anaacha rekodi kamili.

Kinachoboresha: kiwango cha wathibitishaji kwa vocha za thamani kubwa, ili upande mmoja ulioathirika usitoshe. Hili ni [suala lililo wazi](https://github.com/Godgift-care/sci-healthcare-contracts/issues), si kitu kilichojengwa tayari.

### Msimamizi

Msimamizi anaweza kuamsha na kusimamisha kliniki, kuteua na kuondoa wathibitishaji, na kutatua migogoro.

Kinachopunguza: kwenye vocha iliyopingwa chaguo la msimamizi pekee ni *ni yupi kati ya wahusika wawili* anayepokea pesa zilizowekwa escrow. Hakuna wito unaomruhusu msimamizi kuhamisha pesa kwa anwani ya tatu, kutoa vocha, au kuchukua pesa kutoka vocha iliyolipwa. Msimamizi hawezi kugusa vocha ambayo haiko `Disputed`.

Ufunguo wa msimamizi ni sehemu moja ya kushindwa leo. Kuuhamisha kwenye multisig ni uamuzi wa usambazaji, na mainnet haipaswi kuzinduliwa bila hiyo.

### Sajili

Ikiwa kliniki isiyostahili imethibitishwa, wafadhili wanaweza kutuma pesa kwake. Uthibitishaji ni mchakato wa nje ya mnyororo — kukagua leseni, kutembelea eneo — na sajili inarekodi matokeo yake pekee.

Kila mabadiliko ya hali kwenye sajili hurekodi **kwa nini**, kama `reason_code` iliyohifadhiwa kwa mtoa huduma na kuchapishwa katika `provider_status_changed`. Kliniki iliyosimamishwa inasoma msimbo wake na kujua cha kurekebisha; mfadhili anayesoma historia ya sajili anaweza kutofautisha kusimamishwa kwa udanganyifu na hati zilizoisha muda. Sajili ni "bubu" kwa makusudi kuhusu thamani — inahifadhi na kuchapisha msimbo, haifasiri — kwa hiyo programu lazima zifafanue misimbo zenyewe:

#### Misimbo ya sababu

| Msimbo | Maana |
| --- | --- |
| 0 | Hakuna mabadiliko ya hali bado (huwekwa wakati wa usajili; `set_provider_status` inaukataa kwa `MissingReasonCode`) |
| 1 | Imethibitishwa |
| 2 | Hati zimeisha muda |
| 3 | Malalamiko yanachunguzwa |
| 4 | Udanganyifu |
| 5 | Imefungwa (hutolewa kliniki inapojistaafu) |

Misimbo **huongezwa, haibadilishwi namba wala kutumika tena**. Msimbo uliostaafu unahifadhi maana yake kwa kila usambazaji na kila tukio la kihistoria, kwa hiyo ukaguzi wa kusimamishwa kwa mwaka jana bado unasomeka kwa usahihi. Maana mpya hupata misimbo mipya iliyoongezwa hapa katika pull request ileile inayochapisha kwa mara ya kwanza.

Jambo lingine sajili inarekodi kuhusu hali ni kile inachokataa: `set_provider_status` inakubali tu `Pending → Active`, `Pending → Suspended`, `Active → Suspended` na `Suspended → Active` (tazama [mzunguko wa hali](../../developers/contracts.md#status-lifecycle), kwa Kiingereza), kwa hiyo historia ya matukio haina mabadiliko yasiyo na maana wala kurudi nyuma kusikoweza kuelezwa. Kliniki inayofunga inajistaafu; hakuna kitu — hata msimamizi — kinachoweza kurudisha kliniki iliyostaafu, kwa hiyo hakuna msimbo wa sababu unaotoka `Retired`.

Kurudisha kliniki iliyosimamishwa (`Suspended → Active`) inaifungua tena kwa ufadhili, kwa hiyo inashikiliwa na kanuni ileile kama kila mabadiliko mengine: msimamizi aliyohifadhiwa pekee, na saini yake mwenyewe, na daima na msimbo wa sababu usio sifuri. Kliniki haiwezi kujirudisha yenyewe, na wala mthibitishaji, kliniki nyingine, msimamizi aliyeteuliwa ambaye hajakubali, au msimamizi aliyekwisha hamisha. Kila moja ina jaribio lake.

### Mtoaji wa tokeni ya malipo

USDC ni stablecoin inayodhibitiwa, na mtoaji wake anaweza **kuirudisha** pesa kutoka salio lolote na **kuifungia** (kuondoa idhini) moja. Vocha zilizo kwenye escrow ziko kwenye salio la tokeni la mkataba wa vocha wenyewe, kwa hiyo mtoaji anaweza kuzifikia; hakuna kitu kwenye mkataba kinachoweza kuzuia hilo. Mkataba unachofanya ni kuishi kwa jinsi inayotabirika inapotokea:

- **Kurudisha (Clawback).** Kabla ya kulipa chochote, `settle`, `refund`, `resolve_dispute`, `resolve_split` na `reduce_amount` hukagua kuwa mkataba bado unashikilia kiasi cha vocha. Ikiwa haushikilii, hushindwa kwa `EscrowShortfall` (#23) badala ya panic ndani ya tokeni. Escrow ni salio moja linaloshirikiwa na kila vocha iliyo wazi, kwa hiyo clawback inabebwa na vocha zinazotumia salio mwisho: zile za awali hulipwa au kurejeshwa kikamilifu, na zilizobaki hushindwa kwa `EscrowShortfall` hadi salio lirejeshwe.
- **Kufungia (Freeze).** Ikiwa mtoaji amefungia mkataba, kliniki au mfadhili, tokeni inakataa uhamisho na wito unashindwa kwa `TokenTransferFailed` (#24).
- **Tokeni zinazotoza ada au zenye hooks.** Mkataba unadhani tokeni ya malipo huhamisha kiasi kilichoombwa kamili, jinsi tokeni ya USDC ya Stellar inavyofanya. Tokeni inayochukua ada ya uhamisho, inayotoza zaidi kwa mtumaji au inayoendesha hook inayobadilisha salio ingevunja sharti la escrow bila mtu yeyote kugundua. Kwa hiyo mkataba hupima salio karibu na kila uhamisho. Ufadhili hukagua kuwa escrow iliongezeka kwa kiasi cha vocha kamili. Kila malipo hukagua kuwa escrow ilipungua kwa kiasi kilichotumwa kamili na mpokeaji aliongezeka kwa kiasi hicho hicho. Tofauti yoyote inashindwa kwa `TokenAmountMismatch` (#34). Majaribio ya `nonstandard_token` katika `contracts/voucher/src/test/nonstandard_token.rs` yanaendesha mzunguko dhidi ya tokeni ya kuiga katika kila moja ya hali hizi.
- **Hakuna kulipwa nusu.** Kosa lolote kati ya haya linarejesha wito wote: vocha inabaki na hali yake, hakuna risiti inayotolewa na hakuna ada inayochukuliwa. Vocha iliyozuiwa na kufungia hulipwa au kurejeshwa kawaida pindi kufungia kunapotolewa.

Haya yanafunikwa na majaribio ya `clawback_*`, `partial_clawback_*` na `frozen_*` katika `contracts/voucher/src/test.rs`, yanayoendeshwa dhidi ya mkataba wa mali ya Stellar ukiwa na clawback na revocation zimewashwa.

## Kile itifaki haidai kwa makusudi

- Haithibitishi kuwa huduma ilikuwa *nzuri*, inathibitisha tu kuwa mtu alithibitisha ilitokea.
- Haithibitishi utambulisho. Kumbukumbu ya mnufaika ni jina la kubuni.
- Haizuii kliniki kutoza zaidi. Inalazimisha kiwango cha chini kwa bei iliyoorodheshwa, si kikomo cha juu cha kile kliniki inaweza kuorodhesha.
