# Benchmarks de movimentação sintética

## Metodologia

Cada contagem de threads inicializa uma simulação nova com terreno plano, configuração e seed iguais. São executados 30 ticks de warmup e depois 200 ticks medidos individualmente com `Instant`. Inicialização, warmup, cálculo do checksum, impressão e exportação ficam fora da medição. A coleta de amostras reserva memória antes do loop; cada tempo inclui o despacho ao pool e a barreira de conclusão do tick.

Média e mediana usam todas as amostras. p95/p99 usam nearest rank (índice `ceil(n × p) - 1`). Ticks/s é a quantidade medida dividida pela soma dos tempos de tick; speedup é a média com uma thread dividida pela média da linha. Os percentis e estatísticas usam floats apenas no host de medição, nunca no estado da simulação.

O benchmark exige uma thread como referência, ordena e deduplica as contagens e permite oversubscription, identificado quando threads excedem CPUs lógicas disponíveis. Uma divergência de checksum exporta o relatório e encerra com erro.

## Cenário observado — 9 de outubro de 2026

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

## Reprodução

~~~sh
cargo run -p federation-bench --release -- --width 512 --height 512 --vehicles 100000 --ticks 200 --warmup 30 --threads 1,2,4,8,16 --seed 42 --output-json target/bench-results.json
~~~

No Windows desta validação foi necessário ativar `vcvars64.bat` das Build Tools antes de compilar, porque o linker MSVC não estava no PATH padrão. Após a compilação, os executáveis release foram chamados diretamente com os argumentos acima. As dependências foram obtidas por `cargo fetch`; verificações posteriores usaram `--locked --offline`.

## Limitações

Esta carga mede somente deslocamento inteiro independente, com wrap no mapa. Não mede pathfinding, tráfego, economia, interação entre entidades ou cidades completas. Há overhead do relógio, do scheduler e influência do sistema operacional. Ordem fixa, uma única execução e ausência de isolamento térmico tornam estes resultados exploratórios, sem intervalos de confiança. Repita em diferentes mapas, populações e ambientes antes de orientar decisões de arquitetura. Maior contagem de threads não garante ganhos em outras cargas.
