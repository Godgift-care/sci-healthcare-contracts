---
title: Financiar cuidados para alguém
nav_order: 6
language: pt
translations:
  en: /guides/funders.html
  fr: /fr/funders.html
  sw: /sw/funders.html
  yo: /yo/funders.html
  ig: /ig/funders.html
  ha: /ha/funders.html
  ar: /ar/funders.html
  am: /am/funders.html
---

# Financiar cuidados para alguém

Está a pagar um tratamento específico numa clínica específica, para si próprio ou para outra pessoa.

Se financia muitos vales a partir da carteira partilhada de uma organização ou de uma conta
multisig, leia antes [Financiar a partir da carteira de uma organização](../guides/organisation-funders.md):
abrange as aprovações, os reembolsos para a conta partilhada e a manutenção
de registos.

## O que precisa

- Uma carteira Stellar. [Freighter](https://freighter.app) é a escolha habitual.
- A sua carteira configurada em **Testnet** enquanto o protocolo está em fase de testes.
- Algum USDC nessa carteira, e uma linha de confiança para esse ativo (ver abaixo). Na testnet, obtenha USDC de teste gratuito na aplicação: **Os meus vales → Obter USDC de teste** (uma vez por dia por carteira).
- A referência clínica do paciente e uma chave secreta combinada entre ambos.

## Sobre a linha de confiança

A Stellar exige que aceite explicitamente deter um ativo antes de o poder receber. A isto chama-se linha de confiança (trustline) e, sem uma para o USDC, a sua carteira não o pode deter e o financiamento falhará.

Na testnet, adicione-a manualmente na sua carteira. No Freighter: **Manage assets → Add an asset**, código `USDC`, emissor `GCKX5KAFVF7GLFZ3JWQGJ4F62RYYZU67PI3TACBGKVPJ73JELMI3L4KR`.

## Financiamento

1. **Ligue a sua carteira.** No canto superior direito. Nada é guardado; pode desligar-se a qualquer momento.
2. **Escolha a clínica.** O diretório mostra apenas clínicas verificadas, com os respetivos preços publicados.
3. **Escolha o serviço.** Os preços são definidos pela clínica e publicados na cadeia.
4. **Introduza a referência do paciente e a chave secreta.**

   A referência é aquilo que identifica o paciente nessa clínica — um número de cartão, um número de inscrição. A chave é um segredo partilhado com pelo menos 32 caracteres.

   Estes dois elementos são combinados *no seu navegador* num valor opaco. Nenhum deles sai do seu dispositivo. Nenhum deles nos é enviado. Nenhum deles vai para a blockchain.

   **Use sempre a mesma referência e a mesma chave para a mesma pessoa.** São elas que ligam o historial de cuidados dessa pessoa. Se perder a chave, os recibos anteriores deixam de poder ser consultados. Não há recuperação — é esse o preço de não sermos nós a guardá-la.

5. **Confirme o montante.** Pode pagar mais do que o preço publicado, nunca menos.
6. **Assine.** A sua carteira mostra a transação. Ao aprovar, o USDC passa para o contrato.

O seu dinheiro está agora em garantia (escrow). Ainda não é da clínica.

## Depois do financiamento

Acompanhe-o em **Os meus vales**.

| Estado | O que significa |
| --- | --- |
| Funded | Em garantia. O paciente ainda não compareceu. |
| Claimed | A clínica declara que o paciente compareceu. |
| Attested | Um atestador confirmou a prestação. O prazo de contestação está aberto. |
| Settled | A clínica foi paga. |
| Refunded | O dinheiro voltou para si. |

## Se algo correr mal

**Pagou por engano mais do que o preço.** Enquanto o vale ainda estiver `Funded`, pode recuperar o excedente sem abdicar do vale: `reduce_amount` reduz o montante em garantia até ao preço publicado pela clínica e devolve-lhe a diferença. Nunca desce abaixo do preço publicado e deixa de estar disponível assim que a clínica reclama o vale. A aplicação web ainda não oferece este botão — uma carteira ou um integrador pode chamar o contrato diretamente.

**O paciente nunca foi.** Aguarde a data de expiração e depois prima **Reembolsar-me**. Recebe o montante total de volta, sem taxa. Qualquer pessoa pode desencadear esta operação, por isso não depende de nós. Se a clínica for suspensa antes de o vale ser reclamado, pode ser reembolsado de imediato, sem esperar pela expiração.

**A clínica reclamou, mas ninguém confirmou a consulta.** Pode contestar de imediato. Se não fizer nada, o vale torna-se reembolsável sete dias após a data de expiração e **Reembolsar-me** volta a aparecer.

**A clínica reclamou, mas não tratou o paciente.** Prima **Contestar** enquanto o estado for `Claimed` ou `Attested`. Os fundos ficam congelados e um administrador analisa o caso. Faça-o antes de o prazo de contestação terminar — 72 horas após a atestação. Depois de terminar, a liquidação é automática e irreversível.

**O prazo terminou e não o aproveitou.** O dinheiro foi para a clínica. O protocolo não o pode recuperar. Trate o assunto diretamente com a clínica.

## O que isto não faz

Não garante que os cuidados foram *bons*. Garante que uma parte independente atestou que aconteceram e que teve um prazo para contestar.

_Tradução assinalada para revisão por um falante nativo._
