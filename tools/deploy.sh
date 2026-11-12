#!/bin/sh
# Build, package, install on a TV over ssh and (re)launch the picker as a normal app.
#   tools/deploy.sh [--no-launch] [tv-ip]
# The TV and its ssh options come from the argument, the environment, or .build/tv.env
# (copy tools/tv.env.example there; .build/ is not committed). --no-launch installs without opening it.
set -eu
ROOT=$(dirname "$(dirname "$(realpath "$0")")")
LAUNCH=1
while [ $# -gt 0 ]; do
  case "$1" in
    --no-launch) LAUNCH=0; shift ;;
    -h|--help) sed -n '2,5p' "$0"; exit 0 ;;
    -*) echo "unknown option: $1" >&2; exit 2 ;;
    *) break ;;
  esac
done
# shellcheck disable=SC1091
# The environment wins over the file.
ENV_TV=${TV:-} ENV_SSH_OPTS=${SSH_OPTS-unset}
[ -f "$ROOT/.build/tv.env" ] && . "$ROOT/.build/tv.env"
[ -n "$ENV_TV" ] && TV=$ENV_TV
[ "$ENV_SSH_OPTS" != unset ] && SSH_OPTS=$ENV_SSH_OPTS
TV=${1:-${TV:-192.168.1.180}}
ID=com.thoxy.outpick
SSH="ssh -o ConnectTimeout=5 ${SSH_OPTS:-} root@$TV"
"$ROOT/tools/build-webos.sh" >/dev/null
# package.py prints the .ipk it wrote on its first line.
IPK=$(python3 "$ROOT/tools/package.py" | head -1)
[ -f "$IPK" ] || { echo "package.py wrote no ipk ($IPK)" >&2; exit 1; }
echo "$IPK -> $TV"
scp -q ${SSH_OPTS:-} "$IPK" root@$TV:/tmp/$ID.ipk
$SSH "luna-send -n 1 luna://com.webos.applicationManager/closeByAppId '{\"id\":\"$ID\"}' >/dev/null 2>&1 </dev/null
luna-send -n 30 -w 60000 luna://com.webos.appInstallService/dev/install '{\"id\":\"$ID\",\"ipkUrl\":\"/tmp/$ID.ipk\",\"subscribe\":true}' </dev/null | grep -q '\"state\":\"installed\"' && echo installed
rm -f /tmp/$ID.ipk
[ $LAUNCH = 1 ] && luna-send -n 1 luna://com.webos.applicationManager/launch '{\"id\":\"$ID\"}' </dev/null
true"
