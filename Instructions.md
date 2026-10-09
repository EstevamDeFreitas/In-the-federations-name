# CODEX TASK 002 — Deterministic Commands & Immutable Snapshots

## 1. Contexto

O projeto **In The Federation's Name** já possui o Marco 001 concluído e revisado.

A base atual contém:

- workspace Rust;
- `federation-sim` como núcleo headless;
- terreno baseado em heightmap global;
- chunks com `dirty` e `revision`;
- veículos armazenados em SoA;
- scheduler com pool Rayon própria;
- timestep fixo;
- inicialização determinística;
- checksum estável;
- testes de determinismo entre diferentes quantidades de threads;
- executável headless;
- microbenchmark multithread;
- CI.

A arquitetura atual foi aprovada como fundação.

Esta tarefa deve **evoluir a implementação existente**, sem reescrever componentes que já funcionam.

---

# 2. Objetivo

Criar uma fronteira formal entre:

```text
futuro frontend / ferramentas
            ↓
         COMMANDS
            ↓
    Simulation autoritativa
            ↓
        SNAPSHOTS
            ↓
futuro frontend / ferramentas
```

A simulação deve permanecer como única proprietária do estado mutável.

O consumidor externo poderá:

1. solicitar mudanças por comandos;
2. avançar a simulação;
3. consultar snapshots imutáveis;
4. detectar alterações incrementais através de revisões.

O objetivo arquitetural é preparar o núcleo para:

- renderer futuro;
- ferramentas de edição;
- construção de estradas;
- terraformação;
- UI;
- debugging;
- replay;
- savegame;
- possíveis sistemas de rede no futuro.

**Não implemente nenhum desses sistemas nesta tarefa.**

---

# 3. Princípio fundamental

Depois deste marco:

> Estado autoritativo da simulação não deve ser alterável diretamente por um consumidor externo.

Operações externas devem seguir:

```text
submit command
      ↓
pending commands
      ↓
tick boundary
      ↓
validation
      ↓
deterministic application
      ↓
simulation phases
```

E leitura deve seguir:

```text
Simulation
    ↓
immutable snapshot
    ↓
consumer
```

---

# 4. IDs tipados e opacos

Este é um requisito importante deste marco.

Atualmente os veículos possuem IDs sequenciais que coincidem com sua posição nos arrays internos.

Isso é aceitável como implementação inicial, mas **não deve virar contrato público**.

Crie um tipo público equivalente a:

```rust
pub struct VehicleId(u64);
```

Requisitos:

- `VehicleId` deve ser um tipo distinto.
- Não exponha o valor interno como representação de slot.
- O consumidor não pode assumir que:

```text
vehicle_id == index do Vec
```

- Internamente, a implementação atual pode continuar usando correspondência direta enquanto não houver remoções.
- Documente explicitamente que isso é um detalhe interno temporário.
- APIs públicas devem usar `VehicleId`.
- O projeto deve permanecer livre para futuramente usar:
  - sparse sets;
  - generation IDs;
  - free lists;
  - tabelas ID → slot;
  - compactação de SoA.

Não implemente ainda remoção ou reutilização de IDs.

Não crie um sistema genérico complexo de entity IDs nesta etapa.

---

# 5. Sistema de comandos

Criar tipos públicos aproximadamente equivalentes a:

```rust
SimulationCommand
CommandEnvelope
CommandSequence
CommandResult
CommandError
```

Os nomes exatos podem variar se houver justificativa clara.

## 5.1 Comandos iniciais

Implementar pelo menos:

```text
SetTerrainVertexHeight
SetVehicleVelocity
```

Exemplo conceitual:

```rust
SimulationCommand::SetTerrainVertexHeight {
    x,
    y,
    height,
}
```

e:

```rust
SimulationCommand::SetVehicleVelocity {
    vehicle_id,
    vx,
    vy,
}
```

---

# 6. Submissão e aplicação

Um comando enviado à simulação **não deve alterar imediatamente o estado autoritativo**.

Exemplo:

```rust
simulation.submit_command(command)?;
```

não deve modificar terreno ou veículo naquele instante.

A alteração deve acontecer apenas durante:

```rust
simulation.tick()
```

em uma fase explícita de comandos.

A nova ordem mínima do scheduler será:

```text
ApplyCommands
     ↓
Movement
     ↓
Finalize
```

A fase `Diagnostics` continua sob demanda.

---

# 7. Sequenciamento determinístico

Cada comando pendente deve possuir uma sequência estável.

Crie algo conceitualmente equivalente a:

```rust
CommandSequence(u64)
```

A ordem de aplicação deve ser definida explicitamente.

Requisitos:

- comandos devem ser aplicados em ordem determinística;
- a ordem não pode depender do Rayon;
- a ordem não pode depender de endereço de memória;
- a ordem não pode depender de hash map com ordenação instável;
- a quantidade de threads não pode alterar o resultado;
- dois comandos sobre o mesmo objeto devem respeitar a sequência.

Exemplo:

```text
sequence 10:
SetHeight(20,20,4)

sequence 11:
SetHeight(20,20,7)
```

Resultado:

```text
height = 7
```

Não permita duas sequências iguais dentro da mesma fila, salvo se houver uma regra explícita e testada.

A opção preferida é rejeitar duplicação.

---

# 8. Origem da sequência

Nesta etapa, não implemente produtores concorrentes.

A própria `Simulation` pode emitir uma sequência monotônica no momento da submissão.

Exemplo conceitual:

```text
submit A → sequence 1
submit B → sequence 2
submit C → sequence 3
```

Isso deverá ser determinístico para uma mesma ordem de chamadas.

No futuro poderemos separar:

```text
client command id
simulation sequence
network sequence
```

Mas não antecipe essa complexidade agora.

---

# 9. Validação dos comandos

Comandos devem ser validados antes de modificar o estado.

Erros possíveis incluem:

- vértice fora do mapa;
- `VehicleId` inexistente;
- valor fora de limites aceitos;
- sequence inválida;
- overflow de sequence.

Um comando inválido:

- não deve causar panic;
- não deve corromper a fila;
- não deve impedir necessariamente os comandos seguintes;
- deve produzir um resultado explícito.

A política recomendada é:

```text
cada comando produz seu próprio resultado
```

Exemplo:

```text
command 10 → Ok
command 11 → InvalidVehicle
command 12 → Ok
```

---

# 10. Resultado da aplicação

A aplicação do tick deve permitir consultar os resultados dos comandos processados.

Pode existir algo conceitualmente como:

```rust
AppliedCommandResult {
    sequence,
    result,
}
```

Não é necessário guardar o histórico indefinidamente.

Uma estratégia simples é manter os resultados do último tick ou retorná-los por uma API explícita.

Documente a escolha.

---

# 11. Remover mutações externas diretas

Hoje existe uma API pública semelhante a:

```rust
Simulation::set_height(...)
```

Após este marco, consumidores externos não devem poder modificar estado autoritativo por esse caminho.

A alteração de terreno deve passar por:

```text
SimulationCommand::SetTerrainVertexHeight
```

Portanto:

- tornar setters autoritativos internos (`pub(crate)` ou equivalente);
- preservar funções internas necessárias para testes unitários de `Terrain`;
- a API pública de `Simulation` não deve permitir contornar a fila de comandos.

Faça o mesmo para alterações de veículos introduzidas neste marco.

---

# 12. Snapshots imutáveis

Criar uma camada pública de leitura independente.

Não exponha `&Simulation` como futura API do renderer.

Criar estruturas equivalentes a:

```rust
SimulationSnapshot
RegionSnapshot
VehicleSnapshot
TerrainChunkSnapshot
```

Evite exagerar no número de tipos se uma estrutura mais simples for suficiente.

---

# 13. Propriedade dos snapshots

Um snapshot deve possuir os dados necessários de forma independente.

Este teste conceitual deve funcionar:

```text
snapshot A criado no tick 10

simulation.tick()
simulation.tick()
simulation.tick()

snapshot A continua representando tick 10
```

Ou seja:

- não use referências mutáveis para estado interno;
- não deixe snapshot apontando para buffers que serão sobrescritos;
- snapshots antigos permanecem válidos.

---

# 14. Snapshot não significa copiar o mundo inteiro

A criação de snapshot deve ser explícita e configurável.

Não copie automaticamente:

```text
todo terreno
+
todos os veículos
+
todos os sistemas
```

a cada tick.

Crie uma API de solicitação.

Algo conceitualmente semelhante a:

```rust
SnapshotRequest {
    vehicles: ...
    terrain_chunks: ...
}
```

ou APIs específicas.

Escolha a abordagem mais simples que preserve extensibilidade.

---

# 15. Snapshot de veículos

Inicialmente deve ser possível solicitar dados visuais mínimos de veículos:

```text
VehicleId
x
y
vx
vy
```

Esse formato será usado futuramente pelo renderer.

Não inclua ainda:

- mesh;
- textura;
- modelo;
- cor;
- rota;
- carga;
- proprietário.

Esses dados ainda não existem na simulação.

---

# 16. Snapshot de terreno

Deve ser possível solicitar dados de chunks específicos.

Cada chunk retornado deve conter:

```text
chunk coordinate
revision
height data necessária
```

Não dependa de `dirty` como protocolo externo.

---

# 17. Revision como mecanismo de sincronização

`dirty` continua sendo uma ferramenta interna válida.

Porém consumidores externos devem detectar mudanças através de:

```text
chunk.revision
```

Exemplo:

```text
renderer possui:
chunk (4,2) revision 18

simulation possui:
chunk (4,2) revision 20
```

Então:

```text
renderer solicita atualização
```

Isso é importante porque futuramente poderemos ter simultaneamente:

```text
renderer
debugger
editor
telemetry
```

e nenhum deles deve precisar limpar um `dirty flag` global.

---

# 18. Consulta incremental

Criar uma API equivalente a:

```text
get chunks changed since known revisions
```

Não precisa ser extremamente sofisticada.

Uma solução inicial pode aceitar algo como:

```rust
ChunkRevisionQuery
```

contendo:

```text
chunk coordinate
known revision
```

e retornar somente os chunks divergentes.

Alternativamente:

```text
snapshot_chunks_if_newer(...)
```

É aceitável.

O importante é provar o conceito.

---

# 19. Revisão e overflow

A implementação atual usa `u64` para revisão de chunk.

Não precisa substituir isso.

Porém documente o comportamento atual em overflow.

Se continuar saturando em `u64::MAX`, isso deve permanecer explícito.

Não crie mecanismo complexo de epoch nesta etapa.

---

# 20. Dirty state

`clear_terrain_dirty()` não deve fazer parte do protocolo público necessário ao frontend.

Se continuar existindo:

- deve ser API interna;
- ou claramente documentada como ferramenta interna.

Um consumidor nunca deve precisar chamar:

```text
clear all dirty flags
```

para sincronizar estado.

---

# 21. Scheduler

Atualizar as fases documentadas para:

```text
ApplyCommands
Movement
Finalize
```

O comportamento deve ser:

### ApplyCommands

```text
ordenar/verificar sequência
validar
aplicar de forma determinística
armazenar resultados
```

### Movement

```text
executar atualização paralela dos veículos
```

### Finalize

```text
incrementar tick
finalizar bookkeeping
```

Não paralelize `ApplyCommands` nesta etapa.

O volume ainda será pequeno e determinismo é mais importante.

---

# 22. Sem comandos

Executar:

```rust
simulation.tick()
```

sem comandos pendentes deve continuar tendo custo mínimo.

Evite:

- alocar vetor novo todo tick;
- ordenar vetor vazio;
- criar estruturas temporárias desnecessárias.

---

# 23. Determinismo

Preservar os requisitos do Marco 001.

Mesmo cenário:

```text
seed igual
comandos iguais
ordem igual
ticks iguais
```

deve produzir:

```text
estado igual
checksum igual
```

em:

```text
1 thread
2 threads
4 threads
8 threads
```

se o ambiente permitir.

---

# 24. Checksum

Atualizar o checksum se necessário para incluir qualquer novo estado **autoritativo persistente**.

Não inclua no checksum:

- snapshots;
- caches puramente derivados;
- dados temporários de benchmark.

Avalie cuidadosamente se:

```text
next_command_sequence
```

e filas pendentes devem fazer parte do checksum.

Se dois estados possuem o mesmo mundo mas filas diferentes que modificarão o próximo tick, eles não são logicamente equivalentes.

Portanto, o estado determinístico deve considerar comandos pendentes ou documentar outra forma de representação.

---

# 25. Testes obrigatórios — comandos

Adicionar testes cobrindo:

1. comando submetido não altera estado imediatamente;
2. comando é aplicado no próximo tick;
3. comandos respeitam sequência;
4. dois comandos sobre mesmo vértice produzem resultado previsível;
5. comando de terreno fora do mapa retorna erro;
6. comando para `VehicleId` inexistente retorna erro;
7. alteração de velocidade ocorre antes de `Movement`;
8. nova velocidade afeta o movimento daquele mesmo tick;
9. comando inválido não impede comando válido subsequente;
10. sequência permanece monotônica;
11. overflow ou condição extrema de sequência é tratada.

---

# 26. Testes obrigatórios — VehicleId

Adicionar testes demonstrando:

- `VehicleId` é usado pelas APIs públicas;
- IDs permanecem estáveis entre ticks;
- APIs não exigem conhecimento de índice interno;
- um ID inexistente é rejeitado corretamente.

Não é necessário implementar remoção para provar isso.

---

# 27. Testes obrigatórios — snapshots

Adicionar testes cobrindo:

1. snapshot possui tick correto;
2. snapshot de veículo possui estado correto;
3. snapshot antigo permanece inalterado após novos ticks;
4. alteração do estado autoritativo não modifica snapshot anterior;
5. consulta de chunk retorna revision correta;
6. chunk não alterado não precisa ser reenviado;
7. chunk alterado é retornado quando revision diverge;
8. alteração em vértice compartilhado incrementa revisões relevantes;
9. múltiplos consumidores podem manter revisions diferentes;
10. limpar `dirty` não interfere no mecanismo por revision.

---

# 28. Testes obrigatórios — determinismo

Construir cenário com:

```text
seed fixa
sequência fixa de comandos
múltiplos ticks
```

Rodar com pelo menos:

```text
1 thread
2 threads
4 threads
```

Comparar:

- checksum;
- terreno;
- posições;
- velocidades;
- resultados dos comandos relevantes.

Se possível, incluir 8 threads sem tornar a CI lenta.

---

# 29. Auditoria do benchmark do Marco 001

O benchmark atual foi considerado válido como microbenchmark, porém preliminar.

A tarefa deve melhorar sua metodologia.

## Problema atual

O resultado observado foi obtido com:

```text
100.000 veículos
30 warmup ticks
200 measured ticks
uma execução
ordem crescente de threads
```

O melhor resultado ocorreu com 16 threads em uma CPU com 12 CPUs lógicas.

Isso não é necessariamente incorreto, mas deve ser investigado.

---

# 30. Benchmark — múltiplas rodadas

Adicionar suporte a:

```text
--runs
```

Exemplo:

```text
--runs 5
```

Para cada quantidade de threads:

```text
criar simulação
warmup
medir ticks
repetir N vezes
```

Gerar estatísticas entre execuções.

---

# 31. Benchmark — duração maior

O benchmark deve permitir executar cenários significativamente maiores, por exemplo:

```text
2.000
5.000
10.000 ticks
```

Não altere necessariamente o default para algo muito pesado.

Documente um perfil recomendado para benchmark manual.

---

# 32. Benchmark — ordem

Evitar sempre executar:

```text
1 → 2 → 4 → 8 → 16
```

Adicionar opção para variar a ordem.

Uma solução simples:

```text
--shuffle-thread-order
```

com seed explícita.

Ou alternar rodadas em ordem diferente.

A reprodutibilidade da ordem deve ser possível.

---

# 33. Benchmark — resultados

Reportar pelo menos:

por quantidade de threads:

```text
mean tick
median tick
p95
p99
ticks/s
speedup
```

e entre execuções:

```text
median run
min
max
```

Se for simples, incluir desvio padrão.

Não transforme o benchmark em framework estatístico complexo.

---

# 34. Benchmark e comandos

Não inclua comandos no benchmark principal do Marco 001.

O objetivo dele ainda é medir:

```text
movimentação sintética independente
```

Se desejar medir overhead da nova fase vazia:

crie comparação simples entre:

```text
Marco anterior
vs
ApplyCommands sem comandos
```

ou registre o custo atual.

Não misture workloads diferentes na mesma métrica.

---

# 35. Limites temporários

A implementação atual possui proteções como:

```text
máximo de veículos
máximo de vértices
máximo de threads
```

Mantenha proteções razoáveis contra:

- overflow;
- alocações absurdas;
- configurações obviamente inválidas.

Porém:

> Esses números NÃO representam limites de design do jogo.

Atualizar documentação para chamar explicitamente esses valores de:

```text
temporary safety guards
```

ou equivalente.

Não escreva documentação dizendo que:

```text
o jogo suporta no máximo X
```

com base nesses números.

---

# 36. API pública

Após esta tarefa, a direção desejada é:

```rust
Simulation::new(...)

Simulation::submit_command(...)

Simulation::tick()

Simulation::snapshot(...)

Simulation::checksum()
```

Não é obrigatório usar exatamente essas assinaturas.

O importante é preservar a fronteira conceitual.

---

# 37. Estrutura sugerida

Pode ser criada algo como:

```text
crates/federation-sim/src/
├── commands.rs
├── snapshots.rs
├── ids.rs
```

além dos módulos existentes.

Não crie crate nova só para commands ou snapshots.

Eles pertencem ao domínio do núcleo neste estágio.

---

# 38. Alocações e performance

Este marco ainda deve preservar a filosofia data-oriented.

Evite:

- uma `Box` por comando;
- trait objects para cada comando;
- heap allocation individual por veículo em snapshots;
- mutex por objeto;
- clone completo de `Simulation`.

Um enum:

```rust
enum SimulationCommand
```

é preferível nesta etapa a uma hierarquia baseada em traits.

---

# 39. Snapshots e SoA

Snapshots não precisam necessariamente copiar a mesma estrutura SoA interna.

Para renderer futuro, um formato compacto pode ser apropriado.

Exemplo conceitual:

```rust
VehicleVisualSnapshot {
    id,
    x,
    y,
}
```

ou vetores estruturados.

Escolha a opção mais simples.

Documente que:

```text
snapshot representation != simulation storage representation
```

Essa separação será importante no futuro.

---

# 40. Não implementar nesta tarefa

Não adicionar:

- renderer;
- `wgpu`;
- `winit`;
- Bevy;
- ECS;
- pathfinding;
- estradas;
- ferrovias;
- cidadãos;
- emprego;
- transporte público;
- economia;
- fábricas;
- logística;
- save/load;
- replay completo;
- networking;
- multiplayer;
- Tokio;
- banco de dados;
- sistema de plugins;
- scripting;
- undo/redo.

Alguns desses sistemas usarão Commands/Snapshots depois.

Não os antecipe.

---

# 41. Documentação

Atualizar:

```text
README.md
docs/architecture.md
docs/benchmarks.md
AGENTS.md
```

Documentar claramente:

## Commands

```text
submit
queue
apply
result
ordering
```

## Snapshots

```text
ownership
immutability
revision queries
copy cost
```

## IDs

```text
public identity
internal storage independence
```

## Scheduler

```text
ApplyCommands
Movement
Finalize
```

## Determinismo

Explicar como comandos entram no estado determinístico.

---

# 42. AGENTS.md

Adicionar regras para futuras implementações:

- Nunca assumir que `VehicleId` é índice de array.
- Estado externo deve ser alterado por comandos.
- Renderers não recebem acesso mutável ao estado.
- Snapshots não devem determinar arquitetura interna da simulação.
- `dirty` interno não é protocolo de sincronização externo.
- Revisões são preferidas para sincronização incremental.
- Não introduzir concorrência em comandos sem benchmark e necessidade concreta.
- Novos tipos de comando precisam de testes de determinismo.

---

# 43. CI

Preservar:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build --workspace --release --locked
```

Não executar benchmark pesado em toda CI.

Testes do sistema de comandos e snapshots devem ser rápidos.

---

# 44. Critérios de aceitação

O Marco 002 será considerado concluído somente quando:

- [ ] existir `VehicleId` público e opaco;
- [ ] APIs públicas não dependerem de índice interno;
- [ ] comandos forem submetidos sem alterar imediatamente o estado;
- [ ] comandos forem aplicados na fase `ApplyCommands`;
- [ ] a ordem de aplicação for determinística;
- [ ] comandos inválidos retornarem resultados explícitos;
- [ ] comandos inválidos não corromperem a simulação;
- [ ] alteração de terreno externa exigir comando;
- [ ] alteração de veículo externa exigir comando;
- [ ] não existir bypass público equivalente a `Simulation::set_height`;
- [ ] snapshots forem imutáveis e independentes;
- [ ] snapshots antigos continuarem válidos;
- [ ] snapshots puderem ser solicitados explicitamente;
- [ ] terreno não seja copiado integralmente sem necessidade;
- [ ] revisões de chunks permitirem sincronização incremental;
- [ ] `dirty` não seja necessário como protocolo externo;
- [ ] determinismo funcione entre diferentes quantidades de threads;
- [ ] checksum represente adequadamente o estado determinístico;
- [ ] benchmark suporte múltiplas rodadas;
- [ ] benchmark permita metodologia mais longa/reproduzível;
- [ ] documentação seja atualizada;
- [ ] testes existentes continuem passando;
- [ ] CI continue válida;
- [ ] nenhuma dependência gráfica entre no core.

---

# 45. Processo recomendado

Executar nesta ordem:

1. Inspecionar novamente a implementação existente.
2. Identificar APIs públicas que permitem mutação direta.
3. Introduzir `VehicleId`.
4. Adaptar APIs existentes.
5. Implementar tipos de comando.
6. Implementar fila e sequence.
7. Adicionar `ApplyCommands` ao scheduler.
8. Implementar comandos de terreno.
9. Implementar comandos de veículo.
10. Criar resultados de aplicação.
11. Implementar snapshots.
12. Implementar consultas incrementais por revision.
13. Adicionar testes.
14. Atualizar checksum se necessário.
15. Melhorar benchmark.
16. Atualizar documentação.
17. Executar fmt/clippy/test/build.
18. Corrigir todas as falhas.
19. Rodar benchmark representativo.
20. Entregar relatório final.

---

# 46. Não interromper por decisões pequenas

Se surgir uma decisão de implementação local que:

- não altera o game design;
- não altera os princípios arquiteturais;
- é facilmente reversível;

tome uma decisão razoável e documente-a.

Não interrompa o trabalho para pedir confirmação sobre nomes de métodos ou pequenas estruturas internas.

---

# 47. Entrega final esperada

Ao concluir, responder com:

## A. Resumo

O que foi implementado.

## B. Mudanças arquiteturais

Principalmente:

```text
IDs
Commands
Scheduler
Snapshots
Revisions
```

## C. API resultante

Mostrar exemplos curtos da API pública.

## D. Testes

Listar testes adicionados e resultados.

## E. Verificações

Informar resultados reais de:

```text
fmt
clippy
test
build
```

## F. Benchmark

Apresentar resultados reais da metodologia revisada.

Não inventar números.

## G. Limitações

Identificar conscientemente o que ainda está simplificado.

## H. Riscos encontrados

Apontar qualquer decisão atual que possa dificultar:

```text
Marco 003 — Transport Network
```

## I. Próximo marco

Não implementar o próximo marco.

Apenas avaliar se a arquitetura atual está pronta para ele.

---

# 48. Objetivo arquitetural final

Ao término do Marco 002, queremos chegar a:

```text
                 FUTURE CLIENT
                      │
                  Commands
                      ↓
        ┌──────────────────────────┐
        │       Simulation         │
        │                          │
        │    pending commands      │
        │           ↓              │
        │     ApplyCommands        │
        │           ↓              │
        │       Movement           │
        │           ↓              │
        │       Finalize           │
        │                          │
        │   authoritative state    │
        └────────────┬─────────────┘
                     │
                Snapshots
                     ↓
                 CLIENT
```

O cliente nunca possui o estado autoritativo.

O renderer futuro não determina como a simulação armazena dados.

IDs públicos não determinam slots internos.

Chunks usam revisões para sincronização incremental.

Comandos definem uma fronteira determinística para alteração do mundo.

Essa será a fundação necessária antes de começarmos o:

```text
MARCO 003
TRANSPORT NETWORK & ROUTING FOUNDATION
```