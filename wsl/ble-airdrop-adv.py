#!/usr/bin/env python3
"""Stand-in for omdrop-awdl's ble-airdrop-adv.py inside WSL.

WSL has no Bluetooth, so instead of registering the advert with BlueZ this
builds the same Continuity value and hands it to the Windows app
(`airdrop.exe ble-adv HEX SECONDS`), which broadcasts it through Windows'
Bluetooth stack (WSL interop runs Windows programs from here). Takes the
original's arguments; send-to-peer only uses --arm, --seconds and
--interval-ms, and reads the START/FAILED lines.

/etc/omdrop/windows-ble holds the app's path, as a WSL path.
"""
import argparse
import os
import sys
import time

CONF = '/etc/omdrop/windows-ble'


def payload(arm, tlv_type, hashes, tag):
    # Same layout as omdrop-awdl's ble-airdrop-adv.py.
    h, t = bytes.fromhex(hashes), bytes.fromhex(tag)
    if arm == 'A':
        body = bytes([0x40]) + t + bytes(4) + bytes([0x03]) + h + bytes(1)
    elif arm == 'V2':
        body = bytes(8) + bytes([0x02]) + h + bytes(1)
    else:
        body = bytes(8) + bytes([0x01]) + h + bytes(1)
    return bytes([tlv_type, len(body)]) + body


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--arm', choices=['A', 'V2', 'B'], default='A')
    ap.add_argument('--tag', default=None)
    ap.add_argument('--type', default='0x05')
    ap.add_argument('--hashes', default=None)
    ap.add_argument('--hashes-from-record', nargs='?', const='', default=None)
    ap.add_argument('--seconds', type=int, default=120)
    ap.add_argument('--interval-ms', type=int, default=100)
    ap.add_argument('--broadcast', action='store_true')
    ap.add_argument('--adapter', default='hci0')
    a = ap.parse_args()
    # A self-signed identity has no Apple validation record, so the contact
    # hashes are zeros: an Everyone-mode receiver accepts those.
    value = payload(a.arm, int(a.type, 0), a.hashes or '00' * 8, a.tag or os.urandom(3).hex())
    try:
        with open(CONF) as f:
            app = f.readline().strip()
        if not os.path.exists(app):
            raise OSError(app)
    except OSError:
        print(f'{time.time():.3f} REGISTER FAILED no Windows app configured in {CONF}', flush=True)
        return 3
    print(f'{time.time():.3f} contact hashes: zeros', flush=True)
    os.execv(app, [app, 'ble-adv', value.hex(), str(a.seconds)])


if __name__ == '__main__':
    sys.exit(main())
