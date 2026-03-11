# QEMU test VM launcher for FeOS.
#
# Uses direct kernel boot (-kernel + -initrd) to launch FeOS in a VM.
# This avoids needing UEFI firmware for quick testing.
#
# Networking uses passt (Plug A Simple Socket Transport) which provides
# full IPv6 support including Router Advertisements and DHCPv6 — required
# by FeOS's network initialization. passt runs unprivileged (no root needed).
#
# Usage:
#   nix run .#vm                    # launch with defaults
#   nix run .#vm -- --memory 4G     # override memory
#   nix run .#vm -- --kvm           # explicitly enable KVM (auto-detected)
#
# The VM exposes:
#   - Serial console on the terminal (interactive)
#   - gRPC API: connect to the guest's IPv6 address on port 1337
#   - All guest ports are accessible via the host's network stack (passt)

{
  lib,
  writeShellApplication,
  qemu,
  passt,
  kernel,
  initramfs,
}:

writeShellApplication {
  name = "feos-vm";

  runtimeInputs = [
    qemu
    passt
  ];

  text = ''
    # Defaults
    MEMORY="''${FEOS_VM_MEMORY:-2G}"
    CPUS="''${FEOS_VM_CPUS:-4}"
    KVM_ARGS=""
    EXTRA_ARGS=()

    # Auto-detect KVM support
    if [ -w /dev/kvm ]; then
      KVM_ARGS="-enable-kvm -cpu host"
      echo "KVM acceleration enabled"
    else
      KVM_ARGS="-cpu max"
      echo "WARNING: KVM not available, using software emulation (slow)"
    fi

    # Parse arguments
    while [[ $# -gt 0 ]]; do
      case "$1" in
        --memory)
          MEMORY="$2"
          shift 2
          ;;
        --cpus)
          CPUS="$2"
          shift 2
          ;;
        --kvm)
          KVM_ARGS="-enable-kvm -cpu host"
          shift
          ;;
        --no-kvm)
          KVM_ARGS="-cpu max"
          shift
          ;;
        --uefi)
          # Boot via UEFI firmware instead of direct kernel boot
          # Requires the UKI to be built separately
          echo "UEFI boot mode not yet supported via this launcher."
          echo "Build the UKI with: nix build .#feos-uki"
          exit 1
          ;;
        *)
          EXTRA_ARGS+=("$1")
          shift
          ;;
      esac
    done

    KERNEL="${kernel}/bzImage"
    INITRD="${initramfs}/initramfs.zst"

    # Create a temporary directory for the passt socket
    PASST_DIR=$(mktemp -d --tmpdir feos-vm.XXXXXX)
    PASST_SOCK="$PASST_DIR/passt.sock"
    PASST_PID="$PASST_DIR/passt.pid"

    cleanup() {
      if [ -f "$PASST_PID" ]; then
        kill "$(cat "$PASST_PID")" 2>/dev/null || true
      fi
      rm -rf "$PASST_DIR"
    }
    trap cleanup EXIT

    # Start passt in the background.
    # passt provides:
    #   - Router Advertisements (NDP) for IPv6 SLAAC
    #   - DHCPv6 server (assigns the host's IPv6 address to the guest)
    #   - Full IPv4/IPv6 connectivity without root
    #   - Port forwarding: all guest ports are reachable from host
    #
    # --tcp-ports and --udp-ports forward specific ports.
    # By default, passt forwards all ports.
    passt \
      --socket "$PASST_SOCK" \
      --pid "$PASST_PID" \
      --foreground &
    PASST_BG_PID=$!

    # Wait for the socket to appear
    for i in $(seq 1 30); do
      if [ -S "$PASST_SOCK" ]; then
        break
      fi
      sleep 0.1
    done

    if [ ! -S "$PASST_SOCK" ]; then
      echo "ERROR: passt socket not created after 3s"
      exit 1
    fi

    echo "Starting FeOS test VM..."
    echo "  Kernel:  $KERNEL"
    echo "  Initrd:  $INITRD"
    echo "  Memory:  $MEMORY"
    echo "  CPUs:    $CPUS"
    echo "  Network: passt (IPv6 with RA + DHCPv6)"
    echo "  gRPC:    connect to guest IPv6 address on port 1337"
    echo ""
    echo "Press Ctrl-A X to exit QEMU"
    echo ""

    # shellcheck disable=SC2086
    qemu-system-x86_64 \
      $KVM_ARGS \
      -m "$MEMORY" \
      -smp "$CPUS" \
      -kernel "$KERNEL" \
      -initrd "$INITRD" \
      -append "console=ttyS0 init=/init" \
      -nographic \
      -serial mon:stdio \
      -device virtio-net-pci,netdev=net0 \
      -netdev stream,id=net0,server=off,addr.type=unix,addr.path="$PASST_SOCK" \
      -device virtio-rng-pci \
      -no-reboot \
      "''${EXTRA_ARGS[@]}"
  '';

  meta = {
    description = "Launch a FeOS test VM with QEMU";
    platforms = [ "x86_64-linux" ];
  };
}
