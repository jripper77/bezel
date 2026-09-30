# Phase 4: Review  (slug: render-engine)

**Verdict:** BLOCKED

> Revisão em modo `verify` (2026-09-30), `HEAD` = `fa538c6`. Durante a revisão entrou `c7476ae`, que só mexe em
> `.jdi/phases/storage-video/PLAN.md`. Os commits da fase são `58e2518`, `73bcd39`, `96f7d4b`, `6e6cbc8`, `d3a4158`,
> `504ab76` e `46c903f`. Nenhum commit posterior alterou `crates/` ou `themes/`. A fase rodou fora do `/jdi-do`, com a
> T-4.5 feita pelo `jdi-doer-bezel` num worktree (o SUMMARY declara isso). Por isso julguei o código e os artefatos
> contra PLAN, CONTEXT (DoD), PROJECT e as decisões D-1 e D-2026-09-30-render-engine-1..5.
> Nenhum comando abriu a tela real, que continua com o `turing-smart-screen.service`. Só rodei `bezel` com `--fake`,
> o teste de corpus `--ignored` do DoD (lê só arquivos locais) e, para conferir o critério da T-4.2, o teste de
> tempo `--ignored` do `bezel-render` em release, que não usa hardware.

## Gates
| Gate | Status | Details |
|---|---|---|
| Build | PASS | `cargo build --workspace --locked`: ok, lock aceito |
| Tests | PASS | 360 passed, 0 failed, 2 ignored (timing do renderer e corpus local; nenhum de hardware). O SUMMARY registrou 351 no fim da T-4.5, e os commits do `studio-app` depois dela acrescentaram testes, então não houve queda |
| Coverage | PASS | 95.08% lines (TOTAL, sem `main.rs`/`build.rs`); 95.03% com o comando do DoD (sem filtro) |
| Lint | PASS (com WARN) | `cargo fmt --check` e `clippy --all-targets -D warnings` limpos. Os 6 `#[allow(clippy::panic)]` de `bezel-themes/src/import/*.rs` são de módulo de teste: a isenção fica confirmada, com normalização sugerida (W4) |
| Hexagonal/Safety/Protocol/Hygiene | **BLOCK** | 5.1–5.11: o B1 é uma regra de domínio implementada no adapter de renderização. O resto passa, com WARN em 5.11 |
| Consistency | **BLOCK** | O B1 contradiz D-2026-09-30-sensors-1 e D-2026-09-30-render-engine-1. W1, W8 e W9 são de processo e de escopo. Critério de tempo da T-4.2 cumprido: média de 4,15 ms contra o limite de 15 ms |
| UI Validation | SKIPPED | `frontend.has_frontend` ausente no PROJECT.md; a fase não tem UI |
| DoD | PENDING MANUAL | 7/7 itens Auto PASS; 3 itens Manual pendentes |

### Detalhe do gate 5
| Check | Resultado |
|---|---|
| 5.1 dependências do core | PASS: só `thiserror` |
| 5.2 I/O, threads, cfg de plataforma no core | PASS: nada encontrado. `domain/clock.rs` recebe `LocalTime` como valor, e quem lê o relógio é o adapter (`bezel-cli/src/clock.rs`) |
| 5.3 ports | PASS: `FrameRenderer`, `ThemeStore` e `RenderContext` ficam em `bezel_core::ports` (`ports/mod.rs:57-98`), e nada é implementado no core. Traits fora do core: `Pace` (`bezel-cli/src/live.rs:26`, desta fase) é auxiliar interno do laço do `run`, que o core não usa. `Wire`, `Pause` e `Clock` são de fases anteriores |
| 5.4 adapters só na composição | PASS: `SkiaRenderer`, `SystemSensors` e `FsThemeStore` são construídos só em `crates/bezel-cli/src/main.rs` e em `apps/bezel-studio/src-tauri/src/lib.rs`. A lib do CLI recebe `&dyn ThemeStore`, `&mut dyn FrameRenderer` e `&mut dyn SensorSource` (`Rendering`, `lib.rs:241-254`) |
| 5.5 `unsafe` | PASS: `#![forbid(unsafe_code)]` em `bezel-render`, `bezel-themes` e `bezel-cli`. A única ocorrência é a string `"unsafe asset path"` (`native.rs:39`) |
| 5.6 panics fora de teste | PASS: todas as ocorrências da fase estão em `#[cfg(test)]`. `golden.rs` e `testkit.rs` só compilam em teste (`bezel-render/src/lib.rs:31-34`) |
| 5.7 escrita no dispositivo | PASS: nenhum `Confirm::Yes` e nenhum `open` de transporte em teste. O `bezel run` só faz `set_orientation`, que no rev C gira no host sem enviar comando (`turing_rev_c.rs:152-158`), `present` e `release`. Isso segue D-2026-09-30-device-protocols-2 |
| 5.8 fidelidade de protocolo | PASS: todo encoder de `protocol/` tem teste. Conferi o parser NRBF desta fase contra `docs/reverse-engineering/themes-turzx.md:42-71`: `0x0c` BinaryLibrary, `0x05` ClassWithMembersAndTypes, `0x0f` ArraySinglePrimitive (id, tamanho, tipo 2 = Byte), `0x0d` ObjectNullMultiple256 (contagem de 1 byte) e `0x0b` MessageEnd batem com `nrbf.rs:587-619` |
| 5.9 caminhos/plataforma no core | PASS: os acertos são comentários de doc e dados de teste de fases anteriores (`device.rs:75`, `discovery.rs:11,248-309`, `sensor.rs:7,137,180`). Nenhum caminho de código |
| 5.10 comandos Tauri | N/A nesta fase (`list_fonts`, `get_autostart` e `set_autostart` ficam para a revisão do `studio-app`) |
| 5.11 supply chain / segredos | WARN (W7): o `cargo audit` só traz os 2 avisos já conhecidos. Nenhum segredo |

## Blockers
- **B1. Regra de domínio no adapter de renderização (hexágono, regra 14; D-2026-09-30-sensors-1; D-2026-09-30-render-engine-1).**
  `crates/bezel-render/src/quantity.rs:10-57` (`quantity_of`, `by_segments`, `by_last_segment`) decide o que cada
  chave de sensor mede: `cpu.usage` é %, `hwmon.<chip>.fan2` é RPM, `in3` é volt, `disk.<m>.read` é B/s, e assim
  por diante. `crates/bezel-render/src/renderer.rs:183-186` usa essa função quando o catálogo não tem a chave (o
  catálogo falhou, ou o tema usa uma chave que nenhum provider publica). O mapeamento é `SensorKey → Quantity`,
  core → core: não traduz nada para pixels. D-2026-09-30-sensors-1 diz que "formatting, units, °F and byte
  multiples live in the core", e D-2026-09-30-render-engine-1 diz que "pixels are the renderer adapter". O próprio
  core promete esse comportamento (`bezel-core/src/domain/sensor.rs:190`, "every unit then comes from the key's
  well-known name"), mas quem o implementa é o adapter. Assim, outro `FrameRenderer`, ou o inspetor do Studio
  mostrando um valor formatado, teria de duplicar a regra. Ela nasceu nesta fase (`96f7d4b`, `73bcd39`).
  **Correção (mecânica):** mover `quantity_of` e os testes de `quantity.rs:59-97` para `bezel_core::domain::sensor`,
  ao lado de `keys` (por exemplo `Quantity::of_key(&SensorKey)`), e dar a `Quantities` um método que já faça o
  fallback (por exemplo `Quantities::of(&self, key) -> Quantity`). O `text_of` do renderer passa então a chamar o
  core. O comportamento não muda, e os goldens devem continuar iguais.

## Warnings
- **W1. Importadores fora de uma porta:** `crates/bezel-cli/src/theme.rs:17,161` chama
  `bezel_themes::import::import_path` direto. Além disso, `theme.rs:18,69-77` decide o que é um tema nativo com
  `MANIFEST`/`EXTENSION` do adapter de temas: o adapter de entrada conhece o layout de arquivo que a porta
  `ThemeStore` deveria esconder. O SUMMARY declara a escolha ("não há porta de importação no core"). Não bloqueei
  porque o módulo `import` não implementa porta nenhuma: é conversão da entrada do usuário. Mesmo assim, é uma
  exceção ao D-1 sem decisão registrada, e o Studio faz o mesmo (`apps/bezel-studio/src-tauri/src/backend.rs:18`).
  Há dois caminhos: registrar um D-XX que aceite "importers are a conversion library the driving adapters call",
  ou criar uma porta dirigida (por exemplo `ThemeImporter`, que devolve tema, assets e avisos em tipos do core),
  implementada pelo `bezel-themes` e injetada no `main.rs`.
- **W2. DRY (conhecimento duplicado pela T-4.5):**
  - (a) `crates/bezel-cli/src/clock.rs:1-55` é cópia literal de `apps/bezel-studio/src-tauri/src/clock.rs`, testes
    inclusive. `language_of` (locale `pt*` → `PortugueseBr`) é regra pura e pode ir para o core
    (`Language::from_locale`).
  - (b) `is_native` (`bezel-cli/src/theme.rs:69`) e `is_native_theme` (`apps/.../library.rs:97`) já divergem: só o
    CLI aceita `theme.json`. O lugar dessa regra é `bezel_themes::native`.
  - (c) `WARM_UP = 250 ms` está definido duas vezes no mesmo crate (`bezel-cli/src/sensors.rs:21` e
    `bezel-cli/src/theme.rs:24`). O mínimo de 0,25 s aparece ainda em `sensors.rs:24` (`MIN_INTERVAL`),
    `live.rs:20` (`MIN_REFRESH`) e `apps/.../backend.rs:454`: são mais de três ocorrências do "menor intervalo de
    amostragem útil".
- **W3. Regra "o tema cabe no painel" em dois adapters de entrada:** `bezel-cli/src/live.rs:101-112` e
  `apps/bezel-studio/src-tauri/src/studio.rs:263-272` comparam, cada um do seu jeito, o canvas ou o frame com
  `model.panel.in_orientation(o)`. Vale uma função no core (por exemplo `Theme::fits(panel)`), que as duas
  entradas chamam.
- **W4. Gate 4, pendência de `device-protocols` W8 e `sensors` W11:** os 6 `#[allow(clippy::panic)] // a failing
  test panics` em `bezel-themes/src/import/{colors.rs:238,mod.rs:282,nrbf.rs:961,python_yaml.rs:1073,
  turzx.rs:1183,yaml.rs:251}` ficam sobre `#[cfg(test)] mod tests`, e cada um traz a razão na mesma linha.
  **Isenção confirmada** (é código de teste, que o filtro do gate pretende isentar). Para normalizar sem nenhuma
  linha `// reason:`, basta pôr `allow-panic-in-tests = true` no `clippy.toml`, que já tem `allow-unwrap-in-tests`
  e `allow-expect-in-tests` (a opção existe: `cargo clippy --explain panic`), e apagar os 6 atributos.
- **W5. Clean code:**
  - (a) Tipos de erro: `bezel-themes/src/native.rs:26-28` põe todo erro de arquivo ou JSON em
    `BezelError::Transport`. Reproduzi: `bezel --fake render <pasta com theme.json quebrado>` imprime
    "transport error: theme.json: EOF…". Já `native.rs:165-168` devolve `ScreenNotFound` para tema ausente
    ("screen not found: theme not found: …"). Para formato e schema, `InvalidInput` é o variant certo.
  - (b) As tags de registro MS-NRBF entram como inteiros crus em `nrbf.rs:587-625` (`12`, `1`, `4`, `5`, …), sem
    constantes nomeadas. Os valores conferem com a spec.
  - (c) Funções longas: `turzx.rs:917` `bar` (91 linhas), `python_yaml.rs:915` `radial` (85) e `:783` `cldr` (71).
    Os mapeadores de `dto.rs:599,715` são `match` exaustivos e estão aceitáveis.
  - (d) `let _ = write!(log, …)` em `live.rs:157,164` e `theme.rs:215` descarta erro de escrita sem o comentário que
    `main.rs:131` tem.
- **W6. YAGNI:** `Theme::sensor_keys` (`bezel-core/src/domain/theme.rs:447`) diz na doc "for sampling only what is
  needed", mas nada amostra por ele. `Theme::element_mut` (`:442`), `Theme::next_id` (`:432`) e `BoxF::translated`
  (`:47`) também só são chamados em testes. Ou ganham chamador (o editor), ou saem.
- **W7. Supply chain (5.11), pendência de W9/W11:** agora o `ttf-parser 0.25.1` sem manutenção (RUSTSEC-2026-0192)
  chega pelo `fontdb 0.23`, que é dependência direta do `bezel-render` desta fase, e pelo `cosmic-text 0.19`. Falta
  uma entrada justificada em `.cargo/audit.toml`, com condição de reavaliação (fontdb/cosmic-text trocando para
  skrifa/read-fonts). Já o `yoke-derive 0.8.3` yanked se resolve com `cargo update -p yoke-derive`: o dry-run mostra
  0.8.3 → 0.8.4.
- **W8. D-2026-09-30-render-engine-5, parte de serviço:** o `bezel run` trata Ctrl+C e SIGTERM (`ctrlc` com
  `termination`, `main.rs:74-77`) e devolve a tela, mas não há unidade systemd de usuário, tarefa de logon do
  Windows nem instrução no README para "run as a systemd user service or Windows logon task". O PLAN também não
  tinha tarefa para isso. Registrar como todo para a `release-polish`, ou documentar um exemplo de unit
  `--user` no README.
- **W9. Processo:**
  - (a) Fase fora do `/jdi-do` (declarado).
  - (b) T-4.1, T-4.2 e T-4.3 têm dois commits cada, e `d3a4158` é compartilhado pela T-4.2 e pela T-4.3, contra a
    regra "1 task = 1 commit".
  - (c) `73bcd39` mexe em `crates/bezel-cli/src/sensors.rs` e `crates/bezel-core/src/domain/sensor.rs`, fora da
    lista da T-4.1 (não declarado). Os arquivos do `bezel-render` alterados pela T-4.5 estão declarados.
  - (d) `crates/bezel-themes/tests/import_corpus.rs:17-18` fixa `/home/slipalison/...` num repositório público
    (dá para sobrescrever por variável de ambiente).

Pendências resolvidas: `ThemeRuntime` agora tem 100% das linhas cobertas (`bezel-core/src/app/runtime.rs`, 52/52),
exercitado por `crates/bezel-cli/tests/runtime.rs` (4 testes com `FakeSensors`, `SkiaRenderer` e `FakeConnector`)
e pelo `live.rs`.

Cobertura dos arquivos da fase: `bezel-render` de 91,48% a 100% (`text.rs` é o menor), `bezel-themes` de 91,87% a
100%, core (`theme.rs`, `clock.rs`, `history.rs`, `runtime.rs`) de 99,53% a 100%, e `bezel-cli` com `theme.rs`
99,47%, `live.rs` 100% e `clock.rs` 100%.

Prévias: `BEZEL_THEME_PREVIEWS=… cargo test --release -p bezel --test bundled_themes` gerou os 12 PNGs, com os 6
temas em en e pt-BR. Conferi o 8.8" horizontal, o 3.5" horizontal e o 2.1" redondo (pt-BR): estão legíveis, sem
sobreposição e com as unidades certas. Também importei e renderizei com `--fake` o `AMD.turtheme` (9 elementos,
fonte Bahnschrift trocada por Inter, com aviso) e o `Cyberpunk-net` do Python (320x480). As duas saídas saíram
fiéis.

## DoD Checklist (gate 8)
| # | Criterion | Source | Type | Status | Evidence |
|---|---|---|---|---|---|
| 1 | `cargo test --workspace` exits 0 | PROJECT | Auto | PASS | `cargo test --workspace --locked`: exit 0, 360 passed, 0 failed, 2 ignored |
| 2 | Coverage >= 80% of lines | PROJECT | Auto | PASS | `cargo llvm-cov --workspace --locked --fail-under-lines 80 --summary-only`: `OK`, TOTAL 95.03% lines |
| 3 | No `TODO`/`FIXME` without linked issue | PROJECT | Auto | PASS | comando `git grep` do DoD: `OK` |
| 4 | Every element kind renders to the expected pixels in a golden test | CONTEXT | Auto | PASS | `cargo test -p bezel-render --locked -- --exact tests::every_element_kind_matches_its_golden`: `OK` |
| 5 | A theme survives save → load unchanged in zip and folder form | CONTEXT | Auto | PASS | `cargo test -p bezel-themes --locked -- --exact native::tests::round_trip_zip_and_folder`: `OK` |
| 6 | Every installed TURZX theme and every Python theme imports without error | CONTEXT | Auto | PASS | `cargo test -p bezel-themes --locked --test import_corpus -- --ignored --exact imports_the_local_corpus`: `OK`. Com `--nocapture`: "imported 53 TURZX and 73 Python themes; 0 failed", que cobre os 26 `.turtheme` de `theme/4801920` mais `restore/` e `visual/panel_common` |
| 7 | `bezel render` writes a PNG of the canvas size through the fake sensors | CONTEXT | Auto | PASS | `cargo test -p bezel --locked --test render -- --exact render_writes_a_png_of_the_canvas_size`: `OK` |
| 8 | `bezel run` shows a bundled theme with live sensors on the real 8.8" for a minute without errors | CONTEXT | Manual | MANUAL_REQUIRED | SUMMARY § Hardware validation: `timeout -s INT 60 bezel -v run turing-8.8-horizontal` mostrou 42 frames em 41,5 s, sem erro, com `needReSend:0` e "released" no Ctrl+C. Mas uns 16 s do minuto foram de wake, então o tema ficou menos de um minuto na tela, e falta a checagem visual. Sugestão: com `systemctl --user stop turing-smart-screen.service`, rodar `timeout -s INT 90 bezel -v run turing-8.8-horizontal`, confirmar pelo menos 60 frames e o tema legível e correto, anotar "conferido por <nome> em <data>" no SUMMARY e religar o serviço |
| 9 | CHANGELOG.md updated with entry per release | PROJECT | Manual | MANUAL_REQUIRED | `CHANGELOG.md:29-41` (`## [Unreleased]`) já traz `bezel render`, `bezel run`, `bezel import` e os temas "Midnight". Confirmar o cabeçalho de versão no `/jdi-ship` |
| 10 | README accurately describes current behavior | PROJECT | Manual | MANUAL_REQUIRED | `README.md:7-10` (status) e a seção `## Themes` (`README.md:30-51`) descrevem `run`, `render`, `import`, os temas embutidos e a busca `$BEZEL_THEMES_DIR` → `share` → `~/.local/share`. Falta mencionar o uso como serviço (W8). Confirmar na revisão do PR |

## Recommendation
Bloqueado por um único item, pequeno e mecânico. A cobertura de quantidade por nome de chave (B1) é regra de
domínio no adapter de renderização e contradiz D-2026-09-30-sensors-1 e D-2026-09-30-render-engine-1: basta levá-la
para `bezel_core::domain::sensor` e fazer o renderer chamá-la. Todo o resto passa: build, 360 testes, 95,08% de
linhas, fmt/clippy, as checagens de segurança de dispositivo e de fidelidade NRBF, os 7 itens Auto do DoD e o
limite de tempo da T-4.2 (4,15 ms contra 15 ms).

Ordem sugerida:
1. Corrigir o B1 num commit `refactor(render-engine): …` e rodar `/jdi-verify render-engine` de novo.
2. Na mesma leva, se couber: W4 (`allow-panic-in-tests = true`), `cargo update -p yoke-derive` e a entrada do
   `ttf-parser` no `.cargo/audit.toml` (W7), além do W2(c) (um só `WARM_UP`).
3. Decidir o W1 (D-XX aceitando importadores como biblioteca dos adapters de entrada, ou porta `ThemeImporter`)
   antes da revisão do `studio-app`, que tem o mesmo padrão.
4. Levar W2(a/b), W3, W5, W6 e W8 para `.jdi/todos` (`render-engine follow-up` e `release-polish`).
5. Repetir a execução na 8.8" por pelo menos 60 s de frames, com checagem visual (DoD 8), e confirmar CHANGELOG e
   README no `/jdi-ship`.
