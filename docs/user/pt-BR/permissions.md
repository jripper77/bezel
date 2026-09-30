# Deixe o Bezel abrir a tela

[English](../permissions.md)

O Bezel nunca pede permissão de administrador e nunca instala drivers. O que o
sistema precisa, uma vez só, depende de como você instalou.

## Linux

**Pacotes deb e rpm:** nada a fazer. O pacote instala uma regra udev que dá à
pessoa conectada no computador acesso a todas as telas suportadas. Se a tela já
estava conectada antes da instalação, desconecte e conecte de novo.

**AppImage, o arquivo da linha de comando ou compilação do código-fonte:**
instale a regra uma vez. O `bezel udev-rules` imprime a regra e, logo abaixo, o
comando único que a instala:

```bash
bezel udev-rules
```

Copie esse comando e rode no terminal. Ele é parecido com este (pede a sua senha
porque grava em `/etc`):

```bash
bezel udev-rules 2>/dev/null | sudo tee /etc/udev/rules.d/60-bezel.rules >/dev/null && sudo udevadm control --reload && sudo udevadm trigger
```

O aplicativo faz o mesmo: quando o Linux recusa a porta da tela, o Bezel mostra
o comando, pronto para copiar. O Bezel nunca o executa por você.

Depois desconecte e conecte a tela de novo, e confira se ela aparece:

```bash
bezel devices
```

A regra cobre as telas seriais (`/dev/ttyACM*`), as famílias USB bulk (Turing
USB, WCH) e os painéis Turing USB em modo desktop (`hidraw`). Ela só dá acesso a
quem está conectado no computador; não muda mais nada.

## Windows

- **Telas seriais** (Turing rev A e rev C, XuanFang, Kipye, WeAct): o driver do
  próprio Windows (usbser) cuida delas; elas aparecem como portas COM. Nada a
  instalar.
- **Geração Turing USB (id USB `1CBE:xxxx`) e painéis WCH (`43A8:xxxx`):**
  precisam do driver WinUSB. Se o `bezel devices` não lista a tela e o
  Gerenciador de Dispositivos a mostra sem driver:
  1. Baixe o Zadig em [zadig.akeo.ie](https://zadig.akeo.ie).
  2. No Zadig, escolha **Options → List All Devices** e selecione a tela na
     lista (confira se o USB ID começa com `1CBE` ou `43A8`).
  3. Escolha **WinUSB** como driver e clique em **Install Driver** (ou
     **Replace Driver**).

  Para desfazer, abra o Gerenciador de Dispositivos, clique com o botão direito
  na tela, escolha **Desinstalar dispositivo** e marque a opção que remove o
  driver.
- **Painéis Turing USB em modo desktop** usam o driver HID do Windows: nada a
  instalar. Veja [Telas suportadas](devices.md#modo-desktop).
- **Sensores:** temperatura, ventoinhas e potência da CPU vêm do
  LibreHardwareMonitor, que precisa estar rodando como administrador. Veja
  [Sensores](sensors.md#windows).
