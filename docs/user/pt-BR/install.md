# Instalar o Bezel

[English](../install.md)

Baixe na [página de releases](https://github.com/slipalison/bezel/releases).
Cada release traz estes arquivos (`<versão>` é o número da release, como `1.0.0`):

| Sistema | Arquivo | Conteúdo |
|---|---|---|
| Debian 12+, Ubuntu 22.04+, Mint 21+ | `bezel_<versão>_amd64.deb` | aplicativo, comando `bezel`, regra udev, serviço, temas |
| Fedora, openSUSE | `bezel-<versão>-1.x86_64.rpm` | aplicativo, comando `bezel`, regra udev, serviço, temas |
| Qualquer Linux | `bezel_<versão>_amd64.AppImage` | só o aplicativo; roda sem instalar |
| Linux, só o comando | `bezel-x86_64-unknown-linux-gnu.tar.gz` | `bezel` |
| Windows 10 e 11 | `bezel_<versão>_x64-setup.exe` ou `bezel_<versão>_x64_en-US.msi` | o aplicativo |
| Windows, só o comando | `bezel-x86_64-pc-windows-msvc.zip` | `bezel.exe` |

Antes de instalar, feche qualquer outro programa que controle a tela (o app do
fabricante, o turing-smart-screen-python): só um programa pode usá-la por vez.
Veja [Vindo do turing-smart-screen-python](migrating.md).

## Instaladores sem assinatura

Os instaladores do Bezel saem sem assinatura de código (sem Authenticode no
Windows, sem GPG no Linux). Para conferir se o download veio íntegro, compare o
SHA-256 dele com o que a página de releases mostra ao lado de cada arquivo:

```bash
sha256sum bezel_<versão>_amd64.deb
```

```powershell
Get-FileHash .\bezel_<versão>_x64-setup.exe
```

## Linux

**Debian, Ubuntu, Mint:**

```bash
sudo apt install ./bezel_<versão>_amd64.deb
```

**Fedora:**

```bash
sudo dnf install ./bezel-<versão>-1.x86_64.rpm
```

**openSUSE:** `sudo zypper install ./bezel-<versão>-1.x86_64.rpm`

O pacote instala:

- o *Bezel* no menu de aplicativos (`bezel-studio`) e o comando `bezel`;
- a regra udev que deixa você abrir a tela sem root
  (`/usr/lib/udev/rules.d/60-bezel.rules`), aplicada durante a instalação;
- o serviço de usuário `bezel-run@` do systemd
  ([Iniciar com o computador](run-at-login.md));
- os temas que vêm com o Bezel.

Se a tela já estava conectada e o Bezel ainda não consegue abri-la, desconecte e
conecte de novo o cabo USB.

Para remover: `sudo apt remove bezel` ou `sudo dnf remove bezel`.

**AppImage:**

```bash
chmod +x bezel_<versão>_amd64.AppImage
./bezel_<versão>_amd64.AppImage
```

O AppImage não consegue instalar a regra udev: faça isso uma vez, como em
[Deixe o Bezel abrir a tela](permissions.md). Se o AppImage não abrir, rode-o com
`--appimage-extract-and-run`.

**Só o comando:** descompacte o arquivo e ponha o `bezel` no seu `PATH`, por
exemplo em `~/.local/bin`:

```bash
tar -xzf bezel-x86_64-unknown-linux-gnu.tar.gz bezel
install -m 755 bezel ~/.local/bin/bezel
bezel --version
```

## Windows

Rode o `bezel_<versão>_x64-setup.exe` (instala para o seu usuário) ou o `.msi`
(instala para todos os usuários do computador e pede permissão de
administrador). O Bezel aparece no menu Iniciar. Se faltar o Microsoft Edge
WebView2, o instalador baixa.

Como o instalador não tem assinatura, o SmartScreen do Windows pode barrá-lo
com "O Windows protegeu o computador". Clique em **Mais informações**, confira se
o arquivo é o que você baixou e clique em **Executar assim mesmo**.

Para a linha de comando, descompacte o `bezel-x86_64-pc-windows-msvc.zip` numa
pasta da sua escolha (por exemplo `C:\Tools\bezel`) e rode o `bezel.exe` de lá,
ou ponha essa pasta no `PATH`.

Depois: [Deixe o Bezel abrir a tela](permissions.md). As telas da geração Turing
USB e os painéis WCH precisam do driver WinUSB, e as temperaturas da CPU precisam
do LibreHardwareMonitor.

## Compilando o código-fonte

Veja *Build from source* no [README](../../../README.md#build-from-source) do
projeto.
