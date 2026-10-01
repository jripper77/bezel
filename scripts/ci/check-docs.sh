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
#   - every relative link of the docs, README.md and CHANGELOG.md points at a
#     file that exists, and an `#anchor` at a heading of that file;
#   - nothing private: no home-folder paths, e-mail addresses, tokens, keys or
#     screen serial numbers;
#   - README.md no longer says "early development" and links to the guide;
#     CHANGELOG.md has an `## [Unreleased]` section with the changes of each
#     phase (the video framing's included, and the live screen controls fix
#     by the "Device or resource busy" it ends; a phrase may wrap).
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
    "troubleshooting.md", "devices.md",
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
}
BY_LANGUAGE = {
    EN: {
        "install.md": ["More info", "Run anyway", "not signed"],
        "devices.md": ["not validated on hardware"],
        "fps.md": ["not validated on hardware"],
        "troubleshooting.md": ["unplug"],
    },
    PT: {
        "install.md": ["Mais informações", "Executar assim mesmo", "sem assinatura"],
        "devices.md": ["não validado no hardware"],
        "fps.md": ["não validado no hardware"],
        "troubleshooting.md": ["desconecte"],
    },
}
# Headings a page must have, each a whole line outside code blocks
# (phase video-background-framing: the guide to framing a video background).
HEADINGS = {
    EN: {"storage-and-video.md": ["### Framing the video"]},
    PT: {"storage-and-video.md": ["### Enquadrar o vídeo"]},
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
                   "framing", "Device or resource busy"):
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
