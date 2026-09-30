# Phase 2: Protocolos de dispositivo — Context  (slug: device-protocols)

## Goal
Todos os protocolos (rev A/B/C/D, WeAct A/B, TUR_USB 0x1CBE, WCH 0x43A8, HID) com vetores de teste, transportes serial/USB/HID, wake do MCU, e CLI para brilho, rotação, liga/desliga e envio de imagem validados na 8.8" real.

## Locked decisions
- D-2026-09-30-device-protocols-1: frames RGBA8 na orientação do tema; rev C usa a run-list do app oficial sobre o buffer BGRA nativo; famílias por retângulo usam os dirty rects do core.
- D-2026-09-30-device-protocols-2: tráfego automático só com o que os apps oficiais mandam em todo start/stop; reboot, opções persistentes, rotação persistente, armazenamento, firmware e C9 só por comando explícito (+ Confirm quando destrutivo).
- D-2026-09-30-device-protocols-3: porta ocupada por outro programa vira `InUse` com o nome/PID do dono.
- D-2026-09-30-device-protocols-4: USB bruto via nusb; DES-CBC (TUR_USB, PKCS#7) e DES-ECB (WCH) via RustCrypto.
- D-2026-09-30-device-protocols-5: famílias sem hardware saem com vetores golden e `hardware_validated=false`; a 8.8" rev C é validada nesta phase.

## Canonical refs
- `docs/reverse-engineering/protocol-*.md`, `devices.md`, `pixel-formats.md`
- Validação real já feita: HELLO → `chs_88inch.dev1_rom1.90`; quadro cheio → `full_png_sucess`; parciais → `needReSend:0|renderCnt:0`.

## Out of scope
- Armazenamento/SD, vídeo, imagem de boot, firmware (phase `storage-video`)
- HID "Desktop mode" do 1A86:AD11 e o driver IddCx (só identificação nesta phase)

## Definition of Done

### Auto-verifiable
- [ ] Every family's encoder matches the reference vectors (one named test per family)
      **Verify:** `export LC_ALL=C.UTF-8; for t in turing_rev_c::tests::command_packets_match_the_reference_vectors turing_rev_a::tests::packets_match_the_reference_vectors xuanfang_rev_b::tests::packets_match_the_reference_vectors kipye_rev_d::tests::packets_match_the_reference_vectors weact::tests::packets_match_the_reference_vectors turing_usb::tests::packets_match_the_reference_vectors wch::tests::packets_match_the_reference_vectors; do cargo test -p bezel-devices --locked --lib -- --exact "protocol::$t" 2>&1 | grep -q '1 passed' || { printf 'missing %s\n' "$t"; exit 1; }; done && echo OK`
      **Source:** CONTEXT
- [ ] A port held by another process is refused with its name and PID
      **Verify:** `cargo test -p bezel-devices --locked --lib -- --exact busy::tests::finds_other_processes_holding_the_device 2>&1 | grep -q '1 passed' && echo OK`
      **Source:** CONTEXT
- [ ] `bezel test-pattern` drives a screen through the connector in the chosen orientation
      **Verify:** `cargo test -p bezel --locked --lib -- --exact screen::tests::test_pattern_presents_frames_in_the_orientation 2>&1 | grep -q '1 passed' && echo OK`
      **Source:** CONTEXT

### Manual
- [ ] On the real 8.8", the test pattern shows the corner legend correctly in all four orientations and the strip animates
      **Verify:** human confirmation required
      **Evidence:** `bezel test-pattern --orientation <o> --seconds 5` for the four orientations; SUMMARY.md § Hardware validation
      **Source:** CONTEXT

## Notes
- Ordem: rev C (hardware real) → A/B/D/WeAct (serial, retângulos) → TUR_USB → WCH. Catálogo ganha os modelos seriais do TURZX (2.4", 2.8" sq, 3.4", 4", 6.5", 6.8", 8" 1D6B:A080 …).
