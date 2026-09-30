# FPS de jogos

[English](../fps.md)

O sensor `gpu.fps` (*Game frame rate*) mostra os quadros por segundo do jogo que
você está jogando. O Bezel não se injeta nos jogos: ele lê o que uma ferramenta
de FPS que você já usa publica. Arraste o sensor para um tema como qualquer
outro.

> A leitura segue os formatos publicados dessas ferramentas, mas ainda não foi
> conferida com um jogo rodando (não validado no hardware). Se o valor parecer
> errado, avise.

## Windows: RivaTuner Statistics Server

1. Instale o RivaTuner Statistics Server (RTSS). Ele vem com o MSI Afterburner,
   ou sozinho.
2. Abra o RTSS e deixe-o rodando (ele pode ficar na bandeja).
3. Abra o jogo. O Bezel mostra o FPS do último jogo que o RTSS mediu.

Com o RTSS fechado, o sensor fica indisponível e diz isso ("RivaTuner Statistics
Server is not running: start it ...").

## Linux: registros do MangoHud

O Bezel lê o registro CSV mais novo que o MangoHud grava enquanto registra um
jogo.

1. Rode o jogo com o MangoHud: `mangohud ./jogo`, ou na Steam ponha
   `mangohud %command%` nas opções de inicialização.
2. Comece o registro no jogo com **Shift_L+F2** (a tecla padrão do MangoHud), ou
   faça começar sozinho: acrescente `autostart_log=1` ao
   `~/.config/MangoHud/MangoHud.conf`.

O Bezel procura os registros na `output_folder` do MangoHud (definida no
`MangoHud.conf`), ou na sua pasta pessoal quando ela não está definida. Se você
os guarda em outro lugar, aponte o Bezel para essa pasta: **Preferências →
Sensores → Pasta dos registros do MangoHud** no aplicativo, ou `--mangohud-dir`
na linha de comando:

```bash
bezel run turing-8.8-horizontal --mangohud-dir ~/registros-mangohud
```

O MangoHud grava um arquivo novo a cada registro; apague os antigos de vez em
quando.

## O que aparece

- Um número enquanto o jogo roda. Um 0 informado pela ferramenta aparece como 0.
- `—` quando nenhuma ferramenta está medindo um jogo, ou quando o último valor
  tem mais de 3 segundos (jogo pausado, minimizado ou fechado). O Bezel nunca
  fica mostrando um valor velho.
