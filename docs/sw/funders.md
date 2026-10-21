---
title: Kulipia matibabu ya mtu
nav_order: 6
---

# Kulipia matibabu ya mtu

🇬🇧 [Soma mwongozo huu kwa Kiingereza](../guides/funders.md) · 🇫🇷 [Lire ce guide en français](../fr/funders.md)

Unalipia matibabu mahususi katika kliniki mahususi, kwa ajili yako mwenyewe au kwa ajili ya mtu mwingine.

## Unachohitaji

- Pochi ya Stellar. [Freighter](https://freighter.app) ndiyo inayotumika mara nyingi.
- Pochi yako iwekwe kwenye **Testnet** wakati itifaki bado inajaribiwa.
- Kiasi fulani cha USDC kwenye pochi hiyo, pamoja na trustline yake (tazama hapa chini). Kwenye testnet, pata USDC za majaribio bila malipo kutoka kwenye programu: **My vouchers → Get test USDC** (mara moja kwa siku kwa kila pochi).
- Namba ya utambulisho wa mgonjwa katika kliniki, na ufunguo wa siri mliokubaliana nyote wawili.

## Kuhusu trustline

Stellar inakutaka ukubali waziwazi kushikilia mali fulani kabla ya kuweza kuipokea. Hii inaitwa trustline. Bila trustline ya USDC, pochi yako haiwezi kushikilia USDC na ufadhili utashindwa.

Kwenye testnet, iongeze mwenyewe kwenye pochi yako. Katika Freighter: **Manage assets → Add an asset**, msimbo `USDC`, mtoaji `GCKX5KAFVF7GLFZ3JWQGJ4F62RYYZU67PI3TACBGKVPJ73JELMI3L4KR`.

## Kufadhili

1. **Unganisha pochi yako.** Juu kulia. Hakuna kinachohifadhiwa; unaweza kujiondoa wakati wowote.
2. **Chagua kliniki.** Orodha inaonyesha kliniki zilizothibitishwa pekee, pamoja na bei zao.
3. **Chagua huduma.** Bei zinawekwa na kliniki na kuchapishwa kwenye mnyororo (on chain).
4. **Weka namba ya utambulisho wa mgonjwa na ufunguo wa siri.**

   Namba ya utambulisho ni chochote kinachomtambulisha mgonjwa katika kliniki hiyo — namba ya kadi, namba ya usajili. Ufunguo ni siri mnayoshirikiana yenye herufi angalau 32.

   Vitu hivi viwili vinaunganishwa *ndani ya kivinjari chako* kuwa thamani isiyoweza kusomeka. Hakuna kati yao kinachoondoka kwenye kifaa chako. Hakuna kinachotumwa kwetu. Hakuna kinachowekwa kwenye blockchain.

   **Tumia namba ileile na ufunguo uleule kila wakati kwa mtu yuleyule.** Ndivyo vinavyounganisha historia yake ya matibabu. Ukipoteza ufunguo, risiti za zamani haziwezi kutafutwa tena. Hakuna njia ya kuurejesha — hiyo ndiyo gharama ya sisi kutoushikilia.

5. **Thibitisha kiasi.** Unaweza kulipa zaidi ya bei iliyoorodheshwa, kamwe si chini yake.
6. **Saini.** Pochi yako inaonyesha muamala. Ukiukubali, USDC zinahamishiwa kwenye mkataba (contract).

Pesa zako sasa ziko kwenye escrow (zimeshikiliwa kwa dhamana). Bado si mali ya kliniki.

## Baada ya kufadhili

Fuatilia chini ya **My vouchers**.

| Hali | Maana yake |
| --- | --- |
| Funded | Zimeshikiliwa kwa dhamana. Mgonjwa bado hajahudhuria. |
| Claimed | Kliniki inasema mgonjwa alifika. |
| Attested | Mthibitishaji amethibitisha huduma ilitolewa. Muda wa kupinga uko wazi. |
| Settled | Kliniki imelipwa. |
| Refunded | Pesa zimerudi kwako. |

## Ikiwa kuna tatizo

**Mgonjwa hakwenda kabisa.** Subiri tarehe ya mwisho ipite, kisha bonyeza **Refund me**. Unarudishiwa kiasi chote, bila ada. Mtu yeyote anaweza kuanzisha hatua hii, kwa hiyo hutegemei sisi.

**Kliniki ilidai, lakini hakuna aliyewahi kuthibitisha ziara.** Unaweza kupinga mara moja. Usipofanya chochote, vocha inaweza kurejeshewa pesa siku saba baada ya tarehe yake ya mwisho, na **Refund me** inaonekana tena.

**Kliniki ilidai lakini haikumtibu.** Bonyeza **Dispute** wakati hali ni `Claimed` au `Attested`. Pesa zinazuiliwa na msimamizi anapitia suala hilo. Fanya hivyo kabla muda wa kupinga haujafungwa — saa 72 baada ya uthibitisho. Ukishafungwa, malipo hufanyika yenyewe na hayawezi kurudishwa.

**Muda ulifungwa na hukuwahi.** Pesa zimekwenda kwa kliniki. Itifaki haiwezi kuzirudisha. Lizungumze suala hilo na kliniki moja kwa moja.

## Kile ambacho haya hayafanyi

Hayahakikishi kuwa matibabu yalikuwa *mazuri*. Yanahakikisha kuwa mtu huru alithibitisha kuwa yalitolewa, na kwamba ulipata muda wa kupinga.
