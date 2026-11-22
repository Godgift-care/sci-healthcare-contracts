---
title: Gerir uma clínica
nav_order: 7
language: pt
translations:
  en: /guides/clinics.html
  fr: /fr/clinics.html
  sw: /sw/clinics.html
  yo: /yo/clinics.html
  ig: /ig/clinics.html
  ha: /ha/clinics.html
  ar: /ar/clinics.html
  am: /am/clinics.html
---

# Gerir uma clínica

É uma clínica, farmácia ou laboratório que pretende ser pago através do protocolo.

## Obter a verificação

1. **Ligue a carteira da clínica** na página Balcão da clínica. Este endereço receberá todos os pagamentos — use um que controle e possa manter seguro.
2. **Registe-se.** Introduza o nome da clínica e o código de país de duas letras. Isto coloca-o no registo com o estado `Pending`.
3. **Obtenha a verificação.** Um administrador analisa a clínica — licença, localização, contacto — e passa-a para `Active`. Até lá, não pode publicar serviços nem receber vales.

A verificação é um processo humano. Registar-se não o torna verificado.

## Publicar serviços

Uma vez ativa, publique aquilo que fatura. Cada serviço precisa de:

- **Um código.** Um número que identifica a categoria. Use um esquema coerente.
- **Uma designação.** O que os pacientes verão, por exemplo "Consulta externa".
- **Um preço** em USDC.

> Mantenha as designações e os códigos **genéricos**. "Consulta externa", e não "Sessão de aconselhamento sobre VIH". Estes dados são publicados permanentemente numa blockchain pública, e um código suficientemente específico para ser clinicamente útil é suficientemente específico para expor um paciente.

Os preços são um mínimo. Um financiador pode pagar mais, nunca menos. Pode atualizar um preço a qualquer momento — os vales já financiados mantêm o montante acordado quando foram criados.

## Receber o pagamento

1. **O paciente chega.** Encontre o vale no Balcão da clínica e prima **Marcar paciente como atendido**. Isto é o `claim`.
2. **Preste os cuidados.**
3. **Um atestador confirma.** Não o senhor — o contrato rejeita uma atestação feita pela clínica que vai ser paga. Esta é a principal salvaguarda do protocolo e não é algo que possa contornar.
4. **Decorre o prazo de contestação.** 72 horas durante as quais o financiador pode contestar.
5. **Recebe o pagamento.** Depois do prazo, qualquer pessoa pode desencadear a liquidação, incluindo o senhor. Prima **Libertar pagamento**.

Recebe o montante do vale menos a taxa de protocolo de 1%. Um vale de $3.00 paga-lhe $2.97.

## Coisas que vale a pena saber

**Não depende de ninguém para ser pago.** A liquidação não requer permissão. Assim que o prazo termina, os fundos são seus para reclamar e nenhum operador os pode reter.

**Reclame sem demora.** Um vale não pode ser reclamado depois de expirar — o financiador recupera-o.

**Suspensão.** Um administrador pode suspender uma clínica. Uma clínica suspensa não pode receber novos vales nem alterar preços. Os vales já financiados continuam a ser liquidados normalmente.

**Obter moeda local.** A liquidação é feita em USDC. A conversão para naira, xelins ou numerário passa por uma âncora Stellar — Yellowcard, Cowrie, HoneyCoin, MoneyGram. Trata-se de uma etapa separada e regulamentada, com as suas próprias verificações de identidade. A integração na aplicação é [uma questão em aberto](https://github.com/Godgift-care/sci-healthcare-frontend/issues), ainda não desenvolvida.

## Mais perguntas

Consulte as [Perguntas frequentes para clínicas](../guides/clinics-faq.md) sobre prazos de pagamento, contestações, taxas e outras questões comuns, cada uma com ligações para a fonte.

_Tradução assinalada para revisão por um falante nativo._
