# Phase 11: Review  (slug: gif-sticker-search)

**Verdict:** APPROVED_WITH_WARNINGS

> Revisão em modo `verify`, rodada 2, iteração 5 (10ª no total, depois do AUTO-RESET 1) do loop autônomo
> (`/jdi-issue`). Branch `jdi/gif-sticker-search`, `HEAD` = `87d710c`, igual ao remoto do início ao fim (conferido
> de novo antes da linha 7). Escopo: `git diff origin/main...HEAD` inteiro (104 arquivos, 59 commits), com atenção aos
> commits de código da iteração:
> - `6f4afe0` (`fix(gif-sticker-search): key file's JSON holds a KlipyKey, no String`): `KeyJson.key` passa a ser
>   `KlipyKey`, lido por `read_key` (`deserialize_with`, visitor `KeyText` direto para `KlipyKey::parse`, erro de
>   texto fixo) e gravado por `write_key` (`serialize_with`, o único `expose_secret` do arquivo). O argumento da
>   janela passa pelo mesmo visitor. A guarda troca o literal de `save` por `write_key`/`serialize_str`, e
>   `key_never_reaches_the_window` carrega 12 arquivos de chave inutilizáveis;
> - `2d1272d` (`fix(gif-sticker-search): fixed codes say why the app did not start`): 8 `DiagCode` novos (46 no
>   total), `start_failure`/`refresh_loop_failure`/`restart_failure` por variante ou `ErrorKind`, teste
>   `tests::a_failed_start_says_which_part_failed` e a seção "The app does not open" / "O aplicativo não abre" do
>   `troubleshooting.md`;
> - `87d710c`: D-13 e SUMMARY.
>
> Li D-1..D-13, PROJECT, CONTEXT, PLAN, SUMMARY, LOOP e o REVIEW da rodada 2 iter 4, inclusive a seção do crítico.
> `npx -y jdi-cli render` rodou antes e não mudou nenhum arquivo versionado.
>
> - **Números:** todos vêm das minhas execuções (`CARGO_TARGET_DIR=target/review`, `cargo llvm-cov clean --workspace`
>   antes da cobertura). Nenhum veio do SUMMARY. Extraí os `Verify:` do Markdown por script, sem editar, e rodei
>   cada um com `bash`.
> - **Rede e hardware:** nada tocou `/dev/ttyACM*`, o app instalado, os servidores do KLIPY nem o arquivo da chave.
>   Playwright com `BEZEL_E2E_PORT=1442`. A única execução de binário foi a do build de revisão
>   (`target/review/debug/bezel-studio`, copiado para o scratchpad), sem sessão gráfica, sem D-Bus e com `HOME`
>   temporário: ele cai antes de iniciar qualquer plugin (ver W1).
> - **Mutações:** em cópias por `git archive` de `87d710c` no scratchpad (`rev10/mut/<mutante>`). Cada cópia tem um
>   `git init` próprio, com `origin/main` = `e527240` buscado do repositório, para a linha 3 rodar como está escrita,
>   e um `CARGO_TARGET_DIR` próprio (`target/rev10mut/<mutante>`). O repositório não foi alterado. Controle sem
>   mutação (`ctl`): a linha 3 dá `OK`.

## Gates
| Gate | Status | Details |
|---|---|---|
| Build | PASS | `cargo build --workspace --locked`: exit 0 |
| Tests | PASS | **943 passed, 0 failed, 12 ignored (hardware, ffmpeg real, KLIPY real)**, em 39 binários. É 1 a mais que na iter 4 (942): `tests::a_failed_start_says_which_part_failed`. O `key_never_reaches_the_window` cresceu sem mudar de nome. Studio lib: **163** no Linux. Bate com o SUMMARY (943) |
| Coverage | PASS | **94.79%** de linhas (TOTAL, sem `main.rs`/`build.rs`), exit 0. Pelo comando do DoD do PROJECT: **94.74%**. Studio: `diag.rs` 100.00%, `lib.rs` 93.60%, `gifs.rs` 96.87%, `gifs/key.rs` 92.13%, `gifs/asked.rs` 50.00%, `commands.rs` 10.72% (invólucros finos, como antes) |
| Lint | PASS | `cargo fmt --all --check` e `cargo clippy --workspace --all-targets --locked -- -D warnings`: exit 0. Clippy local com `--target x86_64-pc-windows-msvc`, sem `bezel-studio`/`bezel-klipy`: exit 0 (esses crates não mudaram na iteração). Nenhum `allow` novo: os 4 de `rtss.rs` são anteriores à fase e têm `reason =` |
| Hexagonal/Safety/Protocol/Hygiene | PASS | 5.1–5.11 sem achado novo. A iteração só mexe no studio e nas docs. 5.5: `unsafe` só em `rtss.rs` (anterior); o studio tem `#![forbid(unsafe_code)]`. 5.6: nenhum `unwrap`/`expect`/`panic!` novo em produção (a guarda continua exigindo isso). 5.10: nenhum comando novo. `cargo audit`: exit 0 (620 crates). Nenhum segredo. O `Cargo.lock` não mudou desde `779a1ed` |
| Consistency | PASS | 59 commits: 58 com escopo `gif-sticker-search` e 1 `chore(jdi)`. `6f4afe0` implementa a D-13 como escrita (arquivo com `KlipyKey`, `read_key`/`write_key`, 12 arquivos). Não contradiz D-3, D-10, D-11 nem D-12. `2d1272d` segue a D-12 (só `DiagCode`, escolhido por variante ou `ErrorKind`, nunca por texto). Ver W1 sobre o alcance real dos códigos novos |
| UI Validation | PASS | `npm ci` ok. `npm run test:unit`: **231/231**, 99.94% de linhas. Playwright completo: **208/208** (claro/escuro × pt-BR/en, axe, `watchErrors`). A iteração não mudou a UI (`git diff 779a1ed..HEAD -- apps/bezel-studio/src apps/bezel-studio/tests`: vazio) |
| DoD | PASS | As 10 linhas Auto passam. A linha 7 rodou depois que o CI do `HEAD` terminou. As 2 Manual são da release |

## Itens da rodada 2, iter 4
| Item | Estado | Evidência |
|---|---|---|
| Crítico: `KeyJson.key` era `String`; um erro de `KeyFile::load` que cita o valor lido (`format!("… (found {found:?})")`) devolvia a chave à janela por `klipy_key` | **CLEARED** | Reapliquei o mutante (`crit`) em `KeyFile::load` (`gifs/key.rs:242-246`): `let found = saved.as_ref().map(\|s\| s.key.clone());` e `format!("{NOT_A_KEY_FILE} (found {found:?})")`. Compila e passa: linha 3 **`OK`**, guarda ok, `key_never_reaches_the_window` ok. O valor citado agora é `Some(KlipyKey(..))`, que não traz nada da chave. O mesmo mutante sobre o `key.rs` de `779a1ed` (`old`), com o teste de `HEAD`: linha 3 **exit 1**, porque o teste acusa `found Some("Zq7Xw2Vb9Nm4…")` em `gifs/tests.rs:563` |
| W1 (revisor): os 3 diagnósticos de terminal perderam o texto do erro, sem registro para o usuário | **CLEARED**, com ressalva | O texto continua de fora, como a D-12 quer. A troca agora está registrada na D-13 e no `troubleshooting.md` (en/pt-BR). As causas que se distinguem têm código fixo: `PluginNotStarted`, `FoldersNotFound`, `TrayNotAdded`, `RefreshLoopNoResources`, `DmabufRestartNoFile` e `DmabufRestartDenied` são alcançáveis e saem no terminal. **Ressalva:** 2 dos códigos novos nunca chegam ao terminal no desktop, e o guia descreve essas linhas (novo W1) |
| W2 (revisor): o SUMMARY não listava os arquivos da migração | **CLEARED** | "Files modified" agora traz `src-tauri/src/**` com gifs, `gifs/{key,asked,tests}`, `diag`, commands, lib, backend, dto, messages "e os módulos que passaram a logar pelo `diag`", e `troubleshooting.md` nas docs. O SUMMARY não repete o "36" do commit. O total de testes (943) bate |

## Mutações sobre a chave (linha 3)
Cada mutante foi aplicado em `KeyFile::load` e rodado com a linha 3 como está escrita e, à parte, com
`gifs::tests::key_never_reaches_the_window`.

| Mutante | O que cita no erro | Linha 3 | Leitura |
|---|---|---|---|
| `ctl` | nada (sem mutação) | `OK` | controle |
| `crit` | `{found:?}` da chave do `KeyJson` (o do crítico) | `OK` | sai `Some(KlipyKey(..))`, sem vazamento |
| `critd` | `{found}` (`Display`) | exit 1 | não compila: `E0277`, `KlipyKey` não implementa `Display` |
| `bytes` | `String::from_utf8_lossy(&bytes)` | exit 1 | o teste acusa `Zq7Xw` no `fileError` (`tests.rs:563`) |
| `value` | o arquivo lido como `serde_json::Value` | exit 1 | idem: `Object {"key": String("Zq7Xw2…")}` |
| `serr` | o erro do `serde_json` (`map_err(\|e\| UiError::file(path, e))?`) | `OK` | sem vazamento: o `read_key` devolve erro de texto fixo, e os outros erros do serde não citam valor de texto |
| `old` | o mutante do crítico sobre o `key.rs` de `779a1ed` | exit 1 | `found Some("Zq7Xw2…")`: o teste novo pega o caso do crítico |

## Compatibilidade do arquivo da chave e contrato da janela
- **Mesmo formato desde a primeira versão.** De `4449005` (T-4) a `1b66167`, o arquivo é `to_vec_pretty` de um
  struct `camelCase` com `key: String` e `customer_id: String`, nessa ordem. Antes de `f6f4030` esse struct era o
  próprio `SavedKey`; depois, `KeyJson`. O `write_key` grava `serialize_str` no mesmo campo.
- **Provado numa cópia (`compat`)**, com um teste temporário em `gifs/key.rs` e uma réplica exata do `KeyJson`
  antigo:
  - um arquivo gravado como a build anterior gravava carrega, com a mesma chave e o mesmo customer id;
  - o `KeyFile::save` atual grava **os mesmos bytes**;
  - o struct antigo lê o que a build atual grava;
  - as formas compacta, com campos em outra ordem ou com um campo extra continuam carregando.

  O teste passou, e a linha 3 deu `OK` nessa cópia. A validação (`is_valid_key`) não mudou, então nenhuma chave
  gravada antes fica recusada.
- **Contrato da janela inalterado:**
  - `bridge.js:325` ainda chama `invoke('save_klipy_key', { key })`;
  - `messages.rs`, `commands.rs`, a UI e o i18n não mudaram desde `779a1ed`;
  - `tests::the_window_sends_the_key_as_before` (IPC do mock runtime, só fora do Windows) passa: chave inválida =
    `invalidInput` sem citá-la, chave válida salva e `last4` = `cdef`;
  - arquivo inutilizável continua `fileError`, com o motivo fixo `it does not hold a KLIPY key`.

## Docs de problemas × códigos novos
- **As frases da tabela batem com o código, palavra por palavra.** Cada uma das 11 linhas (en e pt-BR) é
  `bezel-studio: ` + `DiagCode::text()` (`diag.rs:194-212`), que é o formato do canal `Terminal`. O
  `DmabufRestartDenied` usa continuação `\` na string e resulta em "…program file was not allowed", igual ao guia.
- **As duas línguas têm as mesmas linhas.** `check-docs.sh` passa (linha 6).
- **O que não bate é quando cada linha aparece** (W1).

## Blockers
- nenhum

## Warnings
- **W1 — Dois códigos de "não abriu" nunca saem no desktop, e o guia descreve linhas que o usuário não verá.**
  - **Onde no código:**
    - `lib.rs:137` (`tauri::Error::Runtime` → `NoWindow`) e `lib.rs:138` (`tauri::Error::Setup` → `SetupFailed`),
      usados por `main.rs:19`;
    - `lib.rs:355` e `lib.rs:367` (`FoldersNotFound` e `TrayNotAdded` no `setup`).
  - **Onde nas docs:** `docs/user/troubleshooting.md:132-135` e `docs/user/pt-BR/troubleshooting.md:142-145`.
  - **Por quê:** no Tauri 2.12, `Builder::run` é `self.build(context)?.run(…); Ok(())` (`tauri-2.12.0/src/app.rs:2616-2619`).
    O `setup` roda dentro do loop de eventos, no `Ready`, e uma falha ali vira
    `panic!("Failed to setup app: {e}")` (`app.rs:1443-1445`), sem nunca voltar como `Err` ao `main`. Isso vale para
    o closure de setup do studio e para a janela `main` do `tauri.conf.json` (criada no mesmo `setup`,
    `app.rs:2691-2699`). Por isso:
    - `SetupFailed` é inalcançável. Depois de `the app's folders were not found` / `the tray icon was not added`, a
      linha seguinte é o panic do Tauri, que traz o texto do erro, e não `…its setup failed`.
    - `NoWindow` também é, na prática. O `R::new` do `tauri-runtime-wry` 2.12 sempre devolve `Ok`, e a falha do GTK
      é um `expect` no tao (`tao-0.37.1/src/platform_impl/linux/event_loop.rs:217`). A webview que não cria (o caso
      "WebView2" do guia) cai no mesmo `setup` e vira panic.
    - Conferido no build de revisão, sem sessão gráfica (`GDK_BACKEND=x11`, sem `DISPLAY`): saída
      `thread 'main' panicked at …tao-0.37.1/…/event_loop.rs:217:53: Failed to initialize gtk backend!`, exit 101 e
      nenhuma linha `bezel-studio:`.

    O teste `a_failed_start_says_which_part_failed` só prova o mapeamento das funções, não o caminho real.
  - **Gravidade:** baixa. Não há vazamento: o `setup` não lê a chave, e o panic é texto do Tauri/tao, fora da
    D-12. O relato de bug continua tendo causa, e os códigos de plugin, pastas, bandeja, laço e reinício são reais.
    O que fica errado é a documentação de 2 linhas e o "a linha seguinte/anterior".
  - **Correção sugerida**, uma das duas:
    - (a) ajustar as duas páginas para o que aparece de fato: o panic do Tauri `Failed to setup app` depois da linha
      de pastas ou bandeja, e o panic do tao `Failed to initialize gtk backend!` sem sessão gráfica. Depois, tirar
      `NoWindow`/`SetupFailed` ou marcá-los como reserva;
    - (b) instalar no `main` um panic hook que reporte um `DiagCode` fixo (dentro da D-12). Assim as linhas
      documentadas passam a existir, e o texto do erro sai também do panic.

## DoD Checklist (gate 8)
| # | Criterion | Source | Type | Status | Evidence |
|---|---|---|---|---|---|
| 1 | Core: filtro, GIF ≤ 25 MiB, cópia única, não-GIF recusado; sem KLIPY/HTTP | CONTEXT | Auto | PASS | `OK` (`HEAD` = `87d710c`) |
| 2 | `KlipyClient` contra servidor HTTP loopback com JSON gravado | CONTEXT | Auto | PASS | `OK` |
| 3 | Studio e disco: nada sai sem chave/no início, chave privada, itens só da última busca, alpha, fundo animado, excluir nomeia temas; CSP igual | CONTEXT | Auto | PASS | `OK`. Controle `ctl`: `OK`. Mutante do crítico (`crit`): `OK` sem vazamento (`Some(KlipyKey(..))`); `critd` não compila; `bytes`, `value` e `old`: exit 1; `serr`: `OK` sem vazamento; `compat`: `OK` |
| 4 | `Cargo.lock` só ganha pacotes; `ureq` só via `bezel-klipy`; CLI sem `ureq`/`rustls` | CONTEXT | Auto | PASS | `OK` |
| 5 | UI: lógica (debounce, setas, carregar mais, 429), i18n e os 6 testes nomeados nos 4 projetos com axe | CONTEXT | Auto | PASS | `OK` (`# pass 4`, `test:unit` ok, 6 testes × 4 projetos com axe) |
| 6 | Guia en/pt-BR com Privacidade, check-docs e CHANGELOG | CONTEXT | Auto | PASS | `OK` (o `check-docs.sh` passa com as seções novas do `troubleshooting.md`) |
| 7 | CI do Windows verde no HEAD do PR | CONTEXT | Auto | PASS | Esperei o run 37018818777 terminar (`gh run watch --exit-status`: exit 0; `conclusion=success`). Depois rodei a linha como está escrita, com `HEAD` = remoto = `87d710c`: `OK` (`rust-windows` = `success`, **160** testes do studio no Windows) |
| 8 | `cargo test --workspace` exits 0 | PROJECT | Auto | PASS | `OK`; 943/0/12 |
| 9 | Coverage >= 80% of lines | PROJECT | Auto | PASS | `OK`; TOTAL 94.74% |
| 10 | No `TODO`/`FIXME` without linked issue reference | PROJECT | Auto | PASS | `OK` |
| 11 | CHANGELOG.md updated with entry per release | PROJECT | Manual | MANUAL_REQUIRED (release) | `[Unreleased]` cita KLIPY. Evidência sugerida: `## [x.y.z] - <data>` no corte de release |
| 12 | README accurately describes current behavior | PROJECT | Manual | MANUAL_REQUIRED (release) | O README cita a busca e o guia. Evidência sugerida: diff do README revisado no PR |

## CI (run 37018818777, `87d710c`)
- **Jobs:** `conclusion=success`.
  - success: `rust-windows`, `rust-linux`, `node-ui`, CodeQL (actions, rust, javascript-typescript), Varreduras,
    Versao e `Portao`;
  - skipped, como antes: `sonar`, `imagem`, `publicar` e `lancar`.
- **`rust-windows`:**
  - `cargo fmt --all -- --check` e `cargo clippy --all-targets --all-features -- -D warnings` ok;
  - testes (`cargo llvm-cov`): **872 passed, 0 failed, 12 ignored**;
  - unittests do studio: **`160 passed; 0 failed`**. São os 159 da iter 4 mais `a_failed_start_says_which_part_failed`;
  - `key_never_reaches_the_window`, `nothing_in_the_app_forges_an_invocation` e
    `what_the_terminal_shows_is_what_was_printed_before` rodaram ok. Com isso, o `KeyFile` e o `read_key`/`write_key`
    novos foram exercitados no Windows.
- **`rust-linux`:** verde, com 942/0/12 e 163 no studio. Localmente deram 943: o doctest `compile_fail` de `diag.rs`
  roda no `cargo test`, não no `llvm-cov` do CI. Linux (163) − Windows (160) = os 3 testes de mock runtime que só
  rodam fora do Windows (D-8).

## Observações (sem aviso)
- **O IPC continua guardando a chave como texto.** O `CommandItem` do Tauri guarda o argumento da janela como
  `serde_json::Value`, com a chave em texto, antes do `KeyText`. Isso é interno do Tauri e vale para qualquer
  argumento. O "nenhuma `String` da chave" da D-13 vale para o código do app, como ela diz.
- **Dois casos que a linha 3 não cobre, ambos inofensivos:**
  - um `KeyText::visit_str` que trocasse `Ok(parse(text))` pelo idioma `E::invalid_value(Unexpected::Str(text), …)`
    devolveria à janela, pelo `InvalidArgs`, o texto que ela mesma enviou. O `read_key` mascara esse caso no
    arquivo, e nada vaza de volta do disco;
  - um `{"key": <número>}` com o erro do `serde_json` citado e o `read_key` propagando o erro exigiria duas
    mutações ao mesmo tempo, e um número não é uma chave.
- **SUMMARY:** "DoD 7: … (iter 3: 153 testes do studio no Windows)" é histórico e está rotulado como tal. Hoje são
  160. O "bezel-klipy 97%" segue como antes (medi `client.rs` 90.04% e `dto.rs` 99.16%; o crate não mudou).
- **Continuam valendo as observações das iters anteriores:**
  - o timeout de 10 s;
  - `collected_users` lendo a biblioteca;
  - os nomes reservados do Windows;
  - `klipy.json` sem nova tentativa;
  - o proxy;
  - a raiz temporária de teste;
  - o limite declarado da D-12 (arquivo/evento, o `ipcf`);
  - `diag.rs` falando em "the log" sem subscriber;
  - a cobertura de `commands.rs` (10.72%).

## Recommendation
Os itens da rodada 2, iter 4 estão resolvidos:
- **Crítico (`KeyJson.key` como `String`):** reaplicado, o mutante compila, mas cita `Some(KlipyKey(..))`. As
  variantes que citariam a chave por outro caminho (bytes, `Value`) reprovam na linha 3, e `{found}` não compila. O
  mesmo mutante sobre o `key.rs` anterior é pego pelo teste novo.
- **W1:** a troca de segurança está registrada na D-13 e no guia, e as causas alcançáveis têm código fixo.
- **W2:** o SUMMARY lista os arquivos.

O arquivo da chave é compatível byte a byte com o da build anterior, e o contrato da janela (`save_klipy_key {key}`,
`invalidInput`, `fileError`) não mudou. Os gates 1–8 passam. O CI do `HEAD` está verde, com 160 testes do studio no
Windows.

Antes de fechar a fase:
1. **W1:** corrigir as duas páginas do `troubleshooting.md` sobre o que aparece de fato (panic do Tauri depois de
   pastas/bandeja; panic do tao sem sessão gráfica) e tirar ou reservar `NoWindow`/`SetupFailed`. Ou instalar um
   panic hook pelo `diag`, para que as linhas documentadas existam.

No PR, fazer os itens de "Deferred to PR review": T-8 na 8.8", termos do KLIPY e visual.

## DoD Critic (enhanced)

- DoD row «3 | Studio e disco … chave privada»: oca e objetiva — engano realista: copiar o logger `--verbose` da CLI
  para o studio (feature `tracing` do tauri + `tracing-subscriber`, já no lock) passa a linha; com essa feature o
  próprio Tauri grava o corpo de cada IPC no span `ipc::request` (`protocol.rs:41-71`), chave incluída. A guarda só
  conhece os caminhos `log`/`tracing` e não lê as features do Cargo. A mutação da iteração anterior (erro que cita a
  chave lida) agora é inofensiva (`KlipyKey(..)`).
- Demais linhas: provam o critério.

**Verdict:** BLOCKED
