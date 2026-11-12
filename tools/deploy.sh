#!/bin/sh
# Build, package, install on the TV and (re)launch the picker as a normal app.
#   tools/deploy.sh [tv-ip]     (default 192.168.1.180)
set -eu
ROOT=$(dirname "$(dirname "$(realpath "$0")")")
TV=${1:-${TV:-192.168.1.180}}
ID=com.thoxy.sourcepicker
SSH="ssh -o ConnectTimeout=5 ${SSH_OPTS:-} root@$TV"
"$ROOT/tools/build-webos.sh" >/dev/null
python3 "$ROOT/tools/package.py"
IPK=$(ls "$ROOT"/dist/${ID}_*_arm.ipk | tail -1)
scp -q ${SSH_OPTS:-} "$IPK" root@$TV:/tmp/$ID.ipk
$SSH "luna-send -n 1 luna://com.webos.applicationManager/closeByAppId '{\"id\":\"$ID\"}' >/dev/null 2>&1 </dev/null
luna-send -n 30 -w 60000 luna://com.webos.appInstallService/dev/install '{\"id\":\"$ID\",\"ipkUrl\":\"/tmp/$ID.ipk\",\"subscribe\":true}' </dev/null | grep -q '\"state\":\"installed\"' && echo installed
rm -f /tmp/$ID.ipk
luna-send -n 1 luna://com.webos.applicationManager/launch '{\"id\":\"$ID\"}' </dev/null"
