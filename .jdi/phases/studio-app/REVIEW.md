# Phase 5: Review  (slug: studio-app)

**Verdict:** APPROVED_PENDING_MANUAL

> Reverificação em modo `verify` depois do BLOCKED de `7616bb7` (2026-09-30). `HEAD` = `8fc6cb7`. A árvore está
> limpa, exceto pela remoção local do REVIEW.md anterior, que este arquivo substitui.
>
> - Rodei todos os gates do zero sobre a árvore inteira, incluindo T-7.2, T-7.3 e T-7.4 da `release-polish`, já
>   integradas. Nenhum número veio do SUMMARY.
> - O escopo julgado é o da fase: T-5.1..T-5.6, os commits de correção `71fb204..8fc6cb7`, o DoD do CONTEXT e as
>   decisões D-2026-09-30-studio-app-1..7.
> - Nenhum comando abriu a 8.8" real, que segue com o serviço do usuário. Não rodei nenhum `#[ignore]`. O teste opt-in
>   de hardware foi rodado pelo orquestrador (SUMMARY § Hardware validation).

## Gates
| Gate | Status | Details |
|---|---|---|
| Build | PASS | `cargo build --workspace --locked`: ok, lock aceito (rustc 1.98.1) |
| Tests | PASS | 578 passed, 0 failed, 7 ignored (4 de ffmpeg real, 1 de timing, 1 de corpus, 1 de hardware do studio opt-in). Eram 509 no review anterior e o SUMMARY diz 578, então não houve queda. `bezel-studio`: 59 unitários |
| Coverage | PASS | **94.93%** lines (TOTAL, sem `main.rs`/`build.rs`), exit 0. Pelo comando do DoD, sem filtro: 94.87%. `runtime.rs` do core: 100%. No studio: `studio.rs` 97.70%, `backend.rs` 97.59%, `library.rs` 99.56%. A cola Tauri segue baixa: `commands.rs` 2.02%, `lib.rs` 23.04%, `tray.rs` 22.55% |
| Lint | PASS | `cargo fmt --all --check` e `cargo clippy --workspace --all-targets --locked -- -D warnings`: exit 0. Nenhum `#[allow]` fora de teste |
| Hexagonal/Safety/Protocol/Hygiene | PASS | 5.1–5.11 limpos. B1 resolvido: o studio dirige o `ThemeRuntime` do core. Sobra uma duplicação pequena entre os adapters de entrada (W4) |
| Consistency | PASS (com WARN) | D-2026-09-30-studio-app-4 cumprida, D-7 registrada e commits com escopo `studio-app`. Listas de arquivos e docs com lacunas menores (W3) |
| UI Validation | PASS (com WARN) | `npm ci` ok. `npm run test:unit`: 65/65, 99,81% de linhas. `npx playwright test`: 30/30 em claro e escuro, sem erro de console e sem axe sério ou crítico. Checagem extra em en-US, com CSP e `reduced-motion`: limpa |
| DoD | PASS / PENDING MANUAL | Os 3 Auto do PROJECT e os 4 Auto do CONTEXT passam como escritos. Faltam 2 Manual do PROJECT e 1 do CONTEXT |

### Achados do review anterior
| Achado | Situação | Evidência |
|---|---|---|
| **B1** o ao vivo não rodava o `ThemeRuntime` | **Resolvido** (opção a) | Ver o detalhe logo abaixo |
| W1 setas nas abas moviam a seleção | Resolvido | `shortcuts.js` ignora setas quando o foco está num widget com papel ARIA que usa setas (`ARROW_ROLES`, `usesArrows`). Cobrem isso o e2e "arrow keys on tabs switch tabs and leave the element alone" (abas da biblioteca e subabas da Tela) e o unitário "arrows stay with widgets that move with them" |
| W2 teto de 30 renders/s | Resolvido | `render-scheduler.js`: um render por vez e no máximo `1000/30` ms entre inícios durante o gesto. O pedido que encerra o gesto sempre renderiza. Há 5 testes unitários, entre eles "at most 30 renders start per second" e "the request that ends the gesture always renders" |
| W3 edições perdidas sem aviso | Resolvido em parte | Diálogo in-app Salvar/Descartar/Cancelar (`ui/dialog.js`, `<dialog>` modal, foco inicial em Salvar, Esc cancela e o foco volta). Aparece antes de abrir, criar ou importar tema (`settleUnsaved`) e no `CloseRequested` fora do modo ao vivo (`lib.rs:70`, `OnClose::Ask`). Há 2 e2e. **Sobra a bandeja** (W2 abaixo) |
| W4 erros do backend em inglês | Registrado com alvo | D-2026-09-30-release-polish-6 define códigos estáveis mais argumentos. A T-7.6 da `release-polish` (pending) tem os testes `backend-messages.test.mjs` e `backend-codes.json`. Continua como W1 abaixo |
| W5 bandeja sem liga/desliga do ao vivo | Resolvido | `tray.rs`: Abrir, Ocultar, "Ao vivo na tela" (check que segue a sessão, alternado fora da thread principal) e Sair. Teste `the_tray_switches_live_mode_on_the_connected_screen` |
| W6 `open_theme` aceitava qualquer caminho | Resolvido | `ThemeLibrary::allows` só aceita as pastas da biblioteca (caminho absoluto sem `..`) e o que o usuário escolheu no diálogo nesta sessão. `save` recusa um destino não escolhido. Um save simples de um tema de fora vira cópia na pasta do usuário. Teste `the_window_opens_and_saves_only_where_allowed`. Fica de fora `prepare_upload(source)`, que é da `storage-video` (informativo) |
| W7 lock preso durante a E/S da tela | Resolvido | O frame é renderizado sob o lock e sai como `Delivery`. `Backend::deliver` (`backend.rs:176-186`) chama `present()` fora do lock e devolve o link, e um condvar acorda quem espera o link. O `Pacer` agenda pelo relógio. Teste `previews_render_while_the_screen_shows_a_frame`. Na 8.8" o orquestrador mediu 8 atualizações em 8,0 s (antes eram 7 em 8,8 s) |
| W8 PLAN, SUMMARY e docs | Resolvido em parte | README ganhou a seção "Bezel Studio", o CHANGELOG ganhou o bullet do studio e o PLAN lista os arquivos reais da entrega original. Sobram lacunas nos arquivos da correção e nas docs (W3 abaixo) |

#### Como o B1 foi resolvido
- **O que mudou no core:** o `ThemeRuntime` ganhou o que um editor precisa. São `replace_theme`, `render_with` com `Backdrop` explícito, `snapshot`/`quantities`/`use_catalog`, `forget_screen` e `add_asset`, em `crates/bezel-core/src/app/runtime.rs:274-320` e `:453`. Os testes estão em `crates/bezel-core/tests/runtime_editor.rs`.
- **O que o `Studio` faz agora:** dirige um único runtime.
  - Amostra: `runtime.sample` (`studio.rs:229`).
  - Edição: `replace_theme` (`:263`), que mantém os históricos.
  - Preview: `render_with(Poster)` (`:340-343`).
  - Tela: `runtime.render` com o tempo do vídeo (`:517-520`).
  - Vídeo: `start_video` com `HostVideo` quando a tela não toca vídeo (`:481` e `decode_here`, `:630-652`).
- **Onde o `ThemeRuntime` não fica sozinho:** o laço próprio e os campos `histories`/`snapshot`/`quantities` saíram do adapter. O studio só não usa `runtime.show`, porque apresenta fora do lock.
- **Testes:** `the_live_screen_and_the_preview_share_one_runtime`, `a_screen_without_playback_gets_the_video_decoded_here` e `a_video_decoded_here_sets_the_pace_and_samples_keep_theirs`.
- **Decisão:** a D-2026-09-30-studio-app-7 registra que o vídeo decodificado no PC para WCH entra no 1.0 e emenda o backlog da D-2026-09-30-release-polish-1. A referência órfã do todo saiu (`.jdi/todos/2026-09-30-release-polish.md`).

### Detalhe do gate 5
| Check | Resultado |
|---|---|
| 5.1 dependências do core | PASS: só `thiserror` |
| 5.2 I/O e threads no core | PASS: nada. As APIs novas do runtime são puras |
| 5.3 ports | PASS. Nenhum dos 10 ports de `bezel_core::ports` (incluindo o novo `DesktopModeHid`) é implementado no core. Traits fora do core: `Pause`, `Clock`, `Wire`, `Pace` e `MediaSetup`, que são auxiliares de adapter e não são novas |
| 5.4 adapters na composição | PASS: nada fora de `main.rs`/`lib.rs`/testes. `compose()` em `lib.rs` é a raiz |
| 5.5 `unsafe` | PASS: só a string `"unsafe asset path"` (`bezel-themes/src/native.rs:39`) |
| 5.6 panics | PASS. Fora de `#[cfg(test)]` só aparecem `bezel-sensors/src/testing.rs` e `bezel-render/src/{golden,testkit}.rs`, que são módulos declarados sob `#[cfg(test)]`. No studio, os panics estão todos depois do `#[cfg(test)]` de cada arquivo |
| 5.7 escrita no dispositivo | PASS. Fora de teste, `Confirm::Yes` aparece só em doc, em `Confirmed::require`, no novo `MonitorModeConfirmed::require` (`discovery.rs:310`, mesmo padrão) e em `commands.rs`. As ocorrências em `turing_usb.rs:877`, `turing_rev_c.rs:861`, `fake.rs:582-643` e `hid_desktop.rs:412` ficam dentro de `mod tests`. `tests/hardware.rs` tem `#[ignore]` e sai sem `BEZEL_HW_TESTS` (`:76-79`) |
| 5.8 protocolo | PASS: todo encoder tem teste. Conferi duas constantes da T-7.3 contra a spec: `FILLER = 0x2C` (`turing_rev_c.rs:294`) é o `2c` x 249 + `00` de protocol-turing-rev-c.md § 13.4, e `data_phase_len` = `ceil(len/249)` blocos de 250 (§ 2.2). `MONITOR_MODE_MAGIC` `5f3759df` (`hid_desktop.rs:44`) bate com protocol-turing-usb.md:325 |
| 5.9 caminhos no core | PASS: só doc (`device.rs:75`, `discovery.rs:16`, `sensor.rs:7,216,259,266`) e testes |
| 5.10 comandos síncronos | PASS. `list_fonts`, `get/set_autostart`, `cancel_job`, `set_unsaved` e `close_window` não tocam tela, sensor nem renderer. Ver a nota sobre `close_window` nos informativos |
| 5.11 supply chain | PASS: `cargo audit` 0 (1277 advisories, 604 crates), também com `--deny warnings`. Nenhum segredo |

### Superfície Tauri (segurança)
- **Capabilities:** 33 `allow-*` em `capabilities/default.json`. Batem um a um com o `COMMANDS` de `build.rs` e com o `generate_handler!` de `lib.rs` (conferido por `diff`). Os novos são `set_unsaved` e `close_window`. Além deles só há `core:event:default`, sem `shell` e sem `fs`.
- **CSP:** a de `tauri.conf.json:24` não mudou. Servi a UI com esse cabeçalho no Playwright, em claro e escuro, e não houve nenhuma `securitypolicyviolation`. Um `fetch` externo foi bloqueado (`connect-src`), o que mostra que a CSP estava ativa.
- **Caminhos:** `open_theme` e `save_theme` agora estão restritos (W6 resolvido).

### UI (gate 7)
- **Suíte do app:** 65 unitários com piso de 80% (`store.js` 282/282) e 30 e2e com axe em claro e escuro.
- **Checagem extra** (spec temporário no scratchpad, fora do repositório), em en-US com a CSP real:
  - `lang="en"` e nenhuma palavra em pt-BR na tela.
  - O diálogo de alterações não salvas aparece em inglês, com axe limpo.
  - Tab circula só entre Fechar, Cancelar, Descartar e Salvar (modal nativo). Esc fecha e devolve o foco ao botão que abriu.
  - Setas nas abas não mudam o X do elemento.
- **Reduced motion:** com `reduced-motion: reduce`, nenhuma transição ou animação fica com duração diferente de zero.
- **Teclado:** o atalho global não age com um `dialog[open]` (`app.js:418`).

## Blockers
Nenhum.

## Warnings
- **W1. Mensagens do backend ainda chegam em inglês na UI pt-BR (D-2026-09-30-studio-app-6; W4 anterior).**
  - A correção acrescentou mais duas: `backend.rs:379` ("… is not in the theme library; import it instead") e `backend.rs:404` ("… was not picked to save to").
  - **Está registrado de forma aceitável:** D-2026-09-30-release-polish-6 fixa o critério (códigos estáveis mais argumentos, traduzidos pela UI). A T-7.6 da `release-polish` tem arquivos, aceite e testes definidos (`backend-codes.json` conferido por teste Rust, `backend-messages.test.mjs`).
  - Fica como aviso até a T-7.6. Ela precisa cobrir também as duas mensagens novas de `open`/`save`.
- **W2. Sair pela bandeja descarta edições não salvas sem perguntar (frontend-rules § ação destrutiva; resto do W3).**
  - `tray.rs:134` faz `QUIT => app.exit(0)`, mas o backend já sabe se há edições pendentes (`Unsaved`, `commands.rs:231`).
  - **Caminho real:** ao vivo ligado, fechar a janela a oculta e mantém as edições (`lib.rs:70`, `on_close(true, true) == Hide`). Em seguida, "Sair" na bandeja encerra o app e as edições se perdem.
  - **Correção:** no `QUIT`, com `Unsaved`, mostrar a janela e emitir o pedido para a UI perguntar. A resposta "Descartar"/"Salvar" então encerra o app, o que pede uma variante "sair" do `close_window`, que hoje só oculta quando está ao vivo. Alternativa: tratar `RunEvent::ExitRequested`. Um teste de `on_close` com a origem "sair" cobre a regra.
- **W3. PLAN, SUMMARY e docs com lacunas depois da correção (resto do W8).**
  - **Arquivos fora do PLAN não sinalizados:** a correção tocou `crates/bezel-core/src/app/{mod,runtime}.rs`, `crates/bezel-core/tests/runtime_{editor,video}.rs`, `crates/bezel-cli/src/live.rs` (`71fb204`), `src/render-scheduler.js`, `src/ui/dialog.js` e `tests/ui/render-scheduler.test.mjs`. O PLAN não cita nenhum deles. A lista "fora do PLAN" do SUMMARY § Files modified só traz `bezel-render` e `bezel-themes`. O `ci.yml` de `b821b18` também segue sem registro.
  - **Cobertura no SUMMARY:** diz 94,66%, e hoje são 94,87% pelo comando do DoD (94,93% no gate).
  - **`release-polish/CONTEXT.md` desatualizado:** as linhas 7 e 24 ainda põem "vídeo no host (WCH)" no backlog, contra a D-2026-09-30-studio-app-7.
  - **CHANGELOG:** o `[Unreleased]` só diz que o `bezel run` decodifica vídeo no PC (`CHANGELOG.md:62-65`). O bullet do studio não cita isso, e a D-7 põe a função no 1.0.
- **W4. Duas regras pequenas ainda duplicadas entre os adapters de entrada (DRY; resto do B1).**
  - **Tema cabe no painel nessa orientação:** `studio.rs:614` (`fits`) repete `live.rs:112-122`. Já está no todo de render-engine ("a core function for the theme fits this panel").
  - **Cadência de amostra com vídeo no PC:** a regra "amostra a cada refresh do tema, quadros a `HOST_VIDEO_FPS`" aparece em `studio.rs:599-610` (por contagem) e em `live.rs:305-339` (`stream_frames`, por relógio).
  - **Sugestão:** um auxiliar no core (por exemplo `ThemeRuntime::sample_due(elapsed)`) na T-7.1, que já é a tarefa de refatorar core, CLI e studio.
- **Menores (informativo).**
  - **Lock na thread principal:** `close_window` (síncrono) e o `CloseRequested` pegam o lock da sessão na thread principal. O lock fica preso durante `start_live_video`, que grava a cópia do vídeo e consulta a tela ao (re)iniciar o vídeo. A espera é rara e curta, mas pode travar a janela por um instante nesse momento.
  - **Preview e tela:** o preview também troca o tema do runtime (`backend.rs:340-345`). Assim, estados no meio de um arraste podem aparecer na tela no próximo tick. Isso vai além do "committed edit" da D-4 sem contrariá-la.
  - **Sem hardware WCH:** a D-7 entrega o vídeo decodificado no PC validado só com fakes (WeAct como tela sem reprodução). A decisão diz isso explicitamente.
  - **Aprovação da D-7:** o reviewer não consegue confirmar que a D-2026-09-30-studio-app-7 (mudança de escopo do 1.0) teve o aval do usuário. O orquestrador deve confirmar.
  - **axe:** continuam o moderado `page-has-heading-one` e o menor `aria-allowed-role` (`<footer role="status">`).
  - **Cola Tauri:** `commands.rs`, `lib.rs` e `tray.rs` seguem com pouca cobertura. As regras (`on_close`, `labels`) têm teste.

## DoD Checklist (gate 8)
| # | Criterion | Source | Type | Status | Evidence |
|---|---|---|---|---|---|
| 1 | `cargo test --workspace` exits 0 | PROJECT | Auto | PASS | 578 passed, 0 failed, 7 ignored |
| 2 | Coverage >= 80% of lines | PROJECT | Auto | PASS | comando do DoD: TOTAL 94.87%, `OK` |
| 3 | No `TODO`/`FIXME` without issue | PROJECT | Auto | PASS | `OK` |
| 4 | CHANGELOG.md updated per release | PROJECT | Manual | MANUAL_REQUIRED | Não houve release. O `[Unreleased]` já tem o bullet do Bezel Studio, sem a decodificação no PC (W3). Evidência sugerida: `## [x.y.z]` no corte da 1.0 (T-7.8) |
| 5 | README describes current behavior | PROJECT | Manual | MANUAL_REQUIRED | Seção "Bezel Studio" nova (`README.md:14-35`). Evidência sugerida: revisão humana do diff do README no PR |
| 6 | Drag a widget onto the canvas creates and selects it | CONTEXT | Auto | PASS | `npx playwright test -g "drag a widget onto the canvas"`: `OK` (claro e escuro) |
| 7 | Keyboard move + undo restores the position | CONTEXT | Auto | PASS | `-g "keyboard move and undo"`: `OK` |
| 8 | Editor store commands covered ≥ 80% | CONTEXT | Auto | PASS | `src/editor/store.js` LH 282 / LF 282 (100%): `OK` |
| 9 | `render_preview` returns an RGBA frame of the canvas size | CONTEXT | Auto | PASS | `backend::tests::render_preview_returns_the_canvas_size` (1 passed): `OK` |
| 10 | Tema desenhado no studio aparece ao vivo na 8.8" e segue pela bandeja após fechar a janela | CONTEXT | Manual | MANUAL_REQUIRED | O teste opt-in do backend passou na 8.8" depois das correções (SUMMARY), mas a janela continua sem validação. Evidência sugerida: com `scripts/install-local.sh`, captura do studio com um tema editado e foto ou descrição da 8.8". Depois fechar a janela, conferir que a tela segue atualizando, alternar "Ao vivo na tela" pela bandeja, reabrir pelo ícone e usar "Sair". Registrar em SUMMARY § Hardware validation |

## Recommendation
**Aprovar com pendências manuais.**
- O B1 foi resolvido do jeito preferido. O studio dirige o mesmo `ThemeRuntime` do `bezel run`: amostra, render, troca do tema com históricos e vídeo com `HostVideo`. O core ganhou só API pura e testada, com `runtime.rs` a 100%.
- W1, W2 (30/s), W5, W6 e W7 do review anterior foram corrigidos, com teste.
- Os gates automáticos passam na árvore inteira: 578 testes, 94,9% de cobertura, lint e audit limpos, 30 e2e com axe e checagem extra em en-US, com CSP e `reduced-motion`.

Antes do `/jdi-confirm-dod` ou junto dele:
1. Fechar o item 10 na 8.8" com a janela real e registrar no SUMMARY.
2. Tratar W2 (Sair pela bandeja com edições não salvas). É barato e fecha o W3 do review anterior.
3. Ajustar W3 nas docs: SUMMARY com os arquivos fora do PLAN e a cobertura atual, `release-polish/CONTEXT.md` alinhado à D-7, CHANGELOG com a decodificação no PC pelo studio.
4. Levar W1 e W4 para a T-7.6 e a T-7.1 da `release-polish`, onde já têm lugar.
