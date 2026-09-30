#!/usr/bin/env bash
# Checks what the Linux packages carry (D-2026-09-30-release-polish-3).
#
#   bash scripts/ci/check-packaging.sh [PACKAGE.deb|PACKAGE.rpm ...]
#
# Always, from the repository:
#   - tauri.conf.json: the deb and the rpm map /usr/bin/bezel (the CLI), the
#     udev rule, the bezel-run@ systemd user unit and the bundled themes
#     (/usr/share/bezel/themes, where /usr/bin/bezel looks for them), with the
#     postinstall script; the studio's own copy of the themes is a resource.
#   - ci.yml: the Linux job builds `bezel` (binarios_extra) in the workspace's
#     target/ before the Tauri bundle, which is where tauri.conf.json takes
#     /usr/bin/bezel from.
#   - the unit starts /usr/bin/bezel, the rule only grants uaccess to the
#     catalog's ids (tty, usb and hidraw), postinstall.sh re-triggers the three
#     subsystems, every bundled theme has its theme.json, and install-local.sh
#     points the unit at the `bezel` it installs.
# Then, for every package given (or found in <target>/release/bundle/{deb,rpm}
# and dist/): usr/bin/bezel and usr/bin/bezel-studio executable, the rule and
# the unit identical to packaging/linux/, every file of themes/ under
# usr/share/bezel/themes and usr/lib/Bezel/themes, the postinstall with the
# hidraw trigger, and the packaged `bezel udev-rules` printing the packaged
# rule. Listing uses `dpkg-deb` (else `ar` + `tar`) and `rpm` (+ `rpm2cpio`).
#
# With no package found only the repository is checked; set
# BEZEL_REQUIRE_PACKAGES=1 to make that a failure. Build the packages the way
# the pipeline does (the CLI first, into the workspace's target/):
#   cargo build --release --locked -p bezel
#   (cd apps/bezel-studio/src-tauri && cargo tauri build --bundles deb,rpm -- --locked)
set -euo pipefail
export LC_ALL=C.UTF-8
cd "$(dirname "$0")/../.."

problems=0
problem() { printf 'check-packaging: %s\n' "$*" >&2; problems=$((problems + 1)); }
ok() { printf 'ok: %s\n' "$*"; }
# `ok` only when the check that started with `problems` at $1 found nothing.
ok_since() { # problems before, message
  if [ "$problems" = "$1" ]; then ok "$2"; fi
}

conf=apps/bezel-studio/src-tauri/tauri.conf.json
unit=packaging/linux/bezel-run@.service
rule=packaging/linux/60-bezel.rules
postinst=packaging/linux/postinstall.sh
exec_start='ExecStart=/usr/bin/bezel run %i'

# ------------------------------------------------------------ repository --

check_config() {
  local out
  if out=$(python3 - "$conf" .github/workflows/ci.yml <<'PY'
import json, os, sys

conf_path, ci_path = sys.argv[1], sys.argv[2]
tauri_dir = os.path.dirname(conf_path)
errors = []
conf = json.load(open(conf_path, encoding="utf-8"))
bundle = conf.get("bundle", {})
if bundle.get("active") is not True:
    errors.append("bundle.active is not true")
if bundle.get("resources", {}).get("../../../themes/") != "themes/":
    errors.append('bundle.resources lacks "../../../themes/": "themes/" (the studio\'s themes)')
expected = {
    "/usr/bin/bezel": "../../../target/release/bezel",
    "/usr/lib/udev/rules.d/60-bezel.rules": "../../../packaging/linux/60-bezel.rules",
    "/usr/lib/systemd/user/bezel-run@.service": "../../../packaging/linux/bezel-run@.service",
    "/usr/share/bezel/themes": "../../../themes/",
}
for kind in ("deb", "rpm"):
    section = bundle.get("linux", {}).get(kind, {})
    files = section.get("files", {})
    for dest, src in expected.items():
        if files.get(dest) != src:
            errors.append(f"{kind}.files[{dest!r}] is {files.get(dest)!r}, expected {src!r}")
    for dest, src in files.items():
        if dest == "/usr/bin/bezel":
            continue  # built by the pipeline before the bundle
        if not os.path.exists(os.path.join(tauri_dir, src)):
            errors.append(f"{kind}.files[{dest!r}]: {src} does not exist")
    script = section.get("postInstallScript")
    if script != "../../../packaging/linux/postinstall.sh":
        errors.append(f"{kind}.postInstallScript is {script!r}")

# The component list of ci.yml is a JSON block under `componentes: |`.
lines = open(ci_path, encoding="utf-8").read().splitlines()
start = next((i for i, l in enumerate(lines) if l.strip() == "componentes: |"), None)
if start is None:
    errors.append("ci.yml has no `componentes: |` block")
else:
    indent = len(lines[start]) - len(lines[start].lstrip())
    block = []
    for line in lines[start + 1:]:
        if line.strip() and len(line) - len(line.lstrip()) <= indent:
            break
        block.append(line)
    components = {c["nome"]: c for c in json.loads("\n".join(block))}
    linux = components.get("rust-linux", {})
    wants = {
        "empacotar_tauri": True,
        "caminho_tauri": "apps/bezel-studio/src-tauri",
        "build_release": "bezel-studio",
    }
    for key, value in wants.items():
        if linux.get(key) != value:
            errors.append(f"ci.yml rust-linux: {key} is {linux.get(key)!r}, expected {value!r}")
    if "bezel" not in linux.get("binarios_extra", "").split():
        errors.append("ci.yml rust-linux: binarios_extra does not build `bezel`")
    if linux.get("caminho", ".") != ".":
        errors.append("ci.yml rust-linux: the workspace must be the repository root (target/ there)")
for e in errors:
    print(e)
sys.exit(1 if errors else 0)
PY
  ); then
    ok "tauri.conf.json maps the CLI, rule, unit and themes into deb and rpm; ci.yml builds bezel first"
  else
    while IFS= read -r line; do problem "$line"; done <<< "$out"
  fi
}

check_unit() {
  if ! grep -qxF "$exec_start" "$unit"; then
    problem "$unit: no \`$exec_start\` line"
  elif [ "$(grep -c '^ExecStart=' "$unit")" != 1 ]; then
    problem "$unit: more than one ExecStart"
  elif ! grep -qx 'WantedBy=default.target' "$unit"; then
    problem "$unit: not WantedBy=default.target"
  else
    ok "$unit starts /usr/bin/bezel"
  fi
}

check_rule() {
  local bad subsystem before=$problems
  bad=$(grep -vE '^(#.*)?$' "$rule" |
    grep -vE '^SUBSYSTEM=="(tty|usb|hidraw)", ATTRS\{idVendor\}=="[0-9a-f]{4}", ATTRS\{idProduct\}=="[0-9a-f]{4}", TAG\+="uaccess"$' || true)
  if [ -n "$bad" ]; then
    problem "$rule: lines that are not a uaccess grant for one USB id:"
    printf '%s\n' "$bad" >&2
  fi
  for subsystem in tty usb hidraw; do
    grep -q "^SUBSYSTEM==\"$subsystem\"" "$rule" || problem "$rule: no $subsystem line"
  done
  ok_since "$before" "$rule grants uaccess only, for tty, usb and hidraw ids"
}

check_postinstall() {
  local subsystem before=$problems
  sh -n "$postinst" || { problem "$postinst: syntax error"; return; }
  grep -qF 'udevadm control --reload' "$postinst" || problem "$postinst: no udev reload"
  for subsystem in tty usb hidraw; do
    grep -qF "udevadm trigger --subsystem-match=$subsystem" "$postinst" ||
      problem "$postinst: no trigger for $subsystem"
  done
  [ "$(tail -n 1 "$postinst")" = "exit 0" ] || problem "$postinst: does not end with exit 0"
  ok_since "$before" "$postinst reloads udev and triggers tty, usb and hidraw"
}

check_themes() {
  local dir name count=0 before=$problems
  for dir in themes/*/; do
    name=$(basename "$dir")
    if [ "$name" = fonts ]; then continue; fi
    count=$((count + 1))
    [ -f "$dir/theme.json" ] || problem "themes/$name has no theme.json"
  done
  [ "$count" -gt 0 ] || problem "themes/ has no theme"
  for name in Inter JetBrainsMono; do
    [ -f "themes/fonts/$name-OFL.txt" ] || problem "themes/fonts: no $name-OFL.txt next to the font"
  done
  ok_since "$before" "themes/: $count bundled themes with their fonts' licenses"
}

check_install_local() {
  local script=scripts/install-local.sh before=$problems
  bash -n "$script" || { problem "$script: syntax error"; return; }
  grep -qF 's|^ExecStart=/usr/bin/bezel |' "$script" ||
    problem "$script: does not point the unit's ExecStart at the installed bezel"
  grep -qF 'cp -r themes "$themes_dir"' "$script" || problem "$script: does not install the themes"
  ok_since "$before" "$script parses and rewrites the unit's ExecStart"
}

# -------------------------------------------------------------- packages --

scratch=$(mktemp -d)
trap 'rm -rf "$scratch"' EXIT

# `mode<TAB>path` per entry of a deb's data archive, without a leading `./`.
deb_list() {
  deb_data "$1" | tar -tvf - |
    awk '{ p = $6; for (i = 7; i <= NF; i++) p = p " " $i; sub(/^\.\//, "", p); print $1 "\t" p }'
}

# The same for an rpm, without the leading `/`.
rpm_list() {
  rpm -qlvp "$1" |
    awk '{ p = $9; for (i = 10; i <= NF; i++) p = p " " $i; sub(/^\//, "", p); print $1 "\t" p }'
}

deb_data() {
  if command -v dpkg-deb >/dev/null 2>&1; then
    dpkg-deb --fsys-tarfile "$1"
  else
    ar_member "$1" data.tar
  fi
}

deb_control() {
  if command -v dpkg-deb >/dev/null 2>&1; then
    dpkg-deb --ctrl-tarfile "$1"
  else
    ar_member "$1" control.tar
  fi
}

# The uncompressed `<prefix>.*` member of a deb (an ar archive).
ar_member() {
  local member
  member=$(ar t "$1" | grep "^$2" | head -n 1)
  case "$member" in
    *.gz) ar p "$1" "$member" | gzip -dc ;;
    *.xz) ar p "$1" "$member" | xz -dc ;;
    *.zst) ar p "$1" "$member" | zstd -dcq ;;
    *.tar) ar p "$1" "$member" ;;
    *) printf 'check-packaging: %s: no %s member\n' "$1" "$2" >&2; return 1 ;;
  esac
}

# Extracts the package's files into a folder.
extract() { # package, destination
  mkdir -p "$2"
  case "$1" in
    *.deb) deb_data "$1" | tar -xf - -C "$2" ;;
    *.rpm) (cd "$2" && rpm2cpio "$1" | cpio -idm --quiet) ;;
  esac
}

# The package's entries (`listing`) and its postinstall (`script`).
read_package() { # package, listing file
  case "$1" in
    *.deb)
      if ! command -v dpkg-deb >/dev/null 2>&1 && ! command -v ar >/dev/null 2>&1; then
        problem "$1: neither dpkg-deb nor ar to read it"
        return 1
      fi
      deb_list "$1" > "$2" || { problem "$1: cannot list it"; return 1; }
      script=$(deb_control "$1" | tar -xOf - --wildcards '*postinst' 2>/dev/null || true)
      ;;
    *.rpm)
      if ! command -v rpm >/dev/null 2>&1 || ! command -v rpm2cpio >/dev/null 2>&1; then
        problem "$1: rpm and rpm2cpio are needed to read it"
        return 1
      fi
      rpm_list "$1" > "$2" || { problem "$1: cannot list it"; return 1; }
      script=$(rpm -qp --scripts "$1" || true)
      ;;
    *) problem "$1: not a .deb or .rpm"; return 1 ;;
  esac
}

check_package() {
  local pkg listing paths root theme_file dest mode script="" before=$problems
  pkg=$(realpath "$1")
  listing="$scratch/listing"
  paths="$scratch/paths"
  read_package "$pkg" "$listing" || return 0
  cut -f2 "$listing" > "$paths"

  for dest in usr/bin/bezel usr/bin/bezel-studio; do
    mode=$(awk -F '\t' -v p="$dest" '$2 == p { print $1 }' "$listing")
    case "$mode" in
      -rwx*) ;;
      '') problem "$1: no $dest" ;;
      *) problem "$1: $dest is $mode, not executable" ;;
    esac
  done
  for dest in usr/lib/udev/rules.d/60-bezel.rules usr/lib/systemd/user/bezel-run@.service; do
    grep -qxF "$dest" "$paths" || problem "$1: no $dest"
  done
  while IFS= read -r theme_file; do
    for dest in "usr/share/bezel/$theme_file" "usr/lib/Bezel/$theme_file"; do
      grep -qxF "$dest" "$paths" || problem "$1: no $dest"
    done
  done < <(find themes -type f | sort)
  grep -qF 'udevadm trigger --subsystem-match=hidraw' <<< "$script" ||
    problem "$1: its postinstall does not trigger hidraw"

  root="$scratch/root-$(basename "$pkg")"
  if extract "$pkg" "$root"; then
    cmp -s "$root/usr/lib/udev/rules.d/60-bezel.rules" "$rule" ||
      problem "$1: its udev rule differs from $rule"
    cmp -s "$root/usr/lib/systemd/user/bezel-run@.service" "$unit" ||
      problem "$1: its unit differs from $unit"
    if [ -x "$root/usr/bin/bezel" ]; then
      if "$root/usr/bin/bezel" udev-rules 2>/dev/null | cmp -s - "$rule"; then
        ok "$1: its $("$root/usr/bin/bezel" --version) prints the packaged udev rule"
      else
        problem "$1: its \`bezel udev-rules\` does not print $rule"
      fi
    fi
  else
    problem "$1: could not extract it"
  fi
  ok_since "$before" "$1: bezel and bezel-studio executable; rule, unit, themes and postinstall as packaged"
}

find_packages() {
  local target="${CARGO_TARGET_DIR:-target}"
  shopt -s nullglob
  local found=("$target"/release/bundle/deb/*.deb "$target"/release/bundle/rpm/*.rpm dist/*.deb dist/*.rpm)
  shopt -u nullglob
  printf '%s\n' "${found[@]}"
}

check_config
check_unit
check_rule
check_postinstall
check_themes
check_install_local

packages=("$@")
if [ "${#packages[@]}" = 0 ]; then
  mapfile -t packages < <(find_packages | sed '/^$/d')
fi
if [ "${#packages[@]}" = 0 ]; then
  if [ "${BEZEL_REQUIRE_PACKAGES:-0}" = 1 ]; then
    problem "no .deb or .rpm given or found in \${CARGO_TARGET_DIR:-target}/release/bundle or dist/"
  else
    printf 'note: no .deb or .rpm given or found: only the repository was checked\n'
  fi
fi
for pkg in "${packages[@]}"; do
  if [ -f "$pkg" ]; then
    check_package "$pkg"
  else
    problem "$pkg: no such file"
  fi
done

if [ "$problems" != 0 ]; then
  printf 'check-packaging: FAILED: %s problem(s)\n' "$problems" >&2
  exit 1
fi
printf 'check-packaging: all checks passed\n'
