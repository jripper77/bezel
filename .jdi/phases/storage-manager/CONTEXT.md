# Phase 8: Gerenciador de armazenamento — Context  (slug: storage-manager)

## Goal
Gerenciar interna e cartão SD (studio e CLI): lado a lado com miniaturas, multi-seleção, ordenar/filtrar, mover/renomear por reenvio da cópia local, catálogo, assistente de limpeza e restaurar cartão.

## Locked decisions
- D-1/D-4: aba Armazenamento em largura total (interna × cartão, arrastar = mover); mesmas operações em `bezel storage`.
- D-2: cópia local exata de todo envio (≤ 25 MiB rev C) + metadados e miniatura; mover/renomear/restaurar = reenvio + apagar verificado.
- D-3: limpeza do que o Bezel não enviou, só com a lista exata confirmada; associar original do PC por tamanho.
- D-5: catálogo em `<data>/bezel/storage/` (`catalog.json`, `files/<sha256>`, `thumbs/`); chave `model_id` + nome opcional; cartão pela capacidade (0x64); estados `pending`/`stored`/`missing`/`deleted`; boot registrado.
- D-6: limite 2 GiB só para cópias de apagados pelo Bezel (sai a mais antiga); presentes ou sumidos nunca saem sozinhos; "Limpar cache" lista e confirma.
- D-7: por arquivo: preflight → envia a cópia → confere tamanho → só então apaga a origem; lote para na 1ª falha ou Cancel e a origem fica; nomes no padrão de envio; avisa boot/vídeo de tema.
- D-8: restaurar: soma cabe no livre e teto 25 MiB checados antes do 1º byte; mesmo nome+tamanho pula; conflito só confirmado; nunca apaga.
- D-9: limpeza pura no core: `x.ext.ext`/`x.ext<dígitos>.ext` de tamanho igual, parcial de 29.577.216 B e `pending` pré-marcados; resto só listado; nunca boot nem vídeo de tema.
- D-10: associar por tamanho exato, confirmado par a par, copia para o store; miniatura só da cópia; sem cópia = ícone + "Tocar na tela".
- D-11: TUR_USB: tamanho do catálogo ou "desconhecido"; mover/renomear/apagar/limpeza desabilitados com motivo.
- D-12: CLI `mv`, `rename`, `restore`, `cleanup [--dry-run]`, `catalog`, `cache`; imprime a lista exata e sem `--yes` não muda a tela.
- D-13: multi-seleção e mover por teclado (arrastar opcional), aria-live, pt-BR/en, demo com o cartão real.

## Canonical refs
- `.jdi/decisions/D-2026-09-30-storage-manager-{1..14}.md`, release-polish-{10,12,13}, storage-video-{1,7}
- `docs/reverse-engineering/protocol-turing-rev-c.md` § 13, 16, 19; `protocol-turing-usb.md` § 6; `devices.md` § 5.4
- `bezel-core` `{domain,app}/storage.rs`, `device_video_name`; `bezel-cli/src/storage.rs`; studio `ui/storage.js`, `src-tauri/src/storage.rs`, demo backend

## Out of scope
- D-14: formatar, firmware, baixar/renomear no device, mkdir, `/tmp/video`, cópia entre telas, converter ao mover, sync "exatamente este conjunto". Backlog: `.jdi/todos/2026-09-30-storage-manager.md`.

## Definition of Done

### Auto-verifiable
- [ ] Mover/renomear só apaga a origem após a cópia verificada; falha ou Cancel mantém a origem e para o lote; restaurar checa espaço e teto antes do 1º byte
      **Verify:** `cargo test -p bezel-core --locked --test storage_manager -- --exact move_deletes_the_source_only_after_the_copy_is_verified a_failed_or_cancelled_move_keeps_the_source_and_stops_the_batch restore_checks_space_and_the_cap_before_sending_anything 2>&1 | grep -q 'ok. 3 passed' && echo OK`
      **Source:** CONTEXT
- [ ] Limpeza na listagem real do usuário acha os pares do fornecedor e o parcial, não `8.8APEX_2.mp4`; nunca sugere boot nem vídeo de tema
      **Verify:** `cargo test -p bezel-core --locked --lib -- --exact domain::cleanup::tests::finds_the_users_vendor_duplicates_and_the_hang_partial domain::cleanup::tests::never_suggests_the_boot_media_or_a_theme_video 2>&1 | grep -q 'ok. 2 passed' && echo OK`
      **Source:** CONTEXT
- [ ] Catálogo casa listagens por tela/cartão; despejo só de cópias de apagados; store mantém os bytes exatos ao recarregar
      **Verify:** `cargo test -p bezel-core --locked --lib -- --exact domain::archive::tests::entries_match_listings_per_screen_and_card domain::archive::tests::eviction_drops_only_copies_of_deleted_files 2>&1 | grep -q 'ok. 2 passed' && cargo test -p bezel-media --locked --lib -- --exact archive::tests::copies_are_the_exact_bytes_sent_and_survive_a_reload 2>&1 | grep -q 'ok. 1 passed' && echo OK`
      **Source:** CONTEXT
- [ ] CLI: `mv`/`rename`/`restore`/`cleanup` sem `--yes` não mudam a tela; `cleanup --dry-run` não apaga
      **Verify:** `cargo test -p bezel --locked --lib -- --exact storage::tests::mv_rename_restore_and_cleanup_without_yes_change_nothing storage::tests::cleanup_dry_run_lists_without_deleting 2>&1 | grep -q 'ok. 2 passed' && echo OK`
      **Source:** CONTEXT
- [ ] Studio (demo, claro/escuro × pt-BR/en, axe): mover por arrastar e teclado após confirmar; limpeza lista duplicatas e só apaga o confirmado
      **Verify:** `set -o pipefail; cd apps/bezel-studio && node --test tests/ui/i18n.test.mjs >/dev/null && npx playwright test -g "storage manager" --reporter=line 2>&1 | grep -E '(^| )([89]|[1-9][0-9]+) passed' && echo OK`
      **Source:** CONTEXT

### Manual
- [ ] Na 8.8" real: mover um `bezel_test_*` interna → cartão → interna, renomear e restaurar no cartão após apagá-lo pelo Bezel, tamanhos verificados; arquivos do usuário intactos
      **Verify:** human confirmation required
      **Evidence:** `bezel storage ls` antes/depois e saída de `mv`/`rename`/`restore --yes`; SUMMARY.md § Hardware validation
      **Source:** CONTEXT
- [ ] Na 8.8" real: studio e `cleanup --dry-run` listam os pares do fornecedor (`demon_open`, `demon`, `NVI`, `Rani`, `m04`) e nada é apagado sem confirmar
      **Verify:** human confirmation required
      **Evidence:** saída do `--dry-run`, captura do studio (confirmação cancelada), `ls` igual antes/depois
      **Source:** CONTEXT

## Notes
- Ordem: core (`domain::archive` = catálogo; `domain::cleanup`; mover/restaurar) → store em `bezel-media` → CLI → studio.
- No hardware só `bezel_test_*`; parcial apagado segue contando como usado até reiniciar (§ 19).
