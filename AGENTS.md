# Repository Guidelines

## Project Structure & Module Organization

This Rust 2024 workspace implements the headless simulation foundation for **In The Federation's Name**. Read `Instructions.md` for the milestone requirements. Always look for `.github/prompts/` and read applicable Markdown instructions before working; that directory is currently absent.

- `crates/federation-sim/src/`: configuration, terrain/chunks, SoA vehicles, deterministic RNG, scheduler, simulation and metrics.
- `crates/federation-sim/src/transport/`: logical directed multigraph, IDs, Dijkstra and network snapshots.
- `crates/federation-sim/tests/`: behavioral integration tests through public snapshots and commands.
- `crates/federation-headless/`: executable host and shared CLI/report utilities.
- `crates/federation-bench/`: separate movement and `federation-route-bench` grid routing benchmarks.
- `docs/`: architecture and measured benchmark methodology/results.

## Build, Test, and Development Commands

Use the stable toolchain selected by `rust-toolchain.toml`. Keep `Cargo.lock` versioned.

- `cargo build --workspace --release --locked`: compile optimized hosts and library.
- `cargo test --workspace --locked`: run unit, integration and documentation tests.
- `cargo fmt --all -- --check`: verify formatting.
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: reject lint warnings.
- `cargo run -p federation-headless --release -- --ticks 200 --threads 4`: run the simulation.
- `cargo run -p federation-bench --release -- --threads 1,2,4,8,16`: measure parallel tick performance and export JSON.

## Coding Style & Architecture Rules

Use Rustfmt, four-space indentation, snake_case modules/functions and PascalCase types. Unsafe code is forbidden. Keep the core independent of rendering, windows and UI frameworks. Expose controlled operations and immutable state views. Preserve integer simulation state and deterministic initialization. Avoid per-entity allocations in the hot path and large dependencies without justification. Document architectural changes and provisional units.

## Testing & Performance Guidelines

Use Rust's built-in test framework and descriptive behavior-based test names. Add tests for new rules, boundaries and invalid inputs. Preserve checks across 1, 2 and 4 threads. Keep everyday tests small; run extended benchmarks separately. No coverage percentage is currently mandated. Prioritize measurable performance and never claim speedups without actual benchmarks.

## Commit & Pull Request Guidelines

No existing commit history establishes conventions. Use concise imperative subjects, such as `Add deterministic vehicle movement`. Keep changes focused. Pull requests should explain behavior, link relevant issues, report validation and document limitations; include screenshots only for visible interface changes.

## Commands, IDs and snapshots

- Never assume `VehicleId` is an array index; resolve identity through controlled APIs.
- External authoritative changes must use commands, applied before Movement.
- Renderers must not receive mutable simulation state; consume owned snapshots.
- Snapshot representation must not dictate internal simulation storage.
- Internal dirty flags are not an external synchronization protocol; prefer revisions.
- Do not introduce concurrent command producers without a concrete need and benchmark.
- Every new command needs deterministic behavior tests across thread counts.
- Keep next sequence and pending commands represented in the deterministic checksum.
- Configuration and velocity bounds are temporary safety guards, not game design limits.
- Tick phases are ApplyCommands, Movement, Finalize; Diagnostics remains on demand.

## Transport network and routing

- Equal node positions never imply connectivity.
- Edges are directed; bidirectional transport requires two edges.
- Never use `(from, to)` as edge identity; parallel edges are valid.
- Never interpret transport identity tokens as storage slots.
- Reserve an ID immediately before submitting its creation command; IDs are not reused.
- Network mutations must use commands, and effective routing/topology changes update revision.
- Disabled live edges still prevent removal of incident nodes; do not cascade removal.
- Routing uses integer costs and deterministic output, including equal-cost ties.
- RoutingWorkspace is disposable scratch, excluded from authoritative checksum.
- Dirty flags are operational state, excluded from the checksum.
- Consumers use snapshots/diagnostics/routing, not terrain or SoA storage getters.
- Do not add route caches without benchmarks or A* without proving heuristic admissibility.
- Keep routing benchmarks separate from movement and out of routine CI.
- Do not connect vehicles to routes or add congestion in the logical network milestone.
