# O seu primeiro tema

[English](../first-theme.md)

Abra o **Bezel** pelo menu de aplicativos (ou rode `bezel-studio`). A barra de
cima mostra a tela conectada, ou "Nenhuma tela conectada".

## 1. Comece por um tema

Na biblioteca, à esquerda, abra a aba **Temas**:

- clique num tema incluso para abri-lo (há um para cada tamanho de tela), ou
- **Novo vertical** / **Novo horizontal** para um tema vazio do tamanho da sua
  tela.

## 2. Edite

- Aba **Widgets**: arraste um widget (texto, valor, relógio, imagem, forma,
  barra, anel, ponteiro, gráfico) para a tela de edição, ou clique nele para
  adicioná-lo no centro.
- Aba **Sensores**: arraste um sensor para mostrar o valor dele. Use a busca
  para achar um.
- Clique num elemento para mudá-lo no painel **Propriedades**, à direita: sensor,
  fonte, cores, tamanho e de quanto em quanto tempo ele atualiza.
- Aba **Camadas**: ordene, oculte e trave elementos.
- Aba **Mídia**: **Adicionar imagem…** para usar uma imagem no tema,
  **Adicionar vídeo…** para um vídeo de fundo
  ([Um vídeo no fundo](#um-vídeo-no-fundo)) e **Usar como fundo**.
- **Desfazer** (Ctrl+Z) e **Refazer** (Ctrl+Shift+Z) ficam na barra de cima, ao
  lado do zoom.

O que aparece na edição é exatamente o que a tela vai receber: o desenho sai do
mesmo renderizador.

## 3. Mostre na tela

Ligue a chave **Ao vivo** na barra de cima. A tela acompanha as suas mudanças
enquanto você edita. Clique em **Salvar** para guardar o tema na sua biblioteca.

Fechar a janela deixa o tema rodando pelo ícone na bandeja. O menu da bandeja
tem **Abrir o Bezel**, **Ao vivo na tela** e **Sair**. Para ele voltar sozinho
quando você entrar no computador, veja
[Iniciar com o computador](run-at-login.md).

## 4. Ajustes da tela

A aba **Tela** tem duas partes:

- **Ajustes**: **Brilho**, **Devolver ao modo próprio da tela** (o relógio dela
  ou a mídia guardada) e a opção de iniciar com o computador.
- **Armazenamento**: imagens e vídeos guardados na tela
  ([Armazenamento e vídeo](storage-and-video.md)).

## Um vídeo no fundo

1. Na aba **Mídia**, clique em **Adicionar vídeo…** e escolha um arquivo MP4,
   MOV, M4V, MKV, WebM ou AVI, ou um GIF animado. Você também pode soltar o
   arquivo na aba Mídia, ou direto na área de edição: ali ele vira o fundo na
   hora.
2. O Bezel copia o vídeo para dentro do tema e tira dele uma imagem parada, a
   *imagem de capa*: um segundo depois do início, ou a primeira imagem de um
   clipe curto ou de um GIF, recortada no formato do tema. A edição mostra a
   imagem de capa por baixo dos seus elementos. Tirá-la precisa do ffmpeg
   ([Instalar o ffmpeg](ffmpeg.md)); sem ele o vídeo entra mesmo assim, sem
   imagem de capa, e o painel **Propriedades** diz como instalar o ffmpeg. A
   aba Mídia mostra a duração e o tamanho do vídeo.
3. Clique em **Usar como fundo** no vídeo. O painel **Propriedades** mostra o
   vídeo e a imagem de capa, com **Trocar vídeo…**, **Usar imagem…** e **Usar
   cor sólida**. Desfazer (Ctrl+Z) traz o fundo anterior de volta.

O que a tela faz com ele:

- Telas que tocam vídeo sozinhas (Turing rev C, como a de 8,8", e a geração
  Turing USB) repetem uma cópia guardada nelas, e o Bezel desenha o tema por
  cima. Com o **Ao vivo** ligado, o painel **Propriedades** diz se a tela já
  guarda o vídeo; se não, **Abrir Armazenamento** leva a **Enviar para a
  tela**, que converte o vídeo para a tela (um GIF animado vira MP4) e o envia.
- As telas Turing rev C aceitam no máximo **25 MiB por arquivo**. A conversão
  baixa a taxa de bits conforme a duração do vídeo para ele caber, então um
  clipe longo perde um pouco de qualidade; se mesmo assim não couber, o Bezel
  avisa antes de enviar qualquer coisa
  ([Qual o tamanho máximo de um arquivo](storage-and-video.md#qual-o-tamanho-máximo-de-um-arquivo)).
- Telas que não tocam vídeo recebem o vídeo decodificado no computador enquanto
  o **Ao vivo** está ligado, o que também precisa do ffmpeg.

Salvar o tema guarda o vídeo e a imagem de capa dentro do arquivo
`.bezeltheme`.

## Temas de outros aplicativos

**Temas → Importar…** converte um tema do app do fabricante (`.turtheme`) ou do
turing-smart-screen-python (a pasta do tema ou o `theme.yaml`). O que não deu
para converter exatamente aparece numa lista depois da importação.

## O mesmo pela linha de comando

```bash
bezel run turing-8.8-horizontal                  # um tema incluso, ao vivo, até Ctrl+C
bezel run ~/temas/meu.bezeltheme                 # um arquivo de tema
bezel render turing-8.8-vertical -o previa.png   # um quadro numa imagem, sem precisar da tela
bezel import AMD.turtheme -o amd.bezeltheme      # converte o tema de outro aplicativo
```

Um tema é um arquivo `.bezeltheme`: um zip com `theme.json` e uma pasta
`assets/`.

**Ainda sem tela?** `BEZEL_FAKE=1 bezel-studio` abre o Bezel com uma tela de 8,8"
simulada e sensores de demonstração.
