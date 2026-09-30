# Phase 4: Review  (slug: render-engine)

**Verdict:** APPROVED_PENDING_MANUAL

> Re-verificação em modo `verify` (2026-09-30), `HEAD` = `795a5e1`, depois do veredito BLOCKED de `620fc65`.
> Entre as duas revisões entraram `711f9e6` (B1), `315ea2e` (W2, W4, W7 e W9) e `795a5e1`
> (D-2026-09-30-render-engine-6 e o todo de pendências). Refiz todos os gates do zero, sem reaproveitar o
> relatório anterior. Nenhum comando abriu a tela real, que continua com o `turing-smart-screen.service`.
> Fora da suíte, rodei só dois testes `--ignored`, e nenhum deles usa hardware: o de corpus de importação, do
> jeito que o DoD pede (lê só arquivos locais), e o de tempo do `bezel-render` em release, para o critério da
> T-4.2.

## Gates
| Gate | Status | Details |
|---|---|---|
| Build | PASS | `cargo build --workspace --locked`: exit 0, lock aceito (rustc 1.98.1) |
| Tests | PASS | 361 passed, 0 failed, 2 ignored (timing do renderer e corpus local; nenhum de hardware). São 360 + 1: os 2 testes de `bezel-render/src/quantity.rs` foram para o core e entrou `quantities_fall_back_to_the_key_name` |
| Coverage | PASS | 95.09% lines (TOTAL, sem `main.rs`/`build.rs`). Com o comando do DoD (sem filtro), 95.03%. `bezel-core/src/domain/sensor.rs` tem 99.69% |
| Lint | PASS | `cargo fmt --all --check` e `cargo clippy --workspace --all-targets --locked -- -D warnings` limpos. Nenhum `#[allow]` fora de teste: os 6 de `bezel-themes/src/import/*.rs` saíram (W4 anterior resolvido) |
| Hexagonal/Safety/Protocol/Hygiene | PASS | 5.1–5.11 sem acerto que bloqueie. O B1 anterior está resolvido (ver abaixo), e `cargo audit --deny warnings` sai com 0 |
| Consistency | PASS (com WARN) | D-2026-09-30-sensors-1 e D-2026-09-30-render-engine-1 agora são cumpridas. D-2026-09-30-render-engine-6 cobre o W1 anterior. A parte "serviço" de D-2026-09-30-render-engine-5 segue sem registro (W1). Tempo da T-4.2: mediana de 3,97 ms contra o limite de 15 ms |
| UI Validation | SKIPPED | `frontend.has_frontend` ausente no PROJECT.md e a fase não tem UI. A única mudança no Studio é Rust (`dto.rs`), fora do front |
| DoD | PENDING MANUAL | 7/7 itens Auto PASS; 3 itens Manual pendentes |

### Detalhe do gate 5
| Check | Resultado |
|---|---|
| 5.1 dependências do core | PASS: só `thiserror` |
| 5.2 I/O, threads e cfg de plataforma no core | PASS: nada encontrado. O `quantity_of` novo é função pura sobre `&SensorKey` |
| 5.3 ports | PASS: nada é implementado no core. As traits fora do core são `Wire`, `Pause` e `Clock` (fases anteriores) e `Pace` (`bezel-cli/src/live.rs:26`), que é auxiliar interno do laço do `run` |
| 5.4 adapters só na composição | PASS: nenhum acerto fora de `main.rs`/`lib.rs`/testes |
| 5.5 `unsafe` | PASS: a única ocorrência é a string `"unsafe asset path"` (`bezel-themes/src/native.rs:39`) |
| 5.6 panics fora de teste | PASS: nos arquivos alterados desde `620fc65`, todo `unwrap`/`expect` fica dentro de `#[cfg(test)]` (`sensor.rs:464+`, `renderer.rs:530+`, `dto.rs:314+`, `theme.rs:246+`). Os `import/{python_yaml,turzx}/tests.rs` são módulos de teste |
| 5.7 escrita no dispositivo | PASS: nenhum `Confirm::Yes` fora das fronteiras humanas e nenhum `open` de transporte em teste |
| 5.8 fidelidade de protocolo | PASS: todo encoder de `protocol/` tem teste. Conferi de novo as tags NRBF de `nrbf.rs:586-619` contra `docs/reverse-engineering/themes-turzx.md:42-71`: `12` = `0x0c` BinaryLibrary, `5` = `0x05` ClassWithMembersAndTypes, `15` = `0x0f` ArraySinglePrimitive, `13` = `0x0d` ObjectNullMultiple256 (contagem de 1 byte), `11` = `0x0b` MessageEnd e `9` = `0x09` MemberReference. Todas batem |
| 5.9 caminhos e plataforma no core | PASS: os acertos novos em `sensor.rs:185,489-515` são doc e dados de teste com o prefixo de chave `hwmon.` de D-2026-09-30-sensors-1, não caminho nem API de plataforma. Os demais são das fases anteriores (`device.rs:75`, `discovery.rs:11,248-309`) |
| 5.10 comandos Tauri | N/A: `dto.rs` não é comando |
| 5.11 supply chain e segredos | PASS: `cargo audit` sem nenhum aviso (`--deny warnings`, exit 0). `yoke-derive` foi para 0.8.4, e `.cargo/audit.toml:22-28` ignora RUSTSEC-2026-0192 (`ttf-parser`) com caminho, motivo e condição de reavaliação. Nenhum segredo |

### Verificação do B1 anterior (resolvido)
- A regra "o nome da chave diz a unidade" saiu do adapter: `crates/bezel-render/src/quantity.rs` foi removido em
  `711f9e6`. Agora `quantity_of`, `by_segments` e `by_last_segment` ficam em
  `crates/bezel-core/src/domain/sensor.rs:188-235`. `Quantities::quantity` (`:265-267`) usa o valor do catálogo e,
  sem ele, o nome da chave.
- O renderer chama o core (`crates/bezel-render/src/renderer.rs:182`, `context.quantities.quantity(key)`). O
  Studio também (`apps/bezel-studio/src-tauri/src/dto.rs:204`). Antes o Studio caía em `Quantity::Number`, então
  o inspetor e o frame já não divergem, que era o risco de duplicação citado no B1.
- Nenhum adapter decide unidade fora de teste. `grep 'Quantity::'` em `bezel-render`, `bezel-themes`, `bezel-cli`
  e no Studio só encontra testes (`dto.rs:351`, `sensors.rs:264,340`).
- Os goldens não mudaram: `git diff --stat 620fc65 HEAD` não toca arquivo de golden, nem `golden.rs` ou
  `testkit.rs`, e `tests::every_element_kind_matches_its_golden` passa.
- Os testes foram junto (`well_known_keys` e `open_ended_keys`), e entrou um de fallback do catálogo.

## Blockers
Nenhum.

## Warnings
- **W1. D-2026-09-30-render-engine-5, parte de serviço, ainda sem registro (era o W8).** A decisão diz que o
  `bezel run` "can run as a systemd user service or Windows logon task". O binário aguenta esse uso (trata
  SIGINT/SIGTERM e devolve a tela), mas não há unit `--user`, tarefa de logon nem instrução no README: `README.md`
  §§ Quick start e Themes (`:12-51`) não falam de serviço. Também não entrou em nenhum todo. O
  `2026-09-30-render-engine-review-followups.md` só cobre W2/W3/W5/W6, e a `release-polish` não tem esse item.
  **Ação:** criar um todo `[release-polish]` (unit systemd `--user` de exemplo e tarefa de logon do Windows,
  empacotadas ou documentadas no README) citando D-2026-09-30-render-engine-5.
- **W2. Dívida conhecida, registrada e aceita (eram W2, W3, W5 e W6).** Continua no código e está em
  `.jdi/todos/2026-09-30-render-engine-review-followups.md`:
  - `language_of` e o `clock.rs` duplicados entre CLI e Studio;
  - `is_native` (`bezel-cli/src/theme.rs:67`) contra `is_native_theme` do Studio;
  - o mínimo de 0,25 s em três constantes: `MIN_INTERVAL` (`bezel-cli/src/sensors.rs:24`), `MIN_REFRESH`
    (`bezel-cli/src/live.rs:20`) e `MIN_REFRESH` (`apps/bezel-studio/src-tauri/src/backend.rs:454`);
  - "o tema cabe no painel" implementado duas vezes;
  - o variant de `BezelError` para arquivos de tema;
  - as tags NRBF como inteiros crus (`nrbf.rs:586-619`);
  - as funções longas dos importadores;
  - `Theme::sensor_keys`, `element_mut` e `next_id` e `BoxF::translated`.

  O registro é aceitável: nada disso contradiz uma D-XX, e cada item tem descrição acionável. **Falta uma linha:**
  o W5(d) anterior, `let _ = write!(log, …)` sem comentário em `bezel-cli/src/live.rs:157,164` e
  `bezel-cli/src/theme.rs:213`, não entrou no todo. Vale acrescentar a razão, como `main.rs:131` já faz.
- **W3. Processo, histórico (era o W9 a–c).** Cito só para registro, porque não há o que corrigir sem reescrever
  o histórico:
  - a fase rodou fora do `/jdi-do`;
  - T-4.1, T-4.2 e T-4.3 têm dois commits cada, e `d3a4158` é compartilhado;
  - `73bcd39` saiu da lista de arquivos da T-4.1.

  Os commits de correção seguem o escopo da fase (`refactor|chore|docs(render-engine)`).
- **W4. Detalhes de documentação.**
  - (a) O doc comment de `quantity_of` (`bezel-core/src/domain/sensor.rs:184-187`) junta dois parágrafos. O
    resumo do rustdoc vira "Units of keys a catalog does not know…", e a frase que descreve a função fica
    depois. Basta deixar uma linha de resumo.
  - (b) As views geradas e ignoradas pelo git, `.jdi/DECISIONS.md` e `.jdi/todos.md`, estão desatualizadas: não
    trazem D-2026-09-30-render-engine-6 nem o todo novo (nem D-2026-09-30-sensors-5 e
    D-2026-09-30-storage-video-*). Rodar `npx -y jdi-cli render`.
  - (c) Observação sem ação: o `cargo update -p yoke-derive` de `315ea2e` também trocou a dependência de
    `cssparser-macros 0.7.1` de `syn 3.0.6` para `syn 2.0.119` (`Cargo.lock:976-983`), e a mensagem do commit
    não diz isso. É só de compilação (pilha do webview), e o build `--locked`, os testes e o audit passam.

### Situação das warnings anteriores
| Anterior | Situação | Evidência |
|---|---|---|
| W1 importadores fora de porta | Resolvido por decisão | D-2026-09-30-render-engine-6 (conversão na borda, sem porta `ThemeImporter`, com condição de revisão). A parte "CLI conhece `MANIFEST`/`EXTENSION`" está no todo ("one 'is a native theme' test") |
| W2 DRY | (c) `WARM_UP` resolvido; o resto está no todo | `bezel-cli/src/theme.rs:22` reexporta `sensors::WARM_UP` (`sensors.rs:21`) |
| W3 "tema cabe no painel" | No todo | — |
| W4 `allow(clippy::panic)` | Resolvido | `clippy.toml:3` `allow-panic-in-tests = true`, e os 6 atributos saíram. `tests/import_corpus.rs:10-13` usa `#![allow(clippy::panic, reason = …)]`, porque os helpers fora de `#[test]` não entram na opção |
| W5 clean code | (a–c) no todo; (d) fora do todo | Ver W2 acima |
| W6 YAGNI | No todo | — |
| W7 supply chain | Resolvido | `cargo audit --deny warnings`: exit 0 |
| W8 serviço | **Aberto, sem registro** | Ver W1 acima |
| W9 processo | (d) resolvido; (a–c) histórico | `import_corpus.rs:25-30`: `$BEZEL_IMPORT_CORPUS` ou `../turx` ao lado do repositório. O teste do DoD passou sem nenhuma variável `BEZEL_*` no ambiente |

## DoD Checklist (gate 8)
| # | Criterion | Source | Type | Status | Evidence |
|---|---|---|---|---|---|
| 1 | `cargo test --workspace` exits 0 | PROJECT | Auto | PASS | `cargo test --workspace --locked && echo OK`: `OK`, com 361 passed, 0 failed e 2 ignored |
| 2 | Coverage >= 80% of lines | PROJECT | Auto | PASS | `cargo llvm-cov --workspace --locked --fail-under-lines 80 --summary-only && echo OK`: `OK`, TOTAL 95.03% lines |
| 3 | No `TODO`/`FIXME` without linked issue | PROJECT | Auto | PASS | comando `git grep` do DoD: `OK` |
| 4 | Every element kind renders to the expected pixels in a golden test | CONTEXT | Auto | PASS | `cargo test -p bezel-render --locked -- --exact tests::every_element_kind_matches_its_golden`: `OK`. Os goldens não mudaram desde `620fc65` |
| 5 | A theme survives save → load unchanged in zip and folder form | CONTEXT | Auto | PASS | `cargo test -p bezel-themes --locked -- --exact native::tests::round_trip_zip_and_folder`: `OK` |
| 6 | Every installed TURZX theme and every Python theme imports without error | CONTEXT | Auto | PASS | `cargo test -p bezel-themes --locked --test import_corpus -- --ignored --exact imports_the_local_corpus`: `OK`. Com `--nocapture`: "imported 53 TURZX and 73 Python themes; 0 failed", pelo caminho padrão novo (`../turx`) e sem variável de ambiente |
| 7 | `bezel render` writes a PNG of the canvas size through the fake sensors | CONTEXT | Auto | PASS | `cargo test -p bezel --locked --test render -- --exact render_writes_a_png_of_the_canvas_size`: `OK` |
| 8 | `bezel run` shows a bundled theme with live sensors on the real 8.8" for a minute without errors | CONTEXT | Manual | MANUAL_REQUIRED | SUMMARY § Hardware validation: 42 frames em 41,5 s, sem erro, com `needReSend:0` e "released" no Ctrl+C. Mas uns 16 s do minuto foram de wake, e falta a checagem visual. Sugestão: com `systemctl --user stop turing-smart-screen.service`, rodar `timeout -s INT 90 bezel -v run turing-8.8-horizontal`, confirmar pelo menos 60 frames e o tema legível e correto, anotar "conferido por <nome> em <data>" no SUMMARY e religar o serviço |
| 9 | CHANGELOG.md updated with entry per release | PROJECT | Manual | MANUAL_REQUIRED | `CHANGELOG.md:8,29-41` (`## [Unreleased]`) traz `bezel render`, `bezel run`, `bezel import` e os temas "Midnight". Confirmar o cabeçalho de versão no `/jdi-ship` |
| 10 | README accurately describes current behavior | PROJECT | Manual | MANUAL_REQUIRED | `README.md:9` (status) e `## Themes` (`README.md:30-51`) descrevem `run`, `render`, `import`, os temas embutidos e a ordem de busca a partir de `$BEZEL_THEMES_DIR`. Falta o uso como serviço (W1). Confirmar na revisão do PR |

## Recommendation
Aprovado, pendente dos itens manuais. O único blocker (B1) está resolvido como pedido:
- a inferência de unidade pelo nome da chave mora em `bezel_core::domain::sensor`;
- o renderer e o Studio chamam `Quantities::quantity`;
- os goldens não mudaram.

Todos os gates automáticos passam:
- build;
- 361 testes;
- 95,09% de linhas;
- fmt e clippy sem `allow` fora de teste;
- `cargo audit` limpo;
- as checagens de hexágono, dispositivo e NRBF;
- os 7 itens Auto do DoD;
- o orçamento de tempo da T-4.2 (3,97 ms contra 15 ms).

Antes do `/jdi-ship`:
1. Registrar o W1: um todo `[release-polish]` para a unit systemd `--user` e a tarefa de logon do Windows
   (D-2026-09-30-render-engine-5), ou um exemplo no README.
2. Acrescentar ao todo `render-engine follow-up` a linha do `let _ = write!` sem razão (W2) e ajustar o doc
   comment de `quantity_of` (W4a) quando mexer no arquivo. Rodar `npx -y jdi-cli render` (W4b).
3. Repetir a execução na 8.8" com pelo menos 60 s de frames e checagem visual (DoD 8), e confirmar CHANGELOG e
   README no `/jdi-ship` (DoD 9 e 10).
