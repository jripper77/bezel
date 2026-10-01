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

## A tela travou / parou de responder

Sinais: *"the screen stopped responding: it stopped reading what was sent"* (a
tela parou de responder), "timeout talking to …", um envio ou o tema ao vivo
que empaca. Uma tela Turing rev C (a 8,8" e os outros modelos seriais com chip
de despertar) pode travar, por exemplo com um envio acima do limite de 25 MiB.
**Não precisa desconectar o cabo**: o Bezel a reinicia pelo chip de despertar,
e ela volta em cerca de 10 segundos.

- **Durante o ao vivo**: o `bezel run`, o serviço `bezel-run@` e o **Ao vivo**
  do aplicativo conectam a tela de novo sozinhos, 2, 5 e 10 segundos depois de
  ela parar; conectar reinicia antes uma tela travada, e o tema continua. A
  barra de status do aplicativo diz *Reconectando à tela (tentativa 1 de 3)…*,
  depois *A tela voltou e mostra o tema ao vivo de novo.*; o `bezel run`
  escreve o mesmo no terminal. Depois da terceira tentativa o ao vivo para com
  o erro. Desligar o **Ao vivo** (ou Ctrl+C) nesse meio-tempo para na hora.
- **Sozinho**: o próximo comando, ou ligar o **Ao vivo** de novo, encontra a
  tela travada e a reinicia uma vez antes de conectar. Nada é reiniciado
  enquanto a tela responde.
- **Linha de comando**: `bezel restart` (ou `bezel restart -s /dev/ttyACM1`
  para uma tela). Ele diz que vai reiniciar a tela, espera por ela e mostra onde
  ela voltou. O terminal também mostra essa dica depois de um erro que indica
  que a tela travou.
- **Aplicativo**: **Tela → Ajustes → Reiniciar a tela…**, depois de uma
  confirmação curta. Um erro de armazenamento que indica que a tela travou tem o
  mesmo botão, e o cartão da tela também, quando o ao vivo parou por isso.

Reiniciar para o que a tela mostra ou toca; os arquivos guardados ficam. Se a
tela continuar sem responder depois de reiniciar, ou não tiver chip de
despertar (o Bezel então diz que não consegue reiniciá-la), desconecte o cabo
USB, espere alguns segundos e conecte de novo.

## Depois de cancelar um envio

Apague o arquivo incompleto antes de enviar de novo: o aplicativo oferece
**Apagar o arquivo incompleto**, e o terminal imprime o comando
`bezel storage rm … --yes`. Se o envio seguinte terminar com *"the stored size
differs; delete it and send it again"*, apague esse arquivo e envie mais uma
vez. Veja [Cancelar um envio](storage-and-video.md#cancelar-um-envio).

## "refused: the file is … MiB and this screen takes files up to 25 MiB each"

As telas Turing rev C aceitam no máximo 25 MiB por arquivo: o firmware guarda o
envio inteiro na memória e trava com um arquivo maior. O Bezel recusa o arquivo
antes de enviar qualquer coisa (no aplicativo: *"O arquivo tem … MiB; esta tela
aceita arquivos de até 25 MiB."*). Envie um trecho mais curto, ou deixe o Bezel
converter o vídeo com menos quadros por segundo (`--fps 24`): a conversão limita
a taxa de bits para o resultado caber. Veja
[Qual o tamanho máximo de um arquivo](storage-and-video.md#qual-o-tamanho-máximo-de-um-arquivo).

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
