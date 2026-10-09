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
cargo run -p federation-bench --release -- --width 512 --height 512 --vehicles 100000 --ticks 2000 --warmup 100 --runs 5 --threads 1,2,4,8,12,16 --shuffle-thread-order --order-seed 42 --seed 42 --output-json target/bench-results.json
~~~

É obrigatório incluir uma thread como referência e medir ao menos um tick. Cada rodada de cada contagem recebe uma simulação nova. `--runs` tem default 5; `--shuffle-thread-order` varia a ordem de forma reproduzível por `--order-seed`. Inicialização, warmup, checksum e JSON ficam fora dos tempos de tick. Mais detalhes e resultados reais em [docs/benchmarks.md](docs/benchmarks.md).

## Estrutura e estado atual

- `crates/federation-sim`: terreno, metadados de chunks, veículos SoA, RNG, scheduler e diagnósticos.
- `crates/federation-headless`: host CLI e utilitários de argumentos/relatórios compartilhados com o benchmark.
- `crates/federation-bench`: medição comparativa de movimentação sintética.
- `docs/architecture.md`: decisões, unidades provisórias e extensões futuras.
- `.github/workflows/ci.yml`: formatação, Clippy, testes e build; sem benchmarks pesados.

Existem altura editável por comandos, revisões de chunks, IDs opacos, snapshots independentes e movimento inteiro paralelo com seed reproduzível. Ainda não existem renderer, tráfego real, pathfinding, cadeias produtivas, economia, interface ou persistência.

## Marco 002: comandos e snapshots

Mudanças autoritativas passam por `submit_command` e só ocorrem no próximo tick,
antes do movimento. Resultados são consultados em `command_results()` e substituídos
no próximo tick. `VehicleId` identifica veículos sem estabelecer um índice de array.
Snapshots são cópias independentes solicitadas explicitamente; chunks podem ser
consultados por revisão, sem limpar dirty flags.

```rust
use federation_sim::{Simulation, SimulationConfig, SimulationCommand,
    SnapshotRequest, VehicleSelection, ChunkRevisionQuery};

let mut sim = Simulation::new(SimulationConfig::default())?;
let view = sim.snapshot(SnapshotRequest {
    vehicles: VehicleSelection::All,
    ..Default::default()
})?;
let id = view.vehicles[0].id;
let sequence = sim.submit_command(SimulationCommand::SetVehicleVelocity {
    vehicle_id: id, vx: 5, vy: -2,
})?;
sim.tick()?;
assert_eq!(sim.command_results()[0].sequence, sequence);
let ids = [id];
let queries = [ChunkRevisionQuery { x: 0, y: 0, known_revision: None }];
let snapshot = sim.snapshot(SnapshotRequest {
    vehicles: VehicleSelection::Ids(&ids),
    terrain_chunks: &queries,
})?;
```

Exemplo dentro de uma função que retorna `Result<(), Box<dyn std::error::Error>>`.
Todos os limites numéricos de configuração e velocidade são proteções temporárias
contra overflow e consumo excessivo de recursos, sem estabelecer limites de design
do jogo. Detalhes em [docs/architecture.md](docs/architecture.md).
