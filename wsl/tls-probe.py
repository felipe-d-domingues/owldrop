#!/usr/bin/env python3
"""Diagnostic: TLS-connect to each AWDL peer's AirDrop port the way the
sender does (self-signed client identity) and print exactly what fails.
Run as root inside WSL while AirDrop is on:  tls-probe.py [SECONDS]"""
import socket, ssl, subprocess, sys, time

KEYS = '/home/felipe/.omdrop/keys'

def ll(mac):
    m = [int(x, 16) for x in mac.split(':')]
    m[0] ^= 2
    return 'fe80::%02x%02x:%02xff:fe%02x:%02x%02x' % tuple(m)

def probe(addr, version):
    ctx = ssl.SSLContext(ssl.PROTOCOL_TLS_CLIENT)
    ctx.check_hostname = False
    ctx.verify_mode = ssl.CERT_NONE
    ctx.minimum_version = ctx.maximum_version = version
    ctx.load_cert_chain(f'{KEYS}/certificate.self-signed.pem', f'{KEYS}/key.self-signed.pem')
    s = socket.create_connection((addr + '%awdl0', 8770), timeout=6)
    try:
        t = ctx.wrap_socket(s)
        return f'OK {t.version()} {t.cipher()[0]}'
    except ssl.SSLError as e:
        return f'SSLError {e.reason} / {e}'
    finally:
        s.close()

end = time.time() + float(sys.argv[1] if len(sys.argv) > 1 else 40)
seen = {}
while time.time() < end:
    out = subprocess.run(['/usr/lib/omdrop/omdrop-discoverable', 'peers'], capture_output=True, text=True).stdout
    for mac in [l.split()[0] for l in out.splitlines() if l.strip()]:
        for v in (ssl.TLSVersion.TLSv1_2, ssl.TLSVersion.TLSv1_3):
            try:
                r = probe(ll(mac), v)
            except OSError as e:
                r = f'{type(e).__name__} {e}'
            key = (mac, v.name, r)
            if key not in seen:
                seen[key] = 1
                print(mac, v.name, r, flush=True)
    time.sleep(1)
