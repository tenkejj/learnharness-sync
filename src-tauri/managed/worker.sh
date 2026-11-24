# Namespace UID 1 owns the entire vault; the Electron child sees one generation.
set -euo pipefail
umask 077
[[ $(id -u) == 1 ]]
op=$1 number=$2 qa_log=${3:-}
if [[ $op == status && ! -e /slot/vault ]]; then echo new; exit; fi
mkdir -m 700 /slot/vault 2>/dev/null || [[ -d /slot/vault ]]
cd /slot/vault
[[ ! -L /slot/vault && $(stat -c '%u:%a' .) == 1:700 ]]
exec 9<.
if ! flock -n 9; then
  if [[ $op == status ]]; then echo active; exit; fi
  echo 'Close the managed LearnHarness window before continuing.' >&2; exit 75
fi
current=''
if [[ -e current ]]; then
  current=$(cat current)
  [[ $current =~ ^session\.[a-f0-9-]{36}$ && ! -L $current && $(btrfs property get -ts "$current" ro) == ro=true ]] || { echo 'Last saved generation could not be verified.' >&2; exit 1; }
fi
if [[ $op == status ]]; then
  if [[ -e active ]]; then echo recovery; elif [[ -n $current ]]; then echo saved; else echo new; fi
  exit
fi
if [[ $op == recover ]]; then
  if [[ -e active ]]; then
    mv -T active "interrupted.$(cat /proc/sys/kernel/random/uuid)"
    sync -f .
  fi
  echo recovered; exit
fi
[[ ! -e active ]] || { echo 'Recovery required. The interrupted session has been retained.' >&2; exit 76; }
run=session.$(cat /proc/sys/kernel/random/uuid)
printf '%s\n' "$run" > active
sync -f .
if [[ -n $current ]]; then
  btrfs subvolume snapshot "$current" "$run" >/dev/null
else
  btrfs subvolume create "$run" >/dev/null
fi
chmod 700 "$run"
# The lock belongs to this supervisor. Application descendants live in a
# separate PID namespace. Recovery never reuses an interrupted writable tree.
run_app() {
  bwrap --die-with-parent --new-session --unshare-all --clearenv \
  --ro-bind /usr /usr --symlink usr/lib /lib --symlink usr/lib /lib64 --symlink usr/bin /bin \
  --dir /etc --ro-bind /etc/fonts /etc/fonts --proc /proc --dev /dev --tmpfs /tmp \
  --ro-bind /app /app --ro-bind /auth /auth --ro-bind /display "/tmp/.X11-unix/X$number" \
  --bind "$PWD/$run" /profile --chdir /profile --setenv HOME /profile \
  --setenv PATH /usr/bin --setenv LANG C.UTF-8 --setenv XAUTHORITY /auth --setenv DISPLAY ":$number" \
  --setenv XDG_RUNTIME_DIR /tmp/runtime --setenv XDG_CONFIG_HOME /profile/config \
  --setenv XDG_DATA_HOME /profile/data --setenv XDG_CACHE_HOME /profile/cache \
  --setenv XDG_STATE_HOME /profile/state bash -ec '
    mkdir -m 700 /tmp/runtime
    echo localhost >/etc/hostname
    echo "learner:x:1:1:Managed:/profile:/bin/sh" >/etc/passwd
    echo "learner:x:1:" >/etc/group
    exec /app/learnharness --disable-gpu' 9<&-
}
if [[ $qa_log == 1 ]]; then
  run_app || { echo 'Managed LearnHarness stopped abnormally. Recovery required.' >&2; exit 76; }
else
  run_app >/dev/null 2>&1 || { echo 'Managed LearnHarness stopped abnormally. Recovery required.' >&2; exit 76; }
fi
[[ -z $(find "$run" -mindepth 1 -inum 256 -print -quit) ]] || { echo 'Nested subvolume requires manual recovery.' >&2; exit 76; }
sync -f "$run"
btrfs property set -ts "$run" ro true
[[ $(btrfs property get -ts "$run" ro) == ro=true ]]
printf '%s\n' "$run" >current.next
sync -f .
mv -T current.next current
sync -f .
rm -- active
sync -f .
echo saved
