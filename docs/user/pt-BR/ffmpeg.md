# Instalar o ffmpeg

[English](../ffmpeg.md)

O Bezel usa o [ffmpeg](https://ffmpeg.org) para converter vídeos para o formato
da tela, e para tocar o vídeo de fundo de um tema em telas que não tocam vídeo
sozinhas. O ffmpeg não vem com o Bezel. Ele precisa ter o codificador H.264
`libx264`.

Sem o ffmpeg, imagens e vídeos que já estão no formato da tela continuam sendo
enviados; o Bezel avisa quando um vídeo precisa de conversão.

## Linux

**Fedora.** O `ffmpeg-free` do próprio Fedora não tem `libx264`. Ative o RPM
Fusion e instale o ffmpeg completo (ele substitui o `ffmpeg-free`):

```bash
sudo dnf install https://mirrors.rpmfusion.org/free/fedora/rpmfusion-free-release-$(rpm -E %fedora).noarch.rpm
sudo dnf install ffmpeg --allowerasing
```

**Debian, Ubuntu, Mint:**

```bash
sudo apt install ffmpeg
```

**Arch Linux:** `sudo pacman -S ffmpeg`

## Windows

```powershell
winget install --id Gyan.FFmpeg -e
```

Depois feche e abra de novo o Bezel (e qualquer terminal), para eles verem o
`PATH` novo.

## Conferir

```bash
ffmpeg -hide_banner -encoders | grep libx264
```

No Windows: `ffmpeg -hide_banner -encoders | Select-String libx264`. Uma linha
com `libx264` quer dizer que está pronto.

## Um ffmpeg fora do PATH

- No aplicativo: **Tela → Armazenamento → Localizar ffmpeg…** (aparece quando o
  Bezel não acha um).
- Na linha de comando: `--ffmpeg CAMINHO`, o programa ou a pasta dele:

```bash
bezel storage put clipe.mp4 --ffmpeg /opt/ffmpeg/bin/ffmpeg
```
