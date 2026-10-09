# Transport Network & Routing Foundation — Marco 003

## Modelo e armazenamento

A rede é um multigrafo direcionado lógico. Mão dupla exige duas arestas; `(from,to)`
não identifica uma aresta. Road e Rail coexistem, e cada consulta aceita um único
modo. Nós têm X/Y/Z inteiros: X/Y usam `POSITION_UNITS_PER_TILE` e Z é uma elevação
provisória independente. Coordenadas iguais não conectam nós automaticamente.
Não há geometria de estradas, vias físicas ou relação obrigatória entre custo e distância.

`TransportNetwork` é interno e possui vetores de registros de nós e arestas,
adjacência de saída por nó, contagem de arestas vivas incidentes e revisão global.
`TransportNodeId` e `TransportEdgeId` são tipos distintos e estáveis. A resolução
direta ID → slot é temporária e encapsulada; `to_raw()` é um token de identidade
para serialização, nunca um índice de armazenamento.

As reservas criam slots ainda não vivos. Criação marca o ID como utilizado;
remoção deixa tombstone e mantém os dados históricos. IDs removidos não podem ser
recriados. Reservas abandonadas nunca são reutilizadas. Uma criação inválida não
consome o estado de utilização do ID reservado, permitindo corrigir e reenviar.
Adjacência mantém IDs de arestas removidas, que são ignoradas nas consultas.
Essa escolha facilita edições determinísticas, mas poderá exigir compactação futura.

## Edição por comandos

`submit_create_transport_node` e `submit_create_transport_edge` unem reserva e
submissão, garantindo alocação na ordem das chamadas de submissão. São a opção
mais simples para consumidores. Se a submissão falhar após reservar, o ID permanece
reservado e não será reutilizado. Também é possível separar explicitamente as etapas:
`reserve_transport_node_id()` e `reserve_transport_edge_id()` emitem identidades
monotônicas separadas por tipo. O chamador serial reserva imediatamente antes de
submeter a criação correspondente; mesma ordem de chamadas gera mesmas identidades
e sequências. Não há produtores concorrentes. Reserva afeta checksum, mas não cria
topologia viva nem incrementa a revisão. Comandos são aplicados na ordem FIFO,
permitindo criar dois nós e sua conexão no mesmo tick.

Existem seis novos comandos: criar/remover nó, criar/remover aresta, habilitar
aresta e alterar custo. Setters externos diretos não existem. Criação valida reserva,
utilização do ID, posição, endpoints vivos, custo positivo e ausência de self-loop.
Self-loops são rejeitados neste marco. Modos inválidos são impossíveis pelo enum.
Desabilitar não remove a aresta: ela continua impedindo remoção dos endpoints.
Remover nó exige zero arestas vivas incidentes, inclusive entradas e saídas. Não há
cascata automática. Cada comando inválido retorna erro individual sem impedir os seguintes.

Reservas usam incremento verificado; overflow retorna `TransportIdOverflow` sem
mudar o estado. Proteções temporárias: 1.000.000 reservas de nós, 4.000.000 de arestas
e coordenadas em −2^50..=2^50 em cada eixo. Esses valores protegem recursos e não
representam limites de design do jogo. Nós podem estar fora do mapa físico.
Reservas e crescimento de adjacência usam alocação falível; falhas não criam meia aresta.

## Revisão e snapshots

Uma alteração efetiva de nó, aresta, custo ou habilitação incrementa a revisão global.
No-ops e erros não incrementam. A revisão satura em `u64::MAX`; após saturação, uma
comparação por revisão não distingue novas edições. Solicitar snapshot sem revisão
conhecida força uma cópia. Epochs não foram introduzidos.

A revisão global é um baseline conservador, não um compromisso permanente:
qualquer edição pode invalidar uma rota futura. Revisões de setor, componente ou
aresta dependerão de necessidade e medições.

`SnapshotRequest::transport` é opcional. O pedido escolhe nós e/ou arestas.
Revisão conhecida igual retorna somente revisão e `unchanged=true`, com vetores
vazios; revisão diferente retorna os dados vivos selecionados. Tombstones não são
enviados. Dados são owned e não mudam com ticks posteriores. Consumidores mantêm
suas próprias revisões e cobertura: só informar revisão conhecida se já possuem
o subconjunto solicitado. Um snapshot de rede vazia alterada usa `unchanged=false`.

## Dijkstra e determinismo

`Simulation::find_route(request, &mut workspace)` lê a rede imutavelmente.
Origem e destino devem estar vivos. Origem igual a destino retorna custo zero e
nenhuma aresta; destino inalcançável retorna `RouteError::NoRoute`. Rotas contêm
revisão, origem, destino, modo, custo total e IDs de arestas na ordem do percurso.

Custos de arestas são `u64` positivos. Distâncias e custo total usam `u128`, permitindo
somar inclusive várias arestas de custo `u64::MAX` sem overflow. Uma rota mínima
com custos positivos é simples, e o limite de nós mantém a soma muito abaixo de
`u128::MAX`. Não há heurística geométrica, A* ou cache persistente.

A heap mínima desempata por `(custo, NodeId)`. Ao encontrar a mesma distância para
um destino, o predecessor é a aresta de menor `EdgeId`. Não se promete a sequência
lexicograficamente mínima de todos os IDs da rota. Custos positivos garantem que
os predecessores são examinados antes de finalizar o destino. A regra funciona
independentemente da ordem de adjacência e do número de threads.

`RoutingWorkspace` contém distâncias, predecessores, marcadores de geração,
marcadores de nós finalizados e heap. Buffers crescem quando necessário e são
reutilizados; marcadores evitam limpar arrays do tamanho da rede em toda consulta.
Overflow da geração limpa os marcadores e reinicia em 1. A rota retornada possui
seu próprio vetor de arestas; esse vetor ainda aloca por resultado.

Não existe mutex nem workspace interno na rede. N workspaces podem consultar a
mesma simulação em paralelo. Workspace, resultados de routing e diagnósticos não
fazem parte do estado autoritativo ou checksum. Diagnósticos da última consulta
contam nós finalizados, arestas elegíveis examinadas (vivas, habilitadas e no modo)
e inserções na heap, inclusive candidatas posteriormente descartadas.

A implementação usa Dijkstra com entradas duplicadas e descarte de entradas antigas:
tempo limite `O(V + E log E)` para rede sem histórico removido; a varredura também
paga pelo histórico de tombstones na adjacência. Scratch usa `O(slots de nós + heap)`
por workspace; queries paralelas multiplicam esse consumo. Não foi introduzido CSR.

## Exemplo da API pública

```no_run
use federation_sim::*;
# fn main() -> Result<(), Box<dyn std::error::Error>> {
let mut sim = Simulation::new(SimulationConfig::default())?;
let a = sim.reserve_transport_node_id()?;
sim.submit_command(SimulationCommand::CreateTransportNode {
    node_id: a, position: TransportPosition { x: 0, y: 0, z: 0 },
})?;
let b = sim.reserve_transport_node_id()?;
sim.submit_command(SimulationCommand::CreateTransportNode {
    node_id: b, position: TransportPosition { x: 1024, y: 0, z: 0 },
})?;
let edge = sim.reserve_transport_edge_id()?;
sim.submit_command(SimulationCommand::CreateTransportEdge {
    edge_id: edge, from: a, to: b, mode: TransportMode::Road,
    base_cost: 5, enabled: true,
})?;
sim.tick()?;
for result in sim.command_results() { result.result?; }

let mut workspace = RoutingWorkspace::new();
let route = sim.find_route(RouteRequest {
    origin: a, destination: b, mode: TransportMode::Road,
}, &mut workspace)?;
assert_eq!(route.total_cost, 5);
assert_eq!(route.edges, [edge]);

let snapshot = sim.snapshot(SnapshotRequest {
    transport: Some(TransportSnapshotRequest {
        include_nodes: true, include_edges: true, known_revision: None,
    }),
    ..Default::default()
})?;
assert_eq!(snapshot.transport.unwrap().edges.len(), 1);
# Ok(())
# }
```

## Limitações e próximo marco

O benchmark de grid não representa uma cidade real. Não há congestionamento, lanes,
semáforos, cache, A* ou veículos seguindo rotas. O movimento sintético permanece
inalterado. Uma consulta representa um modo apenas; baldeações exigirão uma política
futura. Z identifica posição, mas não determina conectividade ou custo automaticamente.

Antes de commuting/employment/trip demand, recomenda-se um marco intermediário
de veículos seguindo rotas: progresso na aresta, unidades de velocidade/custo,
chegada, rotas invalidadas e política determinística de rerouting. O Marco 003 oferece
a fronteira necessária, mas não implementa esse ciclo. Em 100k+ consultas, memória
por workspace, exploração do Dijkstra, alocações do resultado, histórico de edições
e invalidação global poderão exigir novos benchmarks e estruturas especializadas.
