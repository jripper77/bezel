# Armazenamento e vídeo

[English](../storage-and-video.md)

As telas com armazenamento (Turing rev C e a geração Turing USB) guardam imagens
e vídeos na memória interna e, quando têm entrada, num cartão SD. Elas os tocam
sozinhas e podem mostrar um deles ao ligar, sem o Bezel rodando.

A tela tem quatro pastas: `internal/image`, `internal/video`, `sd/image` e
`sd/video`. `sd` é o cartão de memória, acessado só através da tela. O Bezel
nunca o formata; veja [Preparar um cartão SD](sd-card.md).

## No aplicativo

**Tela → Armazenamento** ocupa a janela toda: a memória interna e o cartão SD
lado a lado, cada um com quanto está em uso e os seus arquivos. Cada arquivo
mostra uma miniatura (ou um ícone do tipo), o tamanho e quando o Bezel o
enviou, ou que o Bezel não o enviou. Acima das listas, **Nome contém**,
**Tipo**, **Origem** e **Ordenar por** filtram e ordenam os dois lados.

- **Selecionar**: clique num arquivo; Ctrl+clique e Shift+clique somam à
  seleção. Numa lista, as setas andam, o Espaço seleciona ou desfaz, Shift+setas
  estendem a seleção e Ctrl+A seleciona tudo; Delete apaga e F2 renomeia.
- **Enviar**: arraste um arquivo do computador para um dos lados, ou **Enviar
  arquivo…**. O Bezel mostra o que vai fazer (conversão, destino, um arquivo que
  será substituído) e pede confirmação. A barra de progresso passa por
  *Convertendo*, *Enviando* e *Conferindo*.
- **Tocar na tela** e **Parar reprodução**: a tela toca um vídeo guardado em
  repetição, ou mostra uma imagem guardada. Com o **Ao vivo** ligado, o tema
  cobre o que a tela toca; por isso essas ações esperam você desligar o Ao vivo.
- **Apagar…**: pede confirmação, com o nome do arquivo (ou a lista dos
  arquivos).
- **Ao ligar**: **Mostrar ao ligar…** (ou um arquivo arrastado para **Ao
  ligar**) escolhe o arquivo que a tela mostra quando liga; **Voltar ao relógio
  padrão** desfaz. A tela guarda a escolha.
- **Mover para o cartão SD**, **Copiar para o cartão SD** (ou para a memória
  interna), **Renomear…**, **Restaurar…**, **Assistente de limpeza…**,
  **Associar original…** e **Cópias locais…**: veja
  [Gerenciar os arquivos](#gerenciar-os-arquivos).

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

O `bezel storage ls` também mostra, para cada arquivo que o Bezel enviou, o
estado dele (*stored*, gravado, ou *pending*: um envio que não terminou) e se o
Bezel guarda uma cópia local dele. Mover, renomear, restaurar e limpar ficam em
[Gerenciar os arquivos](#gerenciar-os-arquivos).

## O que dá para enviar

- Imagens: JPEG, PNG, BMP, GIF, enviadas como estão.
- Vídeos: convertidos com o ffmpeg para o formato da tela (na 8,8": H.264 MP4 de
  480×1920 sem som), girados para a orientação escolhida (`--orientation`; o
  padrão é o formato do próprio vídeo) e cortados no formato da tela, nunca
  esticados. `--fps 24` reduz os quadros por segundo. Um vídeo que já está no
  formato certo vai como está. O ffmpeg não vem junto:
  [Instalar o ffmpeg](ffmpeg.md).
- Nomes de arquivo: letras minúsculas sem acento `a-z`, números, `_`, `.` e `-`.
- Tamanho: até **25 MiB por arquivo nas telas Turing rev C** (a geração serial:
  8,8", 5", 2,1" redonda e outras), até 120 MB na geração
  Turing USB. Veja [Qual o tamanho máximo de um arquivo](#qual-o-tamanho-máximo-de-um-arquivo).
- Quando um arquivo não cabe, o Bezel diz quanto há livre e lista os arquivos
  guardados, dos maiores para os menores. Ele nunca apaga nada por você.

## Qual o tamanho máximo de um arquivo

Uma tela Turing rev C guarda o envio inteiro na memória antes de gravá-lo. Na
8,8" o firmware para de ler em cerca de 28 MiB e trava até ser reiniciado,
qualquer que seja a velocidade; por isso o Bezel aceita no máximo **25 MiB por
arquivo** nessas telas e recusa um arquivo maior antes de enviar qualquer coisa.
Os maiores arquivos do app do fabricante também ficam em cerca de 24,6 MiB.

- Um vídeo que o Bezel converte é ajustado para caber: pela duração do vídeo
  ele limita a taxa de bits, e um trecho longo perde um pouco de qualidade em
  vez de passar do limite.
- Se o vídeo convertido ainda ficar grande demais (ou o ffmpeg não souber a
  duração), nada é enviado e o Bezel avisa: envie um trecho mais curto, ou
  reduza os quadros por segundo com `--fps` (por exemplo
  `bezel storage put clipe.mp4 --fps 24`).
- Um vídeo que já está no formato da tela vai como está, então ele mesmo
  precisa caber no limite; o `--fps` faz o Bezel convertê-lo, e a conversão o
  ajusta.

O limite aparece em MiB (1 MiB = 1.048.576 bytes): a mensagem do aplicativo,
por exemplo, diz *"O arquivo tem 30 MiB; esta tela aceita arquivos de até
25 MiB."*

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
tempo), ela travou: o próximo comando reinicia uma tela Turing rev C sozinho, ou
use `bezel restart` ou **Reiniciar a tela…** no aplicativo; não precisa
desconectar o cabo. Veja
[A tela travou](troubleshooting.md#a-tela-travou--parou-de-responder).

## Gerenciar os arquivos

Uma tela só lista, grava, apaga e toca os seus arquivos: ela não devolve um
arquivo para o computador, não renomeia e não move. Por isso o Bezel guarda uma
cópia do que envia, e mover, renomear e restaurar enviam essa cópia de novo.

### As cópias locais do Bezel

Todo arquivo que o Bezel envia (pelo **Enviar arquivo…**, pelo
`bezel storage put`, o vídeo de um tema, ao mover ou ao restaurar) também fica
guardado no computador: os bytes exatos que a tela recebeu (o resultado da
conversão, no caso de um vídeo), com uma miniatura (a de um vídeo precisa do
ffmpeg) e o registro de para onde foi (o *catálogo* do Bezel). Eles ficam na sua
pasta de dados:

- Linux: `~/.local/share/bezel/storage` (ou `$XDG_DATA_HOME/bezel/storage`);
- Windows: `%APPDATA%\bezel\storage`.

Os mesmos bytes na memória interna e no cartão são guardados uma vez só.

**O limite.** Só contam as cópias de arquivos que você apagou pelo Bezel, até
2 GiB por padrão (cerca de 80 arquivos de 25 MiB); acima disso, as mais antigas
saem primeiro. As cópias de arquivos que ainda estão numa tela, ou que sumiram
dela (um cartão formatado ou trocado), nunca saem sozinhas; assim uma
restauração sempre as tem.

**Limpar cache.** No aplicativo, **Cópias locais…** mostra quantas cópias há e
quanto ocupam, e ajusta o limite; **Limpar cache…** remove as cópias dos
arquivos apagados pelo Bezel (todas as cópias com **Limpar também as cópias de
arquivos que ainda estão em uma tela**), depois de uma confirmação que diz
quantas são e quanto ocupam. Os arquivos nas telas continuam lá, e as entradas
do catálogo e as miniaturas também, marcadas como "sem cópia local": esses
arquivos não podem ser movidos, renomeados nem restaurados até você
[associar o original](#arquivos-que-o-bezel-não-enviou) de novo. Pela linha de
comando:

```bash
bezel storage cache info                # quantas cópias, quanto ocupam e o limite
bezel storage cache --limit 1GiB        # o limite das cópias de arquivos apagados
bezel storage cache clear --yes         # remove as cópias dos arquivos apagados
bezel storage cache clear --all --yes   # todas as cópias, também de arquivos que estão numa tela
```

O `bezel storage catalog` lista o que o Bezel enviou para a tela: o estado de
cada arquivo (*stored*, gravado; *pending*, envio não concluído; *missing*,
sumiu da tela; *deleted*, apagado pelo Bezel; ou *on another card*, em outro
cartão), se ele tem cópia local, quando foi enviado e de onde.

### Mover, renomear e copiar

O Bezel move (da memória interna para o cartão, ou de volta) e renomeia um
arquivo por vez:

1. primeiro confere o destino: o nome, o tipo, os
   [25 MiB por arquivo](#qual-o-tamanho-máximo-de-um-arquivo) das telas Turing
   rev C e o espaço livre;
2. envia o arquivo de novo a partir da cópia local;
3. confere se o tamanho que a tela gravou é o tamanho da cópia;
4. só então apaga a origem.

A origem nunca é apagada antes, nem quando falta espaço. Se um arquivo falhar,
ou se você clicar em **Cancelar** (Ctrl+C no terminal), a origem fica onde
estava e os arquivos seguintes não começam; o relatório diz o que foi movido, o
que falhou e por quê, e o que não começou, e um arquivo incompleto deixado pelo
envio cancelado aparece com o comando que o apaga.

O novo nome segue a regra de envio (letras minúsculas sem acento, números, `_`,
`.` e `-`, a mesma extensão): mover `NVI.mp4` dá `nvi.mp4`. Um arquivo com esse
nome que já esteja lá fica de fora, a menos que você escolha substituí-lo. Mover
o arquivo que a tela mostra ao ligar, ou renomear um vídeo que um tema toca,
acrescenta um aviso à confirmação: a tela liga com o último arquivo que tocou, e
um tema acha o vídeo dele pelo nome.

No aplicativo, selecione arquivos numa lista e clique em **Mover para o cartão
SD** (ou **Mover para a memória interna**), ou arraste-os para a outra lista.
**Copiar para o cartão SD** envia do mesmo jeito e mantém os originais.
**Renomear…** (F2) pede o novo nome e mostra como a tela vai gravá-lo. Uma única
confirmação lista cada arquivo como origem → destino com o tamanho, o que fica
de fora e por quê (com **Substituir o “…” de lá** para um arquivo de mesmo nome)
e o espaço livre; nada começa antes de você clicar em **Mover**. Durante a
operação, a barra mostra *Movendo “…” (1 de 3)* e **Cancelar**.

Pela linha de comando, `mv`, `rename` e `restore` primeiro imprimem a lista
exata; sem `--yes` eles só consultam a tela e não mudam nada:

```bash
bezel storage mv internal/video/clipe.mp4 --to sd         # imprime a lista; --yes move
bezel storage mv sd/video/a.mp4 sd/video/b.mp4 --to internal --yes
bezel storage rename sd/video/clipe.mp4 abertura.mp4 --yes
```

```text
$ bezel storage mv internal/video/bezel_demo.mp4 --to sd
Move 1 file to the memory card of Turing Smart Screen 8.8", each sent from Bezel's local copy:
  internal/video/bezel_demo.mp4 -> sd/video/bezel_demo.mp4     2.3 MiB
1 file, 2.3 MiB to send; each source is deleted only after its copy is verified.
Nothing on the screen was changed. Add --yes to move it.
```

O `--overwrite` substitui os arquivos de mesmo nome que já estão lá. A linha de
comando não tem um comando de copiar:
`bezel storage restore internal abertura.mp4 --yes` envia também para a memória
interna um arquivo que está no cartão, e mantém o do cartão.

### Restaurar

Restaurar envia de volta para um meio, a partir das cópias locais, arquivos que
o Bezel já tinha enviado: depois de você formatar o cartão no computador, ou
para um cartão novo. Restaurar nunca apaga nada.

Antes do primeiro byte, o Bezel confere se todos os arquivos cabem no espaço
livre e se cada um pode ir para lá, inclusive os 25 MiB por arquivo das telas
Turing rev C; se não, nada é enviado e o Bezel diz quanto falta. Um arquivo que
já está lá com o mesmo nome e tamanho é pulado; o mesmo nome com outro tamanho
fica de fora, a menos que você escolha substituí-lo. Os arquivos vão um por vez,
do enviado há mais tempo ao mais recente, cada um conferido; **Cancelar** e
falhas funcionam como ao mover.

No aplicativo, **Restaurar…** (com quantos arquivos) aparece acima de uma lista
quando arquivos que o Bezel enviou para lá sumiram, estão em outro cartão ou
foram apagados pelo Bezel e as cópias locais continuam guardadas. Os que sumiram
já vêm marcados; os apagados aparecem em *Apagados pelo Bezel*, desmarcados.
Escolha os arquivos, **Continuar…** e então **Restaurar**. Pela linha de
comando:

```bash
bezel storage restore sd                     # os arquivos que o Bezel enviou ao cartão e sumiram
bezel storage restore sd --yes
bezel storage restore internal abertura.mp4 --yes   # um arquivo pelo nome, também um apagado pelo Bezel
```

```text
$ bezel storage restore sd
Restore 1 file to the memory card of Turing Smart Screen 8.8" from Bezel's local copies:
  sd/video/bezel_intro.mp4 -> sd/video/bezel_intro.mp4     1.2 MiB  (missing)
1 file, 1.2 MiB to send; 7.9 GiB free there; nothing is deleted.
Nothing on the screen was changed. Add --yes to restore it.
```

### O assistente de limpeza

Para os arquivos que o Bezel não enviou (os do app do fabricante, por exemplo)
e para os envios que não terminaram, o assistente de limpeza aponta prováveis
sobras:

- **Duplicado**: as cópias que o app do fabricante faz quando converte um
  arquivo de novo, com nomes como `x.mp4.mp4` ou `x.mp4<dígitos>.mp4`
  (`NVI.mp427034822.mp4`), do mesmo tamanho do arquivo que fica. Vêm marcados.
- **Cópia do fornecedor**: os mesmos nomes com outro tamanho. Só listados.
- **Envio interrompido**: um arquivo de exatamente 29.577.216 bytes, o que sobra
  de um envio que travou uma tela Turing rev C. Vem marcado.
- **Envio não concluído**: um arquivo que o Bezel começou a enviar e nunca
  conferiu (o estado dele no catálogo é *pending*). Vem marcado.
- **Mesmo tamanho** e **Tamanho mudou**: outros arquivos exatamente do mesmo
  tamanho e tipo, e um arquivo cujo tamanho não é o que o Bezel enviou. Só
  listados.
- **Sem uso**: um arquivo que o Bezel não enviou e que nenhum tema toca. Só
  listado.

Só os sinais exatos já vêm marcados; o que é apenas provável aparece, mas não é
escolhido. O assistente nunca sugere o arquivo que o Bezel escolheu para a tela
mostrar ao ligar, nem um vídeo que um tema toca (o vídeo de fundo de um tema,
por exemplo). Ele nunca roda sozinho, e nada é apagado até você confirmar a
lista exata.

No aplicativo, **Assistente de limpeza…** (acima das listas) mostra as
sugestões por grupo; marque ou desmarque, e então **Apagar os marcados…** lista
os arquivos exatos e o espaço liberado, e **Apagar estes arquivos** os apaga um
por vez. **Origem → Sugestões de limpeza** também os mostra nas listas.

Pela linha de comando, `bezel storage cleanup --dry-run` só lista; com `--yes`
ele apaga exatamente os arquivos marcados que imprimiu, e nada mais (sem
nenhum dos dois, ele lista e diz o que o `--yes` apagaria):

```text
$ bezel storage cleanup --dry-run
Cleanup suggestions for Turing Smart Screen 8.8" (never the boot media Bezel set nor a video your themes play):
Pre-checked, deleted by `bezel storage cleanup --yes`:
  internal/video/bezel_cut.mp4      320.0 KiB  pending: an upload by Bezel that did not finish or failed its size check
Only listed (`bezel storage rm PATH --yes` deletes one you no longer need):
  sd/video/NVI.mp4                    5.4 MiB  unused: no theme plays it
  sd/video/NVI.mp427034822.mp4        5.1 MiB  variant: a vendor copy of sd/video/NVI.mp4 with another size
  …
1 file pre-checked (320.0 KiB to free), 12 files only listed.
Dry run: nothing was deleted.
```

### Arquivos que o Bezel não enviou

Um arquivo que o Bezel não enviou não tem cópia local: aparece com um ícone em
vez de miniatura (**Tocar na tela** mostra o arquivo) e não pode ser movido nem
renomeado. Se você tem o original no computador, associe os dois: o Bezel copia
o original para as cópias locais, o que dá ao arquivo uma miniatura e permite
movê-lo.

- No aplicativo: selecione o arquivo, **Associar original…**, e então
  **Escolher arquivos…** ou **Escolher pasta…**. Só aparecem arquivos com
  exatamente o mesmo tamanho em bytes e do mesmo tipo, do mais provável ao menos
  provável (pelo nome e, num vídeo, pela duração e pela resolução); confirme o
  par com **Associar**.
- Pela linha de comando:
  `bezel storage catalog associate sd/video/NVI.mp4 ~/Vídeos/NVI.mp4 --yes`
  (ou uma pasta, para procurar nela). `bezel storage catalog forget CAMINHO --yes`
  tira uma entrada do catálogo, e a cópia local dela, a menos que outra entrada
  tenha os mesmos bytes; a tela não muda.

### Telas Turing USB

O Bezel não consegue apagar arquivos na geração Turing USB, então nada que
termine apagando roda nela: **Mover para o cartão SD** (ou para a memória
interna), **Renomear…**, **Apagar…** e o **Assistente de limpeza…** ficam
desativados, com o motivo numa nota acima das listas e em cada botão; o terminal
imprime o motivo. Copiar para o outro lado, restaurar e tocar funcionam. Essas
telas nem sempre informam o tamanho de um arquivo: o Bezel mostra o tamanho que
enviou, ou "tamanho desconhecido" (`?` no `bezel storage ls`).

### Dois cartões do mesmo tamanho

A tela não informa nada sobre o cartão além da capacidade, então o Bezel
reconhece um cartão pela capacidade: arquivos que o Bezel enviou para um cartão
de outra capacidade aparecem como *em outro cartão*, prontos para restaurar
neste. Dois cartões do mesmo tamanho parecem o mesmo: o Bezel confunde um com o
outro, e os arquivos que ele enviou ao primeiro aparecem como *sumiu da tela* no
segundo. Telas do mesmo modelo também não se distinguem, então dividem um mesmo
catálogo.

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
  `bezel storage put` exato (para um vídeo reenquadrado, ele indica o
  aplicativo; veja [Enquadrar o vídeo](#enquadrar-o-vídeo)).
- Telas que não tocam vídeo recebem o vídeo decodificado no computador, o que
  precisa do ffmpeg (`bezel run --ffmpeg CAMINHO` se ele não estiver no `PATH`).
- Para dar a um tema um vídeo de fundo pelo aplicativo (um vídeo ou um GIF
  animado), veja [Um vídeo no fundo](first-theme.md#um-vídeo-no-fundo).

### Enquadrar o vídeo

O enquadramento decide como o vídeo ocupa a tela: girado, preenchendo ou
cabendo, com zoom e posição. Sem nenhum elemento selecionado, o painel
**Propriedades** mostra o tema; em **Fundo**, um vídeo tem o grupo
**Enquadramento**.

**Auto.** Alguns vídeos já vêm guardados girados para a tela: os temas
horizontais do app do fabricante guardam, para um tema de 1920×480 na 8,8", um
vídeo de 480×1920, o formato do próprio painel. Um vídeo com exatamente o
tamanho nativo do painel, num tema na outra orientação (um tema horizontal na
8,8", cujo painel é vertical), é girado de volta sozinho e toca em pé por
baixo do tema: a **Rotação** mostra então, por exemplo, **Auto · 270°**, e uma
nota diz que o Auto girou o vídeo. Qualquer outro vídeo fica em 0°. Temas
importados, os temas que já estão na biblioteca e um vídeo que você adiciona
começam todos em Auto, e o Bezel lê sozinho o tamanho de um MP4, então o Auto
funciona sem o ffmpeg.

Os controles:

- **Rotação**: **Auto** (com o ângulo que escolheu), **0°**, **90°**, **180°**
  ou **270°**, no sentido horário.
- **Ajuste**: **Preencher** (o padrão) cobre a tela toda e corta o que sobra;
  **Caber** mostra o vídeo inteiro, com a **Cor das sobras** em volta (preta
  por padrão; a cor só aparece com Caber).
- **Zoom**: de 100% a 400%, de 5% em 5%, pelo controle deslizante ou pelo
  número.
- **Posição X** e **Posição Y**: de 0% a 100%. Onde o vídeo passa da tela,
  elas escolhem a parte que aparece (com Preencher a borda nunca fica vazia);
  onde ele é menor, elas o posicionam. **Centralizar** volta as duas a 50%.
- **Redefinir enquadramento**: volta a Auto, Preencher, 100%, centralizado.

Cada mudança é um passo de Desfazer (Ctrl+Z) e Refazer.

**No canvas.** **Enquadrar no canvas**, ou um duplo clique no vídeo longe dos
elementos, enquadra o vídeo direto na área de edição, que mostra as bordas do
vídeo e uma grade:

- arraste para movê-lo; gire a roda do mouse para dar zoom em volta do
  ponteiro (Ctrl+roda continua dando zoom na visualização);
- as setas o movem 1% (com Shift, 10%); + e − dão zoom de 5%; 0 volta o zoom
  e a posição ao início;
- Esc, Enter ou **Concluir** terminam.

Uma barra no alto da área de edição mostra o zoom, a posição, essas teclas e
**Concluir**; leitores de tela anunciam o zoom e a posição. Enquanto isso os
elementos não podem ser selecionados; Desfazer, Refazer e Salvar continuam
funcionando. Cada arrasto, e cada sequência de giros da roda, é um passo de
desfazer.

**A prévia.** A área de edição toca o vídeo por baixo dos elementos, já
enquadrado, a até 15 imagens por segundo; uma mudança no enquadramento aparece
na hora. Ela mostra o pôster (a imagem de capa) no lugar:

- quando o movimento está reduzido no computador (a opção de reduzir
  movimento do sistema); o **Enquadramento** avisa;
- sem o ffmpeg: o **Enquadramento** avisa e oferece **Como instalar o
  ffmpeg** ([Instalar o ffmpeg](ffmpeg.md)). O enquadramento continua editável
  e é salvo, e o Auto continua funcionando.

Ela não toca enquanto a janela está escondida. A imagem de capa é tirada com o
enquadramento quando o vídeo é adicionado, e de novo quando o tema é salvo com
outro enquadramento (com o ffmpeg; sem ele a imagem de capa fica como estava).

**Na tela.** Uma tela que toca vídeo sozinha repete uma cópia do vídeo feita
para o enquadramento:

- Com Preencher, 100% e centralizado, qualquer que seja a rotação, a cópia
  mantém os nomes do fabricante: o nome do vídeo, com `_90`, `_180` ou `_270`
  quando a cópia é girada para o painel (`amd_90.mp4`). O vídeo do Dragon
  Ball, guardado girado para a 8,8", não precisa girar nela: ele é o
  `dragon.mp4`, e um vídeo assim, já no formato da tela, vai como está, sem
  conversão e sem o ffmpeg. Um arquivo com esse nome que já esteja na tela com
  exatamente o tamanho do vídeo é usado e nada é enviado; um de outro tamanho é
  outro arquivo, que **Enviar para a tela** substitui depois de uma
  confirmação.
- Caber, um zoom ou outra posição geram uma cópia própria, convertida com o
  ffmpeg: o nome dela ganha `_f` e 8 dígitos hexadecimais (por exemplo
  `dragon_f8ec2b24d.mp4`), então **Enviar para a tela** envia um arquivo novo
  a cada reenquadramento, e voltar a um enquadramento já enviado encontra a
  cópia dele de novo. Sem o ffmpeg, **Enviar para a tela** recusa um vídeo
  reenquadrado e diz como instalá-lo.
- O Bezel nunca apaga as cópias anteriores, e o
  [assistente de limpeza](#o-assistente-de-limpeza) também não as sugere (ele
  nunca sugere um vídeo que um tema toca, em nenhum enquadramento): apague as
  que você não usa mais em **Tela → Armazenamento** (**Apagar…**) ou com
  `bezel storage rm CAMINHO --yes`.
- Os [25 MiB por arquivo](#qual-o-tamanho-máximo-de-um-arquivo) das telas
  Turing rev C continuam valendo: uma conversão é feita para caber, e um vídeo
  enviado como está precisa ele mesmo caber no limite, senão nada é enviado.
- Telas que não tocam vídeo recebem o vídeo decodificado no computador e
  enquadrado do mesmo jeito.

**Pela linha de comando.** A linha de comando respeita o enquadramento que o
tema tem, mas não o muda. O `bezel run` lê o tamanho do vídeo no cabeçalho do
MP4 (sem precisar do ffmpeg) e procura a cópia pelo nome que o enquadramento
dá. Quando a cópia não está na tela, ele imprime o comando `bezel storage put`
para um vídeo com o enquadramento padrão e, para um vídeo reenquadrado, indica
**Enviar para a tela** no aplicativo, que faz a cópia enquadrada.

No arquivo do tema (o `theme.json` dentro do `.bezeltheme`), o enquadramento é
um objeto `framing` opcional no fundo de vídeo, omitido quando tudo está no
padrão:

```json
"framing": {"rotation": 270, "fit": "contain", "zoom": 1.25, "position": {"x": 0.5, "y": 0.4}, "padColor": "#000000ff"}
```

`rotation` é 0, 90, 180 ou 270 (ausente: Auto) e qualquer outro valor é um
erro; `fit` é `cover` (Preencher) ou `contain` (Caber); `zoom` vai de 1 a 4 e
cada eixo de `position` de 0 a 1, e números fora da faixa são trazidos para
dentro dela. Uma chave ausente fica com o padrão.
