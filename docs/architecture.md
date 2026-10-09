# Arquitetura da fundação

## Separação e ownership

`federation-sim` é uma biblioteca Rust sem APIs de janela ou engine. `Simulation` possui configuração, terreno, veículos, pool Rayon privado e contador de ticks. Getters expõem somente referências imutáveis; alterações de terreno usam operações controladas. Um futuro frontend deverá enviar comandos e consumir snapshots. Esse protocolo ainda não foi implementado.

`federation-headless` contém a CLI e uma pequena biblioteca de argumentos, ambiente e exportação JSON, reutilizada por `federation-bench`. Clap e Serde pertencem aos hosts, sem entrar no núcleo.

## Terreno e chunks

Um heightmap contíguo row-major de `(width + 1) × (height + 1)` valores `i32` é autoritativo. Tiles e chunks também usam indexação row-major; chunks parciais são permitidos em mapas retangulares. O tamanho padrão é 32 tiles por lado.

Chunks armazenam coordenadas, dirty e revisão. Nascem dirty com revisão zero. Uma edição efetiva invalida os chunks dos tiles que tocam o vértice: até quatro chunks em uma interseção. Escrever a mesma altura não altera revisões. Limites externos são inclusivos para vértices e exclusivos para tiles. Revisões saturam em `u64::MAX`; isso é uma limitação explícita desta etapa. Limpar dirty não altera a revisão.

## Entidades e unidades provisórias

Veículos usam cinco vetores contíguos: ID `u64` e X/Y/VX/VY `i64`. IDs são índices sequenciais estáveis, sem remoção ou reutilização. Nenhum objeto pesado ou mutex é criado por veículo.

Uma tile contém 1024 unidades de posição. Velocidades iniciais variam entre −8 e 8 unidades por milissegundo lógico. Um tick padrão representa 20 ms; deslocamento é `velocidade × tick_ms`, sem arredondamento por floats. Posições fazem wrap toroidal com resto euclidiano. Alturas são níveis inteiros sem escala física definida. Essas unidades não estabelecem a escala final do jogo.

## Scheduler e paralelismo

Fases ordenadas: Movement e Finalize. Diagnostics é explícita e executada somente por `diagnostics()`. Movement usa `ThreadPool::install` e iteradores paralelos indexados sobre slices, com granularidade mínima de 1024 entidades. Escritas são disjuntas; não há task por veículo, mutex por entidade ou alocação de arrays a cada tick. O pool persiste durante a vida da simulação. Finalize incrementa o contador após a barreira de conclusão da movimentação. Overflow de ticks retorna erro antes de alterar o estado.

Rayon escolhe como dividir e roubar trabalho; uma carga pequena pode usar apenas parte do pool. Não há promessa de speedup para uma carga limitada por overhead ou largura de banda de memória.

## Determinismo e checksum

A inicialização usa SplitMix64, com constantes e aritmética wrapping explícitas. A ordem serial de geração é ID, posição X/Y, velocidade X/Y. Terreno começa plano. Cada veículo evolui independentemente com inteiros, sem reduções de floats.

Checksum v1 usa FNV-1a 64-bit: marcador `federation-state-v1`; largura, altura, chunk size, tick ms, seed, ticks e quantidade de veículos como `u64` little-endian; alturas como `i32` little-endian; revisão `u64` e dirty de um byte por chunk; ID `u64` e X/Y/VX/VY `i64` por veículo. Ordem sempre row-major/ID. Threads não entram no checksum. É um fingerprint com possibilidade de colisão, sem finalidade criptográfica, e não é calculado no tick normal.

## Validação, limites e extensões

Dimensões, chunk size, threads e tick ms devem ser positivos. Limites iniciais: 16.777.216 vértices, 10.000.000 veículos e 1024 threads. As reservas de vetores retornam erros; recursos do sistema ainda podem limitar execuções grandes. O limite de mapa e a velocidade inicial mantêm a aritmética de movimento dentro de `i64`, inclusive com tick ms máximo de `u32`.

Pathfinding, economia e demografia deverão entrar como fases com leituras declaradas e escrita particionada, seguidas de uma barreira. Comandos deverão ser aplicados em ordem estável antes de Movement. Interações entre entidades exigirão buffers de saída e merges determinísticos. Alterações nesses contratos precisam de testes e medições. Nada disso foi antecipado neste marco.
