# Benchmarks — movimento e routing

## Routing baseline — Marco 003 (9 de outubro de 2026)

Dijkstra é medido separadamente do movimento. O gerador cria uma grid determinística
com duas arestas Road por conexão horizontal/vertical e custo 10. Não há wrap da rede,
congestionamento, cache ou veículos seguindo rotas. Grid sintética não representa
uma cidade real e não estabelece uma meta de desempenho do jogo.

Consultas são pares pseudoaleatórios reproduzíveis por SplitMix64/seed 42, com origem
uniforme e destino diferente. Incluem distâncias variadas, não apenas vizinhos.
A rede é construída por comandos e compartilhada para leitura durante as medições.
Cada combinação de rodada e contagem cria um pool e N workspaces; as queries são
particionadas por índice original, sem mutex. Cada workspace é usado serialmente e
reutilizado durante sua partição. Cada rodada embaralha deterministicamente a ordem
das contagens por seed, exportando as ordens reais.

Inicialização, geração, pool, warmup (16 queries por workspace), checksum e JSON ficam
fora do tempo medido. A latência mede `find_route`, inclusive scratch que precisar
crescer e alocação do vetor final da rota. O tempo de lote inclui despacho ao pool,
coleta e ordenação das amostras por índice original. Ticks não fazem parte dessa carga.

Mean/median/p95/p99 agregam latências individuais de todas as rodadas. Percentis usam
nearest rank, mediana faz média dos dois elementos centrais quando necessário.
Routes/s usa total de consultas dividido pela soma dos tempos de lote; speedup usa
a soma de tempos de lote com uma thread dividida pela soma da contagem medida.
Latência individual e throughput são métricas diferentes: executar mais queries
simultâneas aumenta throughput mesmo quando contenção aumenta a latência por query.

Ambiente: Windows x86_64, 12 CPUs lógicas, Intel Core i5-10400 @ 2.90 GHz; rustc/Cargo
1.87.0; build release. Foram executadas três rodadas de cada contagem em ambas as grids.
Os diagnósticos contam nós finalizados, arestas vivas/habilitadas/no modo examinadas
e inserções na heap. Checksum de saída usa FNV-1a-64, marcador
`federation-route-output-v1`, tamanho do lote e, na ordem original das queries,
índice, revisão, origem/destino, modo, custo `u128`, quantidade/IDs das arestas da rota.
Campos são serializados little-endian e não incluem tempos ou diagnósticos.

## Grid 128 × 128

16,384 nós e 65,024 arestas direcionadas; 512 queries por rodada, três rodadas.

| Threads | Latência média ms | P50 ms | P95 ms | P99 ms | Rotas/s | Speedup | Oversubscription |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | 0.8724 | 0.8558 | 1.8220 | 2.2795 | 1145.72 | 1.00× | não |
| 2 | 0.9992 | 0.9498 | 2.2633 | 2.8349 | 1816.86 | 1.59× | não |
| 4 | 1.2113 | 1.1364 | 2.6195 | 3.2533 | 2759.03 | 2.41× | não |
| 8 | 1.7177 | 1.6553 | 3.5050 | 4.1663 | 4127.68 | 3.60× | não |
| 12 | 1.9504 | 1.8316 | 4.0862 | 5.3356 | 5210.35 | 4.55× | não |
| 16 | 2.1770 | 1.7509 | 4.4231 | 15.1578 | 5878.34 | 5.13× | sim |

Médias por query: **8579.5 nós visitados**, **34124.7 arestas examinadas** e **8695.3 inserções na heap**, idênticas entre contagens.

Todos os checksums de saída coincidiram: `4d4b2cec4aa1d708`. Checksum do mundo v3: `bf6def3b72675e9a`. Dados e tempos de cada lote em [route-bench-results-128.json](route-bench-results-128.json).

## Grid 256 × 256

65,536 nós e 261,120 arestas direcionadas; 256 queries por rodada, três rodadas.

| Threads | Latência média ms | P50 ms | P95 ms | P99 ms | Rotas/s | Speedup | Oversubscription |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | 5.2112 | 4.6330 | 10.9009 | 12.7088 | 191.87 | 1.00× | não |
| 2 | 5.5188 | 4.7870 | 12.0897 | 14.2446 | 328.38 | 1.71× | não |
| 4 | 6.7599 | 5.8616 | 14.7085 | 18.2594 | 529.43 | 2.76× | não |
| 8 | 9.0133 | 7.9366 | 20.3950 | 23.6793 | 770.10 | 4.01× | não |
| 12 | 11.2038 | 9.7162 | 25.1873 | 31.4526 | 895.39 | 4.67× | não |
| 16 | 13.1787 | 10.9679 | 32.0789 | 44.6083 | 981.22 | 5.11× | sim |

Médias por query: **30707.3 nós visitados**, **122484.5 arestas examinadas** e **30934.3 inserções na heap**, idênticas entre contagens.

Todos os checksums de saída coincidiram: `a3fcb48860247fcc`. Checksum do mundo v3: `1a1bca442d2ab4c6`. Dados e tempos de cada lote em [route-bench-results-256.json](route-bench-results-256.json).

## Reprodução

```sh
cargo run -p federation-bench --bin federation-route-bench --release --locked -- --width 128 --height 128 --queries 512 --runs 3 --warmup 16 --threads 1,2,4,8,12,16 --seed 42 --output-json target/route-128.json
cargo run -p federation-bench --bin federation-route-bench --release --locked -- --width 256 --height 256 --queries 256 --runs 3 --warmup 16 --threads 1,2,4,8,12,16 --seed 42 --output-json target/route-256.json
```

Os executáveis release foram chamados diretamente com esses argumentos. Para smoke
manual use grid 16×16, 32 queries, uma rodada e threads 1,2,4. Para um perfil mais
longo aumente queries e runs, observando memória por workspace. O binário default
`federation-bench` continua medindo movimento; a adição do routing não muda seus comandos.

## Interpretação e riscos

16 threads tiveram o maior throughput nas duas execuções, excedendo as 12 CPUs
lógicas, mas também latência média e p99 maiores. Não se conclui que essa configuração
será ideal para uma cidade real. Não houve afinidade, isolamento térmico ou intervalo
de confiança. A grid maior explorou mais nós e teve custo por query significativamente
maior; custo depende dos destinos e do tamanho/topologia, não só da quantidade de queries.

Para 100k+ queries, exploração do Dijkstra e memória por workspace são candidatos a
gargalos. Histórico de remoções pode aumentar varredura da adjacência. Revisão global
é conservadora; nenhum cache foi implementado. A*, cache ou estruturas hierárquicas
exigirão benchmarks próprios e, no caso de A*, prova da admissibilidade da heurística.

As tabelas de movimento abaixo preservam os resultados históricos v1/v2. O executável
atual usa checksum v3, portanto fingerprints entre marcos não são comparáveis.
A carga de movimento continua sem comandos nem snapshots. Nenhum speedup foi
atribuído às mudanças do core sem comparação controlada entre versões.

# Referências históricas de movimentação sintética

## Metodologia revisada — Marco 002

O benchmark continua medindo exclusivamente movimento sintético independente.
A fase ApplyCommands está vazia; não há comandos nem snapshots nesse workload.
Não mede transporte, economia, rotas ou uma cidade completa.

`--runs` (default 5) repete cada contagem de threads com uma simulação nova, seed
e configuração iguais. Em cada rodada todas as contagens são medidas uma vez.
`--shuffle-thread-order` aplica Fisher–Yates com SplitMix64 e `--order-seed` (default
42); as ordens reais são exportadas no JSON. Sem essa opção, a ordem é crescente.

Inicialização, warmup, reserva de amostras, checksum, estatísticas e exportação ficam
fora da medição. Cada tick medido com Instant inclui despacho ao pool, ApplyCommands
vazio, barreira de movimento e Finalize. Múltiplas rodadas não são executadas em paralelo.

As métricas por contagem agregam todos os ticks de todas as rodadas: mean/median,
p95/p99 nearest rank, ticks/s e speedup contra a média agregada de uma thread.
Estatísticas entre rodadas usam as médias de tick de cada rodada: mediana, mínimo,
máximo e desvio padrão populacional. O JSON também preserva Timing por rodada.
`total_simulated_ticks` é o tick final de cada simulação, não a soma de todas elas.
Uma divergência de checksum exporta o relatório e encerra com erro.

## Resultado real — Marco 002, 9 de outubro de 2026

- Windows x86_64, Intel Core i5-10400 @ 2.90 GHz, 12 CPUs lógicas disponíveis.
- rustc 1.87.0 e Cargo 1.87.0; compilação release.
- Mapa 512 × 512, chunks 32, 100.000 veículos, seed 42, timestep 20 ms.
- Cinco rodadas por contagem, 100 ticks de warmup, 2.000 ticks medidos por rodada.
- Ordem embaralhada reproduzível, order seed 42; estado final de cada simulação: 2.100 ticks.
- 10.000 amostras por contagem; 60.000 ticks medidos no total.

| Threads | Média ms | Mediana ms | p95 ms | p99 ms | Ticks/s | Speedup | Oversubscription |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | 0.8931 | 0.8661 | 0.9975 | 1.3595 | 1119.68 | 1.00× | não |
| 2 | 0.4650 | 0.4522 | 0.5519 | 0.7780 | 2150.72 | 1.92× | não |
| 4 | 0.2783 | 0.2510 | 0.3795 | 0.4418 | 3592.83 | 3.21× | não |
| 8 | 0.2000 | 0.1963 | 0.2465 | 0.3128 | 4999.05 | 4.46× | não |
| 12 | 0.1902 | 0.1829 | 0.2435 | 0.3214 | 5257.13 | 4.70× | não |
| 16 | 0.1759 | 0.1650 | 0.2248 | 0.3371 | 5684.09 | 5.08× | sim |

Estatísticas entre rodadas, em ms por tick:

| Threads | Mediana das médias | Mínimo | Máximo | Desvio padrão |
| --- | --- | --- | --- | --- |
| 1 | 0.8957 | 0.8824 | 0.8978 | 0.0056 |
| 2 | 0.4678 | 0.4543 | 0.4739 | 0.0076 |
| 4 | 0.2672 | 0.2487 | 0.3484 | 0.0358 |
| 8 | 0.1991 | 0.1982 | 0.2047 | 0.0024 |
| 12 | 0.1876 | 0.1873 | 0.1966 | 0.0036 |
| 16 | 0.1738 | 0.1710 | 0.1846 | 0.0052 |

Todos os checksums finais foram `3790c64fca08d326` (FNV-1a-64 / federation-state-v2).
Dados completos e ordens em [bench-results-milestone-002.json](bench-results-milestone-002.json).

16 threads mantiveram a menor média nesta execução, mesmo excedendo as 12 CPUs
lógicas. A média de 16 foi cerca de 7,5% menor que a de 12; p99 com 16 foi maior.
A vantagem não pode ser atribuída a uma causa específica somente por este ensaio:
agendamento do SO, distribuição do trabalho e variações térmicas permanecem possíveis
influências. A variação da rodada com quatro threads também aparece no máximo e
desvio padrão. O ensaio comprova a observação nesse workload, sem estabelecer que
oversubscription será melhor para cargas de transporte ou sistemas interdependentes.

Não se atribui a diferença contra os números históricos à nova fase: não houve
comparação controlada entre as duas versões sob o mesmo ambiente de execução.
O checksum agora é v2, e os valores v1 não são comparáveis diretamente.

## Reprodução e perfis manuais

```sh
cargo run -p federation-bench --release --locked -- --vehicles 100000 --ticks 2000 --warmup 100 --runs 5 --threads 1,2,4,8,12,16 --shuffle-thread-order --order-seed 42 --output-json target/bench-results-002.json
```

O binário release foi executado diretamente com os mesmos argumentos nesta medição.
Para verificação rápida use `--ticks 200 --warmup 30 --runs 2 --threads 1,2,4`.
Para uma medição manual mais longa use `--ticks 5000` ou `--ticks 10000`, cinco
rodadas e warmup 100. O default de ticks permanece 200. Inclua uma thread como
referência; zero ticks ou zero rodadas são rejeitados. O benchmark pesado não entra
na CI. Contagens repetidas são deduplicadas e cenários são validados antes da medição.

## Limitações

Sem isolamento térmico, afinidade de CPU ou intervalos de confiança. O relógio e
SO influenciam as amostras. Mais threads não garantem ganhos em outros workloads.
Snapshots e comandos ativos precisam de workloads próprios em futuras medições;
não devem ser misturados com os números acima. Limites de configuração são
proteções temporárias e não estabelecem limites de design do jogo.

## Referência histórica — Marco 001 (9 de outubro de 2026)

- Mapa: 512 × 512 tiles; chunks: 32 × 32.
- Veículos: 100.000; seed: 42; tick lógico: 20 ms.
- Warmup: 30; ticks medidos: 200; estado final: 230 ticks.
- Sistema: windows; arquitetura: x86_64; CPUs lógicas disponíveis: 12.
- CPU: Intel(R) Core(TM) i5-10400 CPU @ 2.90GHz.
- Compilação release; rustc 1.87.0; Cargo 1.87.0.
- Uma execução por contagem de threads, em ordem crescente, na máquina local.

| Threads | Média ms | Mediana ms | p95 ms | p99 ms | Ticks/s | Speedup | Oversubscription |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | 1.5609 | 1.5456 | 1.6701 | 1.7608 | 640.65 | 1.00× | não |
| 2 | 0.7779 | 0.7675 | 0.8511 | 0.8742 | 1285.57 | 2.01× | não |
| 4 | 0.4311 | 0.4279 | 0.4810 | 0.5139 | 2319.42 | 3.62× | não |
| 8 | 0.2625 | 0.2554 | 0.3144 | 0.4133 | 3809.60 | 5.95× | não |
| 16 | 0.1631 | 0.1579 | 0.1982 | 0.2255 | 6131.21 | 9.57× | sim |

Todos os checksums finais foram `0e1d724d501061bc` (FNV-1a-64 / federation-state-v1). Dados completos em [bench-results.json](bench-results.json).
