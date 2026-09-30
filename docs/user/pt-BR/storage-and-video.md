# Armazenamento e vídeo

[English](../storage-and-video.md)

As telas com armazenamento (Turing rev C e a geração Turing USB) guardam imagens
e vídeos na memória interna e, quando têm entrada, num cartão SD. Elas os tocam
sozinhas e podem mostrar um deles ao ligar, sem o Bezel rodando.

A tela tem quatro pastas: `internal/image`, `internal/video`, `sd/image` e
`sd/video`. `sd` é o cartão de memória, acessado só através da tela. O Bezel
nunca o formata; veja [Preparar um cartão SD](sd-card.md).

## No aplicativo

**Tela → Armazenamento** mostra quanto da memória interna e do cartão SD está em
uso, e os arquivos de cada um.

- **Enviar**: arraste um arquivo para a lista, ou **Enviar arquivo…**. O Bezel
  mostra o que vai fazer (conversão, destino, um arquivo que será substituído) e
  pede confirmação. A barra de progresso passa por *Convertendo*, *Enviando* e
  *Conferindo*.
- **Tocar** e **Parar reprodução**: a tela toca um vídeo guardado em repetição,
  ou mostra uma imagem guardada. Com o **Ao vivo** ligado, o tema cobre o que a
  tela toca; por isso essas ações esperam você desligar o Ao vivo.
- **Apagar**: pede confirmação, com o nome do arquivo.
- **Ao ligar**: **Mostrar ao ligar** escolhe o arquivo que a tela mostra quando
  liga; **Voltar ao relógio padrão** desfaz. A tela guarda a escolha.

## Pela linha de comando

```bash
bezel storage info                          # espaço usado e livre
bezel storage ls                            # todos os arquivos; ou uma pasta: bezel storage ls sd/video
bezel storage put clipe.mp4                 # converte se precisar e envia, com progresso
bezel storage put logo.png sd/image/logo.png
bezel storage play internal/video/clipe.mp4 # repete na tela (--once: toca uma vez)
bezel storage stop
bezel storage rm internal/video/clipe.mp4 --yes
bezel storage boot internal/video/clipe.mp4 --brightness 60 --yes   # mostrado ao ligar
bezel storage boot default --yes            # volta à tela de início original
```

Tudo o que apaga, substitui ou muda o que a tela mostra ao ligar (`rm`, `put`
sobre um arquivo que já existe, `boot`) primeiro diz o que vai fazer e precisa do
`--yes`; sem ele nada chega à tela.

## O que dá para enviar

- Imagens: JPEG, PNG, BMP, GIF, enviadas como estão.
- Vídeos: convertidos com o ffmpeg para o formato da tela (na 8,8": H.264 MP4 de
  480×1920 sem som), girados para a orientação escolhida (`--orientation`; o
  padrão é o formato do próprio vídeo) e cortados no formato da tela, nunca
  esticados. `--fps 24` reduz os quadros por segundo. Um vídeo que já está no
  formato certo vai como está. O ffmpeg não vem junto:
  [Instalar o ffmpeg](ffmpeg.md).
- Nomes de arquivo: letras minúsculas sem acento `a-z`, números, `_`, `.` e `-`.
  Até 120 MB por arquivo.
- Quando um arquivo não cabe, o Bezel diz quanto há livre e lista os arquivos
  guardados, dos maiores para os menores. Ele nunca apaga nada por você.

## Cancelar um envio

Dá para cancelar um envio (**Cancelar envio** no aplicativo, Ctrl+C no
terminal; um segundo Ctrl+C sai na hora). Parte do arquivo pode ficar na tela:

1. **Apague o arquivo incompleto.** O aplicativo oferece **Apagar o arquivo
   incompleto**; o terminal imprime o comando, por exemplo
   `bezel storage rm internal/video/clipe.mp4 --yes`. Se o aplicativo disser que
   a tela parou de responder, ela volta na próxima ação: clique em **Atualizar** e
   apague o arquivo incompleto se ele aparecer.
2. **Envie de novo.** Se o envio seguinte terminar com *"the stored size
   differs; delete it and send it again"* (o tamanho gravado não bate), bytes do
   envio cancelado chegaram até ele: apague esse arquivo e envie mais uma vez.

Se a tela parar de responder de vez (o envio empaca, ou todo comando estoura o
tempo), desconecte o cabo USB, espere alguns segundos e conecte de novo.

## O que a tela mostra ao ligar

Nas telas rev C, a escolha do que mostrar ao ligar também guarda o brilho com
que a tela liga: no aplicativo, o brilho ajustado em **Ajustes**; na linha de
comando, o `--brightness`; sem isso, o padrão do fabricante, cerca de 67%. Na
geração Turing USB, o Bezel envia e toca arquivos, mas ainda não consegue
apagá-los, tocar um vídeo uma vez só nem mudar o que a tela mostra ao ligar.

## Temas com vídeo de fundo

Um tema pode usar um vídeo como fundo: a tela repete o vídeo e o Bezel desenha o
tema por cima.

- Se o vídeo ainda não está na tela, ela mostra a imagem de capa do tema e o
  aplicativo oferece **Enviar para a tela**; o `bezel run` imprime o comando
  `bezel storage put` exato.
- Telas que não tocam vídeo recebem o vídeo decodificado no computador, o que
  precisa do ffmpeg (`bezel run --ffmpeg CAMINHO` se ele não estiver no `PATH`).
