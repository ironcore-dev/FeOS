{
  description = "FeOS - A minimal Linux init system for hypervisors and container hosts";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

    crane.url = "github:ipetkov/crane";

    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    {
      self,
      nixpkgs,
      crane,
      rust-overlay,
      flake-utils,
      ...
    }:
    let
      # FeOS only targets x86_64-linux (musl static binary)
      supportedSystems = [ "x86_64-linux" ];

      # Version metadata
      version = "0.5.0";
      kernelVersion = "6.12.63";
    in
    flake-utils.lib.eachSystem supportedSystems (
      system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };

        # Rust toolchain with musl target for static linking
        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          targets = [ "x86_64-unknown-linux-musl" ];
          extensions = [
            "rust-src"
            "clippy"
            "rustfmt"
          ];
        };

        # Crane lib configured with our custom toolchain
        craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchain;

        # --- Package derivations ---

        feosPackage = pkgs.callPackage ./nix/feos.nix {
          inherit craneLib version;
          inherit (pkgs) pkgsCross;
        };

        feosCliPackage = pkgs.callPackage ./nix/feos.nix {
          inherit craneLib version;
          inherit (pkgs) pkgsCross;
          buildCli = true;
        };

        feosKernel = pkgs.callPackage ./nix/kernel.nix {
          inherit kernelVersion;
          kernelConfig = ./hack/kernel/config/feos-linux-${kernelVersion}.config;
        };

        # Pre-built hypervisor firmware (cross-compiled to x86_64-none,
        # not directly buildable as a regular x86_64-linux package)
        hypervisorFirmware = pkgs.fetchurl {
          url = "https://github.com/cloud-hypervisor/rust-hypervisor-firmware/releases/download/0.4.2/hypervisor-fw";
          hash = "sha256-WMFGE7xmBnI/GBJNAPujRk+vMx1ssGp//lbeYtgHEkA=";
        };

        feosInitramfs = pkgs.callPackage ./nix/initramfs.nix {
          feos = feosPackage;
          kernel = feosKernel;
          cloud-hypervisor = pkgs.cloud-hypervisor;
          youki = pkgs.youki;
          hypervisor-firmware = hypervisorFirmware;
        };

        feosUki = pkgs.callPackage ./nix/uki.nix {
          kernel = feosKernel;
          initramfs = feosInitramfs;
          osRelease = ./hack/uki/os-release.txt;
          cmdline = ./hack/kernel/cmdline.txt;
        };

        feosVm = pkgs.callPackage ./nix/vm.nix {
          kernel = feosKernel;
          initramfs = feosInitramfs;
        };

      in
      {
        packages = {
          default = feosPackage;
          feos = feosPackage;
          feos-cli = feosCliPackage;
          feos-kernel = feosKernel;
          feos-initramfs = feosInitramfs;
          feos-uki = feosUki;

          # Convenience: build everything
          all = pkgs.symlinkJoin {
            name = "feos-all";
            paths = [
              feosPackage
              feosCliPackage
              feosKernel
              feosInitramfs
              feosUki
            ];
          };
        };

        apps = {
          default = flake-utils.lib.mkApp {
            drv = feosVm;
            name = "feos-vm";
          };
          vm = flake-utils.lib.mkApp {
            drv = feosVm;
            name = "feos-vm";
          };
          feos-cli = flake-utils.lib.mkApp {
            drv = feosCliPackage;
            name = "feos-cli";
          };
        };

        # --- Checks (run via `nix flake check`) ---
        checks = {
          # Verify the main packages build
          feos = feosPackage;
          feos-cli = feosCliPackage;
          feos-kernel = feosKernel;
          feos-initramfs = feosInitramfs;
          feos-uki = feosUki;

          # Cargo fmt check
          feos-fmt = craneLib.cargoFmt {
            src = craneLib.path ./.;
          };

          # Cargo clippy
          feos-clippy = craneLib.cargoClippy (
            feosPackage.passthru.commonArgs
            // {
              inherit (feosPackage.passthru) cargoArtifacts src;
              pname = "feos-clippy";
              cargoClippyExtraArgs = "--all-targets -- -D warnings";
              doCheck = false;
            }
          );
        };

        # Formatter (run via `nix fmt`)
        formatter = pkgs.nixfmt;

        devShells.default = pkgs.mkShell {
          inputsFrom = [ ];

          nativeBuildInputs = [
            rustToolchain
            pkgs.protobuf
            pkgs.pkg-config
            pkgs.perl
            pkgs.openssl
            pkgs.sqlite

            # Development tools
            pkgs.cargo-watch
            pkgs.cargo-edit

            # VM / testing
            pkgs.qemu
            pkgs.passt

            # gRPC testing
            pkgs.grpcurl

            # Nix tools
            pkgs.nixpkgs-fmt
          ];

          # For openssl-sys vendored build
          OPENSSL_NO_VENDOR = "0";
          PROTOC = "${pkgs.protobuf}/bin/protoc";

          shellHook = ''
            echo "FeOS development shell"
            echo "  cargo build --target=x86_64-unknown-linux-musl --all"
            echo "  nix build .#feos        -- build FeOS binary"
            echo "  nix build .#feos-kernel  -- build custom kernel"
            echo "  nix build .#feos-initramfs -- build initramfs"
            echo "  nix build .#feos-uki     -- build Unified Kernel Image"
            echo "  nix run .#vm             -- launch test VM"
          '';
        };
      }
    )
    // {
      # System-independent outputs

      nixosModules = {
        default = self.nixosModules.feos;
        feos = import ./nix/module.nix self;
      };

      # Overlay for use in other flakes
      overlays.default = final: prev: {
        feos = self.packages.${final.system}.feos;
        feos-cli = self.packages.${final.system}.feos-cli;
        feos-kernel = self.packages.${final.system}.feos-kernel;
        feos-initramfs = self.packages.${final.system}.feos-initramfs;
        feos-uki = self.packages.${final.system}.feos-uki;
      };
    };
}
