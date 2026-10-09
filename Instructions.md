# CODEX TASK 003.5 — Vehicle Route Execution & Network Movement

## 1. Contexto

O projeto **In The Federation's Name** possui os Marcos 001, 002 e 003 concluídos.

A arquitetura atual inclui:

- núcleo Rust headless;
- simulação determinística;
- scheduler multithread baseado em Rayon;
- timestep fixo;
- comandos determinísticos;
- snapshots owned;
- terreno em chunks;
- veículos em SoA;
- IDs estáveis;
- multigrafo direcionado de transporte;
- modos `Road` e `Rail`;
- edição de rede por comandos;
- revisão global da rede;
- Dijkstra determinístico;
- `RoutingWorkspace` reutilizável;
- benchmark serial e paralelo de routing;
- checksum v3;
- CI e documentação técnica.

O Marco 003 provou que a rede lógica e o pathfinding funcionam.

O Marco 003.5 deve provar a próxima cadeia:

```text
TransportNetwork
       ↓
     Route
       ↓
RoutePlan
       ↓
    Vehicle
       ↓
edge progress
       ↓
next edge
       ↓
 destination
```

Ainda não implementar:

- congestionamento;
- lanes;
- cruzamentos físicos;
- semáforos;
- transporte público;
- cidadãos;
- empregos;
- logística econômica;
- renderer.

---

# 2. Objetivo principal

Fazer veículos existentes seguirem rotas reais sobre a rede de transporte.

Ao final deste marco, um veículo deverá ser capaz de:

1. receber uma rota válida;
2. entrar na primeira edge;
3. percorrer sua geometria;
4. avançar por múltiplas edges;
5. chegar ao destino;
6. reagir deterministicamente a alterações da rede;
7. expor seu estado por snapshots;
8. executar em larga escala sem pathfinding por veículo a cada tick.

---

# 3. Regra arquitetural fundamental

Routing e movement são sistemas diferentes.

```text
Routing
    ↓
produces route

Movement
    ↓
consumes route
```

Não executar Dijkstra durante o hot path normal de movimento.

O `Movement` deve apenas consumir planos já calculados.

---

# 4. Registrar decisões arquiteturais obrigatórias

Antes ou durante a implementação, atualizar documentação para registrar formalmente estas decisões.

## 4.1 RoutingWorkspace é por worker, nunca por agente

O benchmark do Marco 003 mostrou que `RoutingWorkspace` mantém arrays proporcionais ao tamanho da rede.

Portanto:

> Nunca criar um `RoutingWorkspace` por veículo, cidadão, viagem ou entidade simulada.

Workspaces devem ser reutilizados por workers/sistemas de routing.

Registrar isso em:

```text
AGENTS.md
docs/architecture.md
docs/transport-network.md
```

---

## 4.2 Pathfinding não roda continuamente

O benchmark mostrou que Dijkstra pode visitar dezenas de milhares de nodes por consulta em redes sintéticas maiores.

Registrar a regra:

> Agentes não recalculam caminhos a cada tick.

Routing deve ocorrer apenas quando necessário, por exemplo:

```text
new destination
route invalidated
trip created
network event affecting route
explicit replanning
```

---

## 4.3 `base_cost` não representa distância física

`TransportEdge.base_cost` representa custo lógico de routing.

Não utilizar:

```text
base_cost
```

para determinar deslocamento físico.

Uma edge deve possuir separadamente:

```text
routing cost
physical length
```

---

## 4.4 Network revision global é provisória

A revisão global existente é adequada como baseline.

Porém deve ser documentado:

> Qualquer edição altera a revisão global, portanto ela não deve ser usada para obrigar todos os agentes a recalcular rotas.

Futuramente poderão existir revisões por:

```text
sector
component
edge
region
```

caso benchmarks justifiquem.

---

## 4.5 Invalidação de rota deve ser preguiçosa

Uma alteração qualquer na rede não deve invalidar automaticamente todas as rotas.

Regra:

> Um veículo valida as edges que realmente pretende consumir.

A mudança de uma edge distante não deve afetar um veículo cuja rota continua válida.

---

## 4.6 Rotas devem ser compartilháveis no futuro

Não criar uma arquitetura onde:

```text
Vehicle
owns Vec<TransportEdgeId>
```

seja um contrato obrigatório permanente.

Neste marco a implementação pode ser simples, mas deve existir uma abstração:

```text
RoutePlan
```

que permita futuramente compartilhamento ou deduplicação.

---

# 5. Physical edge length

Adicionar comprimento físico às edges.

Conceitualmente:

```rust
physical_length: u64
```

ou um tipo novo.

Não reutilizar `base_cost`.

---

# 6. Como calcular comprimento inicialmente

Neste marco utilizar a posição dos nodes.

Dados:

```rust
TransportPosition {
    x,
    y,
    z,
}
```

Calcular comprimento determinístico usando inteiros.

Evitar float no estado autoritativo.

É aceitável utilizar uma distância inteira aproximada baseada em:

```text
dx
dy
dz
```

desde que:

- seja determinística;
- nunca produza zero para nodes distintos;
- seja documentada;
- suporte verticalidade futura.

Pode ser usada raiz inteira de:

```text
dx² + dy² + dz²
```

se implementada de forma segura.

Também é aceitável começar com outra métrica inteira justificada.

Não introduzir floating point autoritativo.

---

# 7. Overflow

O cálculo deve proteger contra overflow.

Como posições podem ser grandes, não fazer diretamente:

```rust
dx * dx
```

em `i64`.

Utilizar largura adequada como:

```text
i128 / u128
```

ou algoritmo equivalente.

---

# 8. Edge physical length é derivado ou armazenado?

Decidir entre:

### Opção A

Calcular na criação e armazenar.

### Opção B

Calcular quando necessário.

Preferência atual:

> calcular na criação da edge e armazenar.

Motivo:

- endpoints não podem ser movidos atualmente;
- movement consultará comprimento repetidamente;
- evita recálculo no hot path.

Documentar a escolha.

---

# 9. Edge snapshot

Adicionar:

```text
physical_length
```

a:

```text
TransportEdgeSnapshot
```

para debugging e futuros renderers.

---

# 10. RoutePlan

Criar uma abstração equivalente a:

```rust
RoutePlanId
RoutePlan
```

`RoutePlanId` deve ser identidade estável e opaca.

Uma rota armazenada deve incluir no mínimo:

```text
origin
destination
mode
network_revision_when_created
edge sequence
```

---

# 11. Não usar network revision como validade absoluta

Embora um `RoutePlan` grave:

```text
network_revision_when_created
```

a mudança desse valor não torna a rota automaticamente inválida.

Ele serve apenas para:

- diagnóstico;
- cache futuro;
- debugging;
- análise de idade da rota.

---

# 12. RoutePlan ownership

Neste marco pode existir um armazenamento central:

```text
RoutePlans
```

dentro da Simulation.

Algo equivalente:

```text
Vec<RoutePlanRecord>
```

é aceitável.

Não implementar ainda:

- deduplicação;
- reference counting sofisticado;
- garbage collector de rotas;
- cache OD global.

---

# 13. IDs dos RoutePlans

Adicionar:

```rust
RoutePlanId
```

Requisitos:

- opaco;
- monotônico;
- não reutilizado;
- checksum determinístico;
- não assumir índice como contrato externo.

---

# 14. Criação de RoutePlan

Adicionar API controlada equivalente a:

```rust
Simulation::create_route_plan(...)
```

ou command equivalente.

Preferência:

Routing é uma consulta determinística e não altera mundo.

Criar um `RoutePlan` persistente, porém, altera estado autoritativo.

Portanto a persistência da rota deve ocorrer por command ou outra fronteira determinística claramente documentada.

Uma possibilidade:

```text
find_route()
    ↓
Route

submit StoreRoutePlan(Route)
    ↓
ApplyCommands
```

Outra solução equivalente é aceitável.

---

# 15. Não guardar RoutingWorkspace

`RoutePlan` guarda apenas saída útil.

Nunca armazenar:

```text
distance arrays
heap
seen
settled
RoutingWorkspace
```

---

# 16. Estado de movement dos veículos

Expandir armazenamento dos veículos.

Adicionar campos conceituais equivalentes a:

```text
route_plan_id
route_edge_index
edge_progress
movement_state
```

Manter layout data-oriented.

Não transformar veículo em struct pesada com múltiplos `Option<Vec<...>>`.

---

# 17. VehicleMovementState

Criar algo equivalente a:

```rust
enum VehicleMovementState {
    Idle,
    FollowingRoute,
    Arrived,
    NeedsRoute,
}
```

Podem existir nomes diferentes.

Sem estados excessivos nesta etapa.

---

# 18. Route assignment

Adicionar comando:

```text
AssignVehicleRoute
```

contendo:

```text
vehicle_id
route_plan_id
```

Aplicação deve ocorrer em `ApplyCommands`.

---

# 19. Validação da atribuição

Ao atribuir uma rota:

- veículo deve existir;
- route plan deve existir;
- modo deve ser compatível futuramente, mas veículos ainda não possuem tipo de transporte obrigatório;
- rota deve possuir origem coerente com a posição lógica esperada, quando aplicável.

Não inventar ainda sistemas complexos de classe de veículo.

---

# 20. Posição lógica de rede

Introduzir para veículos em rota:

```text
current_transport_node
```

ou estado equivalente.

É necessário saber em qual node o veículo se encontra quando está parado ou recalculando.

---

# 21. Movimento ao longo de edge

Um veículo seguindo rota terá:

```text
current edge
edge progress
```

onde:

```text
0 <= progress <= physical_length
```

---

# 22. Speed

Separar velocidade física das antigas velocidades sintéticas X/Y.

Os campos:

```text
vx
vy
```

do benchmark original não devem ser usados como modelo final do movimento em rede.

Adicionar um conceito:

```text
network_speed
```

ou:

```text
speed_units_per_ms
```

inteiro.

---

# 23. Migração do movimento sintético

Não remover necessariamente o sistema sintético imediatamente se testes históricos dependem dele.

Mas veículos com:

```text
FollowingRoute
```

devem usar movement sobre a rede.

Veículos fora da rede podem continuar utilizando comportamento anterior apenas se isso simplificar compatibilidade.

Documentar claramente os dois modos temporários.

Se for simples remover a movimentação sintética de veículos reais sem quebrar benchmarks históricos, isso também é aceitável.

---

# 24. Travel budget

Movement usa:

```text
travel_budget = speed × tick_ms
```

com aritmética inteira.

Exemplo:

```text
speed = 30 units/ms
tick = 20 ms

travel_budget = 600 units
```

---

# 25. Consumindo edges

Se:

```text
remaining edge length > travel_budget
```

então:

```text
edge_progress += travel_budget
```

Se:

```text
travel_budget >= remaining edge length
```

então:

```text
finish edge
move to next node
subtract consumed distance
continue
```

---

# 26. Múltiplas edges no mesmo tick

Um veículo rápido deve poder atravessar:

```text
edge A
edge B
edge C
```

no mesmo tick.

Não limitar artificialmente a:

```text
1 edge por tick
```

Isso tornaria simulação dependente do tamanho das edges.

---

# 27. Proteção contra loops

Embora rotas produzidas pelo Dijkstra não devam conter ciclos positivos, proteger o movement contra dados corrompidos.

Adicionar limite defensivo de edges consumidas por veículo em um único tick.

Esse limite deve ser um **safety guard**, não regra de gameplay.

Se acionado:

- retornar/registrar erro determinístico;
- não ficar em loop infinito.

---

# 28. Interpolação da posição

Ao percorrer uma edge, atualizar posição X/Y/Z do veículo usando interpolação inteira entre:

```text
from position
to position
```

baseada em:

```text
edge_progress / physical_length
```

Evitar floating point autoritativo.

Usar aritmética inteira segura.

---

# 29. Renderer futuro

A posição calculada deve ser suficientemente precisa para um renderer futuro.

Não implementar interpolação gráfica frame-a-frame.

O renderer poderá futuramente interpolar entre snapshots.

---

# 30. Arrival

Ao consumir a última edge:

```text
movement_state = Arrived
current_node = destination
```

O veículo deve parar exatamente na posição do node destino.

---

# 31. Empty route

Para:

```text
origin == destination
```

um RoutePlan pode possuir:

```text
edges = []
```

Atribuí-lo deve resultar imediatamente em:

```text
Arrived
```

ou comportamento equivalente claramente documentado.

---

# 32. Validação lazy de edges

Antes de entrar na próxima edge:

validar:

```text
edge alive
edge enabled
mode compatible
edge.from == current node
```

Se qualquer condição falhar:

```text
movement_state = NeedsRoute
```

O veículo permanece no node atual.

---

# 33. Edge removida enquanto veículo está nela

Regra inicial:

> Veículo que já entrou numa edge pode concluir essa edge mesmo que ela seja removida/desabilitada durante o percurso.

Motivos:

- evita teleport;
- evita estado espacial impossível;
- simplifica consistência.

A validação ocorre **antes de entrar na próxima edge**.

Registrar isso explicitamente em documentação.

---

# 34. Rota alterada distante

Se uma edge futura ainda não alcançada continua válida, nenhuma ação acontece.

Não comparar simplesmente:

```text
route.network_revision != network.revision
```

para invalidar a rota.

---

# 35. Re-routing

Não executar Dijkstra automaticamente dentro do Movement.

Quando route fica inválida:

```text
NeedsRoute
```

é produzido.

Outro sistema futuro será responsável por criar nova rota.

---

# 36. Eventos de movement

Criar estrutura leve de eventos ou resultados para casos importantes.

Exemplos:

```text
VehicleArrived
VehicleNeedsRoute
```

Não criar event bus genérico complexo.

Pode ser buffer por tick.

---

# 37. Determinismo

Movement deve produzir mesmo resultado com:

```text
1
2
4
8 threads
```

para o mesmo:

```text
world
commands
routes
ticks
```

---

# 38. Paralelismo

Veículos seguindo rotas devem continuar sendo atualizados em paralelo.

A rede é leitura compartilhada.

RoutePlans também devem ser somente leitura durante Movement.

Cada worker escreve apenas nos slots dos veículos que possui.

---

# 39. Sem locks por veículo

Proibido:

```text
Mutex<Vehicle>
Arc<Mutex<RoutePlan>>
```

Não introduzir locks por entidade.

---

# 40. RoutePlan imutável durante uso

Depois de criado:

```text
RoutePlan
```

deve ser imutável.

Para mudar rota:

```text
assign another RoutePlanId
```

Não editar sequência de edges em um RoutePlan já existente.

---

# 41. RoutePlans compartilháveis

Projetar para permitir:

```text
Vehicle A ─┐
Vehicle B ─┼── RoutePlan #32
Vehicle C ─┘
```

Mesmo se benchmarks deste marco criarem uma rota por veículo.

---

# 42. Reference count

Não implementar reference count sofisticado.

RoutePlans podem permanecer armazenados indefinidamente neste marco.

Documentar potencial crescimento de memória.

Cleanup será tratado futuramente.

---

# 43. Checksum v4

Atualizar marcador:

```text
federation-state-v4
```

Adicionar estado autoritativo novo:

```text
RoutePlans
next RoutePlanId
vehicle movement state
route_plan assignment
edge index
edge progress
network speed
current logical node
```

Não incluir:

```text
routing workspaces
movement diagnostics
temporary event buffers
snapshots
```

---

# 44. Commands pendentes

Novos comandos também devem entrar deterministicamente no checksum enquanto pendentes.

---

# 45. Snapshot de veículos

Expandir `VehicleSnapshot`.

Adicionar o necessário para debugging/render futuro:

```text
movement_state
route_plan_id
current_edge
edge_progress
current_node
x
y
z
speed
```

Evitar expor detalhes internos como array slots.

---

# 46. Snapshot de RoutePlans

Adicionar seleção opcional.

Não enviar todos automaticamente.

Criar request semelhante a:

```text
RoutePlanSelection::None
RoutePlanSelection::Ids(...)
```

É suficiente.

Não implementar sincronização incremental sofisticada de route plans.

RoutePlans são imutáveis.

---

# 47. Benchmark novo — routed movement

Criar benchmark separado.

Não misturar com:

```text
synthetic movement benchmark
routing benchmark
```

Nome sugerido:

```text
federation-routed-movement-bench
```

---

# 48. Cenário de benchmark

Criar uma rede determinística como grid.

Exemplo inicial:

```text
128 × 128 nodes
```

Criar:

```text
100k vehicles
```

ou quantidade configurável.

Não executar Dijkstra durante a medição.

As rotas devem ser geradas/preparadas antes.

---

# 49. Rotas de benchmark

Utilizar conjunto reutilizável de RoutePlans.

Exemplo:

```text
1.000 route plans
100.000 vehicles
```

com veículos distribuídos entre esses planos.

Isso mede justamente nossa capacidade futura de compartilhar rotas.

Também permitir configuração:

```text
unique route per vehicle
```

para comparação de memória/custo, se simples.

---

# 50. O que medir

Medir:

```text
tick mean
p50
p95
p99
vehicles updated/s
edge transitions/s
arrivals/s
```

Também registrar:

```text
vehicles
route plans
average edges per route
memory estimate
```

---

# 51. Benchmark por threads

Testar:

```text
1
2
4
8
12
16
```

quando configurado.

Comparar checksum final.

---

# 52. Benchmark sem routing

O benchmark routed movement deve medir:

```text
consumo de rotas
```

não pathfinding.

Routing já possui benchmark separado.

---

# 53. Benchmark de invalidation

Adicionar cenário secundário pequeno.

Durante simulação:

```text
disable/remove selected edges
```

e medir:

```text
vehicles entering NeedsRoute
```

Não recalcular rotas.

Objetivo:

provar lazy invalidation.

---

# 54. Testes obrigatórios — length

Cobrir:

1. comprimento horizontal;
2. vertical;
3. diagonal;
4. diferença de Z;
5. nodes distintos nunca gerando comprimento zero;
6. valores grandes sem overflow;
7. resultado determinístico.

---

# 55. Testes obrigatórios — RoutePlan

Cobrir:

1. criação;
2. ID estável;
3. edge sequence preservada;
4. múltiplos veículos usando mesmo plan;
5. origin = destination;
6. plan imutável.

---

# 56. Testes obrigatórios — Vehicle movement

Cobrir:

1. entrar em uma edge;
2. movimento parcial;
3. terminar edge;
4. mudar para edge seguinte;
5. atravessar múltiplas edges no mesmo tick;
6. chegada;
7. posição final exatamente no destino;
8. progress nunca exceder length.

---

# 57. Testes obrigatórios — lazy invalidation

Cobrir:

### Caso A

Rota:

```text
A → B → C
```

Veículo está em:

```text
A → B
```

Remover:

```text
B → C
```

Comportamento:

```text
conclui A → B
chega B
NeedsRoute
```

### Caso B

Modificar edge em outra parte da rede.

Veículo continua normalmente.

### Caso C

Desabilitar edge atual.

Veículo termina edge atual.

---

# 58. Testes obrigatórios — edge consistency

Ao tentar entrar em edge cujo:

```text
from != current node
```

marcar:

```text
NeedsRoute
```

Não teleportar.

---

# 59. Testes obrigatórios — determinismo

Criar cenário com:

```text
network
route plans
1000+ vehicles
commands
edge invalidation
multiple ticks
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
vehicle snapshots
movement events
```

---

# 60. Tests com network revision

Criar alteração na rede que incremente revisão sem tocar rota do veículo.

Confirmar:

```text
vehicle continues
```

Isso deve ser teste obrigatório para impedir regressão futura.

---

# 61. RoutingWorkspace documentation

Adicionar ao README/docs explicitamente:

```text
RoutingWorkspace count
≈ routing worker count
```

e não:

```text
vehicle count
citizen count
```

Registrar a justificativa de memória.

---

# 62. Route recalculation documentation

Registrar:

```text
Decision frequency != movement frequency
```

Movimento pode executar frequentemente.

Routing é event-driven / periódico quando necessário.

Essa regra deve aparecer em:

```text
docs/architecture.md
docs/transport-network.md
AGENTS.md
```

---

# 63. Registro do benchmark do Marco 003

Não apagar resultados existentes.

Adicionar interpretação formal:

- Dijkstra é baseline correto;
- exploração cresce significativamente com rede;
- não usar pathfinding contínuo;
- throughput paralelo cresce enquanto latência individual também cresce;
- otimizações serão avaliadas depois.

---

# 64. A* e hierarchical routing continuam fora do escopo

Não implementar ainda:

```text
A*
CH
HPA*
sector routing
route cache
```

O Marco 003.5 mede movimento, não otimização de routing.

---

# 65. Não implementar citizens

Não criar:

```text
Citizen
Household
Employment
Commute
```

ainda.

Esse será o Marco 004.

---

# 66. Não implementar cargo

Não criar:

```text
Cargo
Inventory
TruckLoad
Factory
```

ainda.

---

# 67. Não implementar traffic interactions

Não implementar:

```text
vehicle collision
following distance
lane occupancy
intersection queues
traffic lights
priority
congestion
```

Vários veículos podem ocupar logicamente a mesma edge.

---

# 68. Não implementar lanes

Uma edge continua representando conexão lógica.

Lane model virá depois, se necessário.

---

# 69. Não implementar fuel

Sem consumo de combustível neste marco.

---

# 70. Não implementar acceleration

Velocidade pode permanecer constante.

Não adicionar:

```text
acceleration
braking
vehicle physics
```

---

# 71. Não implementar renderer

Nenhuma dependência:

```text
wgpu
winit
Godot
Bevy
```

entra no core.

---

# 72. Documentação obrigatória

Atualizar:

```text
README.md
AGENTS.md
docs/architecture.md
docs/transport-network.md
docs/benchmarks.md
```

Criar se útil:

```text
docs/vehicle-routing.md
```

---

# 73. README — registrar visão

Adicionar seção resumindo a arquitetura atual:

```text
Simulation
├── Terrain
├── Vehicles
├── Transport Network
├── Routing
└── Route Execution
```

Explicar que:

- routing calcula;
- RoutePlan armazena;
- movement executa;
- routing não ocorre a cada tick.

---

# 74. Architecture docs — regras permanentes

Registrar explicitamente:

> `base_cost` is routing cost, not physical length.

> Network revision changes do not automatically invalidate every route.

> Vehicles validate route edges lazily.

> Vehicles may finish an edge they already entered even if it is later disabled or removed.

> Routing workspaces belong to routing workers, never to agents.

> Route plans are immutable and designed to be shareable.

> Pathfinding and movement run at different frequencies.

---

# 75. AGENTS.md — regras para futuros agentes

Adicionar:

- Do not perform pathfinding inside vehicle movement.
- Do not allocate a `RoutingWorkspace` per agent.
- Do not use `base_cost` as physical distance.
- Do not invalidate every route when network revision changes.
- Validate future route edges lazily.
- A vehicle already on an edge may complete it.
- RoutePlans must remain immutable after creation.
- Multiple vehicles may reference one RoutePlan.
- Movement must remain integer/fixed-point authoritative state.
- Do not introduce lane/congestion systems without a dedicated milestone.
- Do not add routing optimizations without benchmarks.

---

# 76. CI

Preservar:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build --workspace --release --locked
```

Benchmarks pesados permanecem fora da CI.

---

# 77. Critérios de aceitação

O Marco 003.5 estará concluído quando:

- [ ] edge possuir comprimento físico separado de `base_cost`;
- [ ] comprimento for determinístico e integer-based;
- [ ] existir `RoutePlan`;
- [ ] existir `RoutePlanId`;
- [ ] RoutePlans forem imutáveis;
- [ ] RoutePlans puderem ser compartilhados por veículos;
- [ ] veículos tiverem estado explícito de network movement;
- [ ] veículos puderem receber RoutePlan via command;
- [ ] veículos avançarem parcialmente numa edge;
- [ ] veículos cruzarem múltiplas edges no mesmo tick;
- [ ] veículos chegarem exatamente ao node destino;
- [ ] edge futura inválida produzir `NeedsRoute`;
- [ ] alteração distante da rede não invalidar rota válida;
- [ ] edge atual removida/desabilitada puder ser concluída;
- [ ] routing não rodar dentro de Movement;
- [ ] movement continuar paralelo;
- [ ] não existir lock por veículo;
- [ ] checksum v4 incluir novo estado autoritativo;
- [ ] snapshots mostrarem estado de movimento;
- [ ] determinismo passar com 1/2/4/8 threads;
- [ ] benchmark routed movement existir;
- [ ] benchmark não incluir Dijkstra no hot path;
- [ ] lazy invalidation possuir benchmark/teste;
- [ ] regras arquiteturais forem registradas na documentação;
- [ ] CI permanecer verde.

---

# 78. Processo recomendado

Executar nesta ordem:

1. revisar Marco 003;
2. registrar regras arquiteturais na documentação;
3. adicionar physical edge length;
4. testar cálculo de comprimento;
5. criar `RoutePlanId`;
6. criar storage de RoutePlans;
7. definir persistência determinística de RoutePlan;
8. adicionar estado de movement aos veículos;
9. adicionar assignment por command;
10. implementar movement em uma edge;
11. implementar transição entre edges;
12. implementar múltiplas edges/tick;
13. implementar arrival;
14. implementar lazy validation;
15. implementar `NeedsRoute`;
16. adicionar movement events;
17. paralelizar movement preservando ownership;
18. atualizar checksum v4;
19. atualizar snapshots;
20. escrever testes;
21. criar routed movement benchmark;
22. criar invalidation scenario;
23. rodar benchmarks;
24. atualizar docs com resultados reais;
25. rodar fmt/clippy/test/build;
26. corrigir falhas;
27. entregar relatório final.

---

# 79. Entrega final

Responder com:

## A. Resumo

O que foi implementado.

## B. Movement architecture

Explicar:

```text
Route
↓
RoutePlan
↓
Vehicle assignment
↓
edge progress
↓
arrival / NeedsRoute
```

## C. Data layout

Mostrar novos arrays/campos dos veículos e storage dos RoutePlans.

## D. Physical length

Explicar cálculo e proteção contra overflow.

## E. Lazy invalidation

Mostrar comportamento real com edge removida/desabilitada.

## F. Determinismo

Listar testes e resultados entre contagens de threads.

## G. Performance

Apresentar benchmark routed movement:

```text
threads
vehicles
route plans
tick time
vehicles/s
edge transitions/s
```

## H. Memória

Apresentar estimativa ou medição aproximada para:

```text
vehicles
route plans
movement state
```

Não inventar valores.

## I. Limitações

Reforçar:

```text
no lanes
no congestion
no acceleration
no automatic rerouting
no public transit
no citizen simulation
```

## J. Próximo marco

Avaliar readiness para:

```text
MARCO 004 — Employment, Households & Commute Demand
```

Não iniciar automaticamente.

---

# 80. Princípio final

Depois deste marco queremos provar:

```text
ROUTING
   ↓
RoutePlan
   ↓
Vehicle
   ↓
Physical Network Movement
```

sem transformar:

```text
pathfinding
```

em:

```text
per-agent-per-tick work
```

A arquitetura deve permitir que futuramente:

```text
Citizen
   ↓
TripRequest
   ↓
RoutePlan
   ↓
Vehicle / Public Transport
```

seja adicionado sem reescrever a fundação de transporte.

O Marco 003.5 deve demonstrar que a rede não serve apenas para responder "qual é o caminho?", mas também consegue sustentar milhares ou centenas de milhares de entidades realmente consumindo esses caminhos de forma determinística e eficiente.