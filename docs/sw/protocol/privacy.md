---
title: Faragha
nav_order: 5
language: sw
translations:
  en: /protocol/privacy.html
  fr: /fr/protocol/privacy.html
---

# Faragha

Kanuni ngumu zaidi katika mradi huu: **hakuna taarifa ya afya inayolindwa inayofika kwenye leja, kamwe.**

Blockchain ya umma ni ya kudumu na ya kimataifa. Kitu chochote kinachoandikwa humo kinaandikwa milele. Data ya afya haipaswi kuwa humo, na majaribio mengi ya kuiweka humo — kwa usimbaji fiche, hashing, au njia nyingine — mwishowe huvuja kupitia metadata au kupoteza funguo.

## Kilicho kwenye mnyororo

Kwa kila vocha:

| Sehemu | Mfano | Kinachofichua |
| --- | --- | --- |
| `funder` | `GDUP…JG45` | Anwani ya Stellar iliyolipa |
| `provider` | `GDOO…ZKBK` | Kliniki gani |
| `service_code` | `101` | Kundi pana, k.m. "mashauriano ya nje" |
| `amount` | `30000000` | $3.00 |
| `beneficiary_ref` | `72676a6f…094745` | Baiti 32 zisizoeleweka |
| muda | | Ilipofadhiliwa, kudaiwa, kuthibitishwa, kulipwa |

## Kilicho nje, na hakitakuwa kamwe

- Majina, tarehe za kuzaliwa, namba za simu, anwani
- Utambuzi, dalili, matokeo ya vipimo, dawa
- Sehemu yoyote ya maandishi huru ambayo daktari anaweza kuandika

Hakuna sehemu ya vitu hivi. Kuongeza moja kutakataliwa katika ukaguzi.

## Misimbo ya huduma inabaki pana kwa makusudi

`101` inamaanisha "mashauriano ya nje". Haimaanishi "kipimo cha virusi vya HIV".

Hii ni mvutano wa kweli wa muundo. Misimbo nyembamba ingefanya bidhaa kuwa na manufaa zaidi — uchanganuzi bora, bei bora, utambuzi bora wa udanganyifu. Pia ingegeuza leja kuwa rekodi ya umma ya kudumu ya kile watu wanatibiwa. Msimbo ulio mahususi vya kutosha kuwa na manufaa kitabibu ni mahususi vya kutosha kumfichua mtu.

Kwa hiyo misimbo inabaki pana, na itifaki inakubali kuwa na manufaa kidogo kwa ajili ya hilo.

## Kumbukumbu za mnufaika

Kumbukumbu ni HMAC-SHA256 ya kitambulisho cha mgonjwa chini ya ufunguo wa siri:

```
beneficiary_ref = HMAC-SHA256(key, lowercase(trim(identifier)))
```

Inahesabiwa kwenye kivinjari cha mgonjwa kupitia Web Crypto. Kitambulisho na ufunguo havifikii API, havirekodiwi kwenye kumbukumbu, na havigusi leja kamwe.

**Kwa nini HMAC na si hash ya kawaida.** `SHA256(phone_number)` ya kawaida inaweza kurudishwa kwa nguvu — kuna namba za simu chache tu, na mshambuliaji anaweza kuzihash zote. HMAC chini ya ufunguo wa siri haitafutiki bila ufunguo huo.

**Inakupa nini.** Vocha mbili za mtu mmoja hutoa kumbukumbu ileile, kwa hiyo historia ya huduma inaweza kuunganishwa. Mtu aliye na data ya mnyororo pekee hawezi kujua historia ni ya nani, na hawezi kuorodhesha wagonjwa.

**Haikupi nini.** Hii ni pseudonymity, si utambulisho usiojulikana. Mtu yeyote anayejua kitambulisho na ufunguo anaweza kuthibitisha ulinganifu. Mtu anayemtazama mgonjwa maalum akiingia kliniki maalum kwa muda maalum anaweza kuhusisha hilo na tukio la mnyororo. Kutumia anwani moja mara kwa mara kwa mfadhili kunaunganisha vocha zake.

Mtu yeyote anayedai blockchain ya umma inawapa wagonjwa kutojulikana ana makosa au anauza kitu.

## Hifadhidata

Hifadhidata ya indexer haihifadhi kitu chochote ambacho si tayari cha umma kwenye mnyororo. Hiyo ni kizuizi cha makusudi, na inamaanisha uvunjaji wa hifadhidata hiyo si uvunjaji wa data ya afya. Ni uvunjaji wa akiba ya taarifa za umma.
