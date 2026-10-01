# Sensores

[English](../sensors.md)

O Bezel lê os sensores do computador por conta própria: processador, placa de
vídeo, memória, discos, rede, placa-mãe e ventoinhas. No aplicativo eles ficam na
aba **Sensores**, nos mesmos grupos; arraste um para a edição para mostrá-lo.

Pelo terminal:

```bash
bezel sensors                    # todos os sensores, com a chave de cada um
bezel sensors --watch 1          # atualiza a cada segundo, até Ctrl+C
bezel sensors --json             # para scripts
```

## Quando falta um valor

O Bezel nunca inventa um número. Um sensor que ele não consegue ler fica
*indisponível*: o tema mostra `—`, e o `bezel sensors` imprime o motivo ao lado
(em inglês), por exemplo:

```text
  CPU package power           —  cpu.power  (the RAPL energy counter is readable by root only ...)
  Game frame rate             —  gpu.fps  (no MangoHud log in ...: run the game with MangoHud and start logging ...)
```

Motivos comuns:

| Sensor | Por que fica indisponível | O que fazer |
|---|---|---|
| Temperatura, ventoinhas, potência da CPU (Windows) | o LibreHardwareMonitor não está rodando | veja [Windows](#windows) abaixo |
| Potência da CPU (Linux) | o kernel só deixa o root ler o contador RAPL | deixe fora do tema, ou dê permissão de leitura ao arquivo que o motivo cita |
| Ventoinhas, voltagens (Linux) | o chip de sensores da placa-mãe está sem driver (`nct6775`, `it87`) | carregue o driver da sua placa (`sudo sensors-detect`, do lm-sensors, ajuda) |
| FPS de jogos | nenhuma ferramenta está medindo um jogo | veja [FPS de jogos](fps.md) |
| Ping | ainda sem resposta do destino do ping | confira a rede ou troque o destino (abaixo) |
| Volume de saída | ainda não suportado | — |

Um sensor que simplesmente não existe (sem segunda placa de vídeo, sem swap)
também fica indisponível.

## Linux

O Bezel lê direto do kernel: `/proc` para CPU, memória e rede, `hwmon` para
temperaturas e ventoinhas, o driver da NVIDIA (NVML) para placas NVIDIA, o
`amdgpu` para placas AMD. Nada a instalar.

## Windows

Uso da CPU, memória, discos e rede funcionam direto, e as placas NVIDIA pelo
driver delas. Temperaturas, ventoinhas, voltagens e potência precisam do
[LibreHardwareMonitor](https://github.com/LibreHardwareMonitor/LibreHardwareMonitor):

1. Baixe e rode o `LibreHardwareMonitor.exe` **como administrador** (ele precisa
   disso para ler o hardware).
2. Deixe-o aberto. Em **Options → Run On Windows Startup** ele passa a abrir com
   o Windows.

O Bezel lê o que o LibreHardwareMonitor publica (pelo provedor WMI dele); não
instala drivers próprios.

## Ping

O `net.ping` mede o tempo de ida e volta até `8.8.8.8`, por padrão: um eco ICMP
ou, onde o sistema não deixa um programa enviá-lo (sempre no Windows), uma
conexão TCP na porta 53 ou 443. O Bezel envia esses pacotes uma vez por segundo,
e só enquanto algo que você vê usa o ping:

- um tema com um elemento de `net.ping`, mostrado pelo `bezel run`, pelo serviço
  `bezel-run@` ou pelo aplicativo (ao vivo na tela, ou no editor);
- a lista de sensores do aplicativo, enquanto ela mostra o ping (aba aberta e o
  ping não escondido pela busca);
- o `bezel sensors`, que lista todos os sensores.

Fora disso, nada é enviado ao destino do ping, e o `net.ping` aparece
indisponível (*"measured only while shown"*).

Troque o destino em **Preferências → Sensores** no aplicativo, ou com
`--ping-host` na linha de comando (`bezel run --ping-host 1.1.1.1 ...`).
