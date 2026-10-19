#!/usr/bin/env python3
"""Rewrite the symbol CRCs in our modules' __versions with the running kernel's.

build-ath9k.sh builds without debug info to save disk, which also drops
CONFIG_DEBUG_INFO_BTF_MODULES. That changes struct module, and with it the
genksyms CRC of every symbol whose type reaches struct module (module_layout,
usb_register_driver, ...), so the kernel refuses ours: "disagrees about version
of symbol". The modules Microsoft ships import the same symbols and carry the
right CRCs, so we take them from there. Symbols only our own modules export
(ath_*, ath9k_*) are consistent among themselves and are left alone.

  fix-modversions.py MODULE.ko...
"""
import os
import platform
import struct
import subprocess
import sys

ENTRY = 64          # struct modversion_info: unsigned long crc; char name[56]
NAME = ENTRY - 8


def section(data, name):
    """(offset, size) of an ELF64 section by name."""
    shoff, = struct.unpack_from('<Q', data, 0x28)
    shentsize, shnum, shstrndx = struct.unpack_from('<HHH', data, 0x3A)
    def sh(i):
        return struct.unpack_from('<IIQQQQIIQQ', data, shoff + i * shentsize)
    stroff = sh(shstrndx)[4]
    for i in range(shnum):
        s = sh(i)
        end = data.index(b'\0', stroff + s[0])
        if data[stroff + s[0]:end].decode() == name:
            return s[4], s[5]
    raise SystemExit(f'no {name} section')


def known_crcs():
    crcs = {}
    root = f'/lib/modules/{platform.release()}/kernel'
    for d, _, files in os.walk(root):
        for f in files:
            if '.ko' not in f:
                continue
            out = subprocess.run(['modprobe', '--dump-modversions', os.path.join(d, f)],
                                 capture_output=True, text=True).stdout
            for line in out.splitlines():
                crc, _, sym = line.partition('\t')
                if sym:
                    crcs.setdefault(sym, int(crc, 16))
    return crcs


def tree_crc(tree, sym):
    """A CRC no shipped module records: recompile the object that exports it
    with struct module's BTF fields defined (which reproduces the running
    kernel's CRCs, checked against usb_register_driver), read the CRC kbuild
    keeps in the object's .cmd file, then rebuild the object as it was."""
    found = subprocess.run(['grep', '-rlE', '--include=.*.o.cmd', f'^#SYMVER {sym} ', '.'],
                           cwd=tree, capture_output=True, text=True).stdout.split()
    if not found:
        return None
    cmd = found[0]
    obj = os.path.join(os.path.dirname(cmd), os.path.basename(cmd)[1:-len('.cmd')])
    def build(*extra):
        os.remove(os.path.join(tree, obj))
        subprocess.run(['make', '-s', *extra, obj], cwd=tree, capture_output=True, check=True)
    build('KCFLAGS=-DCONFIG_DEBUG_INFO_BTF_MODULES=1')
    with open(os.path.join(tree, cmd)) as f:
        crc = next(int(l.split()[2], 16) for l in f if l.startswith(f'#SYMVER {sym} '))
    build()
    return crc


def main(args):
    tree = None
    if args[:1] == ['--tree']:
        tree, args = args[1], args[2:]
    paths = args
    crcs = known_crcs()
    ours = set()
    for p in paths:  # what our own modules export: the names of their __ksymtab entries
        out = subprocess.run(['nm', p], capture_output=True, text=True).stdout
        ours |= {l.split()[-1][len('__ksymtab_'):] for l in out.splitlines() if ' __ksymtab_' in l}
    missing = set()
    for p in paths:
        data = open(p, 'rb').read()
        off, size = section(data, '__versions')
        for e in range(off, off + size, ENTRY):
            sym = data[e + 8:e + ENTRY].split(b'\0', 1)[0].decode()
            if sym not in crcs and sym not in ours:
                missing.add(sym)
    if tree:
        for sym in sorted(missing):
            crc = tree_crc(tree, sym)
            if crc is not None:
                crcs[sym] = crc
                print(f'{sym}: 0x{crc:08x} (computed from the source tree)')
    for p in paths:
        data = bytearray(open(p, 'rb').read())
        off, size = section(data, '__versions')
        changed, unknown = 0, []
        for e in range(off, off + size, ENTRY):
            crc, = struct.unpack_from('<Q', data, e)
            sym = data[e + 8:e + ENTRY].split(b'\0', 1)[0].decode()
            if sym in crcs:
                if crcs[sym] != crc:
                    struct.pack_into('<Q', data, e, crcs[sym])
                    changed += 1
            elif sym not in ours:
                unknown.append(sym)
        open(p, 'wb').write(data)
        print(f'{os.path.basename(p)}: {size // ENTRY} imports, {changed} CRCs corrected'
              + (f', not found in shipped modules: {" ".join(unknown)}' if unknown else ''))


if __name__ == '__main__':
    main(sys.argv[1:])
