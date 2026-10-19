#!/bin/bash
# Diagnostic: does our BLE wake (Windows Bluetooth via airdrop-ble.exe) make
# nearby Apple devices open AirDrop's port? Checks every AWDL peer's TCP 8770
# for 15 s without the wake, then 30 s with it. Run as root in WSL, AirDrop on.
ll() { python3 -c 'import sys;m=[int(x,16) for x in sys.argv[1].split(":")];m[0]^=2;print("fe80::%02x%02x:%02xff:fe%02x:%02x%02x"%tuple(m))' "$1"; }
probe() {  # LABEL SECONDS
  local end=$((SECONDS + $2)) mac open=""
  while [ $SECONDS -lt $end ]; do
    for mac in $(/usr/lib/omdrop/omdrop-discoverable peers 2>/dev/null | cut -d' ' -f1); do
      if timeout 2 bash -c "exec 3<>/dev/tcp/$(ll "$mac")%awdl0/8770" 2>/dev/null; then
        case " $open " in *" $mac "*) ;; *) open="$open $mac"; echo "$1: $mac opened 8770 after $(( $2 - (end - SECONDS) ))s" ;; esac
      fi
    done
    sleep 1
  done
  [ -n "$open" ] || echo "$1: no device opened 8770"
}
probe "no wake" 15
python3 /usr/lib/omdrop/ble-airdrop-adv.py --arm A --seconds 30 &
probe "with wake" 30
wait
