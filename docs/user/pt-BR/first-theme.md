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
- Aba **Mídia**: **Adicionar imagem…** para usar uma imagem no tema, ou **Usar
  como fundo**.
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
