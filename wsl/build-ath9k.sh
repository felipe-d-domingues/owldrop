#!/bin/bash
# Builds the Atheros AR9271 driver (ath9k_htc and its helpers) for the running
# WSL2 kernel, whose stock config leaves Atheros out. Run as root inside WSL:
#
#   build-ath9k.sh REPO_WSL_PATH
#
# A WSL update can bring a new kernel (6.18.40.1 -> 6.18.54.1 on 2026-10-13),
# and then this has to run again; airdrop-wsl exits 7 when it's needed and the
# app offers to run it. Takes ~10-15 minutes and ~8 GB of disk while it runs.
#
# Why each step:
#  - CONFIG_MODVERSIONS: a module must carry the kernel's symbol CRCs, so the
#    whole kernel is built from the matching microsoft/WSL2-Linux-Kernel tag
#    with /proc/config.gz plus the Atheros options. Debug info is off to save
#    disk.
#  - Debug info off also drops CONFIG_DEBUG_INFO_BTF_MODULES, which removes
#    four fields from struct module ("section size must match"), so our
#    modules are built with that define put back (-D...), out of tree from a
#    copy: this kbuild overwrites the module dir's Makefile with its own
#    wrapper, so the driver's Makefile goes in as Kbuild, which wins.
#  - The define also changes the CRC of every symbol whose type reaches struct
#    module; fix-modversions.py takes the right ones from Microsoft's shipped
#    modules, or computes them from the tree.
set -euo pipefail

REPO=${1:?usage: build-ath9k.sh REPO_WSL_PATH}
WORK=${WORK:-/opt/airdrop}
REL=$(uname -r)                       # 6.18.54.1-microsoft-standard-WSL2
VER=${REL%%-*}                        # 6.18.54.1
SRC=$WORK/WSL2-Linux-Kernel-linux-msft-wsl-$VER
EXT=$WORK/ath-ext-$VER
OUT=$WORK/modules/$REL
step() { echo "STEP $*"; }

mkdir -p "$WORK" "$OUT"
cd "$WORK"
# Trees for kernels that are no longer running only take disk.
for old in WSL2-Linux-Kernel-linux-msft-wsl-* ath-ext*; do
  [ -e "$old" ] && [ "$old" != "$(basename "$SRC")" ] && [ "$old" != "$(basename "$EXT")" ] && rm -rf "$old"
done
rm -f kernel.tgz

if [ ! -f "$SRC/.built" ]; then
  if [ ! -d "$SRC" ]; then
    step "Downloading the WSL $VER kernel source"
    curl -sSfL "https://github.com/microsoft/WSL2-Linux-Kernel/archive/refs/tags/linux-msft-wsl-$VER.tar.gz" | tar xz
  fi
  cd "$SRC"
  zcat /proc/config.gz > .config
  scripts/config --enable WLAN --enable WLAN_VENDOR_ATH \
    --module ATH_COMMON --module ATH9K_HW --module ATH9K_COMMON --module ATH9K_HTC \
    --disable ATH9K --disable ATH5K --disable ATH6KL --disable ATH10K --disable ATH11K --disable ATH12K \
    --disable CARL9170 --disable WIL6210 --disable AR5523 --disable ATH9K_HTC_DEBUGFS \
    --disable DEBUG_INFO --disable DEBUG_INFO_BTF --disable DEBUG_INFO_BTF_MODULES \
    --disable DEBUG_INFO_DWARF4 --disable DEBUG_INFO_DWARF5 --disable DEBUG_INFO_DWARF_TOOLCHAIN_DEFAULT \
    --enable DEBUG_INFO_NONE
  make olddefconfig >/dev/null
  [ "$(make -s kernelrelease)" = "$REL" ] || { echo "kernelrelease $(make -s kernelrelease) != $REL"; exit 1; }
  step "Building the kernel (about 10 minutes)"
  make -s -j"$(nproc)" modules 2>&1 | grep -E "error|Error" || true
  touch .built
fi

step "Building the AR9271 driver"
cd "$SRC"
rm -rf "$EXT"
cp -r drivers/net/wireless/ath "$EXT"
cp drivers/net/wireless/ath/Makefile "$EXT/Kbuild"
make -s -j"$(nproc)" M="$EXT" KCFLAGS=-DCONFIG_DEBUG_INFO_BTF_MODULES=1 modules 2>&1 | grep -vE "overriding|ignoring old" || true
cp "$EXT/ath.ko" "$EXT"/ath9k/ath9k_{hw,common,htc}.ko "$OUT/"

step "Matching symbol versions"
python3 "$REPO/wsl/fix-modversions.py" --tree "$SRC" "$OUT"/{ath,ath9k_hw,ath9k_common,ath9k_htc}.ko

# The dongle's firmware. The kernel can't read it from here (see airdrop-wsl),
# but airdrop-wsl feeds it from this path.
mkdir -p /lib/firmware/ath9k_htc
[ -f /lib/firmware/ath9k_htc/htc_9271-1.4.0.fw ] || curl -sSfL -o /lib/firmware/ath9k_htc/htc_9271-1.4.0.fw \
  https://git.kernel.org/pub/scm/linux/kernel/git/firmware/linux-firmware.git/plain/ath9k_htc/htc_9271-1.4.0.fw

modinfo "$OUT/ath9k_htc.ko" | grep -E '^vermagic'
step "Done"
echo "modules in $OUT"
