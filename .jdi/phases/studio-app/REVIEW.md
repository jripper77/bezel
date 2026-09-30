# Phase 5: Review  (slug: studio-app)

**Verdict:** BLOCKED

> Verificação em modo `verify` (2026-09-30). `HEAD` = `bb7ad78`, árvore limpa. Rodei todos os gates do zero sobre a
> árvore inteira e não reaproveitei nenhum número do SUMMARY. O escopo julgado é o da fase: tarefas T-5.1..T-5.6 do
> PLAN, DoD do CONTEXT e D-2026-09-30-studio-app-1..6. A aba de armazenamento (phase `storage-video`) só entrou nos
> gates. Nenhum comando abriu a 8.8" real, que continua com o serviço do usuário. Não rodei nenhum `#[ignore]`: nem o
> `tests/hardware.rs` do studio, nem ffmpeg real, timing ou corpus.

## Gates
| Gate | Status | Details |
|---|---|---|
| Build | PASS | `cargo build --workspace --locked`: ok, lock aceito (rustc 1.98.1) |
| Tests | PASS | 509 passed, 0 failed, 7 ignored (4 de ffmpeg real, 1 de hardware do studio opt-in, 1 de timing, 1 de corpus). Sem queda (509 no SUMMARY). `bezel-studio`: 46 unitários |
| Coverage | PASS | **94.92%** lines (TOTAL, sem `main.rs`/`build.rs`), exit 0. Pelo comando do DoD, sem filtro: 94.86%. No studio: `backend.rs` 97.84%, `studio.rs` 98.59%, `library.rs` 99.38%. A cola Tauri fica baixa: `commands.rs` 2.21%, `lib.rs` 19.54%, `tray.rs` 23.08% |
| Lint | PASS | `cargo fmt --all --check` e `cargo clippy --workspace --all-targets --locked -- -D warnings`: exit 0. Nenhum `#[allow]` fora de teste |
| Hexagonal/Safety/Protocol/Hygiene | **BLOCK** | 5.1–5.11 limpos nos greps. A leitura de `studio.rs` mostra o caso de uso de refresh do core reimplementado no adapter de entrada (B1) |
| Consistency | **BLOCK** | D-2026-09-30-studio-app-4 contrariada (B1). Commits com escopo `studio-app`. Lista de arquivos do PLAN desatualizada (W8) |
| UI Validation | PASS (com WARN) | `npm ci` ok. `npm run test:unit`: 57/57, 99,80% de linhas (`store.js` 100%). `npx playwright test`: 24/24 em claro e escuro, sem erro de console e sem axe sério ou crítico. Achados de teclado, perda de edições e i18n de erros em W1–W4 |
| DoD | PASS / PENDING MANUAL | Os 3 Auto do PROJECT e os 4 Auto do CONTEXT passam como escritos. Faltam 1 Manual do CONTEXT e 2 do PROJECT |

### Detalhe do gate 5
| Check | Resultado |
|---|---|
| 5.1 dependências do core | PASS: só `thiserror` |
| 5.2 I/O e threads no core | PASS: nada |
| 5.3 ports | PASS no grep. Traits fora do core: `Pause`, `Clock`, `Wire`, `Pace`, `MediaSetup` (auxiliares de adapter, de fases anteriores) |
| 5.4 adapters na composição | PASS: nada fora de `main.rs`/`lib.rs`/testes. `compose()` em `lib.rs:208` é a raiz. `backend.rs`, `studio.rs`, `library.rs`, `settings.rs`, `media.rs`, `dto.rs` e `storage.rs` não importam `tauri`, e só `commands.rs`, `lib.rs` e `tray.rs` o fazem. O backend roda sem Tauri, sobre fakes |
| 5.5 `unsafe` | PASS: só a string `"unsafe asset path"`. `lib.rs` e `main.rs` têm `#![forbid(unsafe_code)]` |
| 5.6 panics | PASS: fora de `#[cfg(test)]`, só `bezel-sensors/src/testing.rs` e `bezel-render/src/{golden,testkit}.rs`, que são declarados sob `#[cfg(test)]` |
| 5.7 escrita no dispositivo | PASS: `Confirm::Yes` fora de teste só em doc, `Confirmed::require` e `commands.rs` (`confirm_of`, `:239-240`). `tests/hardware.rs` tem `#[ignore]` e sai sem `BEZEL_HW_TESTS` (`:75-79`) |
| 5.8 protocolo | PASS: todo encoder tem teste. `protocol/` não mudou na fase |
| 5.9 caminhos no core | PASS: só doc e testes |
| 5.10 comandos síncronos | PASS: `list_fonts`, `get/set_autostart` e `cancel_job` não tocam tela, sensor nem renderer |
| 5.11 supply chain | PASS: `cargo audit` 0 (1277 advisories, 601 crates), também com `--deny warnings`. Nenhum segredo |

### Superfície Tauri (segurança)
- **Capabilities:** `capabilities/default.json` só dá `core:event:default` e os 31 `allow-*` gerados por `build.rs:7-39`. São os mesmos 31 do `generate_handler!` (`lib.rs:110-142`). Não há `shell` nem `fs`. `dialog` e `autostart` são usados só do lado Rust e não têm permissão na janela.
- **CSP:** `tauri.conf.json:24` (`default-src 'self'; script-src 'self'; style-src 'self'; connect-src ipc: http://ipc.localhost; object-src 'none'; frame-ancestors 'none'`…) e `custom-protocol` sempre ligado (`Cargo.toml`).
  - Rodei a UI em Playwright servida **com esse cabeçalho CSP**: nenhuma violação e nenhum erro. O `fetch` para a própria origem foi bloqueado, o que prova que a CSP estava ativa.
  - A UI não usa `innerHTML`, `eval` nem `window.confirm`. `el()` aplica estilo via CSSOM (`ui/dom.js:18`).
- **Path traversal:** os assets estão protegidos.
  - `safe_asset_path` (`bezel-themes/src/native.rs:31-41`) recusa `..`, caminho absoluto e `\`.
  - `add_asset` normaliza o nome (`studio.rs:426-455`).
  - O preview e o save só usam bytes que já estão na memória da sessão.
  - `import_theme`, `add_image` e `locate_ffmpeg` recebem caminhos do diálogo nativo, do lado Rust.
  - Resta `open_theme(location)`, que aceita qualquer caminho vindo da webview (W6).

## Blockers
- **B1. O modo ao vivo não roda o `ThemeRuntime` do `bezel run` e reimplementa o caso de uso no adapter (D-2026-09-30-studio-app-4; hexagonal: "An adapter contains business logic").**
  - **O que a D-4 exige:** "with Live on, the backend runs the same ThemeRuntime as bezel run on a worker thread, and every committed edit swaps the theme in place (histories kept)".
  - **O que o `bezel run` faz:** usa o caso de uso do core: `runtime.show(...)` (`bezel-cli/src/live.rs:290`) e `start_video` com `HostVideo` (`live.rs:262-267`).
  - **O que o studio faz:** monta um laço próprio com `sensors`, `renderer`, `histories`, `snapshot` e `quantities` (`studio.rs:66-80`). O `ThemeRuntime` só decide o vídeo, e o próprio módulo diz isso (`studio.rs:11-13`). O laço duplica o core:
    - `Studio::sample` (`studio.rs:127-133`) duplica `ThemeRuntime::sample` (`runtime.rs:340`);
    - `Studio::set_theme` (`studio.rs:158-171`) duplica a adoção de históricos de `replace` (`runtime.rs:229`);
    - `render_with` (`studio.rs:238-248`) duplica o `RenderContext` de `render` (`runtime.rs:357`);
    - `present`/`present_frame`/`tick` (`studio.rs:368-419`, chamado de `backend.rs:441-450`) cobrem o `show` (`runtime.rs:393`) e a checagem de tamanho da CLI (`live.rs:112`).
  - **Divergência real:** o studio chama `start_video(link, None)` (`studio.rs:359`). Numa tela sem reprodução no dispositivo (WCH), `bezel run` decodifica o vídeo no PC e o studio mostra só o pôster. O todo `2026-09-30-release-polish.md:3` cita uma "D-2026-09-30-release-polish-1" que não existe em `.jdi/decisions/`, então nada emenda a D-4.
  - **Correção, uma das duas:**
    - (a) **Preferida:** levar o que falta ao core, por exemplo `ThemeRuntime::snapshot()` para as leituras da UI e um `render` com `Backdrop` explícito para o preview em pôster. Depois o `Studio` passa a dirigir um `ThemeRuntime`, e o laço vira `sample` → `render` → `present` do core, com `HostVideo` quando houver ffmpeg.
    - (b) **Com aprovação do usuário:** registrar uma D-XX que emende a D-4 e descreva o desenho atual: uma amostra compartilhada entre a UI e a tela, tipos do core, `ThemeRuntime` só para o vídeo e o vídeo no PC adiado com alvo. Nesse caso, corrigir a referência órfã do todo. A duplicação continua como WARN (DRY).

## Warnings
- **W1. Teclado: as setas nas abas também movem o elemento selecionado (D-2026-09-30-studio-app-6, "keyboard operable").**
  - O atalho global (`app.js:382-401`) só ignora `INPUT`, `TEXTAREA` e `SELECT` (`shortcuts.js:4,15`) e trata setas como `nudge` (`shortcuts.js:22-24`).
  - As abas da biblioteca (`library.js:63-69`) e as subabas da Tela (`storage.js:149-155`) usam ←/→ e não param o evento. Com um elemento selecionado, percorrer as abas o move e suja o documento.
  - **Reproduzido:** com CPU selecionado, →→← na lista de abas levou X de 90 para 91, e o status virou "Alterações não salvas".
  - **Correção:** ignorar atalhos quando o foco está em `[role="tab"]`, ou `stopPropagation` nos tablists, com um e2e.
- **W2. D-2026-09-30-studio-app-2 cumprida em parte.**
  - Cumprido: `renderNow` (`app.js:106-128`) mantém um render por vez, junta os pedidos no próximo `requestAnimationFrame`, e o último é exato.
  - Falta: o teto de 30/s não é imposto. Com render rápido pode chegar a ~60/s.
  - **Correção:** intervalo mínimo de ~33 ms entre inícios de render durante um gesto, sem perder o render final.
- **W3. Edições não salvas se perdem sem aviso (frontend-rules § 2, ação destrutiva).**
  - `openTheme`, `newTheme` e `importTheme` (`app.js:294-338`) chamam `store.load`, que zera o histórico (`store.js:259-266`). Não há confirmação nem como desfazer.
  - Fechar a janela fora do modo ao vivo encerra o app mesmo com o tema sujo (`lib.rs:95-109` só intercepta quando está ao vivo).
  - **Correção:** um diálogo in-app "Salvar alterações?" quando `store.isDirty()`, e o mesmo no `CloseRequested`.
- **W4. Erros do backend chegam em inglês na UI pt-BR.**
  - Os comandos com `UiResult<String>` devolvem texto fixo, que o toast mostra via `toast.error`. Exemplos: `backend.rs:134` ("no screen chosen"), `:161` ("brightness is 0 to 100"), `:311`, `:356`, `:89-94`, `studio.rs:398-404`, `commands.rs:161`.
  - A aba de armazenamento já usa códigos traduzidos. Os avisos do importador também vêm em inglês, como o SUMMARY reconhece.
  - **Correção:** códigos de erro como os do `StorageResult`, ou registrar como todo com alvo.
- **W5. A bandeja não cumpre o aceite da T-5.4.** O PLAN pede "tray (show/hide, live toggle, quit)", mas `tray.rs:22-28` só tem "Abrir o Bezel" e "Sair". A D-4 continua cumprida (a bandeja segura a tela até Sair). **Correção:** adicionar o liga/desliga do ao vivo (e ocultar), ou ajustar o aceite no PLAN.
- **W6. `open_theme` aceita qualquer caminho da webview (defesa em profundidade).**
  - `commands.rs:146-149` repassa a string para `Backend::open` (`backend.rs:267-277`, "or any theme file").
  - Um `save_theme` sem `save_as` grava de volta nesse local (`backend.rs:284-285`, `library.rs:87-92`).
  - Com a CSP estrita e sem HTML dinâmico o risco é baixo. Ainda assim, o ideal é restringir o caminho aos itens de `list_themes` e aos locais da sessão (último salvo, diálogo).
  - O mesmo vale para `prepare_upload(source)`, que é do escopo da `storage-video`.
- **W7. O lock da sessão fica preso durante a E/S da tela.**
  - `Backend::tick` segura o mutex de `Studio` durante amostra, render e `present` (`backend.rs:441-450` → `studio.rs:414-419`).
  - Na 8.8" deu cerca de 260 ms por refresh: 7 refreshes em 8,8 s com `refreshSeconds` 1.0, pelo SUMMARY § Hardware validation. Enquanto isso, `render_preview` espera, e o arraste no modo ao vivo trava a cada segundo.
  - O laço também dorme depois do trabalho (`lib.rs:267-270`), então o período real é trabalho + espera.
  - **Sugestão:** renderizar e apresentar fora do lock (clonar tema e assets) e agendar pelo relógio.
- **W8. PLAN, SUMMARY e docs desatualizados.**
  - O § Files modified do PLAN cita `ui/{overlay,topbar,statusbar,color}.js` e `editor/commands.js`, que não existem. Faltam nele `ui/{dom,dragdrop,icons}.js` e `editor/widgets.js`.
  - `b821b18` alterou `.github/workflows/ci.yml` (appindicator), fora do PLAN. É justificado, mas não está registrado.
  - O SUMMARY diz 94,94% de cobertura, e hoje são 94,92%, porque `tests/hardware.rs` entrou depois.
  - O CHANGELOG `[Unreleased]` não tem entrada para o editor do studio (ao vivo, bandeja, início com o sistema, vertical/horizontal, importação pela UI). Só a aba de armazenamento aparece. O README não tem seção sobre o studio.
- **Menores (informativo).**
  - axe moderado `page-has-heading-one` e menor `aria-allowed-role` no `<footer role="status">`; sem skip link.
  - O e2e só roda em pt-BR. Rodei à parte em en-US e com `reduced-motion`: `lang="en"`, textos em inglês, sem erros, transições em `0s` com `reduce`.
  - `app.js:173` repete o 0,25 s de `MIN_REFRESH` (o todo de render-engine já cobre).
  - A cola Tauri tem pouca cobertura, e a regra "esconder ao fechar quando ao vivo" (`lib.rs:95-109`) não tem teste.

## DoD Checklist (gate 8)
| # | Criterion | Source | Type | Status | Evidence |
|---|---|---|---|---|---|
| 1 | `cargo test --workspace` exits 0 | PROJECT | Auto | PASS | 509 passed, 0 failed, 7 ignored |
| 2 | Coverage >= 80% of lines | PROJECT | Auto | PASS | comando do DoD: TOTAL 94.86%, `OK` |
| 3 | No `TODO`/`FIXME` without issue | PROJECT | Auto | PASS | `OK` |
| 4 | CHANGELOG.md updated per release | PROJECT | Manual | MANUAL_REQUIRED | não houve release. O Unreleased não cita o editor do studio (W8). Evidência sugerida: bullet do Bezel Studio (editor, ao vivo, bandeja, início com o sistema) e `## [x.y.z]` no release |
| 5 | README describes current behavior | PROJECT | Manual | MANUAL_REQUIRED | o Status cita o editor, mas não há seção de uso do studio. Evidência sugerida: seção "Bezel Studio" revisada no PR |
| 6 | Drag a widget onto the canvas creates and selects it | CONTEXT | Auto | PASS | `npx playwright test -g "drag a widget onto the canvas"`: `OK` (claro e escuro) |
| 7 | Keyboard move + undo restores the position | CONTEXT | Auto | PASS | `-g "keyboard move and undo"`: `OK` |
| 8 | Editor store commands covered ≥ 80% | CONTEXT | Auto | PASS | `src/editor/store.js` LH 279 / LF 279 (100%): `OK` |
| 9 | `render_preview` returns an RGBA frame of the canvas size | CONTEXT | Auto | PASS | `backend::tests::render_preview_returns_the_canvas_size` (1 passed): `OK`. O teste confere o cabeçalho 480×1920 e `8 + 480*1920*4` bytes |
| 10 | Tema desenhado no studio aparece ao vivo na 8.8" e segue pela bandeja após fechar a janela | CONTEXT | Manual | MANUAL_REQUIRED | o teste opt-in do backend passou na 8.8" (SUMMARY), mas a janela não foi validada. Evidência sugerida: captura do studio com um tema editado + foto ou descrição da 8.8"; depois fechar a janela, conferir que a tela segue atualizando e que o ícone da bandeja reabre a janela, e usar "Sair" |

## Recommendation
**Bloqueada por um item: B1.**
- Todo o resto automático passa. Build, 509 testes, 94,9% de cobertura, lint limpo e audit limpo.
- A superfície Tauri está enxuta: 31 comandos por nome, CSP estrita validada no navegador, sem `shell`/`fs`.
- A UI passa em axe em claro e escuro, tem i18n com paridade e respeita `prefers-reduced-motion`.

Para liberar:
1. Resolver B1 pela opção (a), com o studio dirigindo um `ThemeRuntime` do core, ou pela (b), com uma D-XX aprovada pelo usuário que emende a D-4. Na (b), corrigir a referência órfã do todo.
2. Corrigir W1 (setas nas abas) com um e2e. É barato e atinge a regra "keyboard operable" da D-6.
3. Tratar W3, W4 e W5, ou registrá-los como todo com alvo. Ajustar W8 (PLAN, SUMMARY, CHANGELOG).
4. Fechar o item 10 na 8.8" com o app instalado por `scripts/install-local.sh` e registrar no SUMMARY § Hardware validation.

Resolvido o B1, a fase vai para `APPROVED_PENDING_MANUAL` (itens 4, 5 e 10), com os W restantes como avisos.
