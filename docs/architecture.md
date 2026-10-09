# Arquitetura — Marco 002

## Ownership e fronteira pública

`Simulation` é a única proprietária do estado autoritativo mutável: configuração,
heightmap, veículos SoA, fila de comandos, pool Rayon e contador de ticks. O core
permanece headless e sem novas dependências. Getters antigos oferecem somente
referências imutáveis; futuros renderers e ferramentas devem consumir snapshots.
Não existe `Simulation::set_height`, setter público de velocidade ou acesso mutável
às estruturas internas. Setters de terreno são `pub(crate)`; limpar dirty existe
somente como ferramenta de testes internos nesta etapa.

Clap, Serde, medição de tempo e exportação JSON pertencem aos hosts. Nenhum sistema
de rendering, transporte, replay, persistência ou rede foi antecipado.

## Identidade e armazenamento

`VehicleId` é um token opaco e tipado. `VehicleId::from_raw` permite reconstruir um
token; isso não garante existência. O consumidor obtém IDs por snapshots ou pela
lista imutável de IDs e consulta `Vehicles::get(id)` ou seleciona IDs no snapshot.
Nunca deve interpretar um token como posição em um vetor. A associação direta
ID → slot permanece um detalhe interno temporário, encapsulado em `Vehicles::slot`.
Não há remoção ou reutilização. Sparse sets, gerações e compactação poderão mudar
essa associação sem alterar os comandos nem os snapshots.

O armazenamento continua com cinco vetores contíguos: IDs e X/Y/VX/VY. Snapshot
usa registros compactos por veículo; sua representação não define o armazenamento
interno. Não há heap allocation individual por veículo.

## Comandos: submissão, ordem e resultados

`SimulationCommand` é um enum com `SetTerrainVertexHeight` e `SetVehicleVelocity`.
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
com tick e vetores próprios. `VehicleSelection` permite None, All ou IDs explícitos;
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

FNV-1a-64 v2 começa com `federation-state-v2`, seguido de próxima sequência e
quantidade pendente em `u64` little-endian. Para cada envelope: sequência `u64`,
tag de um byte (0 terreno, 1 velocidade) e payload; terreno usa X/Y convertidos
para `u64` e altura `i32`; velocidade usa identidade `u64` e VX/VY `i64`.
Seguem largura, altura, chunk size, tick ms, seed, ticks e quantidade de veículos
como `u64`; alturas `i32` row-major; revisão `u64` e dirty de um byte por chunk;
ID `u64` e X/Y/VX/VY `i64` por veículo, todos little-endian.

Threads, snapshots, resultados observacionais do último tick, capacidades dos
buffers e medições não entram no checksum. Filas distintas e progressos distintos
da sequência entram, pois alteram o comportamento futuro. Dirty continua incluído
para preservar a representação interna do estado do Marco 001. A versão mudou:
checksums históricos v1 não são diretamente comparáveis. É um fingerprint com
possibilidade de colisão e não é calculado no tick normal.

## Proteções temporárias e Marco 003

16.777.216 vértices, 10.000.000 veículos, 1024 threads e o intervalo de velocidade
são temporary safety guards, não limites de design do jogo. Reservas dos vetores
principais, da fila e dos dados de snapshots são falíveis. Recursos da máquina
podem restringir execuções grandes.

A fronteira está preparada para Transport Network & Routing Foundation, que deverá
adicionar comandos e dados de leitura específicos com testes de determinismo. Não
há rede de transporte implementada. Interações entre entidades precisarão de fases,
buffers de saída e merges determinísticos. O movimento independente e o mapa toroidal
são simplificações que precisarão ser revistas quando houver rotas. Custos de cópia,
associação ID → slot e saturação de revisão devem ser reavaliados quando houver
volumes reais, remoção ou persistência. Não introduzir concorrência de comandos
sem necessidade concreta e benchmark.
