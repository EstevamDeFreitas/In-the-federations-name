# Benchmarks de movimentação sintética

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
