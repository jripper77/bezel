#!/usr/bin/env bash
# Checks the user documentation (D-2026-09-30-release-polish-7):
#   - docs/user/ (English) and docs/user/pt-BR/ have the same pages, every
#     page expected below, each one linking to its translation, and each index
#     (README.md) linking to every page of its language;
#   - every page covers its topic (the phrases each page must contain: the
#     commands, the storage manager's `mv`, `cleanup --dry-run` and
#     `cache clear`, the Windows drivers, the unsigned installers, the
#     "not validated on hardware" label of desktop mode and game FPS) and has
#     the headings expected of it (the video framing section of each language);
#     the GIFs and stickers page names KLIPY's Partner Panel and API hosts, the
#     mandatory "Search KLIPY" placeholder and the test key's 100 requests an
#     hour, and has a Privacy section in each language that says, between its
#     heading and the next one of the same or a higher level, what is sent
#     (each query parameter, the formats and stills of GIFs and stickers), to
#     which host, when (nothing without a key nor at start), that previews
#     come through Bezel, where the key is kept and with which permissions,
#     that the window sees only its last 4 characters, how to remove it and
#     where the collection is; the page on what a screen does when the
#     computer shuts down names the `bezel standby` commands, `--yes`, and
#     the card folders of the album and of the start video;
#   - every relative link of the docs, README.md and CHANGELOG.md points at a
#     file that exists, and an `#anchor` at a heading of that file;
#   - nothing private: no home-folder paths, e-mail addresses, tokens, keys or
#     screen serial numbers;
#   - README.md no longer says "early development" and links to the guide;
#     CHANGELOG.md has an `## [Unreleased]` section with the changes of each
#     phase (the video framing's included, the live screen controls fix
#     by the "Device or resource busy" it ends, the GIF and sticker search
#     by "KLIPY", and the choice of what a screen does when the computer
#     "shuts down"; a phrase may wrap).
#
#   bash scripts/ci/check-docs.sh
set -euo pipefail
export LC_ALL=C.UTF-8
cd "$(dirname "$0")/../.."

python3 - <<'PY'
import os
import re
import sys
from pathlib import Path

EN = Path("docs/user")
PT = EN / "pt-BR"
PAGES = [
    "README.md", "install.md", "permissions.md", "first-theme.md",
    "vertical-or-horizontal.md", "sensors.md", "fps.md", "storage-and-video.md",
    "ffmpeg.md", "sd-card.md", "run-at-login.md", "migrating.md",
    "troubleshooting.md", "devices.md", "gifs-and-stickers.md", "power-off.md",
]
# Phrases a page must contain, in both languages unless a language is named.
COMMON = {
    "install.md": [".deb", ".rpm", ".AppImage", ".msi", "setup.exe", "SmartScreen",
                   "sha256sum", "Get-FileHash", "sudo apt install", "sudo dnf install"],
    "permissions.md": ["bezel udev-rules", "sudo tee /etc/udev/rules.d/60-bezel.rules",
                       "usbser", "WinUSB", "Zadig", "1CBE", "43A8", "hidraw",
                       "LibreHardwareMonitor"],
    "first-theme.md": ["bezel run", "bezel import", "BEZEL_FAKE=1"],
    "vertical-or-horizontal.md": ["--orientation", "vertical-flipped", "horizontal-flipped",
                                  "bezel test-pattern"],
    "sensors.md": ["bezel sensors", "`—`", "LibreHardwareMonitor", "--ping-host"],
    "fps.md": ["gpu.fps", "RivaTuner Statistics Server", "RTSS", "MangoHud",
               "autostart_log=1", "Shift_L+F2", "--mangohud-dir", "3"],
    "storage-and-video.md": ["bezel storage put", "bezel storage rm", "--yes", "120 MB",
                             "the stored size differs; delete it and send it again",
                             "bezel storage mv", "cleanup --dry-run", "cache clear"],
    "ffmpeg.md": ["libx264", "sudo apt install ffmpeg", "sudo dnf install ffmpeg",
                  "winget install --id Gyan.FFmpeg -e", "--ffmpeg"],
    "sd-card.md": ["FAT32", "MBR", "mkfs.vfat -F 32", "bezel storage info"],
    "run-at-login.md": ["bezel-run@", "systemctl --user enable --now", "/usr/lib/systemd/user",
                        "schtasks"],
    "migrating.md": ["turing-smart-screen-python", "bezel import", "is in use by",
                     "systemctl --user disable --now"],
    "troubleshooting.md": ["bezel devices", "bezel udev-rules", "is in use by",
                           "the stored size differs; delete it and send it again"],
    "devices.md": ["bezel devices", "bezel monitor-mode --yes", "1A86:AD10"],
    "gifs-and-stickers.md": ["partner.klipy.com", "api.klipy.com", "Search KLIPY", "100"],
    "power-off.md": ["bezel standby show", "bezel standby set", "bezel standby album add",
                     "--yes", "--sleep", "--file", "--brightness", "sd/image", "sd/video",
                     "bezel storage rm"],
}
BY_LANGUAGE = {
    EN: {
        "install.md": ["More info", "Run anyway", "not signed"],
        "devices.md": ["not validated on hardware"],
        "fps.md": ["not validated on hardware"],
        "troubleshooting.md": ["unplug"],
        # The firmware picks the file a start mode shows (protocol-turing-rev-c.md section 19).
        "storage-and-video.md": ["first video of `sd/video`"],
        # The studio's labels, verbatim (src/i18n/en.js: standby.title, standby.choice.*).
        "power-off.md": ["When the computer shuts down", "**Leave as it is**",
                         "**Turn the screen off**", "**Play a video stored on the screen**",
                         "**Photo album from the card**"],
    },
    PT: {
        "install.md": ["Mais informações", "Executar assim mesmo", "sem assinatura"],
        "devices.md": ["não validado no hardware"],
        "fps.md": ["não validado no hardware"],
        "troubleshooting.md": ["desconecte"],
        "storage-and-video.md": ["primeiro vídeo de `sd/video`"],
        # The studio's labels, verbatim (src/i18n/pt-BR.js).
        "power-off.md": ["Quando o computador desligar", "**Deixar como está**",
                         "**Apagar a tela**", "**Tocar um vídeo guardado na tela**",
                         "**Álbum de fotos do cartão**"],
    },
}
# Headings a page must have, each a whole line outside code blocks
# (phase video-background-framing: the guide to framing a video background;
# phase gif-sticker-search: what the GIF search sends, where and when).
HEADINGS = {
    EN: {"storage-and-video.md": ["### Framing the video"],
         "gifs-and-stickers.md": ["### Privacy"]},
    PT: {"storage-and-video.md": ["### Enquadrar o vídeo"],
         "gifs-and-stickers.md": ["### Privacidade"]},
}
# Phrases a section must contain between its heading (one of HEADINGS) and
# the next heading of the same or a higher level (phase gif-sticker-search:
# the Privacy section says what is sent, to which host and when, where the key
# is kept and how to remove it, D-2026-10-01-gif-sticker-search-3, -6 and -7).
# PRIVACY is in both languages.
PRIVACY = ["api.klipy.com", "static.klipy.com", "HTTPS",
           "`q`", "`page`", "`per_page`", "`customer_id`", "`locale`",
           "`content_filter`", "`format_filter`", "`gif,jpg`", "`gif,png`",
           "JPEG", "PNG", "klipy.json", "io.github.slipalison.bezel", "`0600`",
           "bezel/collection", "collection.json"]
SECTIONS = {
    EN: {("gifs-and-stickers.md", "### Privacy"): PRIVACY + [
        "Nothing is sent without a key.", "Nothing is sent when Bezel starts",
        "only when you act", "**Trending**", "**Load more**", "**Add to collection**",
        "a random number", "JPEG for a GIF, PNG for a sticker",
        "Previews come through Bezel", "only its last 4 characters",
        "**Remove**, beside the key field, deletes `klipy.json`"]},
    PT: {("gifs-and-stickers.md", "### Privacidade"): PRIVACY + [
        "Nada é enviado sem uma chave.", "Nada é enviado quando o Bezel abre",
        "só se conecta ao KLIPY quando você age", "**Em alta**", "**Carregar mais**",
        "**Adicionar à coleção**", "um número aleatório",
        "JPEG para um GIF, PNG para um sticker", "As prévias passam pelo Bezel",
        "só os 4 últimos caracteres",
        "**Remover**, ao lado do campo da chave, apaga o `klipy.json`"]},
}
PRIVATE = [
    (re.compile(r"/home/(?!<)[A-Za-z0-9._-]+"), "a home-folder path (use ~ or <you>)"),
    (re.compile(r"/Users/(?!<)[A-Za-z0-9._-]+"), "a home-folder path (use ~ or <you>)"),
    (re.compile(r"[A-Za-z]:\\Users\\(?![<%])[^\\\s`]+", re.I), "a Windows user folder"),
    (re.compile(r"[A-Za-z0-9._%+-]+@(?!example\.)(?:[A-Za-z0-9-]+\.)+[A-Za-z]{2,}(?![-\w])"),
     "an e-mail address"),
    (re.compile(r"\b(gh[pousr]_[A-Za-z0-9]{20,}|github_pat_[A-Za-z0-9_]{20,})"), "a GitHub token"),
    (re.compile(r"\bAKIA[0-9A-Z]{16}\b"), "an AWS key"),
    (re.compile(r"\bxox[abprs]-[A-Za-z0-9-]{10,}"), "a Slack token"),
    (re.compile(r"-----BEGIN [A-Z ]*PRIVATE KEY-----"), "a private key"),
    (re.compile(r"(?i)\b(api[_-]?key|token|secret|password)\b\s*[:=]\s*['\"]?[A-Za-z0-9/+_-]{12,}"),
     "a credential"),
    (re.compile(r"(?i)\bserial\b[\s:=]+(?![<-])(?=[A-Za-z0-9]*\d)[A-Za-z0-9]{6,}"),
     "a serial number (use <serial>)"),
]
LINK = re.compile(r"!?\[[^\]]*\]\(([^)\s]+)(?:\s+\"[^\"]*\")?\)")
REFERENCE = re.compile(r"^\s*\[[^\]]+\]:\s*(\S+)", re.M)
FENCE = re.compile(r"^(```|~~~).*?^\1", re.M | re.S)
CODE = re.compile(r"`[^`\n]*`")

errors = []


def fail(path, message):
    errors.append(f"{path}: {message}")


def text_of(path):
    return Path(path).read_text(encoding="utf-8")


def slug(heading):
    """GitHub's anchor for a heading."""
    h = re.sub(r"\[([^\]]*)\]\([^)]*\)", r"\1", heading.strip())
    h = h.replace("`", "").lower()
    h = re.sub(r"[^\w\- ]", "", h)
    return h.replace(" ", "-")


def anchors(path):
    seen = {}
    out = set()
    body = FENCE.sub("", text_of(path))
    for line in body.splitlines():
        m = re.match(r"^#{1,6}\s+(.*?)\s*#*\s*$", line)
        if not m:
            continue
        base = slug(m.group(1))
        n = seen.get(base, 0)
        seen[base] = n + 1
        out.add(base if n == 0 else f"{base}-{n}")
    return out


def section(path, heading):
    """The text between `heading` (a whole line outside code blocks) and the
    next heading of the same or a higher level, its spaces collapsed (a
    phrase may wrap); None when the page has no such heading."""
    level = len(heading) - len(heading.lstrip("#"))
    body, fence = None, None
    for line in text_of(path).splitlines():
        marker = re.match(r"^(```|~~~)", line)
        if marker and fence in (None, marker.group(1)):
            fence = None if fence else marker.group(1)
        elif fence is None:
            other = re.match(r"^(#{1,6})\s", line)
            if body is not None and other and len(other.group(1)) <= level:
                break
            if body is None and line.rstrip() == heading:
                body = []
                continue
        if body is not None:
            body.append(line)
    return None if body is None else re.sub(r"\s+", " ", "\n".join(body))


def links(path):
    body = CODE.sub("", FENCE.sub("", text_of(path)))
    return LINK.findall(body) + REFERENCE.findall(body)


def check_links(path):
    for target in links(path):
        if re.match(r"^[a-z][a-z0-9+.-]*:", target, re.I):
            continue  # https:, mailto:
        file_part, _, anchor = target.partition("#")
        dest = Path(path) if not file_part else (Path(path).parent / file_part)
        dest = Path(os.path.normpath(dest))
        if not dest.exists():
            fail(path, f"broken link {target}")
            continue
        if anchor and dest.suffix == ".md" and anchor not in anchors(dest):
            fail(path, f"no heading for #{anchor} in {dest}")


def check_private(path):
    for lineno, line in enumerate(text_of(path).splitlines(), 1):
        for pattern, what in PRIVATE:
            m = pattern.search(line)
            if m:
                fail(path, f"line {lineno}: {what}: {m.group(0)}")


# 1. The same pages in both languages, each linking to its translation.
for lang in (EN, PT):
    present = sorted(p.name for p in lang.glob("*.md"))
    for page in PAGES:
        if page not in present:
            fail(lang, f"missing {page}")
    for page in present:
        if page not in PAGES:
            fail(lang / page, "not an expected page (add it to PAGES in scripts/ci/check-docs.sh)")
for page in PAGES:
    en, pt = EN / page, PT / page
    if en.exists() and f"(pt-BR/{page})" not in text_of(en):
        fail(en, f"does not link to its translation pt-BR/{page}")
    if pt.exists() and f"(../{page})" not in text_of(pt):
        fail(pt, f"does not link to the English ../{page}")
for lang in (EN, PT):
    index = lang / "README.md"
    if index.exists():
        linked = {t.partition("#")[0] for t in links(index)}
        for page in PAGES[1:]:
            if page not in linked:
                fail(index, f"does not link to {page}")

# 2. Every page covers its topic.
for lang in (EN, PT):
    for page in PAGES:
        path = lang / page
        if not path.exists():
            continue
        body = re.sub(r"\s+", " ", text_of(path))  # a phrase may wrap
        if len(body.strip()) < 200:
            fail(path, "nearly empty")
        for phrase in COMMON.get(page, []) + BY_LANGUAGE[lang].get(page, []):
            if phrase not in body:
                fail(path, f"does not mention {phrase!r}")
        lines = [line.rstrip() for line in FENCE.sub("", text_of(path)).splitlines()]
        for heading in HEADINGS[lang].get(page, []):
            if heading not in lines:
                fail(path, f"has no heading {heading!r}")
                continue
            text = section(path, heading) or ""
            for phrase in SECTIONS[lang].get((page, heading), []):
                if phrase not in text:
                    fail(path, f"section {heading!r} does not mention {phrase!r}")

# 3 and 4. Links and privacy, over the guide, README.md and CHANGELOG.md.
documents = sorted(EN.rglob("*.md")) + [Path("README.md"), Path("CHANGELOG.md")]
for path in documents:
    check_links(path)
    check_private(path)

# 5. README.md and CHANGELOG.md.
readme = text_of("README.md")
if re.search(r"early development", readme, re.I):
    fail("README.md", 'still says "early development"')
for target in ("docs/user/README.md", "docs/user/pt-BR/README.md"):
    if f"({target})" not in readme:
        fail("README.md", f"does not link to {target}")
changelog = text_of("CHANGELOG.md")
m = re.search(r"^## \[Unreleased\]\n(.*?)(?=^## \[|\Z)", changelog, re.M | re.S)
if not m:
    fail("CHANGELOG.md", "no ## [Unreleased] section")
else:
    unreleased = re.sub(r"\s+", " ", m.group(1))  # a phrase may wrap
    for phrase in ("bezel udev-rules", "bezel monitor-mode", "gpu.fps", "net.ping",
                   "bezel-run@", "/usr/bin/bezel", "docs/user", "bezel storage mv",
                   "framing", "Device or resource busy", "KLIPY", "shuts down"):
        if phrase not in unreleased:
            fail("CHANGELOG.md", f"## [Unreleased] does not mention {phrase!r}")

for e in errors:
    print(f"check-docs: {e}", file=sys.stderr)
if errors:
    print(f"check-docs: FAILED: {len(errors)} problem(s)", file=sys.stderr)
    sys.exit(1)
print(f"check-docs: {len(PAGES)} pages in English and Portuguese, links and privacy checked; "
      "all checks passed")
PY
