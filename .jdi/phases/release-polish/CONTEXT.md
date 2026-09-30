# Phase 7: Polimento e release 1.0 — Context  (slug: release-polish)

## Goal
Empacotamento deb/rpm/AppImage/msi/nsis com udev, i18n pt-BR/en completo, documentação de usuário, FPS de jogos e release 1.0.

## Locked decisions
- D-1: entram no 1.0 empacotamento, i18n, docs, FPS, chaves de sensor dos temas importados, HID desktop mode (listar + voltar), cancelamento rev C, refatorações do core; ficam no backlog firmware, boot logo, ffmpeg baixado, vídeo no host (WCH), system.volume.
- D-2: 1.0.0 sai de um commit final `feat!` com rodapé `BREAKING CHANGE:` depois de todo DoD Manual das fases 1-6 confirmado na 8.8"; sem assinatura (SmartScreen documentado); conferir a tag v1.0.0, senão usar o input de versão da esteira.
- D-3: AppImage/CLI/fonte: `bezel udev-rules` imprime a regra e o comando sudo (o app nunca eleva; o studio mostra o comando quando a porta é negada); `bezel-run@.service` vai nos deb/rpm; Windows: usbser, WinUSB (Zadig) para 0x1CBE/0x43A8 e LibreHardwareMonitor para temperatura CPU, só documentados.
- D-4: FPS `gpu.fps` somente leitura: Windows lê a memória compartilhada do RTSS; Linux lê o CSV mais novo do MangoHud. Fixtures golden, `hardware_validated=false`. Fonte ausente ou leitura com mais de 3 s = Unavailable com motivo de como ativar; nunca 0 nem último valor; 0 fps de fonte viva é 0.
- D-5: `net.ping` em tarefa própria (ICMP sem privilégio, senão TCP 53/443; timeout = Unavailable); fan/pump/voltagem/totais de rede via hwmon/LHM; hwmon e amdgpu unificam unidade e potência; `system.volume` = Unavailable.
- D-6: i18n completo: todas as strings do studio em pt-BR/en com paridade, mensagens do backend como código + argumentos traduzidos na UI, idioma do sistema com override salvo; CLI segue em inglês.
- D-7: docs em `docs/user/` (en) e `docs/user/pt-BR/`, incluindo guia de cartão SD em texto; README sem "early development".
- D-8: nada destrutivo sem validação em hardware; HID desktop mode: listar e voltar ao modo monitor sempre com Confirm e rótulo "não validado".
- D-9: dívida de revisão entra como refactor sem mudança de comportamento (lista na decisão).

## Canonical refs
- `.jdi/decisions/D-2026-09-30-release-polish-{1..9}.md`; todos `[release-polish]` em `.jdi/todos/`
- `docs/reverse-engineering/sensors.md` (§ FPS, § 6.1/6.2), `protocol-turing-usb.md` § 10, `protocol-turing-rev-c.md`
- `.github/workflows/ci.yml`, `packaging/linux/`, `scripts/install-local.sh`, `apps/bezel-studio/src-tauri/tauri.conf.json`
- Itens Manual pendentes das fases 1-6 (REVIEW/SUMMARY de cada uma)

## Out of scope
- Firmware, boot logo, ffmpeg embutido/baixado, vídeo no host, system.volume, assinatura de código, detecção de SD por código, baixar/renomear arquivos no device, Sonar (dispensa mantida). Ver `.jdi/todos/2026-09-30-release-polish.md`.

## Definition of Done

### Auto-verifiable
- [ ] Regra udev do pacote bate com o catálogo e `bezel udev-rules` imprime a mesma regra
      **Verify:** `cargo test -p bezel --locked -- --exact udev_rules::tests::printed_rule_matches_packaged_file_and_catalog 2>&1 | grep -q '1 passed' && echo OK`
      **Source:** CONTEXT
- [ ] FPS: RTSS e MangoHud por fixture, ausente/velho = Unavailable com motivo
      **Verify:** `cargo test -p bezel-sensors --locked fps:: 2>&1 | grep -qE '[1-9][0-9]* passed' && echo OK`
      **Source:** CONTEXT
- [ ] `net.ping` com timeout não bloqueia a amostragem (< 50 ms) e as chaves de ventoinha/voltagem/total existem no catálogo
      **Verify:** `cargo test -p bezel-sensors --locked -- ping:: imported_keys_are_published 2>&1 | grep -qE '[2-9][0-9]* passed|[1-9][0-9]+ passed' && echo OK`
      **Source:** CONTEXT
- [ ] Paridade i18n pt-BR/en e mensagens do backend traduzidas por código
      **Verify:** `cd apps/bezel-studio && node --test tests/ui/i18n.test.mjs tests/ui/backend-messages.test.mjs && echo OK`
      **Source:** CONTEXT
- [ ] Studio completo no claro/escuro em pt-BR e en (Playwright + axe)
      **Verify:** `cd apps/bezel-studio && npm test`
      **Source:** CONTEXT
- [ ] HID desktop mode aparece na lista e a volta ao modo monitor exige Confirm
      **Verify:** `cargo test -p bezel-devices --locked -- hid_desktop:: 2>&1 | grep -qE '[1-9][0-9]* passed' && cargo test -p bezel --locked -- hid_desktop_requires_confirm 2>&1 | grep -qE '[1-9][0-9]* passed' && echo OK`
      **Source:** CONTEXT
- [ ] Empacotamento inclui regra udev, unit systemd e temas (config + teste de manifesto)
      **Verify:** `bash scripts/ci/check-packaging.sh && echo OK`
      **Source:** CONTEXT
- [ ] Docs de usuário existem em en e pt-BR, sem links quebrados nem segredos
      **Verify:** `bash scripts/ci/check-docs.sh && echo OK`
      **Source:** CONTEXT

### Manual
- [ ] Na 8.8" real: tema desenhado no studio ao vivo, aba Tela (enviar/tocar/apagar), vídeo com alfa, boot e cancelamento de upload sem sobra de bytes; mais os itens Manual das fases 1-6
      **Verify:** human confirmation required
      **Evidence:** saída de `/jdi-confirm-dod` por fase e fotos/descrição
      **Source:** CONTEXT
- [ ] Instalar deb/rpm/AppImage (Linux) e msi/nsis (Windows) do release candidato e abrir o studio, com FPS em um jogo real (RTSS/MangoHud)
      **Verify:** human confirmation required
      **Evidence:** release v1.0.0 com os 5 pacotes e relato da instalação
      **Source:** CONTEXT

## Notes
- `install-local` após cada tarefa; 8.8" usada na horizontal. Unsafe do RTSS (Windows) exige escopo mínimo com `reason`. Plano cria `scripts/ci/check-packaging.sh` e `check-docs.sh`; se o deb não levar o binário `bezel`, corrigir em `tauri.conf.json`.
