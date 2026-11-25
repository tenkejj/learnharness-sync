# Namespace UID 1 owns the entire vault; the Electron child sees one generation.
set -euo pipefail
umask 077
[[ $(id -u) == 1 ]]
op=$1 number=$2 qa_log=${3:-}
qa_root=${6:-}
release=${7:-}
build_sha=${8:-}
[[ $build_sha =~ ^[a-f0-9]{64}$ ]] || { echo 'Missing verified executable identity.' >&2; exit 64; }
case "$release" in
  061) vault=/slot/vault; profile_format=learnharness-0.6.1-disposable-v1 ;;
  070) vault=/slot/vault-070; profile_format=learnharness-0.7.0-disposable-v1 ;;
  *) echo 'Unsupported managed LearnHarness release.' >&2; exit 64 ;;
esac
if [[ $op == status && ! -e $vault ]]; then echo new; exit; fi
mkdir -m 700 "$vault" 2>/dev/null || [[ -d $vault ]]
cd "$vault"
[[ ! -L $vault && $(stat -c '%u:%a' .) == 1:700 ]]
exec 9<.
if ! flock -n 9; then
  if [[ $op == status ]]; then echo active; exit; fi
  echo 'Close the managed LearnHarness window before continuing.' >&2; exit 75
fi
if [[ $release == 070 ]]; then
  if [[ ! -e profile-format ]]; then
    # The subordinate UID binds this vault to 0.7.0 while holding writer custody.
    [[ ! -e current && ! -e active && ! -e apply.pending.json ]] || { echo 'Unversioned 0.7.0 vault refused.' >&2; exit 76; }
    printf '%s\n' "$profile_format" > profile-format
    sync -f .
  fi
  [[ ! -L profile-format && $(cat profile-format) == "$profile_format" ]] || { echo 'Managed vault release mismatch.' >&2; exit 76; }
else
  [[ ! -e profile-format ]] || { echo 'Legacy managed vault release mismatch.' >&2; exit 76; }
fi
if [[ ! -e profile-build ]]; then
  # Older vaults predate this marker. Bind them only to their original,
  # independently qualified executable; never infer a future upgrade.
  if [[ -e current ]]; then
    if [[ $release == 061 ]]; then
      [[ $build_sha == b8a033eb7421677893d96e4988b30153fdfe4fc6f0dc456f7947161bba041b98 ]] || { echo 'Legacy 0.6.1 vault requires its original verified build.' >&2; exit 76; }
    else
      [[ $build_sha == 772b5e04d380cbe8a72a75dbff269aeb6a47f4d4452711b1da8059a872067613 ]] || { echo 'Legacy 0.7.0 vault requires its original verified build.' >&2; exit 76; }
    fi
  fi
  build_marker=$(mktemp .profile-build.XXXXXXXX)
  printf '%s\n' "$build_sha" > "$build_marker"
  sync -f "$build_marker"
  mv -T -- "$build_marker" profile-build
  sync -f .
fi
[[ -f profile-build && ! -L profile-build && $(cat profile-build) == "$build_sha" ]] || { echo 'Managed profile is bound to another verified executable. Keep its pinned image; upgrades need separate qualification.' >&2; exit 76; }
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
    --setenv PATH /usr/bin --setenv LANG C.UTF-8 --setenv LEARNHARNESS_SYNC_PROFILE_FORMAT "$profile_format" /helper --managed-bridge-stdio
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
    --setenv PATH /usr/bin --setenv LANG C.UTF-8 --setenv LEARNHARNESS_SYNC_PROFILE_FORMAT "$profile_format" /helper --managed-bridge-stdio "${helper_args[@]}"
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
  helper_acceptance_env=()
  if [[ $op == apply && ${LEARNHARNESS_SYNC_ACCEPTANCE_FAILPOINT:-} == pause-after-apply-switch\
     || $op == apply && ${LEARNHARNESS_SYNC_ACCEPTANCE_FAILPOINT:-} == enospc-after-backup ]]; then
    helper_acceptance_env+=(--setenv LEARNHARNESS_SYNC_ACCEPTANCE_FAILPOINT "${LEARNHARNESS_SYNC_ACCEPTANCE_FAILPOINT}")
  fi
  if bwrap --die-with-parent --unshare-all --clearenv \
    --ro-bind /usr /usr --symlink usr/lib /lib --symlink usr/lib /lib64 --symlink usr/bin /bin \
    --proc /proc --dev /dev \
    --ro-bind /helper /helper --bind "$PWD" /vault --chdir /vault \
    --setenv PATH /usr/bin --setenv LANG C.UTF-8 --setenv LEARNHARNESS_SYNC_PROFILE_FORMAT "$profile_format" "${helper_acceptance_env[@]}" /helper --managed-bridge-stdio; then
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
if [[ $op == qa-populate || $op == qa-extend ]]; then
  [[ $number =~ ^[0-9]+$ && $qa_log == 1 && $qa_root == /* && $5 == '' ]] || { echo 'Invalid real-application capture acceptance request.' >&2; exit 64; }
  qa_app_mounts+=(--ro-bind /probe.mjs /probe.mjs --ro-bind /provider.mjs /provider.mjs --ro-bind /close-window /close-window --ro-bind /node_modules /node_modules)
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
  --setenv XDG_STATE_HOME /profile/state --setenv LEARNHARNESS_SYNC_PROFILE_FORMAT "$profile_format" bash -ec '
    mkdir -m 700 /tmp/runtime
    echo localhost >/etc/hostname
    echo "learner:x:1:1:Managed:/profile:/bin/sh" >/etc/passwd
    echo "learner:x:1:" >/etc/group
    if [[ "$1" == qa-populate || "$1" == qa-extend ]]; then
      /app/learnharness --disable-gpu --remote-debugging-port=9222 >/tmp/learnharness.log 2>&1 &
      app=$!
      for _ in {1..400}; do
        curl -fsS http://127.0.0.1:9222/json/version >/dev/null 2>&1 && break
        kill -0 "$app" 2>/dev/null || { cat /tmp/learnharness.log >&2; exit 1; }
        sleep .1
      done
      curl -fsS http://127.0.0.1:9222/json/version >/dev/null || { cat /tmp/learnharness.log >&2; kill "$app" 2>/dev/null || true; wait "$app" 2>/dev/null || true; exit 1; }
      if [[ "$1" == qa-populate ]]; then phase=transcript; label=Managed; else phase=extend-transcript; label=Managed; fi
      node /probe.mjs "$phase" "$label" || { kill "$app" 2>/dev/null || true; wait "$app" 2>/dev/null || true; exit 1; }
      if /close-window --ready >/dev/null 2>&1; then /close-window >/dev/null; fi
      wait "$app"
    else
      exec /app/learnharness --disable-gpu
    fi' managed "$op" 9<&-
}
if [[ $qa_log == 1 ]]; then
  run_app || { echo 'Managed LearnHarness stopped abnormally. Recovery required.' >&2; exit 76; }
else
  run_app >/dev/null 2>&1 || { echo 'Managed LearnHarness stopped abnormally. Recovery required.' >&2; exit 76; }
fi
[[ -z $(find "$run" -mindepth 1 -inum 256 -print -quit) ]] || { echo 'Nested subvolume requires manual recovery.' >&2; exit 76; }
if [[ $op == qa-populate || $op == qa-extend ]]; then
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
