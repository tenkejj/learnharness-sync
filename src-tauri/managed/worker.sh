# Namespace UID 1 owns the entire vault; the Electron child sees one generation.
set -euo pipefail
umask 077
[[ $(id -u) == 1 ]]
op=$1 number=$2 qa_log=${3:-}
qa_root=${6:-}
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
is_verified_current() {
  [[ -d transactions ]] || return 1
  for transaction in transactions/apply-*.json; do
    [[ -f $transaction ]] || continue
    grep -Fq '"state":"application_readback_verified"' "$transaction" \
      && grep -Fq "\"new_generation\":\"$current\"" "$transaction" && return 0
  done
  return 1
}
if [[ $op == status ]]; then
  if [[ -e active ]]; then echo recovery
  elif [[ -e apply.pending.json ]]; then echo awaiting-readback
  elif is_verified_current; then echo verified
  elif [[ -n $current ]]; then echo saved
  else echo new
  fi
  exit
fi
helper_args=()
if [[ $op == recover ]]; then
  [[ $4 == '' && $5 == /* ]] || { echo 'Invalid managed recovery helper.' >&2; exit 64; }
  bwrap --die-with-parent --unshare-all --clearenv \
    --ro-bind /usr /usr --symlink usr/lib /lib --symlink usr/lib /lib64 --symlink usr/bin /bin \
    --proc /proc --dev /dev \
    --ro-bind /helper /helper --bind "$PWD" /vault --chdir /vault \
    --setenv PATH /usr/bin --setenv LANG C.UTF-8 /helper --managed-bridge-stdio
  exit
fi
if [[ $op == bridge || $op == bridge-delay ]]; then
  [[ $number == 0 && $4 == '' && $5 == /* ]] || { echo 'Invalid bridge request.' >&2; exit 64; }
  if [[ $op == bridge ]]; then [[ $qa_log == '' ]] || { echo 'Invalid bridge request.' >&2; exit 64; }
  else [[ $qa_log == 1 ]] || { echo 'Helper interruption probe requires explicit QA mode.' >&2; exit 64; }
  fi
  [[ -n $current && ! -e active ]] || { echo 'A saved, idle managed generation is required.' >&2; exit 76; }
  printf 'bridge\n' > active
  sync -f .
  helper_args=()
  if [[ $op == bridge-delay ]]; then helper_args+=(--test-pause-before-read); fi
  bwrap --die-with-parent --unshare-all --clearenv \
    --ro-bind /usr /usr --symlink usr/lib /lib --symlink usr/lib /lib64 --symlink usr/bin /bin \
    --ro-bind /helper /helper \
    --dir /capture --ro-bind "$PWD/active" /capture/active \
    --ro-bind "$PWD/$current" /capture/generation --chdir /capture \
    --setenv PATH /usr/bin --setenv LANG C.UTF-8 /helper --managed-bridge-stdio "${helper_args[@]}"
  rm -- active
  sync -f .
  exit
fi
if [[ $op == apply || $op == confirm ]]; then
  [[ $4 == '' && $5 == /* ]] || { echo 'Invalid managed transaction helper.' >&2; exit 64; }
  if [[ $op == apply ]]; then
    [[ -n $current && ! -e active && ! -e apply.pending.json ]] || { echo 'A saved idle managed profile with no pending transaction is required.' >&2; exit 76; }
    printf 'apply\n' > active
  else
    [[ ! -e active ]] || { echo 'A managed writer is active; close LearnHarness first.' >&2; exit 75; }
    printf 'confirm\n' > active
  fi
  sync -f .
  if bwrap --die-with-parent --unshare-all --clearenv \
    --ro-bind /usr /usr --symlink usr/lib /lib --symlink usr/lib /lib64 --symlink usr/bin /bin \
    --proc /proc --dev /dev \
    --ro-bind /helper /helper --bind "$PWD" /vault --chdir /vault \
    --setenv PATH /usr/bin --setenv LANG C.UTF-8 /helper --managed-bridge-stdio; then
    rm -- active
    sync -f .
  else
    result=$?
    # Before the durable apply journal exists, no generation switch can have
    # occurred. Clear only that safe pre-transaction refusal; otherwise keep
    # the marker so explicit recovery is required.
    if [[ $op == apply && ! -e apply.pending.json ]]; then
      rm -- active
      sync -f .
    fi
    exit "$result"
  fi
  exit
fi
qa_app_mounts=()
if [[ $op == qa-populate ]]; then
  [[ $number =~ ^[0-9]+$ && $qa_log == 1 && $qa_root == /* && $5 == '' ]] || { echo 'Invalid real-application capture acceptance request.' >&2; exit 64; }
  qa_app_mounts+=(--ro-bind /probe.mjs /probe.mjs --ro-bind /provider.mjs /provider.mjs --ro-bind /node_modules /node_modules)
elif [[ $op != launch ]]; then
  exit 64
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
  "${qa_app_mounts[@]}" \
  --bind "$PWD/$run" /profile --chdir /profile --setenv HOME /profile \
  --setenv PATH /usr/bin --setenv LANG C.UTF-8 --setenv XAUTHORITY /auth --setenv DISPLAY ":$number" \
  --setenv XDG_RUNTIME_DIR /tmp/runtime --setenv XDG_CONFIG_HOME /profile/config \
  --setenv XDG_DATA_HOME /profile/data --setenv XDG_CACHE_HOME /profile/cache \
  --setenv XDG_STATE_HOME /profile/state bash -ec '
    mkdir -m 700 /tmp/runtime
    echo localhost >/etc/hostname
    echo "learner:x:1:1:Managed:/profile:/bin/sh" >/etc/passwd
    echo "learner:x:1:" >/etc/group
    if [[ "$1" == qa ]]; then
      /app/learnharness --disable-gpu --remote-debugging-port=9222 >/tmp/learnharness.log 2>&1 &
      app=$!
      for _ in {1..400}; do
        curl -fsS http://127.0.0.1:9222/json/version >/dev/null 2>&1 && break
        kill -0 "$app" 2>/dev/null || { cat /tmp/learnharness.log >&2; exit 1; }
        sleep .1
      done
      curl -fsS http://127.0.0.1:9222/json/version >/dev/null || { cat /tmp/learnharness.log >&2; kill "$app" 2>/dev/null || true; wait "$app" 2>/dev/null || true; exit 1; }
      node /probe.mjs transcript Managed || { kill "$app" 2>/dev/null || true; wait "$app" 2>/dev/null || true; exit 1; }
      wait "$app"
    else
      exec /app/learnharness --disable-gpu
    fi' managed "$([[ $op == qa-populate ]] && echo qa || echo app)" 9<&-
}
if [[ $qa_log == 1 ]]; then
  run_app || { echo 'Managed LearnHarness stopped abnormally. Recovery required.' >&2; exit 76; }
else
  run_app >/dev/null 2>&1 || { echo 'Managed LearnHarness stopped abnormally. Recovery required.' >&2; exit 76; }
fi
[[ -z $(find "$run" -mindepth 1 -inum 256 -print -quit) ]] || { echo 'Nested subvolume requires manual recovery.' >&2; exit 76; }
if [[ $op == qa-populate ]]; then
  node /managed-manifest.mjs "$PWD/$run"
  rm -f -- "$run/transcript-id" "$run/expected-transcript.json"
fi
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
