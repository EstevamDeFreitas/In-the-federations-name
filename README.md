# In The Federation's Name

Fundação de um jogo de construção de cidades, transporte, logística e economia. Este marco implementa uma simulação Rust headless, independente de frontend. O escopo original está em [Instructions.md](Instructions.md).

## Requisitos e instalação

Instale Rust com [rustup](https://rustup.rs/). O arquivo `rust-toolchain.toml` seleciona stable com Rustfmt e Clippy; a edição 2024 exige Rust 1.85 ou superior. No Windows, o toolchain MSVC exige as ferramentas C++ de compilação do Visual Studio. Execute os comandos em um **x64 Native Tools Command Prompt for VS** ou **Developer PowerShell** com o ambiente C++ ativado, para disponibilizar o linker e as bibliotecas do SDK. O Cargo baixa as dependências na primeira compilação; `Cargo.lock` é versionado.

## Compilação e verificações

~~~sh
cargo build --workspace --release --locked
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
~~~

## Execução headless

~~~sh
cargo run -p federation-headless --release -- --width 512 --height 512 --vehicles 100000 --ticks 200 --threads 8 --seed 42 --output-json target/headless-results.json
~~~

Opções adicionais: `--chunk-size 32` e `--tick-ms 20`. Não há espera entre ticks. Zero veículos e zero ticks são válidos no headless. A saída separa inicialização e execução, inclui tempo lógico, checksum e métricas; `--output-json` é opcional. Configurações inválidas encerram com erro.

## Benchmark

~~~sh
cargo run -p federation-bench --release -- --width 512 --height 512 --vehicles 100000 --ticks 200 --warmup 30 --threads 1,2,4,8,16 --seed 42 --output-json target/bench-results.json
~~~

É obrigatório incluir uma thread como referência e medir ao menos um tick. Cada contagem recebe uma simulação nova. Inicialização, warmup, checksum e JSON ficam fora dos tempos de tick. Mais detalhes e resultados reais em [docs/benchmarks.md](docs/benchmarks.md).

## Estrutura e estado atual

- `crates/federation-sim`: terreno, metadados de chunks, veículos SoA, RNG, scheduler e diagnósticos.
- `crates/federation-headless`: host CLI e utilitários de argumentos/relatórios compartilhados com o benchmark.
- `crates/federation-bench`: medição comparativa de movimentação sintética.
- `docs/architecture.md`: decisões, unidades provisórias e extensões futuras.
- `.github/workflows/ci.yml`: formatação, Clippy, testes e build; sem benchmarks pesados.

Existem altura editável nos vértices, invalidação de chunks, IDs estáveis e movimento inteiro paralelo com seed reproduzível. Ainda não existem renderer, tráfego real, pathfinding, cadeias produtivas, economia, interface, persistência ou protocolo de comandos/snapshots.
