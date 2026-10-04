# Quando o computador desligar

[English](../power-off.md)

Muitos computadores mantêm as portas USB energizadas depois de desligar. Uma
tela que não recebe nada nessa hora fica congelada na última imagem do tema a
noite toda. Para cada tela Turing rev C (a geração serial: 8.8", 5", 2.1"
redonda e outras) você escolhe o que acontece no lugar disso:

| Escolha | Quando o computador desliga ou reinicia | Gravado na tela (o plano B) |
|---|---|---|
| **Deixar como está** (o padrão) | nada é enviado, como antes | nada; escolhê-la desfaz o que as outras gravaram |
| **Apagar a tela** | a tela apaga por completo, inclusive a luz de fundo | o temporizador de repouso dela: 1 a 10 minutos (sugestão: 5) |
| **Tocar um vídeo guardado na tela** | o vídeo escolhido toca em loop | ligar com um vídeo: o primeiro de `sd/video` |
| **Álbum de fotos do cartão** | a tela reinicia e, uns 15 s depois, mostra o álbum | ligar com o álbum de `sd/image` |

As outras telas não oferecem a escolha, e o `bezel standby` diz que ela não é
suportada nelas.

## Quem cumpre a escolha

- O **Bezel Studio**, enquanto está aberto (na bandeja já basta). No Linux ele
  pede ao sistema (systemd-logind) que o espere no desligamento, dentro dos
  poucos segundos que o sistema dá (5 s por padrão); no Windows ele age quando
  a sessão termina (desligar, reiniciar ou sair da sessão). Sair do Bezel pela
  bandeja ou pela janela não aplica nada.
- A linha de comando só registra a escolha e grava o plano B. Sem o Bezel
  Studio aberto (só o `bezel run` ou o serviço dele, ou o studio fechado), nada
  é enviado no desligamento e sobra só o plano B: com **Apagar a tela**, o
  temporizador de repouso da tela a apaga alguns minutos depois; um vídeo ou o
  álbum só começam quando a própria tela liga de novo (o computador corta a
  energia da USB, uma queda de energia, uma reinicialização). Enquanto o
  computador mantém a USB energizada depois de desligar, a tela fica congelada
  na última imagem até lá.
- Se a escolha não pode ser cumprida (o vídeo foi apagado, o cartão foi
  retirado), a tela é apagada em vez de ficar congelada. Uma tela que já estava
  dormindo fica como está.
- Quando o computador liga de novo, o Bezel acorda a tela e volta a mostrar o
  seu tema ao vivo.
- Suspender o computador não está coberto, nem sair da sessão no Linux (só
  desligar e reiniciar).

## O plano B

Escolher também grava um ajuste na própria tela, para quando o studio não pode
agir (ele não estava aberto, o computador perdeu energia). A tela o aplica
quando liga sozinha (ao receber energia, ao reiniciar); só o temporizador de
repouso de **Apagar a tela** age também enquanto a tela continua energizada.

- **Apagar a tela** grava o temporizador de repouso da própria tela: ela se
  apaga depois de tantos minutos **sem receber nada do computador**. Enquanto um
  tema está ao vivo, o Bezel envia à tela uma atualização de um pixel depois de
  30 segundos sem mais nada, para que o temporizador não se esgote. O
  temporizador também para um
  vídeo ou uma imagem que a tela toca sozinha (**Tocar na tela** com o Ao vivo
  desligado), por isso só esta escolha o usa.
- **Álbum de fotos do cartão** faz a tela ligar com o álbum: as fotos de
  `sd/image`, uma depois da outra, a cada 3 a 5 segundos. Quem define o ritmo é
  a tela; não há ajuste para ele.
- **Tocar um vídeo guardado na tela** faz a tela ligar com um vídeo, mas não há
  como dizer à tela qual arquivo: depois de reiniciar ou de perder energia ela
  toca o **primeiro vídeo de `sd/video`** do cartão, não necessariamente o que
  você escolheu.
- **Deixar como está** desfaz isso: o ajuste de início volta ao que o
  **Mostrar ao ligar…** escolheu na aba Armazenamento (ou ao relógio da tela), e
  o temporizador fica desligado.

O **Mostrar ao ligar…** e esta escolha mudam o mesmo ajuste de início: vale o
último que você fez, e definir o arquivo de início mantém o temporizador de
repouso. Com **Álbum de fotos do cartão**, o Bezel Studio grava de novo o ajuste
de início do álbum a cada desligamento, porque é assim que a tela reinicia no
álbum. Veja [Armazenamento e vídeo](storage-and-video.md).

Duas telas do mesmo modelo dividem uma escolha, como dividem o catálogo do
Bezel.

## No aplicativo

**Tela → Ajustes → Quando o computador desligar**: escolha uma das quatro; cada
**?** explica a sua. Uma escolha que não vale fica acinzentada com o motivo: a
tela não está conectada, não é uma tela Turing rev C, não há cartão de memória
ou nenhum vídeo está guardado. Para mudar a escolha, a tela precisa estar
conectada.

Escolher abre uma confirmação que diz o que será gravado na tela; nada é
enviado antes de você confirmar. **Apagar a tela** pergunta os minutos,
**Tocar um vídeo guardado na tela** lista os vídeos da memória interna e do
cartão, e **Álbum de fotos do cartão** abre o álbum.

## Pela linha de comando

```bash
bezel standby show                                   # a escolha e o plano B de cada tela conectada
bezel standby set off --sleep 5 --yes                # apagar no desligamento; temporizador de 5 min
bezel standby set video --file sd/video/clip.mp4 --yes
bezel standby set album --brightness 40 --yes        # o álbum liga com 40% de brilho
bezel standby set keep --yes                         # volta ao padrão; desfaz o plano B
```

Sem `--yes`, o `set` só mostra o que gravaria: nada é enviado à tela e nada é
registrado. O `--brightness N` (0 a 100) escolhe o nível da luz de fundo com que
a tela liga, gravado com o plano B, como faz o `bezel storage boot --brightness`;
sem ele, o padrão da tela, cerca de 67%. O Bezel também registra esse nível:
quando o Bezel Studio reinicia a tela no álbum no desligamento, o álbum liga com
ele. Um vídeo tocado no desligamento fica com o nível que a tela tem naquela
hora; o nível gravado vale quando a tela liga sozinha com um vídeo. O
`bezel standby show` diz o plano B gravado por último na tela, por esta escolha
ou pelo `bezel storage boot`, o que veio depois, ou pelo Bezel Studio ao
reiniciar a tela no álbum no desligamento (que grava de novo o início do álbum,
mesmo depois de um `bezel storage boot`). A escolha fica no catálogo do Bezel
(`<data>/bezel/storage`), que o aplicativo lê no desligamento, então uma escolha
feita aqui com o aplicativo aberto vale.

A linha de comando e as mensagens dela ficam em inglês.

## O álbum de fotos

O álbum é a pasta `sd/image` do cartão, então ele precisa de um cartão SD (veja
[Preparar um cartão SD](sd-card.md)). Ele lista todas as imagens dessa pasta,
com miniatura para as que o Bezel enviou e pelo nome para as outras.

**Adicionar uma foto**: JPEG, PNG, BMP ou o primeiro quadro de um GIF. O Bezel a
deixa em pé pela orientação EXIF (a foto do celular chega em pé) e a enquadra do
jeito que a tela fica, na horizontal ou na vertical: no aplicativo, a orientação
que você usa para aquela tela; na linha de comando, o `--orientation` (padrão: a
do modelo). **Preencher** (`--fit cover`, o padrão) cobre a tela e corta;
**Caber** (`--fit contain`) mostra a foto inteira com preto em volta. A foto é
gravada já girada para o painel, como PNG do tamanho nativo dele (480×1920 no
8.8"). O aplicativo mostra a prévia na forma da tela antes de enviar; um nome
que já está no álbum pede confirmação antes de ser substituído (`--yes` na linha
de comando).

**Remover uma foto** pede confirmação, com o nome do arquivo. O Bezel nunca
apaga nada por conta própria.

```bash
bezel standby album add praia.jpg --orientation horizontal       # Preencher
bezel standby album add retrato.jpg --orientation vertical --fit contain
bezel storage ls sd/image                                        # o álbum
bezel storage rm sd/image/praia.png --yes                        # remove uma foto
```

O `--name` escolhe o nome gravado (por padrão, um feito a partir do da foto).
