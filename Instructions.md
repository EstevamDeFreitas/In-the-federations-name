# CODEX TASK 003 — Transport Network & Routing Foundation

## 1. Contexto

O projeto **In The Federation's Name** possui os Marcos 001 e 002 concluídos.

A arquitetura atual inclui:

- núcleo Rust completamente headless;
- scheduler multithread baseado em Rayon;
- timestep fixo;
- terreno em heightmap com chunks;
- veículos armazenados em SoA;
- IDs públicos tipados;
- comandos determinísticos;
- snapshots owned e seletivos;
- revisões de chunks;
- checksum determinístico;
- benchmarks reproduzíveis;
- CI.

O Marco 003 introduzirá a primeira infraestrutura lógica do mundo:

> **uma rede de transporte direcionada, dinâmica e roteável.**

Este marco não deve tentar implementar trânsito completo.

Seu objetivo é construir a estrutura sobre a qual futuramente existirão:

- estradas;
- ferrovias;
- ônibus;
- caminhões;
- transporte de trabalhadores;
- logística industrial;
- pathfinding hierárquico;
- congestionamento;
- linhas de transporte público.

---

# 2. Objetivos principais

Implementar:

1. grafo lógico de transporte;
2. IDs estáveis para nós e arestas;
3. edição da rede via Commands;
4. revisão global da rede;
5. pathfinding determinístico;
6. workspace de routing reutilizável;
7. snapshots de diagnóstico da rede;
8. benchmark específico de routing;
9. testes de determinismo e correção;
10. correções arquiteturais restantes do Marco 002.

---

# 3. Correções obrigatórias do Marco 002

Antes da implementação da rede, aplicar três ajustes.

## 3.1 Remover `dirty` do checksum

O checksum atual inclui:

```text
chunk.revision
chunk.dirty
```

`dirty` é estado operacional interno e não estado autoritativo do mundo.

Remover `dirty` do checksum.

O checksum deve continuar incluindo:

```text
chunk.revision
terrain height data
```

Adicionar teste provando que:

```text
clear_dirty()
```

não altera o checksum.

---

## 3.2 Restringir exposição do storage interno

As APIs públicas semelhantes a:

```rust
Simulation::terrain()
Simulation::vehicles()
```

não devem funcionar como contrato normal para consumidores externos.

Migrar a API pública para:

```text
Commands
Snapshots
Diagnostics
Routing
```

Quando possível:

- tornar getters de storage `pub(crate)`;
- migrar testes de integração para snapshots;
- manter testes unitários internos diretamente sobre `Terrain` e `Vehicles`.

Não crie uma abstração artificial apenas para esconder código.

O objetivo é impedir dependência externa no layout SoA.

---

## 3.3 Metadata de snapshot

Adicionar ao snapshot metadata equivalente a:

```rust
RegionSnapshotMetadata {
    width: u32,
    height: u32,
    chunk_size: u32,
}
```

O consumidor deve conseguir interpretar terreno e coordenadas sem acessar `SimulationConfig`.

---

# 4. Modelo conceitual da rede

Criar um novo módulo aproximadamente:

```text
transport/
├── mod.rs
├── network.rs
├── ids.rs
├── routing.rs
└── snapshot.rs
```

Pequenos ajustes são aceitáveis.

A estrutura principal será:

```text
TransportNetwork
    │
    ├── Nodes
    │
    └── Directed Edges
```

---

# 5. Grafo direcionado e multigrafo

A rede deve ser um **grafo direcionado**.

Uma estrada de mão dupla será representada conceitualmente como:

```text
A → B
A ← B
```

ou seja:

```text
duas arestas direcionadas
```

A rede deve permitir múltiplas arestas entre os mesmos nós.

Exemplo:

```text
A ─road lane────→ B
A ─expressway────→ B
A ─rail──────────→ B
```

Não assuma que:

```text
(from, to)
```

identifica uma aresta de forma única.

Isso é importante para:

- vias paralelas;
- sentidos diferentes;
- futuras lanes;
- modos diferentes;
- pontes e túneis.

---

# 6. IDs

Adicionar:

```rust
TransportNodeId
TransportEdgeId
```

Requisitos:

- tipos públicos distintos;
- internamente podem usar `u64`;
- não devem expor slots internos;
- IDs não devem ser reutilizados neste marco;
- remoção pode deixar tombstones;
- IDs devem permanecer estáveis.

Não criar ainda sistema genérico universal de entidade.

---

# 7. Alocação de IDs

A própria `Simulation` deve controlar a geração dos IDs.

Adicionar contadores monotônicos equivalentes a:

```text
next_transport_node_id
next_transport_edge_id
```

Esses contadores fazem parte do estado determinístico e devem entrar no checksum.

Para permitir criar nós e arestas no mesmo tick, disponibilizar APIs equivalentes a:

```rust
reserve_transport_node_id()
reserve_transport_edge_id()
```

ou métodos de submissão especializados:

```rust
submit_create_transport_node(...)
submit_create_transport_edge(...)
```

A escolha pode variar.

Requisito fundamental:

```text
ID allocation order
=
command submission order
```

e deve ser determinística.

IDs reservados e nunca utilizados não devem ser reutilizados.

---

# 8. Coordenadas dos nós

Cada nó deve possuir posição inteira.

Usar algo equivalente a:

```rust
TransportPosition {
    x: i64,
    y: i64,
    z: i64,
}
```

Não usar floats.

`x` e `y` devem ser compatíveis conceitualmente com:

```text
POSITION_UNITS_PER_TILE
```

já utilizado pelos veículos.

`z` deve existir desde agora para permitir futuramente distinguir:

```text
estrada
ponte
túnel
viaduto
```

Dois nós podem possuir o mesmo:

```text
x
y
```

e ainda serem entidades diferentes.

Portanto:

> posição espacial não determina conectividade.

---

# 9. Modos de transporte

Adicionar enum inicial:

```rust
TransportMode {
    Road,
    Rail,
}
```

Não implementar diferenças físicas complexas entre eles ainda.

Apenas permitir:

- edges com modo;
- routing filtrado por modo.

Não adicionar ainda:

- pedestrian;
- tram;
- subway;
- air;
- ship.

Podem ser adicionados posteriormente.

---

# 10. Arestas

Cada aresta deve conter no mínimo:

```text
TransportEdgeId
from
to
mode
base_cost
enabled
```

Usar custo inteiro.

Sugestão:

```rust
base_cost: u64
```

ou outro inteiro adequado.

Requisitos:

```text
base_cost > 0
```

Não permitir custo negativo.

`enabled = false` representa algo como:

```text
estrada bloqueada
obra
acidente
interdição
```

sem necessidade de remover a aresta.

---

# 11. Não modelar congestionamento ainda

O custo deste marco é:

```text
base_cost
```

Não adicionar:

```text
traffic density
queue length
congestion
vehicle count
weather
fuel
intersection delay
```

Isso será responsabilidade de marcos futuros.

O pathfinding atual fornecerá o **routing baseline**.

---

# 12. Armazenamento

Escolha uma implementação simples e mensurável.

Uma implementação aceitável é:

```text
Vec<NodeRecord>
Vec<EdgeRecord>
adjacency per node
```

com tombstones para remoções.

Não implemente CSR ou estrutura altamente especializada sem evidência de necessidade.

Porém:

- evitar hash maps no hot path quando não necessários;
- IDs devem resolver para slots de forma controlada;
- adjacency deve preservar comportamento determinístico.

Documente o layout escolhido.

---

# 13. Remoção

Implementar:

```text
RemoveTransportEdge
RemoveTransportNode
```

Regras:

### Edge

Uma edge removida:

```text
alive = false
```

ou equivalente.

Não reutilizar ID.

### Node

Um node só pode ser removido se não possuir edges vivas incidentes.

Caso contrário:

```text
CommandError::TransportNodeInUse
```

ou equivalente.

Não fazer remoção automática em cascata.

---

# 14. Commands

Adicionar Commands equivalentes a:

```text
CreateTransportNode
CreateTransportEdge
RemoveTransportNode
RemoveTransportEdge
SetTransportEdgeEnabled
SetTransportEdgeCost
```

Todos devem passar pela fase:

```text
ApplyCommands
```

Nunca modificar rede diretamente através de API pública.

---

# 15. Validação

Validar:

### Node

- ID reservado válido;
- ID ainda não utilizado;
- posição dentro de limites de segurança.

Não necessariamente dentro do mapa físico em todos os eixos, pois conexões externas poderão existir futuramente.

### Edge

Validar:

```text
from exists
to exists
from != to
cost > 0
mode valid
edge id unused
```

Self-loops podem ser rejeitados neste marco.

Documentar essa decisão.

---

# 16. Dependências entre comandos do mesmo tick

Deve ser possível:

```text
seq 1 → CreateNode A
seq 2 → CreateNode B
seq 3 → CreateEdge A→B
```

no mesmo tick.

Como `ApplyCommands` é serial e ordenado:

```text
node A exists
↓
node B exists
↓
edge can be created
```

Adicionar teste explícito.

---

# 17. Revisão da rede

`TransportNetwork` deve possuir:

```rust
revision: u64
```

A revisão aumenta quando uma alteração efetiva muda routing ou topologia.

Incrementar em:

```text
create node
remove node
create edge
remove edge
change edge cost
enable edge
disable edge
```

Não incrementar se:

```text
SetEdgeCost(existing_value)
```

ou:

```text
SetEnabled(existing_value)
```

não altera nada.

---

# 18. Revisão global inicialmente

Neste marco, uma única:

```text
network_revision
```

é suficiente.

Isso significa que qualquer alteração invalida potencialmente qualquer cache futuro.

Essa estratégia é deliberadamente conservadora.

Documentar:

> global revision é baseline, não compromisso arquitetural permanente.

Futuramente poderemos usar:

```text
component revisions
sector revisions
edge revisions
```

se benchmarks justificarem.

---

# 19. Overflow de revisão

Usar comportamento explícito.

Pode continuar seguindo a política atual de revisão saturante:

```text
u64::MAX
```

ou retornar erro.

Escolher uma estratégia consistente e documentar.

Não implementar epochs.

---

# 20. Pathfinding inicial

Implementar **Dijkstra determinístico**.

Não implementar A* ainda como requisito.

Motivo:

- custo da edge é abstrato;
- ainda não existe uma relação garantida entre distância geométrica e custo;
- queremos um baseline correto antes de introduzir heurísticas.

O algoritmo deve encontrar:

```text
menor custo total
```

entre:

```text
origin
destination
```

para um único:

```text
TransportMode
```

---

# 21. RouteRequest

Criar estrutura equivalente a:

```rust
RouteRequest {
    origin: TransportNodeId,
    destination: TransportNodeId,
    mode: TransportMode,
}
```

Validar:

```text
origin exists
destination exists
```

---

# 22. RouteResult

Resultado mínimo:

```rust
Route {
    network_revision,
    origin,
    destination,
    mode,
    total_cost,
    edges,
}
```

`edges` deve estar na ordem de percurso.

Pode incluir nodes se isso facilitar.

Não retornar apenas custo.

O jogo precisará futuramente seguir a rota.

---

# 23. Casos especiais

### origin == destination

Retornar:

```text
cost = 0
edges = []
```

### destino inalcançável

Retornar erro ou enum explícito:

```text
NoRoute
```

Não retornar `None` sem contexto.

---

# 24. Determinismo do pathfinding

Quando duas rotas possuem exatamente o mesmo custo, o resultado deve continuar determinístico.

Definir tie-breaking estável.

Sugestão:

```text
cost
↓
node id
↓
edge id
```

ou regra equivalente.

O resultado não pode depender:

- de endereço de memória;
- de `HashMap` randomizado;
- da ordem escolhida pelo Rayon;
- de quantidade de threads.

Adicionar testes com duas rotas de mesmo custo.

---

# 25. RoutingWorkspace

Evitar alocar estruturas grandes para cada rota.

Criar algo equivalente a:

```rust
RoutingWorkspace
```

para armazenar scratch data reutilizável:

```text
distance
previous edge
visited
heap
generation markers
```

A implementação exata é livre.

Objetivo:

```text
route 1
route 2
route 3
```

não deve precisar recriar buffers do tamanho da rede em toda consulta.

---

# 26. RoutingWorkspace não é estado autoritativo

O workspace:

- não entra no checksum;
- não entra em savegame;
- não faz parte do snapshot;
- pode ser descartado;
- pode ser recriado.

Duas chamadas com workspaces diferentes devem produzir resultado idêntico.

---

# 27. Routing paralelo futuro

O design deve permitir:

```text
1 network
+
N RoutingWorkspace
```

usados em paralelo.

Não colocar:

```text
Mutex<RoutingWorkspace>
```

dentro da rede.

A rede deve ser consultável imutavelmente.

---

# 28. API de routing

Criar API aproximadamente equivalente a:

```rust
simulation.find_route(
    request,
    &mut workspace
)
```

ou:

```rust
TransportNetwork::find_route(...)
```

desde que o storage interno continue encapsulado.

Preferir que sistemas externos não recebam acesso direto a arrays internos.

---

# 29. Métricas de routing

Adicionar métricas opcionais por consulta:

```text
visited_nodes
relaxed_edges
heap_pushes
```

Não precisam fazer parte do resultado normal se isso prejudicar a API.

Pode existir:

```rust
RouteDiagnostics
```

ou modo específico para benchmark.

Essas métricas serão importantes para entender gargalos.

---

# 30. Snapshot da rede

Adicionar ao sistema de snapshots suporte opcional a transporte.

O snapshot pode incluir:

```text
network_revision
nodes
edges
```

Não copiar a rede automaticamente.

Criar seleção explícita.

Algo equivalente a:

```rust
TransportSnapshotRequest {
    include_nodes: bool,
    include_edges: bool,
    known_revision: Option<u64>,
}
```

Se:

```text
known_revision == current_revision
```

nenhum dado pesado precisa ser retornado.

---

# 31. Representação de snapshot

Criar estruturas semelhantes a:

```rust
TransportNodeSnapshot {
    id,
    position,
}

TransportEdgeSnapshot {
    id,
    from,
    to,
    mode,
    base_cost,
    enabled,
}
```

Snapshot não deve expor slots internos.

---

# 32. Checksum

Atualizar checksum para nova versão:

```text
federation-state-v3
```

Adicionar:

```text
next node id
next edge id
network revision
node state
edge state
```

A ordem deve ser estável.

Incluir:

```text
alive/tombstone state
```

porque remoções fazem parte do estado.

Não incluir:

```text
RoutingWorkspace
route results
snapshot buffers
benchmark metrics
```

---

# 33. Commands pendentes

Como no Marco 002, comandos pendentes continuam entrando no checksum.

Novos Commands de transporte também devem ser serializados deterministicamente no checksum da fila.

---

# 34. Testes obrigatórios — Network

Adicionar testes para:

1. criação de node;
2. criação de edge;
3. duas directed edges para simular mão dupla;
4. múltiplas edges entre os mesmos nodes;
5. road e rail coexistindo;
6. edge com node inexistente rejeitada;
7. edge com custo zero rejeitada;
8. self-loop rejeitado;
9. remoção de edge;
10. remoção de node sem edges;
11. remoção de node com edges rejeitada;
12. IDs não reutilizados.

---

# 35. Testes obrigatórios — Commands

Testar:

```text
Create A
Create B
Create A→B
```

no mesmo tick.

Testar também:

```text
Create invalid edge
Create valid edge afterward
```

O comando inválido não deve impedir os seguintes.

---

# 36. Testes obrigatórios — Revision

Testar:

```text
create node → revision +1
create edge → +1
disable edge → +1
disable already disabled → unchanged
same cost → unchanged
remove edge → +1
```

---

# 37. Testes obrigatórios — Routing

Criar redes pequenas verificáveis manualmente.

Exemplo:

```text
A --5--> B --5--> D
 \               ↑
  \--20--> C ----/
```

O algoritmo deve escolher:

```text
A → B → D
```

quando apropriado.

Cobrir:

1. rota simples;
2. origem = destino;
3. destino inalcançável;
4. directed edge;
5. edge disabled;
6. modo incorreto;
7. múltiplas edges;
8. rota de menor custo;
9. route cost grande sem overflow;
10. rede alterada mudando a rota.

---

# 38. Tie-breaking

Criar cenário:

```text
A → B → D
A → C → D
```

com custo total idêntico.

Executar repetidamente.

Resultado deve ser sempre igual.

Executar com diferentes quantidades de threads quando aplicável.

---

# 39. Determinismo completo

Construir um cenário com:

```text
seed fixa
network commands
vehicle commands
terrain commands
routing
```

Executar com:

```text
1
2
4
8 threads
```

Comparar:

```text
checksum
network snapshot
routes
command results
```

Tudo deve coincidir.

---

# 40. Benchmark de routing

Adicionar benchmark separado do movimento.

Preferencialmente como outro binário dentro de:

```text
federation-bench
```

por exemplo:

```text
federation-route-bench
```

Não criar nova crate sem necessidade.

---

# 41. Gerador sintético de rede

Criar redes determinísticas em grid.

Exemplo:

```text
128 × 128 nodes
```

com conexões bidirecionais horizontais e verticais.

Isso gera aproximadamente:

```text
16k nodes
~65k directed edges
```

Também permitir cenários maiores como:

```text
256 × 256
```

Não fixar esses números como limites.

---

# 42. Queries sintéticas

Gerar pares determinísticos:

```text
origin
destination
```

a partir de seed.

Evitar apenas rotas triviais próximas.

O benchmark deve incluir distâncias variadas.

---

# 43. Benchmark serial

Medir:

```text
route latency
routes/s
```

e:

```text
P50
P95
P99
```

Também agregar:

```text
visited nodes
relaxed edges
```

se as métricas existirem.

---

# 44. Benchmark paralelo

Independent route queries podem ser executadas em paralelo.

Usar:

```text
1
2
4
8
12
16
```

threads quando configurado.

A rede deve ser compartilhada somente para leitura.

Cada worker deve possuir seu próprio:

```text
RoutingWorkspace
```

Não usar mutex global.

Uma opção aceitável é:

```rust
rayon::par_iter()
    .map_init(RoutingWorkspace::new, ...)
```

ou equivalente.

---

# 45. Determinismo do benchmark

As queries devem possuir ordem/index original.

Mesmo quando processadas em paralelo:

```text
result[i]
```

deve corresponder à:

```text
query[i]
```

Criar checksum dos resultados.

Todos os thread counts devem produzir o mesmo checksum.

---

# 46. O que medir

Relatório JSON deve incluir pelo menos:

```text
nodes
edges
queries
threads
runs
mean_ms_per_route
median
p95
p99
routes_per_second
speedup
route_output_checksum
```

Se disponível:

```text
average_visited_nodes
average_relaxed_edges
```

---

# 47. Benchmark não estabelece meta ainda

Não criar critérios como:

```text
100k rotas devem rodar em X ms
```

neste marco.

Primeiro precisamos obter baseline.

Documentar:

> benchmark sintético de grid não representa uma cidade real.

---

# 48. Não implementar route cache ainda

Não implementar cache persistente de rotas.

Primeiro medir custo bruto.

A única informação para futuro cache é:

```text
network_revision
```

Um futuro cache poderá usar algo como:

```text
(origin, destination, mode, revision)
```

mas não implementar agora.

---

# 49. Não implementar A* ainda

Dijkstra será nosso baseline.

Depois dos benchmarks poderemos comparar:

```text
Dijkstra
A*
hierarchical routing
contraction hierarchies
sector graph
route caching
```

Não escolher antecipadamente.

---

# 50. Não implementar traffic simulation

Não adicionar:

```text
vehicle following
lane changing
signals
intersection reservation
traffic density
congestion
queueing
accidents
parking
```

Isso não pertence ao Marco 003.

---

# 51. Não conectar veículos às rotas ainda

Os veículos atuais continuam com movimento sintético.

Não modificar:

```text
Vehicles::move_parallel()
```

para seguir edges.

Isso será trabalho posterior.

O Marco 003 deve provar a rede e routing isoladamente primeiro.

---

# 52. Não implementar construção visual de estradas

Ainda não implementar:

```text
road tiles
curves
junction meshes
terrain cutting
bridge meshes
rail meshes
```

A rede neste marco é **lógica**.

O renderer virá posteriormente.

---

# 53. Documentação

Atualizar:

```text
README.md
AGENTS.md
docs/architecture.md
docs/benchmarks.md
```

Criar, se útil:

```text
docs/transport-network.md
```

---

# 54. Documentar decisões

Explicar:

- directed multigraph;
- IDs estáveis;
- tombstones;
- node coordinates;
- z independente;
- Road/Rail;
- global network revision;
- Dijkstra baseline;
- deterministic tie-breaking;
- RoutingWorkspace;
- ausência deliberada de cache;
- ausência deliberada de congestionamento.

---

# 55. AGENTS.md

Adicionar regras:

- Nunca assumir que dois nodes com mesma posição estão conectados.
- Nunca assumir edge bidirecional.
- Bidirectional transport requires two edges.
- Nunca usar `(from,to)` como identity de edge.
- Routing deve usar integer costs.
- RoutingWorkspace não é estado autoritativo.
- Routing output deve ser determinístico.
- Alterações na rede devem ocorrer via Commands.
- Toda mudança que afeta routing deve atualizar network revision.
- Não adicionar cache sem benchmark.
- Não adicionar heurística A* sem provar admissibilidade.

---

# 56. CI

Preservar:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build --workspace --release --locked
```

Routing benchmarks pesados não entram na CI.

Testes pequenos de routing entram normalmente.

---

# 57. Critérios de aceitação

O Marco 003 estará concluído quando:

- [ ] `dirty` não fizer mais parte do checksum;
- [ ] storage interno deixar de ser API pública principal;
- [ ] snapshots possuírem metadata regional;
- [ ] existirem `TransportNodeId` e `TransportEdgeId`;
- [ ] IDs forem estáveis e não reutilizados;
- [ ] existir directed multigraph;
- [ ] múltiplas edges entre nodes forem possíveis;
- [ ] Road e Rail existirem como modos;
- [ ] nodes possuírem X/Y/Z inteiros;
- [ ] edges possuírem custo inteiro;
- [ ] rede for editável somente por Commands externos;
- [ ] commands dependentes puderem rodar no mesmo tick;
- [ ] rede possuir revision;
- [ ] alterações efetivas atualizarem revision;
- [ ] Dijkstra encontrar menor custo;
- [ ] tie-breaking for determinístico;
- [ ] edge disabled não for roteável;
- [ ] routing respeitar modo;
- [ ] RoutingWorkspace for reutilizável;
- [ ] múltiplos workspaces puderem consultar a mesma rede;
- [ ] snapshots opcionais da rede existirem;
- [ ] checksum v3 incluir estado autoritativo da rede;
- [ ] determinismo passar em 1/2/4/8 threads;
- [ ] benchmark serial de routing existir;
- [ ] benchmark paralelo de routing existir;
- [ ] resultados paralelos possuírem checksum idêntico;
- [ ] documentação estiver atualizada;
- [ ] CI continuar verde.

---

# 58. Processo de implementação

Executar aproximadamente nesta ordem:

1. revisar Marco 002;
2. corrigir checksum dirty;
3. restringir getters internos;
4. adicionar snapshot metadata;
5. criar IDs de transporte;
6. criar `TransportNetwork`;
7. implementar node storage;
8. implementar edge storage;
9. implementar adjacency;
10. implementar revision;
11. adicionar commands;
12. adicionar ID allocation;
13. testar edição da rede;
14. implementar Dijkstra;
15. implementar tie-breaking;
16. implementar RoutingWorkspace;
17. adicionar snapshots;
18. atualizar checksum v3;
19. criar testes de routing;
20. testar determinismo;
21. criar route benchmark;
22. executar benchmarks;
23. documentar resultados;
24. executar fmt/clippy/test/build;
25. corrigir todas as falhas;
26. entregar relatório final.

---

# 59. Formato da entrega

Apresentar:

## A. Resumo

O que foi implementado.

## B. Arquitetura da rede

Mostrar estrutura conceitual:

```text
Nodes
Edges
Adjacency
Revision
RoutingWorkspace
```

## C. API pública

Mostrar exemplos de:

```text
create nodes
create edges
find route
network snapshot
```

## D. Determinismo

Mostrar os testes executados.

## E. Routing benchmark

Tabela real contendo:

```text
threads
latency
routes/s
speedup
```

Não inventar resultados.

## F. Complexidade observada

Informar:

```text
nodes visited
edges relaxed
```

quando disponível.

## G. Limitações

Explicar claramente:

```text
no congestion
no lanes
no vehicle routing
no cache
no A*
```

## H. Riscos

Apontar estruturas que podem precisar ser revistas quando houver:

```text
100k+ route queries
network edits
traffic
hierarchical routing
```

## I. Próximo marco sugerido

Não implementar automaticamente.

Avaliar se a arquitetura está pronta para:

```text
MARCO 004
COMMUTING, EMPLOYMENT & TRIP DEMAND
```

ou se antes é necessário um marco intermediário para:

```text
VEHICLES FOLLOWING NETWORK ROUTES
```

---

# 60. Princípio final deste marco

O Marco 003 deve provar:

```text
              TRANSPORT NETWORK

Node ─────── Edge ─────── Node
 │                          │
 │                          │
 └──────── Routing ─────────┘

           deterministic
                +
            measurable
                +
             editable
```

Ainda não estamos tentando simular trânsito.

Estamos construindo a infraestrutura matemática que permitirá que, futuramente:

```text
casa
  ↓
ônibus
  ↓
estação
  ↓
trem
  ↓
fábrica
```

seja uma sequência real de decisões e rotas dentro de uma única região simulada.

O objetivo do Marco 003 é descobrir se conseguimos fazer isso de forma correta, determinística e rápida antes de adicionar centenas de milhares de agentes sobre a rede.