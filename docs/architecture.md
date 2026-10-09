# Arquitetura — Marco 003

## Ownership e fronteira pública

`Simulation` é a única proprietária do estado autoritativo mutável: configuração,
heightmap, veículos SoA, fila de comandos, rede de transporte, pool Rayon e ticks. O core
permanece headless e sem novas dependências. Getters de storage são `pub(crate)`; consumidores externos usam comandos, snapshots,
diagnostics e routing. Os módulos de terreno e veículos também são privados.
Não existe `Simulation::set_height`, setter público de velocidade ou acesso mutável
às estruturas internas. Setters de terreno são `pub(crate)`; limpar dirty existe
somente como ferramenta de testes internos nesta etapa.

Clap, Serde, medição de tempo e exportação JSON pertencem aos hosts. Nenhum sistema
de rendering, trânsito, replay, persistência ou networking foi antecipado. A rede
lógica de transporte foi implementada separadamente do movimento sintético.

## Identidade e armazenamento

`VehicleId` é um token opaco e tipado. `VehicleId::from_raw` permite reconstruir um
token; isso não garante existência. O consumidor obtém IDs e estado por snapshots,
usando `VehicleSelection::Ids` para consultas específicas.
Nunca deve interpretar um token como posição em um vetor. A associação direta
ID → slot permanece um detalhe interno temporário, encapsulado em `Vehicles::slot`.
Não há remoção ou reutilização. Sparse sets, gerações e compactação poderão mudar
essa associação sem alterar os comandos nem os snapshots.

O armazenamento continua com cinco vetores contíguos: IDs e X/Y/VX/VY. Snapshot
usa registros compactos por veículo; sua representação não define o armazenamento
interno. Não há heap allocation individual por veículo.

## Comandos: submissão, ordem e resultados

`SimulationCommand` contém terreno, velocidade e seis comandos de transporte,
descritos em [transport-network.md](transport-network.md).
`submit_command` emite `CommandSequence`, começando em 1. A simulação aceita somente
comandos sem sequência externa; não há API para inserir envelopes, duplicar ou
reordenar sequências. A ordem FIFO é a ordem monotônica das chamadas de submissão.
Não precisa de sort, hash map, mutex ou produtores concorrentes.

Submissão reserva espaço na fila e no buffer de resultados antes de aceitar o comando.
Falha de reserva ou de sequência retorna erro sem consumir a sequência nem inserir
um comando. `u64::MAX` é o estado de esgotamento da próxima sequência; a última
sequência emitida é `u64::MAX - 1`. Submissão não modifica terreno ou veículos.

Na fronteira do tick, ApplyCommands valida e aplica cada comando serialmente.
Vértices inválidos, IDs inexistentes e velocidades fora da proteção temporária
produzem `CommandError` individual e permitem continuar o restante da fila. Todos
os valores `i32` de altura são aceitos. Velocidades aceitas estão em
−1.000.000..=1.000.000 unidades/ms; esse intervalo mantém multiplicação pelo maior
timestep `u32` e soma à posição dentro de `i64`. Ambos os componentes são
validados antes de modificar qualquer velocidade.

`command_results()` expõe sequência e `Result` de cada comando do último tick
bem-sucedido, na ordem da aplicação. O tick seguinte substitui esse buffer, inclusive
quando a fila está vazia. Não existe histórico ilimitado. Buffers são reaproveitados;
ApplyCommands vazio não aloca e não ordena. Nenhuma Box ou trait object por comando.

## Scheduler e unidades

Ordem: ApplyCommands → Movement → Finalize. Diagnostics continua sob demanda.
Overflow do contador é verificado antes de qualquer fase, preservando a fila e o
estado em caso de erro. Movement usa o pool Rayon persistente com slices indexados,
escritas disjuntas e granularidade mínima de 1024 veículos; Finalize incrementa o
tick após a barreira. Mudanças de velocidade afetam Movement no mesmo tick.

Cada tile tem 1024 unidades inteiras de posição. VX/VY são unidades/ms lógico;
velocidades iniciais estão em −8..=8. O timestep padrão é 20 ms. Posições fazem
wrap toroidal com resto euclidiano. Alturas não possuem escala física definida.
Essas unidades são provisórias e não definem a escala final do jogo.

## Terreno, snapshots e revisões

Heightmap global row-major de `(width + 1) × (height + 1)` valores `i32`. Chunks
armazenam coordenadas, revisão e dirty interno; nascem com revisão zero. Edições
que realmente mudam a altura invalidam todos os chunks cujos tiles tocam o vértice,
inclusive até quatro chunks nos cruzamentos. Escrever a mesma altura é um no-op.

`Simulation::snapshot(SnapshotRequest)` produz `SimulationSnapshot` independente,
com tick, `RegionSnapshotMetadata { width, height, chunk_size }` e vetores próprios.
A metadata permite interpretar coordenadas e chunks sem acessar SimulationConfig. `VehicleSelection` permite None, All ou IDs explícitos;
o default não copia veículos nem terreno. O custo é proporcional aos veículos
selecionados e aos vértices dos chunks retornados; não há snapshot automático por tick.
O snapshot não contém referências ou buffers posteriormente sobrescritos. Seus dados
podem ser editados localmente sem afetar a simulação e continuam válidos após outros
ticks ou após destruir a simulação. Imutabilidade aqui significa uma fotografia
independente do estado autoritativo, sem atualização automática.

Cada `ChunkRevisionQuery` contém coordenadas e `known_revision`. None força a cópia
inicial; uma revisão igual omite o chunk, e qualquer revisão divergente solicita a
cópia. A comparação é por desigualdade, inclusive quando o consumidor informa uma
revisão superior. Erros de ID ou coordenada invalidam a solicitação com erro explícito;
nenhuma mudança ocorre na simulação. A ordem do resultado segue a solicitação;
seleções repetidas produzem cópias repetidas, sem deduplicação implícita.

Chunks incluem coordenadas, revisão, origem em vértices, dimensões e alturas
row-major. Bordas compartilhadas são incluídas, permitindo interpretação sem outro
chunk. Chunks parciais copiam somente sua extensão válida. Cada consumidor mantém
suas revisões independentemente; nenhum consumidor limpa dirty para sincronizar.
Dirty não faz parte do snapshot nem da comparação de revisões.

Revisões saturam em `u64::MAX`. Após saturar, novas mudanças não são distinguíveis
pela revisão; o consumidor precisaria solicitar uma cópia incondicional. Não há
epoch nesta etapa. Limpar dirty não altera revisão nem os resultados incrementais.

## Determinismo e checksum

Inicialização serial com SplitMix64 e movimento inteiro independente preservam
resultados entre 1, 2, 4 e 8 threads. Ordem dos comandos não depende do Rayon.

FNV-1a-64 v3 começa com `federation-state-v3`. Ordem dos campos:

1. Próxima sequência e comprimento da fila (`u64`); envelopes em FIFO com sequência,
   tag e payload. Tags 0/1 são terreno/velocidade; 2/3 criação de nó/aresta; 4/5
   remoção de nó/aresta; 6 habilitação; 7 custo. Identidades são `u64`, posições e
   velocidades `i64`, altura `i32`, coordenadas de terreno convertidas para `u64`,
   modo Road/Rail usa byte 0/1, custo é `u64` e enabled um byte 0/1.
2. Próximos IDs de nó/aresta e revisão global (`u64`), quantidade de slots de nós,
   registros em ordem de identidade: ID, used/alive de um byte cada e posição X/Y/Z.
   Quantidade de slots de arestas, ID, alive e presença de dados, endpoints, modo,
   custo e enabled. Reservas e tombstones permanecem representados.
3. Largura, altura, chunk size, tick ms, seed, ticks e quantidade de veículos como
   `u64`; alturas `i32` row-major; revisão `u64` por chunk; ID `u64` e X/Y/VX/VY
   `i64` por veículo. Campos inteiros usam little-endian.

Dirty não entra: é estado operacional, e limpar flags não altera checksum. Alturas
e revisões continuam representadas. Adjacência e contagem de incidentes são índices
derivados dos registros e não são serializados. Threads, snapshots, últimos resultados
de comandos, RoutingWorkspace, rotas retornadas, capacidades e métricas são excluídos.
Filas e contadores de ID/sequência entram porque alteram comportamento futuro.
Checksums históricos v1/v2 não são diretamente comparáveis ao v3. É um fingerprint
não criptográfico com possibilidade de colisão, calculado sob demanda.

## Proteções temporárias e próximos marcos

16.777.216 vértices, 10.000.000 veículos, 1024 threads e o intervalo de velocidade
são temporary safety guards, não limites de design do jogo. Reservas dos vetores
principais, da fila e dos dados de snapshots são falíveis. Recursos da máquina
podem restringir execuções grandes.

A rede de transporte e Dijkstra são descritos em [transport-network.md](transport-network.md).
As proteções adicionais de reservas e coordenadas também são temporárias. Comandos
de transporte validam e alteram registros serialmente; consultas imutáveis podem usar
vários workspaces em paralelo. A fase de movimento dos veículos permanece inalterada.

Antes de commuting/employment/trip demand, recomenda-se um marco intermediário
para veículos seguindo rotas, com progresso, chegada e reação a edições da rede.
Demandas sociais e econômicas não devem ser antecipadas sem esse ciclo básico.
