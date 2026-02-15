# Build the FeOS initramfs (initial root filesystem).
#
# Unlike a NixOS initrd (which is a boot stage that pivots to a real rootfs),
# the FeOS initramfs IS the complete root filesystem. FeOS runs entirely from
# this tmpfs-based rootfs with no disk backing.
#
# Contents:
#   /init              -> symlink to /bin/feos
#   /bin/feos          - FeOS init binary (static musl)
#   /bin/cloud-hypervisor - VM hypervisor
#   /bin/youki         - OCI container runtime
#   /usr/share/cloud-hypervisor/hypervisor-fw - hypervisor firmware
#   /usr/share/feos/vmlinuz - kernel for nested VMs
#   /etc/{hosts,hostname,resolv.conf} - basic network config
#   /proc, /dev, /sys, /tmp, /run, /var/lib/feos - required mount points

{
  lib,
  runCommand,
  feos,
  cloud-hypervisor,
  youki,
  hypervisor-firmware,
  kernel ? null,
  cpio,
  zstd,
  cacert,
}:

runCommand "feos-initramfs"
  {
    nativeBuildInputs = [
      cpio
      zstd
    ];

    passthru = {
      inherit feos cloud-hypervisor youki;
    };
  }
  ''
    # Create the rootfs directory tree
    rootfs=$TMPDIR/rootfs
    mkdir -p $rootfs

    # Directory structure matching FeOS expectations
    mkdir -p $rootfs/{bin,etc,var,lib,run,tmp}
    mkdir -p $rootfs/{proc,dev,sys}
    mkdir -p $rootfs/var/lib/feos
    mkdir -p $rootfs/usr/{bin,lib,sbin,local}
    mkdir -p $rootfs/usr/share/cloud-hypervisor
    mkdir -p $rootfs/usr/share/youki
    mkdir -p $rootfs/usr/share/feos
    mkdir -p $rootfs/usr/local/ssl/certs
    mkdir -p $rootfs/etc/feos

    # Install FeOS binary (statically linked, no library deps)
    cp ${feos}/bin/feos $rootfs/bin/feos
    chmod 755 $rootfs/bin/feos

    # Create /init symlink (kernel starts /init by default)
    ln -s bin/feos $rootfs/init

    # Install cloud-hypervisor
    cp ${cloud-hypervisor}/bin/cloud-hypervisor $rootfs/bin/cloud-hypervisor
    chmod 755 $rootfs/bin/cloud-hypervisor

    # Install hypervisor firmware
    cp ${hypervisor-firmware} $rootfs/usr/share/cloud-hypervisor/hypervisor-fw

    # Install youki container runtime
    cp ${youki}/bin/youki $rootfs/bin/youki
    chmod 755 $rootfs/bin/youki

    ${lib.optionalString (kernel != null) ''
      # Install kernel for nested VMs
      cp ${kernel}/bzImage $rootfs/usr/share/feos/vmlinuz
    ''}

    # SSL certificates (needed for OCI registry pulls)
    cp -rL ${cacert}/etc/ssl/certs/* $rootfs/usr/local/ssl/certs/ || true

    # Basic network configuration
    printf '%s\n' "127.0.0.1    localhost" "127.0.1.1    feos" "::1          localhost feos" > $rootfs/etc/hosts
    echo "feos" > $rootfs/etc/hostname
    echo "nameserver 2001:4860:4860::6464" > $rootfs/etc/resolv.conf

    # Create the initramfs cpio archive compressed with zstd
    mkdir -p $out

    # Uncompressed version (used for nested VM initramfs inside the image)
    (cd $rootfs && find . -print0 | sort -z | cpio --quiet -o -H newc -R +0:+0 --reproducible --null > $out/initramfs)

    # Compressed version (used for booting)
    zstd -3 < $out/initramfs > $out/initramfs.zst

    # Also provide just the rootfs tree for inspection
    cp -a $rootfs $out/rootfs
  ''
