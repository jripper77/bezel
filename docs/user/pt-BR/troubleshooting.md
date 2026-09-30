# Solução de problemas

[English](../troubleshooting.md)

Comece pelo `bezel devices`: ele lista as telas conectadas e o estado delas sem
mandar nada para elas. Acrescente `-v` a qualquer comando para ver o que o Bezel
envia e recebe. As mensagens da linha de comando são em inglês; abaixo, cada uma
aparece como o Bezel a escreve.

## "No smart screen found"

- Confira o cabo USB (alguns cabos só carregam) e tente outra porta.
- Linux: o seu usuário talvez ainda não possa abrir a tela: veja o próximo item.
- Windows: telas Turing USB (`1CBE`) e WCH (`43A8`) precisam do driver WinUSB:
  [Deixe o Bezel abrir a tela](permissions.md#windows).

## "access denied" / "Permission denied" (Linux)

Falta a regra udev. Rode `bezel udev-rules`, copie o comando que ele imprime,
rode-o e depois desconecte e conecte a tela de novo. O aplicativo mostra o mesmo
comando quando a porta é negada. Detalhes:
[Deixe o Bezel abrir a tela](permissions.md#linux).

## "… is in use by …"

Outro programa está usando a tela, por exemplo:

```text
/dev/ttyACM1 is in use by python3 (PID 4242)
```

Feche esse programa: o turing-smart-screen-python, o app do fabricante, um
serviço `bezel-run@`, um segundo `bezel run`, ou a própria janela ou bandeja do
Bezel com o **Ao vivo** ligado. Veja
[Vindo do turing-smart-screen-python](migrating.md#1-pare-o-outro-programa).

## A tela está apagada, ou aparece como "asleep"

As telas rev C dormem quando outro programa as desliga (o
turing-smart-screen-python e o app do fabricante fazem isso ao fechar) e depois
de um `bezel off`. Qualquer coisa que desenhe as acorda: o **Ao vivo** no
aplicativo, `bezel run`, `bezel show`, `bezel test-pattern`. Acordar leva alguns
segundos.

## A tela para de responder

Sinais: "timeout talking to …", "the screen did not wake up", um envio que
empaca. Desconecte o cabo USB da tela, espere alguns segundos e conecte de novo.
Depois tente outra vez.

## Depois de cancelar um envio

Apague o arquivo incompleto antes de enviar de novo: o aplicativo oferece
**Apagar o arquivo incompleto**, e o terminal imprime o comando
`bezel storage rm … --yes`. Se o envio seguinte terminar com *"the stored size
differs; delete it and send it again"*, apague esse arquivo e envie mais uma
vez. Veja [Cancelar um envio](storage-and-video.md#cancelar-um-envio).

## Um vídeo não é enviado

"needs ffmpeg" ou "ffmpeg não encontrado": instale o ffmpeg com `libx264`
([Instalar o ffmpeg](ffmpeg.md)). Imagens, e vídeos que já estão no formato da
tela, vão sem ele.

## "Nenhum cartão SD na tela"

O cartão não está lá ou não está em FAT32 com tabela de partição MBR:
[Preparar um cartão SD](sd-card.md).

## Um sensor mostra `—`

O `bezel sensors` diz por quê. No Windows, rode o LibreHardwareMonitor como
administrador. Veja [Sensores](sensors.md#quando-falta-um-valor).

## O FPS de jogos mostra `—`

Nenhuma ferramenta está medindo um jogo, ou o jogo está pausado:
[FPS de jogos](fps.md).

## Windows: "O Windows protegeu o computador"

Os instaladores do Bezel não têm assinatura. Clique em **Mais informações** e
depois em **Executar assim mesmo**:
[Instalar o Bezel](install.md#instaladores-sem-assinatura).

## Um painel Turing USB aparece em "desktop mode"

Ele está no modo desktop (segundo monitor) do fabricante:
[Telas suportadas](devices.md#modo-desktop).

## Relatar um problema

Inclua a saída de `bezel --version` e de `bezel -v devices`, e o log do que
falhou (`bezel -v …`, ou `journalctl --user -u bezel-run@<tema>` para o
serviço). Tire números de série e caminhos pessoais antes de publicar.
