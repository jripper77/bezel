# Phase 9: Review  (slug: video-background-framing)

**Verdict:** APPROVED_WITH_WARNINGS

> Revisão em modo `verify`, iteração 1 do `/jdi-loop`. Branch `jdi/video-background-framing`, `HEAD` = `cd13083`
> (20 commits desde `main`). A árvore estava limpa: só o `LOOP.md` do orquestrador estava fora do git.
>
> - **Escopo julgado:** código e testes das tasks T-1 a T-7 contra o PLAN, as decisões
>   D-2026-10-01-video-background-framing-1..6, as herdadas (D-1 hexagonal, storage-video-1/-2/-4,
>   storage-manager-9, release-polish-12) e o DoD do CONTEXT e do PROJECT.
> - **SUMMARY.md:** ainda não existe. O orquestrador o escreve depois deste loop, e por isso não abortei, como diz o
>   fallback. Todos os números abaixo saíram das minhas execuções.
> - **Hardware:** nenhum comando abriu a 8.8" real. Não rodei `#[ignore]` de hardware nem `BEZEL_HW_TESTS`, e não
>   toquei em `/dev/tty*`, hidraw, serviços ou `sudo`.
> - **Dados reais (só leitura):**
>   - Copiei o `Dragon-Ball.bezeltheme` do usuário para o scratchpad, e o `bezel import` rodou nessa cópia com
>     `XDG_*` temporários.
>   - O `.bezeltheme` original não mudou: `mtime` 2026-09-30 22:24:47, igual antes e depois desta verificação.
>   - O `dragon.mp4` do fabricante foi só lido, pelos testes ffmpeg gated (`BEZEL_DRAGON_BALL_MP4`) e por um
>     `ffmpeg` direto que gravou o PNG no scratchpad.
> - **Playwright:** rodou com `BEZEL_E2E_PORT=1461`.

## Gates
| Gate | Status | Details |
|---|---|---|
| Build | PASS | `cargo build --workspace --locked`: exit 0, rustc 1.98.1. O lock com a crate nova foi aceito pelo `--locked` |
| Tests | PASS | **861 passed, 0 failed, 11 ignored** (36 binários). Os ignorados: 8 de ffmpeg real (2 novos), 1 de timing, 1 de hardware do studio, 1 de corpus. Subiu de 825 para 861 (+36) desde a storage-manager; nenhum teste sumiu. Windows, no CI de `cd13083`: **797 passed, 0 failed, 11 ignored** (eram 761) |
| Coverage | PASS | **94.33%** lines (TOTAL, sem `main.rs`/`build.rs`), exit 0. Pelo comando do DoD do PROJECT, sem filtro: **94.27%**. Os arquivos novos estão em 100%: `domain/framing.rs`, `framing/picture.rs`, `bezel-media/src/framing.rs` e `domain/poster.rs` |
| Lint | PASS | `cargo fmt --all --check`: exit 0. `cargo clippy --workspace --all-targets --locked -- -D warnings`: exit 0. Clippy cruzado `x86_64-pc-windows-msvc` (7 crates, `target/wincheck`): exit 0. Os únicos `allow` fora de teste são os 4 de `fps/rtss.rs:247,269,302,323`, que já existiam |
| Hexagonal/Safety/Protocol/Hygiene | PASS | 5.1 a 5.11 limpos. `cargo audit`: exit 0, 1278 advisories, 608 crates. Detalhe abaixo |
| Consistency | PASS (com WARN) | Os 20 commits usam o escopo `video-background-framing`, e cada task tem os testes do PLAN. Nenhuma D-XX é contrariada. O PLAN foi desviado na dependência nova (W1) |
| UI Validation | PASS | `npm ci` ok. **176/176** Playwright (4 projetos: claro/escuro × pt-BR/en, axe). `npm run test:unit`: **180/180**, 99.93% de linhas, `editor/video-framing.js` em 100%. `i18n.test.mjs`: 13/13, com 771 chaves em cada idioma e paridade exata |
| DoD | PASS | As 8 linhas Auto do CONTEXT e as 3 Auto do PROJECT passam como escritas, e nenhuma passa com filtro vazio. O CONTEXT não tem Manual. Os 2 Manual do PROJECT são do corte de release (ver tabela) |

### Detalhe do gate 5
| Check | Resultado |
|---|---|
| 5.1 dependências do core | PASS: em `[dependencies]` só há `thiserror` |
| 5.2 I/O e threads no core | PASS: nada. `framing/picture.rs` usa só `std::borrow::Cow` |
| 5.3 ports | PASS. Nenhuma impl de porta no core (procurei também `MediaTranscoder`, `ScreenStorage`, `ArchiveStore` e `VideoFrames`). As traits públicas fora do core são as mesmas da fase anterior (`Clock`, `Pause`, `Wire`, `Pace`, `Pictures`, `MediaSetup`), todas auxiliares de adapter |
| 5.4 adapters na composição | PASS: nada fora de `main.rs`/`lib.rs` e testes (estendi a busca a `FfmpegTranscoder` e `DiskArchive`) |
| 5.5 `unsafe` | PASS: só os blocos de `fps/rtss.rs`, sem mudança |
| 5.6 panics | PASS. Nos 34 `.rs` alterados, os únicos `expect` fora de `#[cfg(test)]` ficam em `bezel-render/src/golden.rs`, módulo `#[cfg(test)]` (`lib.rs:30-31`) |
| 5.7 escrita no dispositivo | PASS. Os acertos de `Confirm::Yes` são doc, `Confirmed::require` e módulos de teste, iguais aos da fase anterior. O "Enviar para a tela" do vídeo nativo sobe um arquivo novo pelo pipeline de antes. Um `dragon.mp4` de outro tamanho fica no mesmo caminho (`MissingVideo.path`), e o core exige `Confirm::Yes` para substituí-lo (`app/storage.rs:202-209`). Nenhum teste de runtime chama upload ou delete (`changes_the_screen` vazio). Nenhum `*Transport::open` em `crates/*/tests` |
| 5.8 protocolo | PASS. Nenhum arquivo de `bezel-devices` nem de `docs/reverse-engineering` mudou. Conferi `FILE_SIZE = 0x6E` e `PLAY_VIDEO = 0x78` (`protocol/turing_rev_c.rs:35,39`) contra `protocol-turing-rev-c.md:99,101` |
| 5.9 caminhos no core | PASS: os acertos são doc e testes antigos (`discovery.rs`, `sensor.rs`, `device.rs`, `reconnect.rs`) |
| 5.10 comandos síncronos | PASS. Os comandos novos `video_auto` e `open_guide` são `async`; o `open_guide` chama o opener em `spawn_blocking`. Os comandos síncronos são os de antes. Ressalva: há trabalho de subprocesso sob o lock da sessão (W2) |
| 5.11 supply chain | PASS. `cargo audit` exit 0, nenhum segredo. Crates novas: `tauri-plugin-opener` 2.7.0, `open` 5.4.4, `is-wsl` 0.4.0 e `is-docker` 0.2.0 (W1) |

### O que conferi a fundo
- **Geometria do enquadramento** (`domain/framing.rs:433-542`).
  - Cover nunca deixa borda vazia, e contain a 100% só perde um pixel de arredondamento.
  - `object-position` segue a semântica do CSS, e `turned()` gira a posição no sentido horário (`:325-339`).
  - Num fuzz próprio no scratchpad, 200 mil casos aleatórios (fonte até 5000², alvo par, 4 voltas, os 2 ajustes,
    zoom 100–400, posição 0–1000) deram **0 problemas**. Cortes e pads têm bordas pares e cabem na fonte e no alvo.
    O aspecto do recorte bate com a imagem de saída.
  - `frame_picture` nunca entrou em pânico, em 3 mil tamanhos ímpares pequenos.
  - A mesma moldura no canvas e no painel (`turned(1)`) diverge no máximo **3 px de fonte** no corte e **2 px** no
    pad, só por arredondamento de borda par.
- **Fonte única da geometria.** Conversão (`framed_options`, `media.rs`), pôster (`PosterSpec::framed`/`geometry`),
  prévia e host (`ThemeRuntime::framed_video` e `render` → `frame_picture`) usam todos `framing::geometry`.
  - A cadeia ffmpeg é uma só função pura (`bezel-media/src/framing.rs:40`), usada pela conversão (`Scaling::Exact`)
    e pelo pôster (`Scaling::Cover`).
  - O stream pede a fonte crua: `fps`, `scale=W:H`, `setsar` (`stream.rs:47`).
  - O enquadrado padrão reproduz a cadeia do fornecedor. `cover_crop` agora é a geometria plain, e o teste compara os
    dois em 6 casos.
  - O `pictureBox` do JS espelha a geometria só para a matemática do ponteiro e para o demo, como diz a D-6.
- **Dragon Ball de ponta a ponta.**
  - Auto 270° no canvas e 0 no total, nome `dragon.mp4` (`media.rs` `device_video_names_carry_the_framing`).
  - Conversão identidade, enviada como está: `convert: None`, bytes do asset, nada convertido
    (`storage/tests.rs` `a_panel_native_theme_video_is_sent_as_it_is`). O teto de 25 MiB vale (`tooLarge`).
  - O arquivo guardado só é reusado com o tamanho certo, `Expect::Bytes` (`runtime.rs` `find_video`). Outro tamanho
    fica no mesmo lugar e não toca nada; o do cartão é achado depois do interno; um tamanho desconhecido conta como
    presente (`runtime_video.rs`).
  - A CLI segue o mesmo caminho (`live::tests::run_honours_the_video_framing`).
  - Os 8 testes ffmpeg reais passam com o `dragon.mp4` real (sha256 igual ao asset do tema do usuário). Renderizei o
    quadro do fabricante com a cadeia do Auto (`transpose=2,...`) e ele saiu em pé, igual ao `poster-195.png` do
    tema.
- **Compatibilidade dos temas.**
  - O `theme.json` real do usuário sai de `bezel import` **byte a byte igual**, testado numa cópia no scratchpad.
  - O teste `video_framing_round_trips_and_older_themes_load_as_auto` cobre a mesma ida e volta.
  - O `BackgroundDto` antigo não usa `deny_unknown_fields` (`main:dto.rs`), então builds antigos ignoram `framing`.
- **Decodificador da prévia.**
  - No máximo 1 por sessão (`ThemeVideo.decoder: Option`).
  - ffmpeg a no máximo 15 fps: `MAX_FPS = PREVIEW_FPS`, `stream.rs:40,74`, `-threads 2`.
  - Fecha após 2 s sem pedido. O laço de refresh roda sempre, com no máximo 250 ms de sono (`lib.rs:425-438`,
    `LOOK_AGAIN`), e chama `stop_idle_preview`.
  - `motion=false` mostra o pôster e para o decodificador. Trocar o vídeo reinicia, e editar o enquadramento não
    reinicia, nos testes de relógio injetado.
  - O host ao vivo segue a 10 fps (`HOST_VIDEO_FPS`), abaixo do novo teto.
- **`tauri-plugin-opener`.**
  - Só o Rust chama o plugin (`commands.rs` `open_guide` → `guide_url`). As URLs são fixas, `ffmpeg` em `en`/`pt-BR`.
    Os testes recusam traversal, URL externa e idioma desconhecido, e conferem que os `.md` existem.
  - A capability não ganhou nenhum `opener:*`, só `allow-open-guide`/`allow-video-auto`.
  - `open_js_links_on_click(false)` (`lib.rs:140`), e a CSP não mudou.

## Blockers
Nenhum.

## Warnings
- **W1. Dependência nova fora do PLAN (gate 6).** É desvio do PLAN, não de D-XX. O código está correto e seguro, mas
  a decisão não ficou registrada.
  - **O que o PLAN dizia:** "Nenhuma crate nova (`Cargo.lock` intacto)" (`PLAN.md:124`). A lista de arquivos da T-6
    não inclui `Cargo.toml`/`Cargo.lock`.
  - **O que mudou:** `84d8d30` acrescentou `tauri-plugin-opener = "2.7.0"` (`src-tauri/Cargo.toml:38`), com 3 crates
    transitivas (`open`, `is-wsl`, `is-docker`), para o link "Como instalar o ffmpeg" da D-5.
  - **Efeito colateral no lock:** `cssparser-macros` 0.7.1 passou de `syn 2.0.119` para `syn 3.0.6`
    (`Cargo.lock:1009`). A mudança não tem relação com a fase.
  - **O que já foi conferido:** só Rust, URLs fixas, nenhuma permissão no webview, `cargo audit` limpo, CI verde.
  - **Sugestão:** citar a dependência no SUMMARY e, se o projeto quiser, numa D-XX curta.
- **W2. Subprocesso sob o lock da sessão.** O `video_auto` sonda fora do lock de propósito (`backend.rs:593-611`).
  Dois caminhos fazem o trabalho por dentro:
  - `Studio::preview` → `learn_video()` (`studio.rs:873`, `:749-790`) roda a sonda. MP4 e GIF são lidos em Rust, mas
    outros contêineres caem no `ffprobe`.
  - `Studio::save` → `retake_poster()` → `take_poster()` (`studio.rs:793-845`) roda o ffmpeg do pôster, com timeout de
    30 s, enquanto `Backend::save` segura `self.studio()` (`backend.rs:681-686`).
  - **Efeito:** o laço de refresh e os quadros ao vivo esperam esse tempo, normalmente menos de 1 s. Não bloqueia.
  - **Sugestão:** o mesmo padrão de `video_to_probe`/`probed` para o pôster.
- **W3. README desatualizado no vídeo de fundo (DoD Manual do PROJECT).**
  - `README.md:163-167` diz que o `bezel run` "prints the exact `bezel storage put` command". Para um vídeo
    reenquadrado, ele agora aponta para o "Send to screen" do studio (`live.rs:325-341`).
  - O README também não cita o enquadramento. O guia en e pt-BR e o CHANGELOG cobrem tudo.
  - **Sugestão:** uma linha no README, no corte de release.
- **Menores (informativo).**
  - **DRY.** A lista 0/90/180/270 aparece em `bezel-themes/src/dto.rs:517` (`quarter_turns`) e de novo em
    `backend.rs:157` (`check_rotation`). A segunda só troca o código de erro para `invalidInput` (o `ROTATIONS` do JS
    é necessário).
  - **Lacuna de teste.**
    - O "Enviar para a tela" sem ffmpeg de um vídeo de tema com moldura identidade, prometido pela D-5, pelo
      CHANGELOG e pelo guia, não tem teste próprio. O caminho de tema só testa a recusa do reenquadrado
      (`storage/tests.rs:1176-1189`).
    - O caminho genérico está coberto: "A video already in the profile still goes" (`storage/tests.rs:868-870`).
  - **Tamanho desconhecido no host.** Sem sonda, `ThemeRuntime::video_stream` (`runtime.rs:538`) decodifica no tamanho
    do canvas. O `scale=W:H` cru então estica a imagem; antes, o crop de cover mantinha o aspecto. Só acontece se a
    sonda falhar com ffmpeg presente, o que é raro.
  - **Nova tentativa da prévia.** Depois que um decodificador falha, a prévia volta ao pôster com `nextMs` nulo
    (`studio.rs:912,926,954`). A nova tentativa só vem no próximo render pedido pela UI (uma edição), e não sozinha
    após `PREVIEW_RETRY`.
  - **Cobertura de `bezel-media/src/lib.rs`.** Caiu de 48,90% para 38,43% porque os 2 testes ffmpeg reais novos são
    `#[ignore]`. Eles passam aqui com ffmpeg e com o `dragon.mp4`.
  - **Cores fixas.** `styles.css:238-240` usa branco e preto literais nas guias sobre o vídeo, e não tokens. É
    intencional, porque precisa de contraste sobre qualquer quadro, e o axe passa nos 4 projetos.

## Status do PLAN
- **Concordância:** concordo com os status de T-1 a T-7 (`completed`). Cada task tem o código e os testes que pede, e
  todos passam.
- **T-8:** está `pending` (hardware, com o orquestrador). Não é linha de DoD, e não a marquei como `MANUAL_REQUIRED`.
- **Divergência única:** a nota "Nenhuma crate nova" do PLAN (W1).
- **Arquivos listados e não alterados:** `demo-theme.js` (T-2), `tests/ui/demo-backend.test.mjs` (T-2) e
  `tests/ui/fixtures/backend-codes.json` (T-6). Nenhum faz falta: não houve código de backend novo, e o demo está
  coberto por `video-framing.test.mjs`.

## DoD Checklist (gate 8)
| # | Criterion | Source | Type | Status | Evidence |
|---|---|---|---|---|---|
| 1 | Core: Auto, geometria e nomes (os do fornecedor seguem) | CONTEXT | Auto | PASS | `OK`; `4 passed; 0 failed; 114 filtered out` |
| 2 | Runtime: `dragon.mp4` guardado toca sem envio; outro tamanho não; re-enquadrado tem nome próprio; PC enquadra | CONTEXT | Auto | PASS | `OK`; `4 passed; 0 failed; 12 filtered out` |
| 3 | Filtros ffmpeg puros; prévia crua ≤ 15 fps; cadeia do fornecedor intacta | CONTEXT | Auto | PASS | `OK`; `3 passed; 0 failed; 66 filtered out` |
| 4 | `.bezeltheme`: `framing` ida e volta; temas antigos e TURZX em Auto | CONTEXT | Auto | PASS | `OK`; `2 passed; 0 failed; 48 filtered out`. O `theme.json` real do usuário também sai byte a byte igual |
| 5 | Studio: prévia ≤ 15 fps, para sem pedidos, pôster sem ffmpeg e ao salvar; nativo vai como está | CONTEXT | Auto | PASS | `OK`; `5 passed; 0 failed; 121 filtered out` |
| 6 | UI: i18n, lógica do enquadramento e Playwright nos 4 projetos com axe | CONTEXT | Auto | PASS | `OK` em bash e em zsh; o filtro `-g` deu **16 passed** (≥ 12; 4 testes × 4 projetos) |
| 7 | Guia en/pt-BR e CHANGELOG | CONTEXT | Auto | PASS | `OK`. Os títulos `### Framing the video` e `### Enquadrar o vídeo` existem, o `[Unreleased]` cita framing, e o `check-docs.sh` agora exige os dois |
| 8 | CI do Windows verde no HEAD do PR | CONTEXT | Auto | PASS | `OK`. Run **36861544813** (`workflow_dispatch`, `cd13083de1d2…`): `completed success`; verdes `rust-linux`, `rust-windows` (797/0/11), `node-ui`, CodeQL (rust, js, actions), Varreduras, Versao e `Portao`; `imagem`, `sonar`, `publicar` e `lancar` `skipped`, como esperado |
| 9 | `cargo test --workspace` exits 0 | PROJECT | Auto | PASS | Gate 2: exit 0, 861/0/11 |
| 10 | Coverage >= 80% of lines | PROJECT | Auto | PASS | `OK`, TOTAL 94.27% (comando como escrito) |
| 11 | No `TODO`/`FIXME` without issue | PROJECT | Auto | PASS | `OK` |
| 12 | CHANGELOG.md updated per release | PROJECT | Manual | MANUAL_REQUIRED (release) | O `[Unreleased]` ganhou o enquadramento (Added) e o conserto do Dragon Ball (Fixed). Evidência sugerida: `## [x.y.z] - <data>` no commit de corte (D-2026-09-30-release-polish-2) |
| 13 | README describes current behavior | PROJECT | Manual | MANUAL_REQUIRED (release) | Ver W3 (`README.md:163-167`). Evidência sugerida: o diff do README revisado no PR do corte |

As linhas 12 e 13 são do corte de release do projeto e estão pendentes desde a release-polish. A fase não tem Manual
próprio, e a validação na 8.8" é do orquestrador (Deferred to PR review). Por isso o veredito segue os formatos pedidos
pelo orquestrador.

## Recommendation
Pode seguir para o SUMMARY e para a T-8 no hardware. Nenhum gate falha, e o caso Dragon Ball está coberto de ponta a
ponta nos testes, com o arquivo real do fabricante.

Antes do PR:
1. Registrar no SUMMARY a dependência `tauri-plugin-opener`, com o motivo e as salvaguardas, e a mudança de `syn` no
   lock (W1). Se o projeto quiser, numa D-XX curta.
2. Se couber nesta fase, mover a retomada do pôster ao salvar para fora do lock da sessão (W2) e acrescentar o teste
   do envio identidade sem ffmpeg. Os dois são baratos.
3. Atualizar a linha do README sobre o `bezel run` (W3), aqui ou no corte de release.

Na T-8, usar um build ≥ `cd13083`. O `install-local` deve ser o deste HEAD.

## DoD Critic (enhanced)

Nenhuma linha Auto vazia: os 11 comandos provam os critérios (relidos e re-executados em `cd13083`). Pontos fracos sem invalidar a linha: na 6, o `grep -q expectAccessible` casa também o import e o limite `n>=12` passaria com um teste ou projeto a menos (o esperado é 16); na 7, a checagem é de presença (o conteúdo foi lido e confere).

**Verdict:** APPROVED
