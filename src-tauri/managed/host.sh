# Invoked by the native desktop with a fixed operation and sanitized environment.
set -euo pipefail
umask 077
op=$1 worker=$2
fail() { print_error="$1"; printf '%s\n' "$print_error" >&2; exit 1; }
base=${XDG_DATA_HOME:-$HOME/.local/share}/io.github.tenkejj.learnharness-sync/managed
[[ $base == /* && $HOME == /* ]] || fail 'An absolute data directory is required.'
case "$op" in status|launch|recover|bridge|bridge-delay|apply|confirm|qa-populate|qa-extend) ;; *) exit 64 ;; esac
mounts=()
helper=''
qa_root=${4:-}
if [[ $op == qa-populate || $op == qa-extend ]]; then qa_root=${3:-}; else helper=${3:-}; fi
if [[ $op == bridge || $op == bridge-delay || $op == apply || $op == confirm || $op == recover ]]; then
  [[ $helper == /* && -f $helper && ! -L $helper ]] || fail 'Invalid managed bridge launch.'
  if [[ $op == bridge-delay ]]; then [[ ${LEARNHARNESS_SYNC_MANAGED_QA_LOG:-} == 1 ]] || fail 'Managed helper interruption test requires explicit QA mode.'; fi
  mounts=(--ro-bind "$helper" /helper)
fi
if [[ $op == qa-populate || $op == qa-extend ]]; then
  [[ ${LEARNHARNESS_SYNC_MANAGED_QA_LOG:-} == 1 && $qa_root == /* && ! -L $qa_root && -d $qa_root ]] || fail 'Managed capture acceptance assets are unavailable.'
  for asset in tests/learnharness/probe.mjs tests/learnharness/provider.mjs tests/learnharness/managed-manifest.mjs node_modules/playwright/index.mjs build/managed-close-window; do
    [[ -f $qa_root/$asset && ! -L $qa_root/$asset ]] || fail 'Managed capture acceptance assets are incomplete.'
  done
  mounts+=(--ro-bind "$qa_root/tests/learnharness/probe.mjs" /probe.mjs
           --ro-bind "$qa_root/tests/learnharness/provider.mjs" /provider.mjs
           --ro-bind "$qa_root/tests/learnharness/managed-manifest.mjs" /managed-manifest.mjs
           --ro-bind "$qa_root/build/managed-close-window" /close-window
           --ro-bind "$qa_root/node_modules" /node_modules)
fi
release=${LEARNHARNESS_SYNC_VERIFIED_RELEASE:-}
source=${LEARNHARNESS_SYNC_VERIFIED_IMAGE:-}
expected_sha=${LEARNHARNESS_SYNC_VERIFIED_SHA:-}
expected_size=${LEARNHARNESS_SYNC_VERIFIED_SIZE:-}
[[ $release == 061 || $release == 070 ]] || fail 'Unsupported verified LearnHarness release.'
[[ $source == /* && -f $source && ! -L $source && -x $source ]] || fail 'Pinned LearnHarness image is unavailable.'
[[ $expected_sha =~ ^[a-f0-9]{64}$ && $expected_size =~ ^[0-9]{8,9}$ ]] || fail 'Invalid verified LearnHarness identity.'
[[ $(stat -c %s "$source") == "$expected_size" ]] || fail 'Pinned LearnHarness image size changed.'
[[ $(sha256sum -- "$source" | cut -c1-64) == "$expected_sha" ]] || fail 'Pinned LearnHarness image digest changed.'
if [[ ! -e $base/slot && $op == status ]]; then echo new; exit; fi
mkdir -p -m 700 -- "$base"
[[ ! -L $base && $(stat -c '%u:%a' "$base") == "$(id -u):700" ]] || fail 'Managed storage is not private.'
[[ $(stat -f -c %T "$base") == btrfs ]] || fail 'Managed profiles require a local Btrfs filesystem.'
# The private parent excludes other host accounts. Namespace UID 1 must be
# able to create its private vault through the directory bind mount.
mkdir -m 733 -- "$base/slot" 2>/dev/null || [[ -d $base/slot ]]
[[ ! -L $base/slot && $(stat -c '%u:%a' "$base/slot") == "$(id -u):733" ]] || fail 'Invalid managed storage entrance.'
displaynum=0
if [[ $op == launch || $op == qa-populate || $op == qa-extend ]]; then
  tmp=$(mktemp -d)
  server=''
  cleanup() {
    if [[ -n $server ]]; then kill "$server" 2>/dev/null || true; wait "$server" 2>/dev/null || true; fi
    rm -r -- "$tmp"
  }
  trap cleanup EXIT
  cd "$tmp"
  cp -- "$source" appimage
  [[ $(sha256sum appimage | cut -c1-64) == "$expected_sha" ]] || fail 'Private LearnHarness executable differs from the reviewed build.'
  chmod 700 appimage
  ./appimage --appimage-extract >/dev/null
  chmod -R a+rX squashfs-root
  auth=$tmp/display.auth
  cookie=$(mcookie)
  start=$((16#${cookie:0:4} % 100 + 40))
  for offset in {0..99}; do
    candidate=$(((start + offset - 40) % 100 + 40))
    [[ ! -e /tmp/.X11-unix/X$candidate ]] || continue
    rm -f -- "$auth"
    xauth -q -f "$auth" add ":$candidate" . "$cookie"
    chmod 444 "$auth"
    if [[ -n ${WAYLAND_DISPLAY:-} && -x /usr/bin/Xwayland ]]; then
      Xwayland ":$candidate" -geometry 1120x760 -decorate -noreset -nolisten tcp -auth "$auth" >display.log 2>&1 &
    elif [[ -n ${DISPLAY:-} && -x /usr/bin/Xephyr ]]; then
      Xephyr ":$candidate" -screen 1120x760 -noreset -nolisten tcp -auth "$auth" >display.log 2>&1 &
    else
      fail 'A Wayland session with Xwayland, or an X11 session with Xephyr, is required.'
    fi
    server=$!
    for _ in {1..120}; do [[ -S /tmp/.X11-unix/X$candidate ]] && break; kill -0 "$server" 2>/dev/null || break; sleep .05; done
    if kill -0 "$server" 2>/dev/null && [[ -S /tmp/.X11-unix/X$candidate ]]; then
      displaynum=$candidate
      printf '%s\n' "$displaynum" >display
      break
    fi
    kill "$server" 2>/dev/null || true
    wait "$server" 2>/dev/null || true
    server=''
  done
  [[ $displaynum =~ ^[0-9]+$ && -S /tmp/.X11-unix/X$displaynum ]] || fail 'The private display did not start.'
  mounts+=(--ro-bind "$tmp/squashfs-root" /app --ro-bind "$auth" /auth --ro-bind "/tmp/.X11-unix/X$displaynum" /display)
fi
acceptance_env=()
if [[ $op == apply && ${LEARNHARNESS_SYNC_ACCEPTANCE_FAILPOINT:-} == pause-after-apply-switch\
   || $op == apply && ${LEARNHARNESS_SYNC_ACCEPTANCE_FAILPOINT:-} == enospc-after-backup ]]; then
  acceptance_env+=(--setenv LEARNHARNESS_SYNC_ACCEPTANCE_FAILPOINT "${LEARNHARNESS_SYNC_ACCEPTANCE_FAILPOINT}")
fi
unshare --user --map-auto --map-root-user --mount --propagation private \
  bwrap --die-with-parent --new-session --unshare-pid --unshare-net --unshare-ipc --unshare-uts --clearenv \
  --ro-bind /usr /usr --symlink usr/lib /lib --symlink usr/lib /lib64 --symlink usr/bin /bin \
  --dir /etc --ro-bind /etc/fonts /etc/fonts --proc /proc --dev /dev --tmpfs /tmp \
  --bind "$base/slot" /slot "${mounts[@]}" --setenv PATH /usr/bin --setenv LANG C.UTF-8 "${acceptance_env[@]}" \
  setpriv --reuid=1 --regid=1 --clear-groups bash -c "$worker" worker "$op" "$displaynum" "${LEARNHARNESS_SYNC_MANAGED_QA_LOG:-}" "" "$helper" "$qa_root" "$release" "$expected_sha"
