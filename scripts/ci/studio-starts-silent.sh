#!/usr/bin/env bash
# Proves by behaviour that the studio sends nothing and opens nothing at
# start (D-2026-10-01-gif-sticker-search-16, -17, -18), whatever would do it:
# Rust, the window's JavaScript, a window from the config, a plugin or the
# system browser.
#
#   bash scripts/ci/studio-starts-silent.sh
#
# Linux with KDE's KWin (kwin_wayland), D-Bus and a C compiler. No graphical
# session is used: nothing appears on the user's desktop. It
#   1. compiles scripts/ci/silent-shim.c into a temporary LD_PRELOAD library
#      that records every connect/sendto/sendmsg/sendmmsg with a destination
#      and every program start (exec*, posix_spawn*, and each program image
#      it enters) of the whole process tree;
#   2. builds the studio (`cargo build -p bezel-studio --locked`, debug, in
#      $CARGO_TARGET_DIR when set);
#   3. starts the real binary four times: with a saved, fake KLIPY key and
#      with none, each hidden (`--hidden`, as autostart starts it) and
#      shown. Each run has
#        - its own `dbus-run-session`, a bus with no service folder: it
#          starts nothing, and neither the single-instance plugin nor the
#          tray reaches a studio that is already running;
#        - its own headless compositor, `kwin_wayland --virtual` on a socket
#          of its own, with no Xwayland, lock screen, global shortcuts or
#          activities (so it starts no helper). KWin starts the studio once
#          its socket is up (`--exit-with-session`), through
#          `env LD_PRELOAD=<shim> WAYLAND_DISPLAY=<its socket>`: KWin has
#          file capabilities (cap_sys_nice), so glibc preloads nothing into
#          it and drops LD_PRELOAD from what it passes on. KWin is the one
#          process of a run the shim does not record; any other one fails;
#        - a temporary HOME, XDG config/data/cache/state folders and
#          XDG_RUNTIME_DIR. Of the user's environment only PATH, LANG and
#          XDG_CURRENT_DESKTOP are kept: DISPLAY, WAYLAND_DISPLAY,
#          XAUTHORITY and DBUS_SESSION_BUS_ADDRESS are not passed, so
#          nothing can reach the user's display or session bus;
#   4. after $SILENT_SECONDS (default 12, counted once the compositor is up)
#      stops the run's process group (compositor and studio), checks that
#      nothing of it is left, and fails the run when
#        - the window never ran: the studio did not run the video tools
#          probe (`ffmpeg -hide_banner -version`), which the window asks for
#          through `media_tools` at the end of its start (app.js); a page
#          that throws, or a web view that never starts, fails;
#        - the studio did not start, panicked, or exited within the window;
#        - a process connected (or sent a datagram) anywhere but
#            - the run's compositor socket,
#            - the run's test bus socket,
#            - the system bus (/run/dbus/system_bus_socket),
#            - systemd's userdb sockets (/run/systemd/userdb/<name>: user
#              lookups),
#            - a loopback address on port 53 (the DNS stub resolver);
#          so the user's own session bus and display sockets (wayland-*,
#          /tmp/.X11-unix/*), any other loopback port and any other address
#          fail;
#        - a program was started but
#            - KWin, by dbus-run-session, with exactly the arguments above,
#              before the studio;
#            - the dbus-* programs dbus-run-session starts before the studio;
#            - the studio itself (its DMA-BUF re-exec);
#            - WebKit's helpers (WebKitWebProcess, WebKitNetworkProcess,
#              WebKitGPUProcess, by basename);
#            - the video tools probe, with exactly these arguments:
#              `ffmpeg -hide_banner -version`, `ffmpeg -hide_banner -encoders`
#              and `ffprobe -version` (bezel-media's probe::check): no input,
#              no output, no URL;
#            - only as an attempt the shim refused: GdkPixbuf's sandboxed
#              image decoder (glycin), `bwrap --unshare-all` without
#              `--share-net` running a program of /usr/libexec/glycin-loaders/
#              or /usr/bin/true. The tray icon is decoded this way when the
#              bus has no tray host, as here.
#
# While it runs the shim refuses what is not accepted (SILENT_BLOCK=1): a
# connection elsewhere fails (ENETUNREACH; EACCES for a unix socket) and
# another program is not started (the glycin sandbox included), so a
# regression never reaches the network, the user's bus or the desktop while
# it is checked.
#
# Prints one summary per run and, when all four pass, a last line
# `studio-starts-silent: OK: ...`. Exit 0: silent, or "SKIPPED" (loudly, with
# no OK line) without kwin_wayland or a C compiler. Exit 1: a run failed.
# Exit 2: it could not be checked (not Linux, a tool missing, the build
# failed). SILENT_KEEP=1 keeps the temporary folder (the event logs).
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
kwin=$(command -v kwin_wayland || true)
[ -n "$kwin" ] || skip 'no kwin_wayland: each run needs a headless compositor of its own (KWin, --virtual)'
cc=""
for c in "${CC:-}" cc gcc clang; do
  if [ -n "$c" ] && command -v "$c" >/dev/null 2>&1; then
    cc=$c
    break
  fi
done
[ -n "$cc" ] || skip 'no C compiler (cc, gcc, clang): the recording shim cannot be built'
for tool in dbus-run-session setsid cargo ps env; do
  command -v "$tool" >/dev/null 2>&1 || cannot "$tool not found"
done
envbin=$(command -v env)
seconds=${SILENT_SECONDS:-12}
[[ "$seconds" =~ ^[1-9][0-9]{0,2}$ ]] || cannot "SILENT_SECONDS must be 1..999, not '$seconds'"
identifier=$(sed -n 's/^  "identifier": "\([^"]*\)",$/\1/p' apps/bezel-studio/src-tauri/tauri.conf.json)
[ -n "$identifier" ] || cannot "no identifier in apps/bezel-studio/src-tauri/tauri.conf.json"

# The user's own session bus and display, which no run may reach.
user_run=${XDG_RUNTIME_DIR:-/run/user/$(id -u)}
user_bus=$user_run/bus
case "${DBUS_SESSION_BUS_ADDRESS:-}" in
  unix:path=*) user_bus_address=${DBUS_SESSION_BUS_ADDRESS#unix:path=} ;;
  *) user_bus_address=$user_bus ;;
esac
user_bus_address=${user_bus_address%%,*}

# A short folder: unix socket paths (the compositor's, the bus's) must fit
# in 108 bytes.
base=${TMPDIR:-/tmp}
[ "${#base}" -le 40 ] || base=/tmp
tmp=$(mktemp -d "$base/studio-silent.XXXXXX")
sid=""
log=/nonexistent

# Every live process of the current run: its session (setsid), and any
# process still carrying its log in its environment, even one that left the
# session. Zombies are left out (their parent reaps them).
members() {
  {
    if [ -n "$sid" ]; then
      ps -o pid=,stat= -s "$sid" 2>/dev/null | awk '$2 !~ /^Z/ {print $1}' || true
    fi
    { grep -lzxF "SILENT_LOG=$log" /proc/[0-9]*/environ 2>/dev/null || true; } |
      sed -n 's|^/proc/\([0-9]*\)/environ$|\1|p'
  } | sort -un
}

# Stops the run (its process group, then any member left) and answers in
# $left what was still running after SIGKILL.
left=""
stop_run() {
  left=""
  [ -n "$sid" ] || return 0
  kill -TERM -- "-$sid" 2>/dev/null || true
  local i rest=""
  for i in $(seq 50); do
    rest=$(members)
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
    sleep 0.5
    left=$(members | tr '\n' ' ')
  fi
  wait "$sid" 2>/dev/null || true
  sid=""
}

cleanup() {
  stop_run
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
case "$studio$shim$envbin" in
  *[\'\ ]*) cannot "a path with a quote or a space: $studio, $shim, $envbin" ;;
esac
webkit="WebKitWebProcess WebKitNetworkProcess WebKitGPUProcess"

# The verdict on one run's log (tab-separated, see silent-shim.c): `OFFENCE`
# lines, then one `SUMMARY` line. Program events: `load` (an image the shim
# entered: exe in $4, command line in $5) and `exec` (a start asked for:
# caller in $3, program in $5, argv in $6, the shim's verdict in $7).
verdict() {
  # KWin's argv through the environment: `-v` would turn its \x20 into spaces.
  SILENT_KWIN_ARGV=$1 awk -F '\t' -v studio="$studio" -v webkit="$webkit" -v kwin="$kwin" \
    -v compositor="$2" -v bus="$3" -v dir="$4" \
    -v user_run="$user_run" -v user_bus="$user_bus" -v user_bus_address="$user_bus_address" '
  function base(p) { sub(/.*\//, "", p); return p }
  function args(argv) { sub(/^[^ ]* ?/, "", argv); return argv }
  function short(p) {
    if (index(p, dir "/") == 1) p = "<run>" substr(p, length(dir) + 1)
    return p
  }
  function bad(why) { if (!(why in seen)) { seen[why] = 1; offences[++n] = why } }
  function count(list, k) {
    if (!((list SUBSEP k) in counted)) order[list, ++size[list]] = k
    counted[list, k]++
  }
  function listed(list,   i, k, s) {
    for (i = 1; i <= size[list]; i++) {
      k = order[list, i]
      s = s (i > 1 ? ", " : "") k " x" counted[list, k]
    }
    return s == "" ? "none" : s
  }
  function glycin_sandbox(argv) {
    return argv ~ / --unshare-all( |$)/ && argv !~ / --share-net( |$)/ &&
      (argv ~ / \/usr\/libexec\/glycin-loaders\/[^ ]+( |$)/ || argv ~ / \/usr\/bin\/true$/)
  }
  # What an allowed program is, or "" for one that is not.
  function kind(name, argv, caller, ppid, shim_verdict,   b) {
    b = base(name)
    if (name == studio) return "bezel-studio"
    if (b in helper) return b
    if (b == "dbus-run-session" && NR == 1) return b
    if (b ~ /^dbus-/ && !started &&
        (caller == "" ? ppid == root : base(caller) == "dbus-run-session"))
      return b
    if (name == kwin && !started && base(caller) == "dbus-run-session" && argv == kwin_argv)
      return "kwin_wayland --virtual"
    if ((b == "ffmpeg" && (args(argv) == "-hide_banner -version" || args(argv) == "-hide_banner -encoders")) ||
        (b == "ffprobe" && args(argv) == "-version"))
      return b " " args(argv)
    if (b == "bwrap" && caller != "" && shim_verdict == "block" && glycin_sandbox(argv))
      return "bwrap for glycin"
    return ""
  }
  function loopback(family, address) {
    if (family == "inet") return address ~ /^127\./
    return family == "inet6" && (address == "::1" || address ~ /^::ffff:7f[0-9a-f][0-9a-f]:/)
  }
  # What an accepted destination is, or "" for one that is not.
  function socket_kind(family, address, port) {
    if (family == "unix") {
      if (address == compositor) return "compositor"
      if (address == bus) return "test bus"
      if (address == "/run/dbus/system_bus_socket") return "system bus"
      if (index(address, "/run/systemd/userdb/") == 1 && length(address) > 20 &&
          substr(address, 21) !~ /\//)
        return "userdb"
      return ""
    }
    if (loopback(family, address) && port == 53) return "DNS (loopback port 53)"
    return ""
  }
  function refused_why(family, address) {
    if (family == "unix") {
      if (address == user_bus || address == user_bus_address) return "the user'"'"'s own session bus"
      if (address ~ /^@?\/tmp\/\.X11-unix\//) return "the user'"'"'s X display"
      if (index(address, user_run "/wayland-") == 1) return "the user'"'"'s Wayland display"
      if (index(address, user_run "/") == 1) return "a socket of the user'"'"'s own session"
      return "not a socket of this run, the system bus or userdb"
    }
    if (loopback(family, address)) return "a loopback port other than 53 (the DNS stub)"
    if (family ~ /^inet/) return "an address outside this computer"
    return "a socket family that is not accepted"
  }
  BEGIN {
    split(webkit, h, " ")
    for (i in h) helper[h[i]] = 1
    kwin_argv = ENVIRON["SILENT_KWIN_ARGV"]
  }
  $1 == "load" {
    if (NR == 1) root = $2
    if ($4 == studio && !started) started = 1
    if ($4 ~ /\/WebKitWebProcess$/) webview = 1
    k = kind($4, $5, "", $3, $6)
    if (k == "") bad("program: pid " $2 " (parent " $3 ") is " $4 ": " $5)
    else count("programs", k)
    next
  }
  $1 == "exec" {
    if ($3 == studio && base($5) == "ffmpeg" && args($6) == "-hide_banner -version") probe = 1
    k = kind($5, $6, $3, "", $7)
    if (k == "") bad("program: " base($3) " (pid " $2 ") asked " $4 " to start " $5 ": " $6)
    else if ($7 == "block") count("refused", k)
    else if (k == "kwin_wayland --virtual") count("programs", k)
    next
  }
  $1 == "net" {
    k = socket_kind($5, $6, $7)
    if (k != "") { count("sockets", k); next }
    dest = $5 == "unix" ? short($6) : $5 " " $6 " port " $7
    bad("connection: " $4 " by " base($3) " (pid " $2 ") to " dest ": " refused_why($5, $6))
    next
  }
  { bad("unreadable log line: " $0) }
  END {
    if (!started) bad("the studio never started: no process ran " studio)
    else if (!webview)
      bad("the window never ran: its web view never started (no WebKitWebProcess)")
    else if (!probe)
      bad("the window never ran: the studio never ran the video tools probe (ffmpeg -hide_banner -version), which the window asks for through media_tools at the end of its start (app.js), so its page stopped before or never loaded")
    for (i = 1; i <= n; i++) print "OFFENCE " offences[i]
    printf "SUMMARY %s; programs: %s; refused by the shim (accepted): %s; sockets: %s\n",
      probe ? "the window ran (video tools probe)" : "the window did not run",
      listed("programs"), listed("refused"), listed("sockets")
  }' "$log"
}

# Which live processes of the run the shim did not record (no `load` line),
# but the compositor; one that is between fork and exec gets a moment.
unrecorded() {
  local kwin_pid pid cmd tab=$'\t' candidates=()
  kwin_pid=$(awk -F '\t' -v k="$kwin" '$1 == "exec" && $5 == k {print $2; exit}' "$log")
  for pid in $(members); do
    [ "$pid" = "$kwin_pid" ] || grep -q "^load${tab}$pid${tab}" "$log" || candidates+=("$pid")
  done
  [ "${#candidates[@]}" -gt 0 ] || return 0
  sleep 0.3
  for pid in "${candidates[@]}"; do
    [ -d "/proc/$pid" ] || continue
    grep -q "^load${tab}$pid${tab}" "$log" && continue
    cmd=$(tr '\0' ' ' <"/proc/$pid/cmdline" 2>/dev/null || true)
    printf 'pid %s (%s)\n' "$pid" "${cmd:-unreadable}"
  done
}

# One run: $1 its number, $2 `key` or `none`, $3 `hidden` or `shown`.
# Prints its summary; answers 1 when it failed.
run_once() {
  local n=$1 key=$2 mode=$3 label dir out sock compositor session allow sockets
  local kwin_args kwin_argv began ready="" ended="" deadline status lost report line
  local failures=()
  label="$([ "$key" = key ] && echo 'saved key' || echo 'no key'), $mode"
  dir=$tmp/$n
  log=$dir/events.log
  out=$dir/out
  mkdir -p "$dir/home" "$dir/config/$identifier" "$dir/data" "$dir/cache" "$dir/state"
  mkdir -m 700 "$dir/rt"
  if [ "$key" = key ]; then
    # A saved key (fake): the start of a user who has set KLIPY up.
    (
      umask 077
      printf '{\n  "key": "silent-test-key-0000",\n  "customerId": "0123456789abcdef"\n}\n' \
        >"$dir/config/$identifier/klipy.json"
    )
  fi
  # The test bus: a session bus with no service folder, so it starts nothing.
  cat >"$dir/bus.conf" <<EOF
<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN"
 "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig>
  <type>session</type>
  <keep_umask/>
  <listen>unix:path=$dir/bus</listen>
  <auth>EXTERNAL</auth>
  <policy context="default">
    <allow send_destination="*" eavesdrop="true"/>
    <allow eavesdrop="true"/>
    <allow own="*"/>
  </policy>
</busconfig>
EOF
  sock=bezel-silent-$$-$n
  compositor=$dir/rt/$sock
  session="$envbin LD_PRELOAD='$shim' WAYLAND_DISPLAY='$sock' '$studio'"
  [ "$mode" = hidden ] && session+=" --hidden"
  kwin_args=(--virtual --socket="$sock" --width 1280 --height 800
    --no-lockscreen --no-global-shortcuts --no-kactivities --exit-with-session "$session")
  # As the shim writes it: arguments joined by spaces, a space inside one as \x20.
  kwin_argv="$kwin ${kwin_args[*]:0:${#kwin_args[@]}-1} ${session// /\\x20}"
  allow="$studio:${webkit// /:}:dbus-run-session:dbus-daemon:ffmpeg:ffprobe:$kwin"
  sockets="$compositor:$dir/bus:/run/dbus/system_bus_socket:/run/systemd/userdb/"

  began=$SECONDS
  setsid env -i \
    PATH="$PATH" LANG="${LANG:-C.UTF-8}" XDG_SESSION_TYPE=wayland \
    ${XDG_CURRENT_DESKTOP:+XDG_CURRENT_DESKTOP="$XDG_CURRENT_DESKTOP"} \
    HOME="$dir/home" XDG_CONFIG_HOME="$dir/config" XDG_DATA_HOME="$dir/data" \
    XDG_CACHE_HOME="$dir/cache" XDG_STATE_HOME="$dir/state" XDG_RUNTIME_DIR="$dir/rt" \
    SILENT_LOG="$log" SILENT_BLOCK=1 SILENT_ALLOW="$allow" SILENT_SOCKETS="$sockets" \
    LD_PRELOAD="$shim" \
    dbus-run-session --config-file="$dir/bus.conf" -- "$kwin" "${kwin_args[@]}" \
    </dev/null >"$out" 2>&1 &
  sid=$!

  # The compositor first: the window is counted once its socket is up.
  deadline=$((SECONDS + 30))
  while [ "$SECONDS" -lt "$deadline" ] && kill -0 "$sid" 2>/dev/null; do
    if [ -S "$compositor" ]; then
      ready=1
      break
    fi
    sleep 0.1
  done
  if [ -n "$ready" ]; then
    deadline=$((SECONDS + seconds))
    while [ "$SECONDS" -lt "$deadline" ]; do
      if ! kill -0 "$sid" 2>/dev/null; then
        status=0
        wait "$sid" || status=$?
        ended="the studio (and so its compositor) exited with status $status after about $((seconds - (deadline - SECONDS)))s, within the ${seconds}s window"
        break
      fi
      sleep 0.5
    done
    lost=$(unrecorded)
  else
    failures+=("the private compositor never started: no socket $sock within 30s")
    lost=""
  fi
  stop_run
  touch "$log"

  report=$(verdict "$kwin_argv" "$compositor" "$dir/bus" "$dir")
  while IFS= read -r line; do
    failures+=("${line#OFFENCE }")
  done < <(grep '^OFFENCE ' <<<"$report" || true)
  [ -z "$ended" ] || failures+=("$ended")
  if grep -q 'panicked' "$out"; then
    failures+=("the studio panicked: $(grep -m1 'panicked' "$out")")
  fi
  while IFS= read -r line; do
    [ -z "$line" ] || failures+=("a process the shim did not record ran in the run: $line")
  done <<<"$lost"
  [ -z "$left" ] || failures+=("left running after the run, even after SIGKILL: $left")

  line="run $n/4 ($label): $([ "${#failures[@]}" -eq 0 ] && echo OK || echo FAIL) in $((SECONDS - began))s: $(sed -n 's/^SUMMARY //p' <<<"$report")"
  say "$line"
  [ "${#failures[@]}" -gt 0 ] || return 0
  for line in "${failures[@]}"; do say "FAIL ($label): $line" >&2; done
  if [ -s "$out" ]; then
    say "last lines printed in the run ($label):" >&2
    tail -n 15 "$out" | sed 's/^/    /' >&2
  fi
  return 1
}

# 3. The four starts.
start=$SECONDS
failed=0
n=0
for run in "key hidden" "key shown" "none hidden" "none shown"; do
  n=$((n + 1))
  # shellcheck disable=SC2086 # two words: key, mode
  run_once "$n" $run || failed=$((failed + 1))
done
if [ "$failed" -gt 0 ]; then
  say "$failed of 4 runs failed (see FAIL above)" >&2
  exit 1
fi
say "OK: in 4 runs of ${seconds}s ($((SECONDS - start))s; saved key and none, hidden and shown; each in its own compositor and bus) the window ran, and the studio connected only to its compositor, its test bus, the system bus, userdb and the DNS stub, and started only the programs above"
