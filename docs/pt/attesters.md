---
title: Atestar a prestação de cuidados
nav_order: 8
language: pt
translations:
  en: /guides/attesters.html
  fr: /fr/attesters.html
  sw: /sw/attesters.html
  yo: /yo/attesters.html
  ig: /ig/attesters.html
  ha: /ha/attesters.html
  ar: /ar/attesters.html
  am: /am/attesters.html
---

# Atestar a prestação de cuidados

Um atestador confirma que os cuidados foram efetivamente prestados. É o papel mais importante do protocolo e o que acarreta maior responsabilidade.

## O que está a fazer

Quando atesta, inicia uma contagem de 72 horas. Quando termina, a clínica é paga de forma automática e irreversível, e o senhor recebe a sua parte configurada da taxa de protocolo.

Não está a aprovar um pedido de pagamento. Está a afirmar um facto sobre o mundo: este paciente recebeu este serviço nesta clínica.

## Quem faz isto

Os atestadores são nomeados por um administrador. Na prática, são pessoas já presentes no percurso de cuidados e que não são empregadas pela clínica que vai ser paga — agentes comunitários de saúde, enfermeiros supervisores de outra unidade, pessoal de terreno de ONG, responsáveis distritais de saúde.

O essencial é a independência. Se o rendimento do atestador depender de a clínica ser paga, a salvaguarda não funciona.

## O que recebe

O protocolo paga ao atestador que confirmou a prestação uma parte configurada da sua taxa. A implementação define `attester_share_bps` na inicialização (50 bps, metade da taxa de 1%, na configuração atual). Esta parte é retirada da taxa de protocolo, não do montante líquido da clínica, e é paga pelo contrato no momento da liquidação.

Esse pagamento é fixado pela configuração e pelo contrato. Uma clínica não o pode definir, alterar, nem oferecer mais em troca de uma decisão favorável. Considere-o uma compensação do protocolo pela atestação, nunca um pagamento da clínica, do paciente ou do financiador.

## Como

1. **Ligue a sua carteira.** A página Atestar mostra todos os vales em espera.
2. **Verifique que os cuidados foram prestados.** Da forma que o seu programa o fizer — estando presente, consultando o registo, falando com o paciente.
3. **Prima Confirmar cuidados prestados.**

## O que o contrato impõe

- **Não pode atestar para uma clínica que é sua.** Se o endereço do atestador coincidir com o endereço do prestador, a chamada é rejeitada. Sempre, sem exceções.
- **Tem de ser um atestador autorizado.** Os atestadores revogados são rejeitados.
- **O vale tem de estar `Claimed`.** Não pode atestar antes de a clínica marcar o paciente como atendido, nem atestar duas vezes.

## O que não impõe

Que está a dizer a verdade. Esta é a hipótese de confiança central do protocolo, documentada com clareza no [modelo de confiança](../protocol/trust.md).

O que faz: a sua atestação é assinada pelo seu endereço e registada permanentemente na cadeia. Todos os vales que alguma vez confirmou são públicos e atribuíveis. Não existe atestação anónima.

## Antes de atestar

Percorra sempre a [lista de verificação de provas e o código de conduta do atestador](../guides/attester-checklist.md): o que confirmar antes de premir **Confirmar cuidados prestados** e quando deve abster-se.

## Se tiver dúvidas

Não ateste. Um vale não atestado não se perde — permanece em garantia e é reembolsado ao financiador na data de expiração, ou de imediato se a clínica já não estiver ativa. A consequência de não atestar é um atraso. A consequência de atestar indevidamente é o dinheiro de um paciente ir para uma clínica que nada fez, sem forma de o recuperar após 72 horas.

Se considerar que uma clínica está a reclamar por cuidados que não prestou, informe um administrador para que possa ser suspensa.

_Tradução assinalada para revisão por um falante nativo._
