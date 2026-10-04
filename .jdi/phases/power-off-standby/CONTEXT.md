# Phase 12: Tela com o computador desligado (apagada, vídeo do cartão ou álbum) — Context  (slug: power-off-standby)

## Goal
Por tela: apagar, vídeo guardado em loop ou álbum do cartão quando o PC desliga (Linux e Windows), com plano B na tela.

## Locked decisions
- D-1: pedido e fatos medidos no 8.8".
- D-2: rev C; `keep`/`off`/`video`/`album` no catálogo; mudar = tela conectada + `Confirm::Yes` + plano B (OPTIONS inteiro).
- D-3: inibidor *delay* + `PrepareForShutdown`; Windows `RunEvent::Exit` + `SM_SHUTTINGDOWN`; estado final; `bezel-power`.
- D-4: álbum = `sd/image`; foto com EXIF, enquadrada, PNG nativo.
- D-5: keepalive rev C: 30 s sem tráfego → parcial mínimo.
- D-6: Tela › Ajustes; `bezel standby`; provas.
- D-7/D-8: contagens mínimas (≥ N); Windows = teste + `rust-windows` do CI.

## Canonical refs
- Card de 2026-10-03; D-1..D-8; device-protocols-2; gif-sticker-search-15; `protocol-turing-rev-c.md` §6.2, §16, §19, §20

## Out of scope
- `.jdi/todos/2026-10-03-power-off-standby.md`; slideshow: fase `slideshow-builder`.

## Definition of Done

### Auto-verifiable
- [ ] Core e driver rev C (simulador): plano B e ações = sequência exata dos pacotes, nenhum outro; OPTIONS inteiro; keepalive; impossível→TURNOFF
      **Verify:** `cargo test -p bezel-devices --locked --lib -- driver::turing_rev_c::tests::standby_ 2>&1 | grep -qE 'test result: ok\. ([4-9]|[1-9][0-9]+) passed' && cargo test -p bezel-core --locked --test standby 2>&1 | grep -qE 'test result: ok\. ([6-9]|[1-9][0-9]+) passed' && echo OK`
      **Source:** CONTEXT
- [ ] Linux (`setup` real, `dbus-daemon` privado, logind falso): 1 inibidor *delay*; `true` aplica e só então fecha o fd; `keep` e após a ação (ticks, reconexão): zero chamadas (contorno deliberado fora, D-6); `liveScreen` fica; prazo com tela travada; `false` retoma
      **Verify:** `cargo test -p bezel-studio --locked --lib -- power::tests::linux_ 2>&1 | grep -qE 'test result: ok\. ([6-9]|[1-9][0-9]+) passed' && echo OK`
      **Source:** CONTEXT
- [ ] Windows: o `RunEvent::Exit` de uma sessão que acaba aplica, sair e outros eventos não; `rust-windows` do CI verde no último commit de código (D-8)
      **Verify:** `cargo test -p bezel-studio --locked --lib -- --exact power::tests::a_session_end_applies_the_choice_and_a_quit_does_not tests::only_the_exit_event_of_an_ending_session_applies_the_choice 2>&1 | grep -q 'ok. 2 passed' && s=$(git log -1 --format=%H -- . ':!.jdi') && i=$(gh run list -w CI -b jdi/power-off-standby -L 30 --json databaseId,headSha -q "map(select(.headSha==\"$s\"))[0].databaseId") && gh run view "$i" --json jobs -q '.jobs[]|select(.name|endswith("rust-windows")).conclusion' | grep -qx success && echo OK`
      **Source:** CONTEXT
- [ ] Studio: com `Confirm::No` zero chamadas e catálogo igual; `keep` desfaz; lê o catálogo compartilhado; álbum lista, adiciona enquadrado na orientação da tela, substituir e remover só confirmados
      **Verify:** `cargo test -p bezel-studio --locked --lib -- standby::tests 2>&1 | grep -qE 'test result: ok\. ([5-9]|[1-9][0-9]+) passed' && echo OK`
      **Source:** CONTEXT
- [ ] CLI (binário e catálogo em disco): sem `--yes` nada chega à tela nem ao registro; com `--yes` o brilho e depois o plano B, e o registro que o studio lê; `album add` horizontal e vertical, com EXIF
      **Verify:** `cargo test -p bezel --locked --test standby 2>&1 | grep -qE 'test result: ok\. ([5-9]|[1-9][0-9]+) passed' && cargo test -p bezel --locked --lib -- --exact standby::tests::set_with_yes_sends_the_brightness_then_the_plan_b standby::tests::set_without_yes_opens_nothing_and_says_so 2>&1 | grep -q 'ok. 2 passed' && echo OK`
      **Source:** CONTEXT
- [ ] UI: lógica pura, i18n e os 5 testes nomeados nos 4 projetos com axe
      **Verify:** `set -o pipefail; cd apps/bezel-studio && node --test --test-reporter=tap tests/ui/standby.test.mjs | awk '/^# pass [1-9]/{p=1} /^# fail 0$/{f=1} END{exit !(p&&f)}' && npm run test:unit >/dev/null && node scripts/e2e-passed.mjs "standby › four choices, each explained" "standby › plan B asks first" "standby › keep undoes" "standby › album: vertical and horizontal" "standby › album: remove asks first" && echo OK`
      **Source:** CONTEXT
- [ ] Guardas: comandos, fonte, sem logger, início silencioso; o monitor do barramento vê só mensagens ao logind (D-6); nenhum pacote externo novo
      **Verify:** `cargo test -p bezel-studio --locked --lib -- --exact tests::every_command_is_allowed_by_name tests::nothing_in_the_app_forges_an_invocation tests::the_studio_installs_no_logger tests::the_app_setup_sends_nothing_at_start 2>&1 | grep -q 'ok. 4 passed' && cargo test -p bezel-power --locked --lib -- --exact logind::tests::speaks_only_to_logind_on_its_bus 2>&1 | grep -q 'ok. 1 passed' && ! git diff "$(git merge-base HEAD origin/main)" -- Cargo.lock | grep '^+name = ' | grep -qv '"bezel-power"' && bash scripts/ci/studio-starts-silent.sh | grep -q '^studio-starts-silent: OK: ' && echo OK`
      **Source:** CONTEXT
- [ ] Guia en/pt-BR, §19/§20 do protocolo, CHANGELOG e check-docs
      **Verify:** `f=docs/reverse-engineering/protocol-turing-rev-c.md; grep -q 'bezel standby' docs/user/power-off.md && grep -q 'bezel standby' docs/user/pt-BR/power-off.md && grep -q '"power-off.md"' scripts/ci/check-docs.sh && [ "$(sed -n '/^## 19\./,/^## 20\./p' $f | grep -cE '^\| (Start mode [12]|Sleep timer|RESTART 0x84|Video after the host) ')" -ge 5 ] && ! grep -q 'which file start modes' $f && sed -n '/^## \[Unreleased\]/,/^## \[[0-9]/p' CHANGELOG.md | grep -qiE 'shuts? down' && bash scripts/ci/check-docs.sh >/dev/null && echo OK`
      **Source:** CONTEXT

### Manual
- _(none)_

## Deferred to PR review
- Desligar de verdade com o 8.8" em cada opção; religar retoma o tema.
- Desligar de verdade no Windows.
- Visual: a escolha e o gerenciador do álbum.
- 8.8": fotos do álbum em pé nas duas orientações; tema parado com timer de 1 min não dorme (D-5).

## Notes
- `install-local.sh` ao fim; só fakes.
