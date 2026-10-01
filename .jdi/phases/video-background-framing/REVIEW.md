# Phase 9: Review  (slug: video-background-framing)

**Verdict:** APPROVED

> Revisão em modo `verify`, iteração 2: re-verificação do Step 6 do `/jdi-issue`, depois da rodada de correção dos
> avisos. Branch `jdi/video-background-framing`, `HEAD` = `007f006` (30 commits desde `main` `ec724d8`).
>
> - **Árvore:** limpa. O único item no `git status` é o `REVIEW.md` anterior, removido pelo orquestrador; o conteúdo
>   dele foi lido em `2fc8ed9`.
> - **Escopo julgado:** os commits `c940531`, `3302c93`, `1790823`, `370ae78`, `c0d0a04`, `668a25e`, `16baa2e`,
>   `4feaa0f` e o SUMMARY (`007f006`). Conferi-os contra os avisos W1 a W3 e os menores da iter 1, contra as decisões
>   D-2026-10-01-video-background-framing-1..7 e contra o DoD do CONTEXT (com a linha 6 mais estrita) e do PROJECT.
>   Também procurei regressões no restante da fase.
> - **Números:** todos saíram das minhas execuções, e nenhum foi copiado do SUMMARY.
> - **Hardware:** nenhum comando abriu a 8.8" real. Não rodei `#[ignore]` de hardware nem `BEZEL_HW_TESTS`. Não
>   toquei em `/dev/tty*`, hidraw, serviços ou `sudo`, e não abri janela.
> - **Dados reais (só leitura):**
>   - Copiei o `Dragon-Ball.bezeltheme` para o scratchpad. Na cópia, com `XDG_*` e `HOME` temporários, rodei
>     `bezel render --fake` e `bezel import`.
>   - O `.bezeltheme` original não mudou: `mtime` 2026-09-30 22:24:47, 4188518 bytes, antes e depois.
>   - O `dragon.mp4` do fabricante (sha256 `419e8543…`) foi só lido, pelos testes ffmpeg gated
>     (`BEZEL_DRAGON_BALL_MP4`).
> - **Playwright:** `BEZEL_E2E_PORT=1464`.

## Gates
| Gate | Status | Details |
|---|---|---|
| Build | PASS | `cargo build --workspace --locked`: exit 0. O lock novo, só com as crates do opener, foi aceito pelo `--locked` |
| Tests | PASS | **866 passed, 0 failed, 11 ignored** (36 binários), contra 861 na iter 1: +5 testes novos e nenhum removido (lista abaixo). No Windows, CI de `007f006`: **802 passed, 0 failed, 11 ignored** (eram 797) |
| Coverage | PASS | **94.40%** lines (TOTAL, sem `main.rs`/`build.rs`), exit 0. Pelo comando do DoD do PROJECT, sem filtro: **94.34%**. `studio.rs` 97.08%, `backend.rs` 98.58%, os dois `framing.rs` em 100%. Nenhum `main.rs` mudou na fase |
| Lint | PASS | `cargo fmt --all --check`: exit 0. `cargo clippy --workspace --all-targets --locked -- -D warnings`: exit 0. Clippy cruzado `x86_64-pc-windows-msvc` (7 crates, `target/wincheck`): exit 0. Os únicos `allow` fora de teste são os 4 de `fps/rtss.rs`, que já existiam, todos com `reason = "…"` |
| Hexagonal/Safety/Protocol/Hygiene | PASS | 5.1 a 5.11 limpos, com os mesmos acertos da iter 1. `cargo audit`: exit 0, 1278 advisories, 608 crates. Detalhe abaixo |
| Consistency | PASS | Os 30 commits usam o escopo `video-background-framing`. Cada correção tem teste, e nenhuma D-XX é contrariada. A crate nova está registrada na D-7 |
| UI Validation | PASS | `npm ci` ok. **176/176** Playwright (claro/escuro × pt-BR/en, axe). `npm run test:unit`: **180/180**, 99.93% de linhas, `video-framing.js` 100%. `i18n.test.mjs`: 13/13. Nenhum JS mudou desde a iter 1 |
| DoD | PASS | As 8 linhas Auto do CONTEXT, com a 6 mais estrita, e as 3 Auto do PROJECT passam como escritas. O CONTEXT não tem Manual. Os 2 Manual do PROJECT são do corte de release |

Testes novos desde a iter 1:
- `backend::tests::the_session_goes_on_while_the_video_is_probed`
- `backend::tests::the_session_goes_on_while_the_poster_is_taken`
- `studio::tests::a_failed_preview_decoder_is_tried_again_a_few_times`
- `studio::tests::a_probe_or_a_poster_comes_back_only_to_what_it_was_for`
- `storage::tests::a_panel_native_theme_video_is_sent_without_ffmpeg`

### Detalhe do gate 5
| Check | Resultado |
|---|---|
| 5.1 dependências do core | PASS: só `thiserror` |
| 5.2 I/O e threads no core | PASS: nada. Nenhum arquivo do core mudou desde a iter 1 |
| 5.3 ports | PASS. Nenhuma impl de porta no core. As traits públicas fora do core são as 6 auxiliares de adapter de antes (`Clock`, `Pause`, `Wire`, `Pace`, `Pictures`, `MediaSetup`). `VideoProbe`, `PosterRetake`, `TakenPoster`, `Probed` e `Wait` são tipos do studio, não traits |
| 5.4 adapters na composição | PASS: nada fora de `main.rs`/`lib.rs` e testes |
| 5.5 `unsafe` | PASS: só os blocos de `fps/rtss.rs` com `// SAFETY:`, sem mudança. O acerto de `native.rs:39` é uma string |
| 5.6 panics | PASS. Nos `.rs` alterados nesta rodada (`backend.rs`, `studio.rs`, `storage/tests.rs`, `live.rs`, `dto.rs`), não há `unwrap`/`expect`/`panic!` antes do `#[cfg(test)]`. Os de `backend.rs` estão de `:962` em diante, e os de `studio.rs` de `:1681` em diante |
| 5.7 escrita no dispositivo | PASS. Os acertos de `Confirm::Yes` são os de antes: doc, `Confirmed::require` e testes. Em `backend.rs` são 4, como em `main`. O teste novo de envio sem ffmpeg usa `FakeStorage` e `Confirm::No`. Nenhum `*Transport::open` em `crates/*/tests` |
| 5.8 protocolo | PASS. Nenhum arquivo de `bezel-devices` nem de `docs/reverse-engineering` mudou na fase |
| 5.9 caminhos no core | PASS: os acertos são doc e testes antigos (`discovery.rs`, `sensor.rs`, `device.rs`, `reconnect.rs`) |
| 5.10 comandos síncronos | PASS. `commands.rs` não mudou nesta rodada. Os comandos síncronos são os de antes, e nenhum deles chega a dispositivo, sensor ou renderer |
| 5.11 supply chain | PASS. `cargo audit` exit 0, nenhum segredo |

## Status dos avisos da iter 1
| Aviso | Status | Evidência |
|---|---|---|
| **W1** `tauri-plugin-opener` sem decisão; `syn` no lock | **Resolvido** | A **D-2026-10-01-video-background-framing-7** registra o plugin, só Rust, com a allow-list de `open_guide`, `open_js_links_on_click(false)` e nenhum `opener:*`. A D-7 está no `DECISIONS.md:145`, e a confinação confere em `lib.rs:139-141` e `capabilities/default.json`, que não tem nenhum `opener:`. Depois do `3302c93`, `git diff main HEAD -- Cargo.lock` mostra só a dependência do `bezel-studio` e as crates `tauri-plugin-opener` 2.7.0, `open` 5.4.4, `is-wsl` 0.4.0 e `is-docker` 0.2.0. O `cssparser-macros` voltou ao `syn 2.0.119`, como em `main`. `--locked` e `cargo audit` estão limpos |
| **W2** subprocesso sob o lock da sessão | **Resolvido** | A sonda (ffprobe) e o pôster refeito ao salvar (ffmpeg, até 30 s) agora rodam fora do lock. Detalhe abaixo |
| **W3** README desatualizado sobre o `bezel run` | **Resolvido** | `README.md:41-45` traz o vídeo de fundo e o Enquadramento, com link para `#framing-the-video`, que existe em `storage-and-video.md:351`. `README.md:168-175` descreve o comando `put` ou o "Send to the screen" do studio para um vídeo reenquadrado, e o envio como está sem ffmpeg. Conferi o texto contra `live.rs:279-285` (`put_for`: `AsIs`, `Turned` só com `turns == 0` e `is_plain`, senão o studio) e `live.rs:330-339` |
| Menor: DRY das rotações | **Resolvido** | `668a25e`. `FramingDto::quarter_turns` (`dto.rs:146`) usa a única lista (`dto.rs:527`). `check_rotation` (`backend.rs:159-170`) responde `invalidInput` com a mensagem do DTO, e `video.rs:499-510` testa 45, 360 e -90. Não há outra lista 0/90/180/270 em Rust. O `ROTATIONS` do JS continua necessário |
| Menor: teste do envio sem ffmpeg | **Resolvido** | `storage/tests.rs:1264` testa o Dragon Ball em Auto com `FakeMedia::missing()`. Dá `Ready`, `convert: None`, 4096 bytes e `internal/video/dragon.mp4`. Depois do `Done { converted: false }`, os bytes guardados são os do asset e o estado é `onDevice` |
| Menor: nova tentativa da prévia | **Resolvido** | `370ae78`. `ThemeVideo::failed`/`waiting` (`studio.rs:328-345`) contam as falhas: depois de uma, o `nextMs` = `PREVIEW_RETRY` (2 s). São no máximo `PREVIEW_ATTEMPTS` = 3 falhas seguidas; depois, o pôster fica até o próximo render. Um quadro mostrado zera a contagem. O caminho sem ffmpeg não muda. O teste (`studio.rs:2825`) cobre 0 ms, 500 ms (1500 ms restantes), 2 s, 4 s (para), 6 s (toca) e uma nova falha isolada |
| Menor: tamanho desconhecido no host | Não tratado (informativo) | Está registrado nas Observações do SUMMARY. Só acontece se a sonda falhar com ffmpeg presente |
| Menor: cobertura de `bezel-media/src/lib.rs` | Igual (informativo) | Segue em 38.43%, porque os testes ffmpeg reais são `#[ignore]`. Rodei os 8 aqui com o `dragon.mp4` real: 8 passed, 61 filtered out |
| Menor: cores fixas nas guias | Igual (informativo) | Intencional; o axe passa nos 4 projetos |

### W2 a fundo
- **O padrão.** A sessão entrega o trabalho ao backend, que o pega sob o lock: `video_to_probe` (`studio.rs:859`),
  `live_video_to_probe` (`:880`) e `poster_to_take` (`:945`). O backend roda o trabalho sem o lock (`VideoProbe::run`,
  `:189`; `PosterRetake::run`, `:225`) e devolve o resultado (`probed`, `:888`; `poster_taken`, `:982`). Cada `Studio`
  tem um `serial` por vídeo, então o trabalho de outro tema com um vídeo do mesmo nome é descartado. O pôster só entra
  com o mesmo pôster e o mesmo `PosterSpec`.
- **Os pontos de chamada no backend:**
  - `render` (`backend.rs:582-603`) solta o lock antes de sondar e o retoma depois.
  - `video_auto` (`:606-615`) usa `Wait::Yes`, e espera só o conversor, nunca o lock da sessão.
  - `save` (`:680-700`) chama `retake_poster` (`:711-728`) antes de pegar o lock.
  - `tick` (`:919-923`) e `show_now` (`:259-265`) sondam só quando a tela ao vivo vai iniciar o vídeo.
- **Sem deadlock.** `converter()` (`studio.rs:163`) com `Wait::No` usa `try_lock`. Uma sonda em curso em outra
  thread faz o `tick` desistir em vez de esperar. Nenhum caminho segura o lock da sessão enquanto espera o conversor.
- **Sem regressão de comportamento:**
  - Um vídeo ainda não sondado mostra o pôster, e a prévia pede de novo em 500 ms (`CONVERTER_BUSY`).
  - A tela ao vivo mantém o pôster até a sonda (`video_known`, `:901`).
  - Uma sonda que falha marca o vídeo como conhecido (`record_probe(None)`), então não há laço de re-sonda.
  - A cópia do vídeo é escrita uma vez e reaproveitada (`ThemeVideo::file`, `:348-363`).
  - O envio de "Send to the screen" depende do `VideoMissing` da runtime, que só existe depois da sonda.
- **Testes que falham se o trabalho voltar para dentro do lock** (`backend.rs:1684`, `:1717`):
  - O `FakeMedia` segura a chamada por canais, sem `sleep`.
  - Enquanto isso, o teste afirma que `studio.try_lock()` está livre, que o refresh desenha um quadro, que o `push`
    mostra outro e que a prévia renderiza o pôster com `nextMs` = 500.
  - Depois, o Auto dá 270° com 480x1920, `internal/video/dragon.mp4` aparece como `missing`, e o pôster refeito
    (1920x480) chega ao tema salvo.

### Dados reais nesta rodada
- `bezel render --fake` da cópia do Dragon Ball: exit 0. O pôster aparece em pé sob o tema (PNG conferido).
- `bezel import` da cópia: os dois `.bezeltheme` têm conteúdo idêntico (`diff -r`), e o `theme.json` sai **byte a
  byte igual**, depois da refatoração do `quarter_turns`.
- Testes ffmpeg reais (`-p bezel-media --lib -- --ignored real_ffmpeg`, `BEZEL_DRAGON_BALL_MP4` apontado para o
  arquivo do fabricante): **8 passed, 0 failed**. O
  `real_ffmpeg_stands_a_panel_native_video_up_and_decodes_it_raw` checa o arquivo real.

## Blockers
Nenhum.

## Warnings
Nenhum novo. Os três avisos e os três menores tratáveis da iter 1 estão resolvidos (tabela acima).

**Notas (informativo, nenhuma pede ação nesta fase):**
- **Decodificador da prévia sob o lock.** Ele ainda é aberto e lido sob o lock da sessão (`studio.rs:1113` →
  `stream.rs:254`, `recv_timeout(FRAME_TIMEOUT = 10 s)`). Isso já estava assim e foi aceito na iter 1 (fora do W2).
  As Observações do SUMMARY registram o ponto. No caso normal, a thread leitora mantém 2 quadros prontos, e a espera só
  existe no primeiro quadro de cada (re)início do ffmpeg. Fica para uma fase futura, se aparecer engasgo no ao vivo
  durante a prévia.
- **Nota do PLAN.** `PLAN.md:124` ainda diz "Nenhuma crate nova (`Cargo.lock` intacto)". A D-7 substitui essa nota.
  O CONTEXT lista só as decisões 1..6 em "Locked decisions" e nas refs.
- **SUMMARY, "Files modified".** A lista cita `bezel-media/src/probe.rs`, que a fase não alterou. Ela omite
  `bezel-cli/src/theme.rs` e `bezel-cli/tests/storage.rs`, que só tiveram mudanças mecânicas.
- **CHANGELOG.** `CHANGELOG.md:51`, uma entrada da release-polish, ainda chama o botão de "Send to screen". O rótulo
  hoje é "Send to the screen" (`en.js:662`, e a CLI desde o `4feaa0f`). Vale ajustar no corte de release.

## Status do PLAN
- **Concordância:** concordo com T-1 a T-7 como `completed`. As correções da rodada não mudam o escopo de nenhuma task.
- **T-8:** segue `pending` (hardware, com o orquestrador, "Deferred to PR review"). Não é linha de DoD, e não a marquei
  como `MANUAL_REQUIRED`.
- **Crate nova:** a divergência da iter 1 agora está coberta pela D-7.

## DoD Checklist (gate 8)
| # | Criterion | Source | Type | Status | Evidence |
|---|---|---|---|---|---|
| 1 | Core: Auto, geometria e nomes (os do fornecedor seguem) | CONTEXT | Auto | PASS | `OK`; `4 passed; 0 failed; 114 filtered out` |
| 2 | Runtime: `dragon.mp4` guardado toca sem envio; outro tamanho não; re-enquadrado tem nome próprio; PC enquadra | CONTEXT | Auto | PASS | `OK`; `4 passed; 0 failed; 12 filtered out` |
| 3 | Filtros ffmpeg puros; prévia crua ≤ 15 fps; cadeia do fornecedor intacta | CONTEXT | Auto | PASS | `OK`; `3 passed; 0 failed; 66 filtered out` |
| 4 | `.bezeltheme`: `framing` ida e volta; temas antigos e TURZX em Auto | CONTEXT | Auto | PASS | `OK`; `2 passed; 0 failed; 48 filtered out`. O tema real também sai byte a byte igual |
| 5 | Studio: prévia ≤ 15 fps, para sem pedidos, pôster sem ffmpeg e ao salvar; nativo vai como está | CONTEXT | Auto | PASS | `OK`; `5 passed; 0 failed; 126 filtered out`. O `saving_retakes_the_poster_with_the_framing` agora passa pelo caminho fora do lock |
| 6 | UI: i18n, lógica do enquadramento e Playwright nos 4 projetos com axe (versão estrita: `await expectAccessible(`, n ≥ 16) | CONTEXT | Auto | PASS | `OK` em zsh e em bash, com `BEZEL_E2E_PORT=1464`. O filtro `-g` deu **16 passed** (4 testes × 4 projetos), e há 6 `await expectAccessible(` no spec |
| 7 | Guia en/pt-BR e CHANGELOG | CONTEXT | Auto | PASS | `OK` |
| 8 | CI do Windows verde no HEAD do PR | CONTEXT | Auto | PASS | `OK`. Run **36874651056** (`workflow_dispatch`, `007f006f37bd…`, `jdi/video-background-framing`): `completed success`. Verdes: `rust-linux`, `rust-windows` (802/0/11), `node-ui`, CodeQL (rust, js, actions), Varreduras, Versao e `Portao`. `sonar`, `imagem`, `publicar` e `lancar` ficaram `skipped`, como esperado |
| 9 | `cargo test --workspace` exits 0 | PROJECT | Auto | PASS | Gate 2: exit 0, 866/0/11 |
| 10 | Coverage >= 80% of lines | PROJECT | Auto | PASS | `OK`; TOTAL 94.34% (comando como escrito) |
| 11 | No `TODO`/`FIXME` without issue | PROJECT | Auto | PASS | `OK` |
| 12 | CHANGELOG.md updated per release | PROJECT | Manual | MANUAL_REQUIRED (release) | O `[Unreleased]` cita o enquadramento (`CHANGELOG.md:161-173`, `:201`). Evidência sugerida: `## [x.y.z] - <data>` no commit de corte (D-2026-09-30-release-polish-2) |
| 13 | README describes current behavior | PROJECT | Manual | MANUAL_REQUIRED (release) | O W3 foi corrigido, e o texto confere com `live.rs` e com a D-3/D-5. Evidência sugerida: o diff do README revisado no PR |

As linhas 12 e 13 são do corte de release do projeto e seguem pendentes desde a release-polish. A fase não tem Manual
próprio, e a validação na 8.8" é do orquestrador (Deferred to PR review). Por isso o veredito segue os formatos pedidos
pelo orquestrador.

## Recommendation
Pode seguir para a T-8 no hardware e para o PR. Nenhum gate falha, nenhum aviso ficou aberto e a rodada de correção
não quebrou nada: os testes subiram de 861 para 866, a cobertura de 94.33% para 94.40%, e o Windows passa no CI do
HEAD.

- **Na T-8:** usar um build ≥ `007f006`. O `install-local` deve ser o deste HEAD, porque a sonda saiu do lock e
  afeta o início do vídeo ao vivo.
- **No PR ou no corte de release:** as notas informativas acima (rótulo do CHANGELOG, a nota do PLAN superada pela
  D-7, o `probe.rs` na lista do SUMMARY) são só de texto.

## DoD Critic (enhanced)

- DoD row «5»: o `1790823` levou o retake do pôster de `Studio::save` para `Backend::save` (backend.rs:685-692); `studio::tests::saving_retakes_the_poster_with_the_framing` passa por um helper `save` do próprio teste (studio.rs:2581), então a linha seguiria verde se o retake saísse do backend. O critério é provado por `backend::tests::the_session_goes_on_while_the_poster_is_taken` e `the_session_goes_on_while_the_video_is_probed`, fora do comando (não objetivo: o critério está cumprido). Linhas 1–4 e 6–11 provam o que dizem; a 6, endurecida, deu 16 passed.

**Verdict:** APPROVED_WITH_WARNINGS
