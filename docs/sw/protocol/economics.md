---
title: Uchumi
nav_order: 3
language: sw
translations:
  en: /protocol/economics.html
  fr: /fr/protocol/economics.html
---

# Uchumi

## Ada

Itifaki inachukua ada kwa basis points wakati wa malipo pekee. Usanidi wa uzalishaji ni **basis points 100 — 1%**. Mkataba unaweka kikomo cha 1000 bps (10%) na kikomo hiki kinasimamiwa wakati wa uanzishaji, kwa hiyo usambazaji uliokonfiguriwa vibaya hauwezi kutoza zaidi.

```
fee = amount * fee_bps / 10_000
net = amount - fee
```

Hesabu zote ni za namba kamili. Hakuna namba za floating point mahali popote katika itifaki hii. Mgawanyo hupunguza, na upunguzaji daima unampendelea kliniki badala ya itifaki.

### Sehemu ya mthibitishaji

Wathibitishaji hufanya kazi inayofanya itifaki iaminike, kwa hiyo malipo huwalipa sehemu inayosanidiwa ya ada. `attester_share_bps` huwekwa mara moja wakati wa uanzishaji pamoja na `fee_bps`, na mkataba unakataa thamani yoyote iliyo juu ya `fee_bps`: sehemu hiyo inatolewa kutoka kwenye ada, haitozwi juu yake kamwe.

```
attester_share = amount * attester_share_bps / 10_000
fee_account    = fee - attester_share
```

`net` ya kliniki haibadiliki. Malipo matatu daima yanajumlisha kiasi cha vocha kwa usahihi:

```
net + attester_share + fee_account = amount
```

Usambazaji wa uzalishaji unaweka `fee_bps = 100` na `attester_share_bps = 50`, kwa hiyo nusu ya ada inakwenda kwa mthibitishaji aliyethibitisha utoaji na nusu kwenye akaunti ya ada. `attester_share_bps = 0` hulipa akaunti ya ada ada yote, sawa kabisa na jinsi mkataba ulivyokuwa kabla sehemu ya mthibitishaji kuwa. Dai lililopingwa ambalo msimamizi analitatulia kliniki bila uthibitisho halina mthibitishaji wa kumbukumbu, kwa hiyo malipo hayo pia hulipa ada yote kwenye akaunti ya ada.

### Nambari zilizohesabiwa kwa 1%

| Vocha | Ada | Kliniki inapokea |
| --- | --- | --- |
| $1.00 (kipimo cha haraka cha malaria) | $0.01 | $0.99 |
| $3.00 (mashauriano ya nje) | $0.03 | $2.97 |
| $5.00 (ziara ya ujauzito) | $0.05 | $4.95 |
| $25.00 (upasuaji mdogo) | $0.25 | $24.75 |

Kabla sehemu ya mthibitishaji haijaanzishwa, vocha ya $3.00 ililipa `29700000` stroops kwa kliniki na `300000` kwenye akaunti ya ada. Kwa mgawanyo wa uzalishaji ulio juu, ada hiyo ya `300000` inakuwa `150000` kwa mthibitishaji na `150000` kwa akaunti ya ada; kliniki bado inapokea `29700000`.

### Mifano ya malipo kwa stroops

Kile `quote` inachorudisha, na kile malipo yanayolipa, kwa ada ya uzalishaji ya 100 bps. USDC kwenye Stellar ina desimali 7, kwa hiyo 1 USDC ni `10,000,000` stroops; mkataba hufanya kazi kwa stroops kote.

| Vocha | Kiasi (stroops) | Ada (stroops) | Kliniki inapokea (stroops) |
| --- | --- | --- | --- |
| 1 USDC | 10,000,000 | 100,000 | 9,900,000 |
| 3 USDC | 30,000,000 | 300,000 | 29,700,000 |
| 10 USDC | 100,000,000 | 1,000,000 | 99,000,000 |
| 100 USDC | 1,000,000,000 | 10,000,000 | 990,000,000 |
| 1.2345678 USDC | 12,345,678 | 123,456 | 12,222,222 |
| 0.0000099 USDC | 99 | 0 | 99 |

Safu mbili za mwisho zinaonyesha upunguzaji: `12,345,678 × 100 / 10,000` ni `123,456.78`, na `.78` inabaki kwa kliniki; kwenye vocha ya stroops 99 ada inapungua hadi sifuri. Mfadhili anayelipa zaidi ya bei iliyoorodheshwa anapewa nukuu kwa kile anacholipa kweli.

### Sehemu ya mthibitishaji kwa 50 bps

Jinsi ada ya uzalishaji inavyogawanywa wakati wa malipo: kliniki bado inapokea `amount - fee`, mthibitishaji anapokea sehemu yake ya kiasi, na akaunti ya ada inapokea kilichobaki. Sehemu ya mthibitishaji ni sehemu ya ada, si makato ya ziada.

| Vocha | Kiasi (stroops) | Mthibitishaji anapokea (stroops) | Akaunti ya ada inapokea (stroops) |
| --- | --- | --- | --- |
| 1 USDC | 10,000,000 | 50,000 | 50,000 |
| 3 USDC | 30,000,000 | 150,000 | 150,000 |
| 10 USDC | 100,000,000 | 500,000 | 500,000 |
| 100 USDC | 1,000,000,000 | 5,000,000 | 5,000,000 |
| 1.2345678 USDC | 12,345,678 | 61,728 | 61,728 |
| 0.0000099 USDC | 99 | 0 | 0 |

Safu ya mwisho ni upunguzaji uleule kwa kiwango kidogo: vocha ya stroops 99 haina ada ya kugawanya, kwa hiyo wala mthibitishaji wala akaunti ya ada haipokei chochote.

Jedwali zote tatu kwenye ukurasa huu zinakaguliwa dhidi ya mkataba: `settlement_examples_table_matches_quote`, `worked_numbers_table_matches_quote` na `attester_share_table_matches_the_settlement_split` katika `contracts/voucher/src/economics_test.rs` huhesabu kila safu upya na kushindwa ikiwa namba hapa si sahihi. Kukagua moja kwa mkono kwenye usambazaji:

```bash
stellar contract invoke --id "$VOUCHER_CONTRACT_ID" --source-account sci-admin \
  --network testnet --send=no -- quote --amount 30000000
# ["300000","29700000"]
```

### Kinachositozwa

- **Marejesho hayana malipo.** Vocha iliyoisha muda au iliyopingwa kwa mafanikio hurudisha kiasi chote. Hakuna ada inayotozwa kwa huduma ambayo haikutokea.
- **Kupunguza malipo ya ziada ni bure.** `reduce_amount` hurudisha kwa mfadhili tofauti kati ya kile kilichowekwa escrow na bei iliyoorodheshwa ya kliniki. Hakuna ada inayotozwa kwa pesa ambazo hazikuwa huduma.
- **Kufadhili ni bure.** Ada inachukuliwa mara moja, wakati wa malipo.
- **Sehemu ya mthibitishaji ni sehemu ya ada.** Inatoka kwenye ada ya itifaki, si kwenye net ya kliniki, kwa hiyo kuisanidi haiwezi kupunguza kile kliniki inachopokea.
- **Hakuna spread.** Itifaki haibadilishi sarafu na haichukui margin ya ubadilishaji.

## Gharama za mtandao

Ada za muamala za Stellar hulipwa kwa XLM na ni sehemu ya senti. Wakati wa kuandika ada ya msingi ni stroops 100 — 0.00001 XLM — kwa operesheni.

Hii ina umuhimu zaidi kuliko inavyosikika. Mzunguko mzima wa vocha ni miamala minne: kuunda, kudai, kuthibitisha, kulipa. Kwenye mnyororo ambapo muamala unagharimu $0.50, hiyo ni $2.00 ya gharama ya ziada kwenye mashauriano ya $3.00, na bidhaa haipo. Hapa ni kosa la kuzungusha namba.

Gharama nyingine ya uendeshaji ni kodi ya hifadhi: kila vocha, mtoa huduma, huduma na risiti ni ingizo la leja linalokodisha nafasi kwa TTL yake. [Gharama za hifadhi na kodi](../../protocol/costs.md) (kwa Kiingereza) zinaorodhesha ukubwa uliopimwa na kodi ya kila kitu na kuonyesha jinsi ya kuzalisha namba tena.

## Nani analipa nini

| Hatua | Nani anawasilisha | Nani analipa ada ya mtandao |
| --- | --- | --- |
| `create_voucher` | Mfadhili | Mfadhili |
| `reduce_amount` | Mfadhili | Mfadhili |
| `claim` | Kliniki | Kliniki |
| `attest` | Mthibitishaji | Mthibitishaji |
| `settle` | Mtu yeyote | Anayewasilisha |
| `refund` | Mtu yeyote | Anayewasilisha |

Kwa sababu `settle` na `refund` hazihitaji idhini, mwendeshaji anaweza kuendesha keeper inayolipa vocha zinazostahili kwa niaba ya kila mtu na kuchukua ada hizo, ikiwemo kodi ya risiti ambayo `settle` huzalisha ([Kuendesha keeper ya malipo](../../developers/keepers.md#what-it-costs), kwa Kiingereza). Hakuna chochote katika itifaki kinachotegemea keeper huyo kuwepo — ni huduma ya urahisi, si tegemeo.

## Dhana: tokeni huhamisha kiasi kamili

Kila namba kwenye ukurasa huu inadhani kuwa uhamisho wa `x` huhamisha `x` kamili: mtumaji anapoteza `x` na mpokeaji anapata `x`. Tokeni ya USDC ya Stellar inafanya hivi. Tokeni yenye ada za uhamisho, kuchoma-wakati-wa-uhamisho au hooks zinazobadilisha salio haifanyi hivi, na escrow iliyojengwa juu yake ingerekodi zaidi ya ilivyonayo.

Mkataba wa vocha hauamini dhana hii kwa upofu. Unakagua salio karibu na kila uhamisho na kushindwa kwa kufunga na `TokenAmountMismatch` (#34) ikiwa kiasi kinatofautiana, kwa hiyo tokeni kama hiyo haiwezi kutumika. Pia haiiunga mkono. Sambaza tu dhidi ya mkataba wa kawaida wa mali ya Stellar; kipengele 3.11 cha [orodha ya ukaguzi ya mainnet](../../developers/mainnet-checklist.md#3-parameters) (kwa Kiingereza) kinaifanya kuwa lango la uzinduzi.

## Kwa nini hakuna float

Mkataba hauna salio na hauna kazi ya kuweka pesa. Pesa huingia tu kama vocha iliyofungwa tayari kwa kliniki, huduma na tarehe ya mwisho, na inaweza kutoka tu kwa kliniki hiyo au kurudi kwa mfadhili.

Hii ni kizuizi cha makusudi cha muundo, si uzembe, na ndiyo tofauti kati ya vocha ya malipo ya mapema na thamani iliyohifadhiwa. Salio la jumla linaloweza kujazwa tena ni e-money katika nchi nyingi na huleta leseni. Vocha ya malipo ya mapema kwa mtoa huduma aliyetambuliwa na huduma iliyotambuliwa kwa ujumla huanguka chini ya misamaha ya mtandao mdogo.

Itifaki pia **haichukui hatari**. Haiweki pamoja malipo ya bima, haitoi bima, na hailipi kwa tukio la afya. Wakati mfumo unapopokea pesa na kuahidi kulipa kwa hitaji la matibabu lisilo na uhakika baadaye, ni bima na unahitaji leseni. Itifaki hii huhamisha tu pesa ambazo mfadhili amekwisha ahidi kwa ununuzi maalum.
