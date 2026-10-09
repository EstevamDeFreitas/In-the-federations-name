# Vehicle Route Execution — Marco 003.5

## Fronteira e ciclo de vida

Routing calcula uma `Route` sem alterar o mundo. `submit_store_route_plan(route)`
reserva um `RoutePlanId` e enfileira `StoreRoutePlan`; a persistência ocorre somente
em ApplyCommands. Um `AssignVehicleRoute` posterior pode consumir esse plano no
mesmo tick. Movement apenas lê os planos e a rede; nunca chama Dijkstra.

Planos são centrais, imutáveis e compartilhados por ID. Guardam origem, destino,
modo, revisão do routing original, custo lógico total e sequência de arestas.
A revisão é diagnóstico de idade, não validade absoluta. Persistência valida
endpoints vivos, continuidade, modo, habilitação e soma do custo. Se o custo lógico
mudar entre consulta e persistência, recalcular/reenviar explicitamente; uma edição
distante não impede persistir uma sequência ainda válida. Planos já persistidos
continuam válidos para movimento após alteração de custo, que não muda comprimento.

IDs são monotônicos, sem reutilização. Reservas abandonadas ficam representadas no
checksum; IDs já usados não aceitam Store novamente. Não há edição de planos,
deduplicação, cache, refcount ou GC. Planos e tombstones podem crescer indefinidamente
até a proteção temporária de reservas. Cleanup exigirá trabalho futuro.

RoutingWorkspace count ≈ routing worker count, nunca número de veículos/agentes.
Arrays de scratch crescem com a rede. Decision frequency != movement frequency:
pathfinding ocorre por demanda, novo destino, invalidação ou replanning explícito.
Nenhum workspace é armazenado em veículos ou planos.

## Comprimento e geometria inteira

Cada aresta armazena `physical_length` calculado na criação. Endpoints não podem ser
movidos neste marco. A métrica escolhida é Manhattan 3D:
`max(1, |dx| + |dy| + |dz|)`, separada de `base_cost`. Nós distintos na mesma posição
recebem comprimento 1. A métrica é uma aproximação determinística, não distância
Euclidiana ou física final; diagonais consomem a soma dos deslocamentos por eixo.
Z participa da distância e interpolação, sem impor conectividade automática.

Diferenças usam `i128`, módulos e soma `u128`, conversão verificada para `u64`.
Com coordenadas em −2^50..=2^50, o maior comprimento é `3 × 2^51`.
Valores fora da largura aceita produzem erro, sem aritmética `dx*dx` em `i64`.
Snapshots de arestas incluem comprimento físico para diagnóstico.

Posição por eixo: `from + (to-from) * progress / length`, usando `i128` e divisão
inteira com truncamento em direção a zero. O erro de interpolação por eixo é menor
que uma unidade. No fim da aresta a posição recebe exatamente o endpoint, sem
acumular erro. Não há floats no estado ou interpolação gráfica frame-a-frame.

## Layout data-oriented e estados

Os arrays originais ID/X/Y/VX/VY continuam contíguos. Acrescentam-se arrays Z,
RoutePlanId opcional, índice da aresta no plano, progresso, estado, CurrentEdgeId
opcional, CurrentNodeId opcional e velocidade de rede. Nenhum veículo possui Vec de
arestas ou RoutingWorkspace. Cópias locais da execução são registros de stack.

Estados: Idle, FollowingRoute, Arrived, NeedsRoute. Veículos iniciais sem plano usam
temporariamente movimento sintético VX/VY com wrap, preservando a carga histórica.
Após atribuir plano, movimento usa velocidade escalar independente. Arrived e
NeedsRoute permanecem estacionários; não voltam ao comportamento sintético.
`SetVehicleVelocity` não altera velocidade de rede. `SetVehicleNetworkSpeed` aceita
0..=1.000.000 unidades/ms como proteção temporária; default 8. Zero permite parar.

Primeira atribuição a um veículo fora da rede estabelece o nó lógico de origem e
posiciona-o ali explicitamente. Uma atribuição posterior exige origem igual ao nó
atual e ausência de aresta já iniciada. Não se troca plano no meio de uma aresta.
O plano vazio origem=destino produz Arrived durante ApplyCommands, sem Movement.

Enquanto em uma aresta, CurrentNode é o último nó concluído, não um ponto interpolado.
Ao terminá-la ele muda para `to`. CurrentEdge é None quando estacionário; progresso
zera ao terminar uma aresta e nunca excede seu comprimento.

## Travel budget e paralelismo

Budget = velocidade × tick_ms, calculado em `u128`. Consome o restante da aresta,
avança para o nó e usa sobra na próxima aresta, incluindo várias transições por tick.
Com budget zero o veículo pode entrar logicamente na próxima aresta válida com
progresso zero. Essa entrada já aplica a política de conclusão de aresta iniciada.

Safety guard: 10.000 arestas concluídas por veículo/tick. Se ainda houver uma
sequência a consumir, o veículo para no nó atual com NeedsRoute e motivo
TransitionSafetyGuard. Chegar ao destino exatamente no limite produz Arrived.
É proteção defensiva, não regra de gameplay. Dijkstra não gera ciclos positivos,
mas a validação estrutural de um plano externo pode aceitar uma sequência com ciclos.

Rayon atualiza slices indexados disjuntos de veículos. Rede e planos são somente
leitura; não há locks por entidade. Contadores de atualizações e transições usam
redução inteira. Um slot de evento por veículo é compactado serialmente em ordem
estável após a barreira. Buffers são reservados na inicialização e reutilizados:
Movement não aloca planos, arrays ou vetores por veículo.

## Lazy invalidation

Antes de entrar em uma aresta, validar existência, alive, enabled, modo e
`from == current_node`. Falha produz NeedsRoute e mantém o veículo no nó atual.
Não se usa divergência de revisão global como motivo de bloqueio. Alterar uma
aresta distante ou apenas o custo não interrompe o percurso.

Uma aresta já iniciada pode ser concluída após remoção ou desabilitação. Seus dados,
comprimento e posições dos endpoints permanecem nos tombstones. A regra evita
teleport e funciona mesmo se os endpoints forem removidos após retirar suas arestas
vivas. Na chegada o ID lógico pode então referir-se a um nó removido; futura política
de edição/replanning deve resolver esse caso. Não há rerouting automático.

Exemplo: veículo em A→B; remover B→C não interfere até chegar a B. Ele conclui A→B,
valida B→C, produz NeedsRoute e fica em B. Para continuar, outro sistema consulta
uma nova Route de B e atribui outro plano por comandos.

## Eventos, snapshots e memória

`movement_events()` retorna eventos do último tick bem-sucedido: VehicleArrived ou
VehicleNeedsRoute(reason), com identidade do veículo e nó lógico. No máximo um
evento terminal por veículo/tick. Atribuições sucessivas no mesmo tick refletem
a última execução efetiva; o histórico de comandos permanece nos CommandResults.
No tick seguinte o buffer é substituído, sem repetir eventos de veículos parados.

`movement_metrics()` informa veículos FollowingRoute processados, transições,
chegadas, NeedsRoute e acionamentos do guard. Métricas incluem chegada por atribuição
de plano vazio. Veículos sintéticos ou já parados não contam como routed updates.

VehicleSnapshot inclui X/Y/Z, estado, plano, índice no plano, aresta atual, progresso,
nó e velocidade escalar. `RoutePlanSelection::Ids` copia apenas os planos pedidos;
default None. Snapshots antigos e cópias modificadas pelo consumidor não mudam a
simulação. Nenhum slot interno é exposto.

`route_execution_memory_estimate()` mede capacidades alocadas × `size_of` dos arrays,
buffers de eventos, registros opcionais de planos e sequências. São estimativas
de payload no alvo compilado, não RSS: excluem allocator overhead, headers dos arrays principais,
stack, terreno, rede, fila de comandos, scratch de routing e buffers do benchmark.
O tamanho dos registros de planos inclui o header de seu Vec de edges embutido.
O benchmark compara planos compartilhados com `--unique-plan-per-vehicle`; a opção
clona as mesmas rotas pré-calculadas por veículo e não executa Dijkstra por agente.

## Checksum v4

Marcador `federation-state-v4`. Ao estado v3 acrescentam-se comprimento físico das
arestas, próximo ID de plano, slots reservados/presença, metadados e sequência de
cada plano. Comandos pendentes Store/Assign/Speed usam tags 8/9/10 e payload explícito,
incluindo a saída Route completa em Store. O enum de comandos passa a ser Clone,
sem Copy, pois Store possui um único Vec de resultado (não um Vec por veículo).

Por veículo, acrescentam-se Z, tag do estado, flags/IDs opcionais de plano, índice,
progresso, velocidade, aresta atual e nó lógico, em ordem estável e little-endian.
Dirty, workspaces, métricas, eventos temporários e snapshots são excluídos.
Fingerprints históricos v3 e v4 não são diretamente comparáveis.

## Exemplo da API

```no_run
use federation_sim::*;
# fn main() -> Result<(), Box<dyn std::error::Error>> {
let mut sim = Simulation::new(SimulationConfig {
    vehicles: 1, threads: 1, ..Default::default()
})?;
let (a, _) = sim.submit_create_transport_node(TransportPosition::default())?;
let (b, _) = sim.submit_create_transport_node(TransportPosition {
    x: 1024, y: 0, z: 0,
})?;
sim.submit_create_transport_edge(a, b, TransportMode::Road, 3, true)?;
sim.tick()?;

let mut workspace = RoutingWorkspace::new(); // Reuse per routing worker.
let route = sim.find_route(RouteRequest {
    origin: a, destination: b, mode: TransportMode::Road,
}, &mut workspace)?;
let (plan, _) = sim.submit_store_route_plan(route)?;
let id = sim.snapshot(SnapshotRequest {
    vehicles: VehicleSelection::All, ..Default::default()
})?.vehicles[0].id;
sim.submit_command(SimulationCommand::SetVehicleNetworkSpeed {
    vehicle_id: id, speed_units_per_ms: 5,
})?;
sim.submit_command(SimulationCommand::AssignVehicleRoute {
    vehicle_id: id, route_plan_id: plan,
})?;
sim.tick()?;
let ids = [plan];
let snapshot = sim.snapshot(SnapshotRequest {
    vehicles: VehicleSelection::All,
    route_plans: RoutePlanSelection::Ids(&ids),
    ..Default::default()
})?;
assert_eq!(snapshot.vehicles[0].edge_progress, 100);
assert_eq!(snapshot.route_plans[0].edges.len(), 1);
# Ok(())
# }
```

## Limitações e Marco 004

Sem lanes, congestionamento, aceleração, colisões, rerouting automático, transporte
público, citizens, households, empregos, carga, combustível ou renderer. Road/Rail
ainda não exigem classes de veículos. Rotas são conexões retas com métrica Manhattan.

O ciclo rede → routing → plano compartilhado → movimento → chegada/NeedsRoute está
pronto para servir como base de Employment, Households & Commute Demand. O Marco 004
deverá definir demanda, frequência de decisões e política de tratamento de NeedsRoute,
além de retenção/cleanup de planos. Não foi iniciado. Continuidade de geometria de
tombstones, memória de planos, revisão global e custo das consultas em volume real
precisam de decisões específicas antes de otimizações ou sistemas de trânsito.
