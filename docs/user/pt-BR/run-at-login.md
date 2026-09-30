# Iniciar com o computador

[English](../run-at-login.md)

Escolha **um** dos jeitos abaixo: só um programa controla a tela por vez, então
o início automático do aplicativo e o serviço não podem ficar ligados juntos.

## Pelo aplicativo (Linux e Windows)

Em **Tela → Ajustes**, marque **Iniciar com o computador, na bandeja, mostrando o
último tema ao vivo**. Quando você entra no computador, o Bezel abre na bandeja,
sem a janela, e mostra o último tema que estava ao vivo.

O menu do ícone da bandeja tem **Abrir o Bezel**, **Ao vivo na tela** (liga ou
desliga o tema ao vivo) e **Sair**.

## Como serviço, sem o aplicativo (Linux)

O `bezel-run@<tema>` é um serviço de *usuário* do systemd: ele roda
`bezel run <tema>` enquanto você está conectado, e tenta de novo se a tela
aparecer atrasada.

- Os pacotes deb e rpm o instalam (`/usr/lib/systemd/user/bezel-run@.service`).
- O `scripts/install-local.sh` (compilação do código-fonte) o instala em
  `~/.config/systemd/user`, apontando para o `bezel` que ele instalou.

Ligue com o nome de um tema incluso (`turing-8.8-horizontal`,
`turing-8.8-vertical`, `turing-5-horizontal`, `turing-3.5-vertical`,
`turing-3.5-horizontal`, `turing-2.1-round`):

```bash
systemctl --user daemon-reload
systemctl --user enable --now bezel-run@turing-8.8-horizontal
systemctl --user status bezel-run@turing-8.8-horizontal
journalctl --user -u bezel-run@turing-8.8-horizontal -f    # o log dele
```

Para desligar:

```bash
systemctl --user disable --now bezel-run@turing-8.8-horizontal
```

**O seu próprio arquivo de tema:** dê qualquer nome à instância e troque o
comando uma vez:

```bash
systemctl --user edit bezel-run@meu
```

No editor, acrescente (com o caminho do seu tema):

```ini
[Service]
ExecStart=
ExecStart=/usr/bin/bezel run %h/temas/meu.bezeltheme
```

Depois, `systemctl --user enable --now bezel-run@meu`.

**AppImage ou o arquivo da linha de comando:** eles não instalam o serviço. Crie
uma vez com `systemctl --user edit --force --full bezel-run@.service`, cole o
texto abaixo e troque o caminho do `ExecStart` pelo lugar onde está o seu
`bezel`:

```ini
[Unit]
Description=Bezel theme %i on the smart screen
After=graphical-session.target

[Service]
ExecStart=%h/.local/bin/bezel run %i
Restart=on-failure
RestartSec=5
KillSignal=SIGTERM

[Install]
WantedBy=default.target
```

Quando o serviço para, o Bezel devolve a tela ao modo próprio dela.

## Como tarefa de logon, sem o aplicativo (Windows)

Com o `bezel.exe` descompactado em `C:\Tools\bezel`:

```powershell
schtasks /Create /SC ONLOGON /TN "Bezel" /TR "C:\Tools\bezel\bezel.exe run turing-8.8-horizontal"
```

Uma janela de console fica aberta enquanto ele roda (fechá-la para o tema). Para
remover a tarefa: `schtasks /Delete /TN "Bezel" /F`. O início pela bandeja do
aplicativo não abre console.
