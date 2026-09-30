# Telas suportadas

[English](../devices.md)

O `bezel devices` lista as telas conectadas a este computador e o modelo de cada
uma, sem mandar nada para elas.

| Família | Exemplos | Conexão |
|---|---|---|
| Turing rev A | Turing Smart Screen 3,5", UsbPCMonitor 3,5" e 5" | serial |
| XuanFang rev B | XuanFang 3,5" (e Flagship) | serial |
| Turing rev C | Turing Smart Screen de 2,1" a 8,8" | serial |
| Kipye rev D | Kipye Qiye 3,5" | serial |
| WeAct | WeAct Studio Display FS 3,5" e 0,96" | serial |
| Turing USB | geração USB da TURZX / Turing, de 1,6" a 12,3" (id USB `1CBE`) | USB |
| WCH | painéis de 2,4" a 4,3" com chip WCH (id USB `43A8`) | USB |

A Turing 8,8" é testada em hardware real; as outras seguem os protocolos
publicados do app do fabricante e do turing-smart-screen-python. A lista completa
de modelos e ids USB (em inglês) está em
[devices.md](../../reverse-engineering/devices.md).

O armazenamento (imagens e vídeos na tela) existe na Turing rev C e na geração
Turing USB: [Armazenamento e vídeo](storage-and-video.md).

## Modo desktop

Alguns painéis Turing USB (ids USB `1A86:AD10` a `1A86:AD13`) têm um *modo
desktop*, ativado pelo app do fabricante, em que o Windows os usa como segundo
monitor. Nesse modo eles não funcionam como tela de monitoramento, e o Bezel não
consegue desenhar neles.

O Bezel os lista como **desktop mode (not validated on hardware)**, isto é, não
validado no hardware, e consegue devolvê-los ao modo monitor USB. Essa troca
segue o protocolo do fabricante, mas ainda não foi testada num painel real.

```bash
bezel monitor-mode          # diz o que faria; não envia nada
bezel monitor-mode --yes    # pergunta o modelo ao painel e depois troca o modo
```

O painel reinicia como tela USB, e o `bezel devices` passa a listá-lo
normalmente. No aplicativo, a troca fica atrás de um diálogo de confirmação. No
Linux, o painel é acessado pelo `hidraw`, que a regra udev cobre
([Deixe o Bezel abrir a tela](permissions.md)).
