# CODEX TASK 001 — In The Federation's Name: Simulation Foundation

## 1. Contexto do projeto

Você está desenvolvendo **In The Federation's Name**, um jogo de construção de cidades, transporte, logística e simulação econômica em larga escala.

Referências conceituais:
- **OpenTTD:** terreno baseado em tiles, redes de transporte, rotas e legibilidade.
- **Workers & Resources: Soviet Republic:** cadeias produtivas, construção física, logística e economia.
- **SimCity 4:** câmera ortográfica isométrica, com quatro rotações discretas.

A tecnologia principal está definida como **Rust**.

O jogo terá futuramente modelos 3D pixelizados e um renderer próprio ou desacoplado, possivelmente baseado em `wgpu`. **Não implemente renderização nesta tarefa.**

A prioridade arquitetural absoluta é permitir uma simulação eficiente, paralela, testável e independente de qualquer frontend.

## 2. Objetivo desta tarefa

Criar a primeira versão funcional do núcleo de simulação, com:

1. Workspace Rust organizado em crates independentes.
2. Terreno em grade com alturas editáveis e metadados por chunk.
3. Armazenamento orientado a dados para entidades móveis.
4. Scheduler de simulação com timestep fixo e fases explícitas.
5. Atualização multithread usando Rayon.
6. Inicialização reproduzível com seed.
7. Execução headless por linha de comando.
8. Benchmarks comparativos de desempenho.
9. Testes automatizados, documentação e CI.

**Esta tarefa deve resultar em código compilável e executável, não apenas em um documento de arquitetura.**

Se o repositório estiver vazio, inicialize-o. Se já houver arquivos, examine-os primeiro e preserve o que for compatível com estes requisitos.

## 3. Estrutura esperada do repositório

Organize o projeto aproximadamente assim:

- `Cargo.toml` — workspace.
- `Cargo.lock` — versionado.
- `rust-toolchain.toml` — toolchain stable.
- `.gitignore`.
- `.github/workflows/ci.yml`.
- `AGENTS.md` — convenções e regras de desenvolvimento.
- `README.md` — visão geral e instruções de execução.
- `docs/architecture.md` — arquitetura, decisões e limites.
- `docs/benchmarks.md` — metodologia e resultados medidos.
- `crates/federation-sim/`
  - `Cargo.toml`
  - `src/lib.rs`
  - `src/config.rs`
  - `src/terrain.rs`
  - `src/chunk.rs`
  - `src/entities.rs`
  - `src/scheduler.rs`
  - `src/simulation.rs`
  - `src/rng.rs`
  - `src/metrics.rs`
- `crates/federation-headless/`
  - `Cargo.toml`
  - `src/main.rs`
- `crates/federation-bench/`
  - `Cargo.toml`
  - `src/main.rs`

Pequenos ajustes na organização são permitidos quando tecnicamente justificados. Não crie dezenas de crates antecipadamente.

Use Rust edition 2024, caso o toolchain disponível suporte essa edição.

## 4. Arquitetura obrigatória

### 4.1. Núcleo independente

O crate `federation-sim` não pode depender de:
- Godot;
- `wgpu`;
- `winit`;
- APIs de janela;
- sistemas de UI;
- frameworks de jogos completos.

Ele deve funcionar como biblioteca Rust pura.

O frontend futuro deverá interagir com a simulação por comandos e snapshots, sem acesso irrestrito a estados mutáveis internos.

Não é necessário implementar esse protocolo nesta tarefa.

### 4.2. Terreno e chunks

Implemente um terreno retangular com:

- Dimensões configuráveis em tiles.
- Chunk size configurável, com padrão de 32 × 32 tiles.
- Alturas inteiras nos vértices dos tiles.
- Leitura e alteração de alturas por coordenadas.
- Validação de limites.
- Metadados de dirty state e revision por chunk.

**Decisão de implementação inicial:** use um heightmap global contíguo como fonte autoritativa das alturas e chunks como unidades de indexação e invalidação.

Isso evita duplicação inconsistente de vértices nas fronteiras.

Ao alterar um vértice compartilhado por chunks adjacentes, todos os chunks afetados devem ser invalidados.

Inclua testes específicos para vértices nas fronteiras e cantos dos chunks.

Não implemente geração de mesh, escavação, pontes ou túneis agora.

### 4.3. Entidades móveis e SoA

Implemente um armazenamento **Structure of Arrays (SoA)** para veículos sintéticos.

Campos mínimos:
- ID estável.
- Posição X.
- Posição Y.
- Velocidade X.
- Velocidade Y.

Use representação inteira/fixed-point para o estado simulado. Não dependa de floats para obter determinismo nesta etapa.

O ID de uma entidade não deve depender de seu endereço na memória. Nesta primeira versão, não é necessário implementar remoção e reutilização de IDs.

O sistema deve conseguir inicializar pelo menos 100.000 veículos sintéticos sem criar um objeto de runtime pesado para cada um.

A movimentação será artificial e simplificada. Seu objetivo é exercitar o processamento paralelo e o layout de memória, **não representar tráfego real**.

### 4.4. Scheduler multithread

Implemente uma simulação com timestep fixo.

A API pública deve oferecer algo equivalente a:

- `SimulationConfig`
- `Simulation::new(config)`
- `Simulation::tick()`
- `Simulation::checksum()`
- `Simulation::tick_count()`

O scheduler deve possuir fases explícitas e ordenadas.

Para esta primeira versão:
- **Movement:** atualizar as entidades em paralelo.
- **Finalize:** concluir o tick e atualizar contadores.
- **Diagnostics:** disponibilizar métricas, quando solicitado.

Evite criar um framework genérico de jobs excessivamente abstrato. Prefira uma implementação pequena, extensível e comprovadamente funcional.

Use `rayon` com pool de threads configurável.

Requisitos:
- Partições disjuntas para escrita.
- Nenhum `Mutex` por veículo.
- Nenhum spawn de task por entidade.
- Nenhuma dependência de ordem de execução entre partições.
- Nenhum estado global mutável compartilhado sem necessidade.
- Evitar alocações por entidade a cada tick.

Documente como serão adicionadas futuramente fases de pathfinding, economia, demografia e aplicação de comandos.

Não implemente essas fases ainda.

### 4.5. Determinismo

Duas simulações com a mesma configuração e seed devem produzir o mesmo estado final após o mesmo número de ticks, independentemente da quantidade de threads.

Implemente:
- Gerador pseudoaleatório determinístico para inicialização.
- Seed configurável.
- Checksum estável do estado simulado.
- Testes comparando execuções com 1, 2 e 4 threads.

Não use `DefaultHasher` como checksum persistente. Escolha e documente um algoritmo estável.

Evite reduções paralelas com ordem variável sobre números de ponto flutuante.

O checksum não precisa ser recalculado integralmente dentro do caminho crítico de cada tick.

### 4.6. Configuração e validação

A configuração deve contemplar:
- Largura do mapa.
- Altura do mapa.
- Tamanho dos chunks.
- Quantidade de veículos.
- Quantidade de threads.
- Seed.
- Duração lógica fixa do tick.

Use valores padrão sensatos.

Retorne erros claros para configurações inválidas, em vez de provocar panic.

Não fixe ainda a escala física definitiva dos tiles nem a velocidade temporal final do jogo. Documente as unidades provisórias utilizadas.

## 5. Executável headless

Crie o binário `federation-headless`.

Exemplo de execução esperada:

`cargo run -p federation-headless --release -- --width 512 --height 512 --vehicles 100000 --ticks 200 --threads 8 --seed 42`

Ele deve:

1. Criar a simulação.
2. Inicializar o terreno e os veículos.
3. Executar a quantidade solicitada de ticks.
4. Medir o tempo de execução.
5. Imprimir um resumo legível.
6. Exibir o checksum final.
7. Encerrar normalmente.

Inclua opção para exportar métricas em JSON.

O núcleo não deve dormir entre ticks. A responsabilidade de sincronizar tempo real será de um host futuro.

## 6. Benchmark de paralelismo

Crie o binário `federation-bench`.

Exemplo de execução:

`cargo run -p federation-bench --release -- --width 512 --height 512 --vehicles 100000 --ticks 200 --warmup 30 --threads 1,2,4,8,16 --seed 42 --output-json target/bench-results.json`

O benchmark deve:

- Inicializar uma simulação nova e equivalente para cada quantidade de threads.
- Excluir a inicialização das medições de tick.
- Executar warmup antes das medições.
- Usar a mesma seed e configuração.
- Medir os ticks com `Instant`.
- Comparar 1, 2, 4, 8 e 16 threads.
- Calcular média, mediana, p95 e, se viável, p99.
- Calcular ticks por segundo e speedup relativo à execução com uma thread.
- Comparar os checksums finais para validar consistência.
- Exportar resultados estruturados em JSON.

Inclua informações do ambiente, quando disponíveis, como sistema operacional, arquitetura, quantidade de CPUs lógicas e modo de compilação.

Se o hardware possuir menos threads físicas/lógicas que o solicitado, permita o teste, mas identifique o oversubscription no relatório.

**Não invente resultados de desempenho.**

Não estabeleça como critério obrigatório que 8 ou 16 threads sejam mais rápidos. Nesta etapa queremos medições reais e identificação de gargalos.

Documente claramente que este benchmark mede uma carga sintética de movimentação, não a capacidade final de simular cidades completas.

## 7. Dependências e práticas

Dependências recomendadas:
- `rayon` para paralelismo.
- `clap` para CLI.
- `serde` e `serde_json` para relatórios.
- `thiserror`, se útil para erros tipados.

Prefira a biblioteca padrão para o restante.

Não introduza nesta tarefa:
- Bevy ECS;
- banco de dados;
- Tokio;
- Godot;
- `wgpu`;
- networking;
- persistência de partidas;
- plugins dinâmicos;
- `unsafe`;
- sistemas econômicos fictícios apenas para preencher arquitetura.

Use comentários para explicar decisões não óbvias, não para repetir o código.

## 8. Testes obrigatórios

Inclua testes automatizados cobrindo:

1. Inicialização válida e inválida.
2. Indexação de tiles, vértices e chunks.
3. Alteração de alturas.
4. Invalidação de chunks adjacentes em fronteiras.
5. Comportamento nas bordas do mapa.
6. Mapas retangulares.
7. Simulação com zero veículos.
8. Integridade dos arrays SoA.
9. IDs estáveis.
10. Reprodutibilidade da mesma seed.
11. Igualdade do estado final entre diferentes quantidades de threads.
12. Evolução correta do contador de ticks.

Os testes de CI devem usar cenários pequenos e rápidos.

O benchmark de 100.000 veículos deve ser executável separadamente, sem tornar a suíte de testes cotidiana lenta.

## 9. Documentação

### README.md

Explique:
- O propósito do projeto.
- Como instalar o toolchain.
- Como compilar.
- Como executar os testes.
- Como executar o headless.
- Como executar benchmarks.
- O que já existe e o que ainda não existe.

### docs/architecture.md

Documente:
- Separação entre simulação e apresentação.
- Ownership do estado.
- Layout de memória das entidades.
- Organização do terreno.
- Estratégia de paralelismo.
- Fases de execução.
- Determinismo.
- Limitações conhecidas.
- Pontos de extensão futuros.

### docs/benchmarks.md

Documente:
- Cenários.
- Warmup.
- Quantidade de ticks medidos.
- Métricas.
- Ambiente.
- Resultados efetivamente observados.
- Limitações do benchmark sintético.

### AGENTS.md

Registre regras para futuras tarefas do Codex:
- Preservar a independência do núcleo.
- Priorizar desempenho mensurável.
- Evitar alocações no hot path.
- Manter determinismo onde possível.
- Não introduzir dependências grandes sem justificativa.
- Adicionar testes para novas regras.
- Não afirmar ganhos de performance sem benchmarks.
- Documentar mudanças arquiteturais relevantes.

## 10. CI e verificações

Configure GitHub Actions para executar:

- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- `cargo build --workspace --release`

Não execute benchmarks pesados em todo pull request.

## 11. Critérios de aceitação

A tarefa será considerada concluída somente quando:

- [ ] O workspace Rust compilar.
- [ ] O núcleo funcionar sem qualquer renderer.
- [ ] O terreno suportar leitura, escrita e invalidação de chunks.
- [ ] Os veículos forem armazenados em SoA.
- [ ] O scheduler realmente executar trabalho em múltiplas threads.
- [ ] A quantidade de threads for configurável.
- [ ] Execuções equivalentes produzirem checksums iguais.
- [ ] O executável headless funcionar por CLI.
- [ ] O benchmark comparar diferentes contagens de threads.
- [ ] O benchmark exportar JSON válido.
- [ ] Os testes automatizados passarem.
- [ ] O Clippy não apresentar warnings.
- [ ] O código estiver formatado.
- [ ] README, arquitetura e metodologia de benchmark estiverem documentados.
- [ ] Não houver dependência de engine gráfica no núcleo.

Se alguma verificação não puder ser executada no ambiente disponível, informe isso explicitamente. Nunca marque como concluído algo que não foi verificado.

## 12. Processo de implementação

Trabalhe nesta ordem:

1. Inspecione o repositório e identifique o estado atual.
2. Apresente um plano curto de implementação.
3. Crie o workspace e os crates.
4. Implemente terreno, chunks e testes.
5. Implemente entidades SoA e inicialização determinística.
6. Implemente scheduler e simulação.
7. Implemente o executável headless.
8. Implemente benchmarks e exportação JSON.
9. Adicione documentação e CI.
10. Execute as verificações disponíveis.
11. Corrija erros encontrados.
12. Apresente um relatório final.

Tome decisões locais razoáveis sem interromper o trabalho para perguntar sobre detalhes não essenciais. Documente essas decisões.

**Não amplie o escopo para criar um jogo completo.**

## 13. Formato da entrega final

Ao concluir, apresente:

**A. Resumo:** o que foi implementado.

**B. Estrutura:** arquivos e módulos principais.

**C. Validação:** comandos executados e seus resultados reais.

**D. Performance:** tabela de benchmarks medidos, se o ambiente permitir.

**E. Limitações:** pontos ainda simplificados ou não implementados.

**F. Próxima tarefa recomendada:** um único marco técnico, com justificativa.

O objetivo final deste marco é possuir uma fundação pequena, confiável, reproduzível e mensurável sobre a qual construiremos a simulação de cidades, logística e economia de In The Federation's Name.