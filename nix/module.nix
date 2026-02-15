# NixOS module for FeOS init system.
#
# This module allows using FeOS as the init system in a NixOS-based image.
# It replaces systemd/other init with FeOS as PID 1 and configures the
# necessary kernel parameters, modules, and initramfs contents.
#
# Usage in a flake:
#
#   {
#     inputs.feos.url = "github:ironcore-dev/FeOS";
#
#     outputs = { nixpkgs, feos, ... }: {
#       nixosConfigurations.myHost = nixpkgs.lib.nixosSystem {
#         system = "x86_64-linux";
#         modules = [
#           feos.nixosModules.feos
#           {
#             services.feos.enable = true;
#           }
#         ];
#       };
#     };
#   }
#
# For standalone image building (without a full NixOS system), use the
# flake's packages directly:
#   nix build .#feos-initramfs   # initramfs with FeOS
#   nix build .#feos-uki         # Unified Kernel Image
#   nix build .#feos-kernel      # custom kernel

flakeSelf:

{
  config,
  lib,
  pkgs,
  ...
}:

let
  cfg = config.services.feos;

  feosPackages = flakeSelf.packages.${pkgs.system};

in
{
  options.services.feos = {
    enable = lib.mkEnableOption "FeOS init system for hypervisors and container hosts";

    package = lib.mkOption {
      type = lib.types.package;
      default = feosPackages.feos;
      defaultText = lib.literalExpression "feos.packages.\${system}.feos";
      description = "The FeOS binary package.";
    };

    kernel = {
      useCustom = lib.mkOption {
        type = lib.types.bool;
        default = false;
        description = ''
          Whether to use the custom FeOS kernel (Linux {version} with
          FeOS-specific config) instead of the NixOS default kernel.

          The custom kernel includes optimized settings for SR-IOV,
          hugepages, VFIO, and other hypervisor features.
        '';
      };

      package = lib.mkOption {
        type = lib.types.package;
        default = feosPackages.feos-kernel;
        defaultText = lib.literalExpression "feos.packages.\${system}.feos-kernel";
        description = "Custom kernel package to use when `useCustom` is true.";
      };
    };

    cloudHypervisor = {
      enable = lib.mkOption {
        type = lib.types.bool;
        default = true;
        description = "Whether to include cloud-hypervisor in the system.";
      };

      package = lib.mkOption {
        type = lib.types.package;
        default = pkgs.cloud-hypervisor;
        defaultText = lib.literalExpression "pkgs.cloud-hypervisor";
        description = "The cloud-hypervisor package.";
      };
    };

    youki = {
      enable = lib.mkOption {
        type = lib.types.bool;
        default = true;
        description = "Whether to include the youki OCI container runtime.";
      };

      package = lib.mkOption {
        type = lib.types.package;
        default = pkgs.youki;
        defaultText = lib.literalExpression "pkgs.youki";
        description = "The youki package.";
      };
    };

    firmware = {
      package = lib.mkOption {
        type = lib.types.path;
        default = pkgs.fetchurl {
          url = "https://github.com/cloud-hypervisor/rust-hypervisor-firmware/releases/download/0.4.2/hypervisor-fw";
          hash = "sha256-WMFGE7xmBnI/GBJNAPujRk+vMx1ssGp//lbeYtgHEkA=";
        };
        defaultText = lib.literalExpression "fetchurl { ... }";
        description = "The hypervisor firmware binary for cloud-hypervisor.";
      };
    };

    grpcPort = lib.mkOption {
      type = lib.types.port;
      default = 1337;
      description = "TCP port for the FeOS gRPC API.";
    };

    extraKernelParams = lib.mkOption {
      type = lib.types.listOf lib.types.str;
      default = [ ];
      description = "Additional kernel command line parameters.";
    };
  };

  config = lib.mkIf cfg.enable {

    # -- Kernel configuration --

    boot.kernelPackages = lib.mkIf cfg.kernel.useCustom (pkgs.linuxPackagesFor cfg.kernel.package);

    boot.kernelParams = [
      "init=${cfg.package}/bin/feos"
      "console=tty0"
    ]
    ++ cfg.extraKernelParams;

    # Kernel modules required by FeOS
    boot.kernelModules = [
      "kvm"
      "kvm_intel"
      "kvm_amd"
      "vfio"
      "vfio_pci"
      "vfio_iommu_type1"
      "vhost_net"
      "tun"
      "bridge"
    ];

    boot.kernel.sysctl = {
      # IPv6 forwarding (FeOS enables this at boot)
      "net.ipv6.conf.all.forwarding" = 1;
      # Hugepages (FeOS configures 1024 x 2MB pages)
      "vm.nr_hugepages" = lib.mkDefault 1024;
    };

    # -- Initramfs contents --
    # Add FeOS and its runtime dependencies to the initramfs
    boot.initrd.availableKernelModules = [
      "virtio_pci"
      "virtio_blk"
      "virtio_net"
      "virtio_console"
      "virtio_rng"
    ];

    # -- System packages --
    # Make FeOS tools available in the system PATH
    environment.systemPackages = [
      cfg.package
    ]
    ++ lib.optional cfg.cloudHypervisor.enable cfg.cloudHypervisor.package
    ++ lib.optional cfg.youki.enable cfg.youki.package;

    # Install hypervisor firmware where FeOS expects it
    environment.etc = lib.mkIf cfg.cloudHypervisor.enable {
      "feos/hypervisor-fw" = {
        source = "${cfg.firmware.package}";
      };
    };

    # Symlink firmware to the path cloud-hypervisor expects
    system.activationScripts.feos-firmware = lib.mkIf cfg.cloudHypervisor.enable ''
      mkdir -p /usr/share/cloud-hypervisor
      ln -sfn ${cfg.firmware.package} /usr/share/cloud-hypervisor/hypervisor-fw
    '';

    # -- Required directories --
    systemd.tmpfiles.rules = [
      "d /var/lib/feos 0755 root root -"
      "d /tmp/feos 0755 root root -"
      "d /tmp/feos/vm_api_sockets 0755 root root -"
      "d /tmp/feos/consoles 0755 root root -"
    ];

    # -- Firewall --
    networking.firewall.allowedTCPPorts = lib.mkIf config.networking.firewall.enable [
      cfg.grpcPort
    ];

    # -- Assertions --
    assertions = [
      {
        assertion = pkgs.system == "x86_64-linux";
        message = "FeOS only supports x86_64-linux.";
      }
    ];

    warnings =
      lib.optional (
        !cfg.cloudHypervisor.enable
      ) "FeOS: cloud-hypervisor is disabled. VM management will not work."
      ++ lib.optional (!cfg.youki.enable) "FeOS: youki is disabled. Container management will not work.";
  };
}
