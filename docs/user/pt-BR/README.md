# Guia do usuário do Bezel

[English](../README.md)

O Bezel controla as telinhas USB de monitoramento vendidas como Turing Smart
Screen, TURZX, XuanFang, Kipye, WeAct e suas variantes, no Linux e no Windows.
Ele tem duas partes:

- **Bezel** (`bezel-studio`), o aplicativo: crie temas arrastando widgets e
  sensores, mostre-os ao vivo na tela e cuide das imagens e vídeos guardados
  nela.
- **`bezel`**, a linha de comando: as mesmas coisas pelo terminal ou por um
  serviço.

A linha de comando e as mensagens dela ficam em inglês; o aplicativo segue o
idioma do sistema, ou o que você escolher em **Preferências → Idioma**.

## Primeiros passos

1. [Instale o Bezel](install.md) no Linux ou no Windows.
2. [Deixe o Bezel abrir a tela](permissions.md): a regra udev do Linux, os
   drivers do Windows.
3. [Crie o seu primeiro tema](first-theme.md) e ligue o *Ao vivo*.
4. [Use a tela na vertical ou na horizontal](vertical-or-horizontal.md).

## Usando o Bezel

- [Sensores](sensors.md): o que o Bezel mede, e por que um valor pode aparecer
  como `—`.
- [FPS de jogos](fps.md): RivaTuner Statistics Server no Windows, MangoHud no
  Linux.
- [Armazenamento e vídeo](storage-and-video.md): imagens e vídeos guardados na
  tela, o que ela mostra ao ligar, e
  [como gerenciá-los](storage-and-video.md#gerenciar-os-arquivos): mover entre a
  memória interna e o cartão, renomear, restaurar um cartão, o assistente de
  limpeza e as cópias locais do Bezel.
- [Instalar o ffmpeg](ffmpeg.md), necessário para converter vídeos.
- [Preparar um cartão SD](sd-card.md) para telas com entrada de cartão.
- [Iniciar com o computador](run-at-login.md): pela bandeja ou como serviço do
  systemd.
- [Vindo do turing-smart-screen-python](migrating.md).

## Quando algo dá errado

- [Solução de problemas](troubleshooting.md)
- [Telas suportadas](devices.md)
