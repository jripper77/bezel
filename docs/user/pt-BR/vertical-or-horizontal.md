# Use a tela na vertical ou na horizontal

[English](../vertical-or-horizontal.md)

A maioria das telas pode ficar em pé (vertical, mais alta que larga) ou deitada
(horizontal, mais larga que alta). Cada tema guarda a orientação para a qual
foi feito.

## No aplicativo

- Os botões **Vertical** e **Horizontal** na barra de cima giram o tema. O layout
  acompanha: uma coluna de medidores vira uma linha.
- **Girar 180°**, ao lado deles, serve para uma tela montada de cabeça para baixo
  (o cabo sai do outro lado).
- **Temas → Novo vertical / Novo horizontal** começam um tema vazio nessa
  orientação. Os temas inclusos da 8,8" e da 3,5" vêm nas duas.

Com o **Ao vivo** ligado, a tela acompanha na hora.

## Pela linha de comando

Desenhe o padrão de teste para ver qual lado é o de cima:

```bash
bezel test-pattern --orientation horizontal --seconds 5
```

Os cantos são marcados em vermelho (em cima à esquerda), verde (em cima à
direita), branco (embaixo à direita) e azul (embaixo à esquerda). Se saírem de
cabeça para baixo, use a orientação invertida.

O `--orientation` aceita `vertical`, `horizontal`, `vertical-flipped` e
`horizontal-flipped` (ou `portrait`, `landscape`, `reverse-portrait`,
`reverse-landscape`):

```bash
bezel show papel-de-parede.png                           # horizontal para imagem larga, vertical nos outros casos
bezel show cartaz.jpg --orientation vertical --fit contain
```

O `bezel run` usa a orientação gravada no tema: abra o tema no aplicativo, gire
e salve.
