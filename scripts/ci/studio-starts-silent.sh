#!/usr/bin/env bash
# Proves by behaviour that the studio sends nothing and opens nothing at
# start (D-2026-10-01-gif-sticker-search-16), whatever would do it: Rust, the
# window's JavaScript, a window from the config, a plugin or the system
# browser.
#
#   bash scripts/ci/studio-starts-silent.sh
#
# Linux with a graphical session. It
#   1. compiles scripts/ci/silent-shim.c into a temporary LD_PRELOAD library
#      that records every connect/sendto/sendmsg/sendmmsg with a destination
#      and every program start (exec*, posix_spawn*, and each program image
#      it enters) of the whole process tree;
#   2. builds the studio (`cargo build -p bezel-studio --locked`, debug, in
#      $CARGO_TARGET_DIR when set) and starts the real binary with --hidden,
#      with a temporary HOME and XDG config/data/cache/state folders holding
#      a saved, fake KLIPY key, inside `dbus-run-session` with its own bus
#      (so neither the single-instance plugin nor the tray reaches a studio
#      that is already running), keeping of the user's environment only
#      PATH, LANG and the session's WAYLAND_DISPLAY, DISPLAY, XAUTHORITY,
#      XDG_RUNTIME_DIR, XDG_SESSION_TYPE and XDG_CURRENT_DESKTOP;
#   3. after $SILENT_SECONDS (default 12) stops the whole process tree, and
#      fails on any connection to an AF_INET/AF_INET6 address that is not
#      loopback (127.0.0.0/8, ::1) and on any program but:
#        - the studio itself (its DMA-BUF re-exec);
#        - WebKit's helpers (WebKitWebProcess, WebKitNetworkProcess,
#          WebKitGPUProcess, by basename);
#        - the dbus-* programs dbus-run-session starts before the studio;
#        - the video tools probe, with exactly these arguments:
#          `ffmpeg -hide_banner -version`, `ffmpeg -hide_banner -encoders` and
#          `ffprobe -version` (bezel-media's probe::check). The window asks at
#          start whether ffmpeg can make posters and convert videos
#          (`media_tools`); no input, no output, no URL;
#        - GdkPixbuf's sandboxed image decoders (glycin): `bwrap` with
#          `--unshare-all` (no network) and without `--share-net`, running a
#          program of /usr/libexec/glycin-loaders/ or /usr/bin/true. The tray
#          icon is decoded this way when the bus has no tray host, as here.
#      It also fails when the studio did not start, its web view did not
#      start (so its page ran nothing), it panicked, or it exited within the
#      window.
#
# The test bus starts no D-Bus service (its config names no service
# folder): on the user's bus the desktop's services already run, and here
# a request for one (the accessibility bus, gvfs, the portal) only fails.
#
# While it runs the shim refuses what is not allowed by name
# (SILENT_BLOCK=1): a connection out fails with ENETUNREACH and another
# program is not started (the glycin sandbox included), so a regression never
# reaches the network or the desktop while it is checked.
#
# Exit 0: silent, or "SKIPPED" (loudly) when there is no graphical session
# ($WAYLAND_DISPLAY and $DISPLAY both empty) or no C compiler. Exit 1: the
# studio did something at start, or did not start. Exit 2: it could not be
# checked (not Linux, no dbus-run-session, the build failed).
# SILENT_KEEP=1 keeps the temporary folder (the event log) for a look.
set -euo pipefail
export LC_ALL=C.UTF-8
cd "$(dirname "$0")/../.."

me=studio-starts-silent
say() { printf '%s: %s\n' "$me" "$*"; }
cannot() {
  say "cannot check: $*" >&2
  exit 2
}
skip() {
  printf '\n%s\n%s: SKIPPED: %s\n%s\n\n' \
    '************************************************************' "$me" "$*" \
    '************************************************************' >&2
  exit 0
}

[ "$(uname -s)" = Linux ] || cannot "Linux only (LD_PRELOAD and /proc)"
if [ -z "${WAYLAND_DISPLAY:-}" ] && [ -z "${DISPLAY:-}" ]; then
  skip 'no graphical session ($WAYLAND_DISPLAY and $DISPLAY are empty): the studio cannot open its window here'
fi
cc=""
for c in "${CC:-}" cc gcc clang; do
  if [ -n "$c" ] && command -v "$c" >/dev/null 2>&1; then
    cc=$c
    break
  fi
done
[ -n "$cc" ] || skip 'no C compiler (cc, gcc, clang): the recording shim cannot be built'
for tool in dbus-run-session setsid cargo; do
  command -v "$tool" >/dev/null 2>&1 || cannot "$tool not found"
done
seconds=${SILENT_SECONDS:-12}
[[ "$seconds" =~ ^[1-9][0-9]{0,2}$ ]] || cannot "SILENT_SECONDS must be 1..999, not '$seconds'"
identifier=$(sed -n 's/^  "identifier": "\([^"]*\)",$/\1/p' apps/bezel-studio/src-tauri/tauri.conf.json)
[ -n "$identifier" ] || cannot "no identifier in apps/bezel-studio/src-tauri/tauri.conf.json"

tmp=$(mktemp -d "${TMPDIR:-/tmp}/studio-silent.XXXXXX")
log=$tmp/events.log
out=$tmp/studio.out
leader=""

# Every process still carrying this run's log in its environment: the whole
# tree, even a process that left the process group.
leftovers() {
  { grep -lzxF "SILENT_LOG=$log" /proc/[0-9]*/environ 2>/dev/null || true; } |
    sed -n 's|^/proc/\([0-9]*\)/environ$|\1|p'
}

stop_tree() {
  [ -n "$leader" ] || return 0
  kill -TERM -- "-$leader" 2>/dev/null || true
  local i rest
  for i in $(seq 50); do
    rest=$(leftovers)
    [ -z "$rest" ] && break
    if [ "$i" = 25 ]; then
      # shellcheck disable=SC2086 # one pid per word
      kill -TERM $rest 2>/dev/null || true
    fi
    sleep 0.1
  done
  if [ -n "$rest" ]; then
    say "killing what did not stop on SIGTERM: $(tr '\n' ' ' <<<"$rest")" >&2
    # shellcheck disable=SC2086 # one pid per word
    kill -KILL $rest 2>/dev/null || true
  fi
  wait "$leader" 2>/dev/null || true
  leader=""
}

cleanup() {
  stop_tree
  if [ "${SILENT_KEEP:-0}" = 1 ]; then
    say "kept $tmp" >&2
  else
    rm -rf "$tmp"
  fi
}
trap cleanup EXIT
trap 'exit 130' INT TERM

# 1. The shim.
shim=$tmp/silent-shim.so
"$cc" -O2 -Wall -Wextra -Werror -shared -fPIC -o "$shim" scripts/ci/silent-shim.c -ldl ||
  cannot "the shim did not compile"

# 2. The studio, as built for a debug run.
built=$(cargo build -p bezel-studio --locked --message-format=json-render-diagnostics |
  sed -n 's/.*"executable":"\([^"]*\/bezel-studio\)".*/\1/p' | tail -n 1) ||
  cannot "the studio did not build"
[ -n "$built" ] && [ -x "$built" ] || cannot "cargo built no bezel-studio executable"
studio=$(realpath "$built")

home=$tmp/home
config=$tmp/config
mkdir -p "$home" "$config/$identifier" "$tmp/data" "$tmp/cache" "$tmp/state"
# A saved key (fake), so the start is the one of a user who has set KLIPY up.
(
  umask 077
  printf '{\n  "key": "silent-test-key-0000",\n  "customerId": "0123456789abcdef"\n}\n' \
    >"$config/$identifier/klipy.json"
)
# The test bus: a session bus with no service folder, so it starts nothing.
cat >"$tmp/bus.conf" <<EOF
<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN"
 "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig>
  <type>session</type>
  <keep_umask/>
  <listen>unix:dir=$tmp</listen>
  <auth>EXTERNAL</auth>
  <policy context="default">
    <allow send_destination="*" eavesdrop="true"/>
    <allow eavesdrop="true"/>
    <allow own="*"/>
  </policy>
</busconfig>
EOF

webkit="WebKitWebProcess WebKitNetworkProcess WebKitGPUProcess"
allow="$studio:${webkit// /:}:dbus-run-session:dbus-daemon:ffmpeg:ffprobe"
say "starting $studio --hidden for ${seconds}s (own D-Bus session, temporary HOME and XDG folders, saved test key)"
setsid env -i \
  PATH="$PATH" LANG="${LANG:-C.UTF-8}" \
  ${WAYLAND_DISPLAY:+WAYLAND_DISPLAY="$WAYLAND_DISPLAY"} \
  ${DISPLAY:+DISPLAY="$DISPLAY"} \
  ${XAUTHORITY:+XAUTHORITY="$XAUTHORITY"} \
  ${XDG_RUNTIME_DIR:+XDG_RUNTIME_DIR="$XDG_RUNTIME_DIR"} \
  ${XDG_SESSION_TYPE:+XDG_SESSION_TYPE="$XDG_SESSION_TYPE"} \
  ${XDG_CURRENT_DESKTOP:+XDG_CURRENT_DESKTOP="$XDG_CURRENT_DESKTOP"} \
  HOME="$home" XDG_CONFIG_HOME="$config" XDG_DATA_HOME="$tmp/data" \
  XDG_CACHE_HOME="$tmp/cache" XDG_STATE_HOME="$tmp/state" \
  SILENT_LOG="$log" SILENT_BLOCK=1 SILENT_ALLOW="$allow" LD_PRELOAD="$shim" \
  dbus-run-session --config-file="$tmp/bus.conf" -- "$studio" --hidden \
  </dev/null >"$out" 2>&1 &
leader=$!

# 3. The start window.
ended=""
deadline=$((SECONDS + seconds))
while [ "$SECONDS" -lt "$deadline" ]; do
  if ! kill -0 "$leader" 2>/dev/null; then
    status=0
    wait "$leader" || status=$?
    ended="exited with status $status after about $((seconds - (deadline - SECONDS)))s"
    break
  fi
  sleep 0.5
done
stop_tree
touch "$log"

# The verdict, from the log (tab-separated, see silent-shim.c). Program
# events: `load` (an image the shim entered: exe in $4, command line in $5)
# and `exec` (a start asked for: caller in $3, program in $5, argv in $6).
report=$(awk -F '\t' -v studio="$studio" -v webkit="$webkit" -v tmp="$tmp" \
  -v run="${XDG_RUNTIME_DIR:-/nonexistent}" '
  function base(p) { sub(/.*\//, "", p); return p }
  function args(argv) { sub(/^[^ ]* ?/, "", argv); return argv }
  function short(p) {
    if (index(p, tmp) == 1) p = "<tmp>" substr(p, length(tmp) + 1)
    if (index(p, run) == 1) p = "$XDG_RUNTIME_DIR" substr(p, length(run) + 1)
    return p
  }
  function bad(why) { if (!(why in seen)) { seen[why] = 1; offences[++n] = why } }
  function glycin_sandbox(argv) {
    return argv ~ / --unshare-all( |$)/ && argv !~ / --share-net( |$)/ &&
      (argv ~ / \/usr\/libexec\/glycin-loaders\/[^ ]+( |$)/ || argv ~ / \/usr\/bin\/true$/)
  }
  # What an allowed program is, or "" for one that is not.
  function kind(name, argv, caller, ppid,   b) {
    b = base(name)
    if (name == studio) return "bezel-studio (the studio, then its DMA-BUF re-exec)"
    if (b in helper) return b
    if (b == "dbus-run-session" && NR == 1) return b " (the test bus, before the studio)"
    if (b ~ /^dbus-/ && !started &&
        (caller == "" ? ppid == root : base(caller) == "dbus-run-session"))
      return b " (by dbus-run-session, before the studio)"
    if ((b == "ffmpeg" && (args(argv) == "-hide_banner -version" || args(argv) == "-hide_banner -encoders")) ||
        (b == "ffprobe" && args(argv) == "-version"))
      return b " " args(argv) " (the video tools probe)"
    if (b == "bwrap" && glycin_sandbox(argv))
      return "bwrap --unshare-all for a glycin image decoder"
    if (base(caller) == "bwrap" && (name ~ /^\/usr\/libexec\/glycin-loaders\// || name == "/usr/bin/true"))
      return "glycin decoder inside its sandbox"
    return ""
  }
  BEGIN { split(webkit, h, " "); for (i in h) helper[h[i]] = 1 }
  $1 == "load" {
    if (NR == 1) root = $2
    if ($4 == studio && !started) started = 1
    k = kind($4, $5, "", $3)
    if (k == "") bad("program: pid " $2 " (parent " $3 ") is " $4 ": " $5)
    else loads[k]++
    next
  }
  $1 == "exec" {
    k = kind($5, $6, $3, "")
    if (k == "") bad("program: " base($3) " (pid " $2 ") asked " $4 " to start " $5 ": " $6)
    else if ($7 == "block") refused[k]++
    next
  }
  $1 == "net" {
    if ($8 == "outside") bad("connection: " $4 " by " base($3) " (pid " $2 ") to " $5 " " $6 " port " $7)
    key = $8 " " $5
    dest = $5 == "unix" ? short($6) : ($5 ~ /^inet/ ? $6 " port " $7 : "")
    if (!(key in kinds)) nk++
    kinds[key]++
    if (dest != "" && !((key SUBSEP dest) in dests)) {
      dests[key SUBSEP dest] = 1
      list[key] = list[key] "\n      " dest
    }
    next
  }
  { bad("unreadable log line: " $0) }
  END {
    if (!started) bad("the studio never started: no process ran " studio)
    else if (!loads["WebKitWebProcess"])
      bad("the web view never started (no WebKitWebProcess): the window ran nothing to check")
    print "programs started:"
    for (p in loads) printf "    %s x%d\n", p, loads[p]
    for (p in refused) printf "    refused by the shim, accepted: %s x%d\n", p, refused[p]
    print "connections (scope, family: count, then each destination):"
    if (nk == 0) print "    none"
    for (k in kinds) printf "    %s: %d%s\n", k, kinds[k], list[k]
    for (i = 1; i <= n; i++) print "OFFENCE " offences[i]
  }' "$log")

grep -v '^OFFENCE ' <<<"$report" | sed "s/^/$me: /"
failures=()
while IFS= read -r line; do
  failures+=("${line#OFFENCE }")
done < <(grep '^OFFENCE ' <<<"$report" || true)
if [ -n "$ended" ]; then
  failures+=("the studio $ended, within the ${seconds}s window")
fi
if grep -q 'panicked' "$out"; then
  failures+=("the studio panicked: $(grep -m1 'panicked' "$out")")
fi

if [ "${#failures[@]}" -gt 0 ]; then
  for f in "${failures[@]}"; do say "FAIL: $f" >&2; done
  if [ -s "$out" ]; then
    say "last lines the studio printed:" >&2
    tail -n 15 "$out" | sed 's/^/    /' >&2
  fi
  exit 1
fi
say "OK: in ${seconds}s the studio connected to nothing outside this computer and started only the programs above"
