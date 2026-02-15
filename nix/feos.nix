{
  lib,
  craneLib,
  version,
  buildCli ? false,
  protobuf,
  perl,
  pkg-config,
  stdenv,
  git,
  pkgsCross,
}:

let
  # Musl cross-compilation toolchain for static linking
  muslCC = pkgsCross.musl64.stdenv.cc;

  # Source filtering: include Rust sources, proto files, migrations, and .sqlx
  srcFilter =
    path: type:
    let
      baseName = builtins.baseNameOf path;
      relPath = lib.removePrefix (toString ./.. + "/") (toString path);
    in
    # Always include proto definitions (needed by tonic-build)
    (lib.hasPrefix "proto/" relPath)
    ||
      # Include .sqlx offline query cache
      (lib.hasSuffix ".json" baseName && lib.hasInfix ".sqlx" relPath)
    ||
      # Include SQL migrations
      (lib.hasSuffix ".sql" baseName && lib.hasInfix "migrations" relPath)
    ||
      # Include Cargo/Rust source files via crane's default filter
      (craneLib.filterCargoSources path type);

  src = lib.cleanSourceWith {
    src = craneLib.path ./..;
    filter = srcFilter;
  };

  # When targeting musl, we need the musl C compiler for vendored OpenSSL.
  # The env var names use the target triple with hyphens replaced by underscores.
  muslEnv = lib.optionalAttrs (!buildCli) {
    CARGO_BUILD_TARGET = "x86_64-unknown-linux-musl";

    # Tell cargo/cc-rs to use the musl C compiler for the target
    CC_x86_64_unknown_linux_musl = "${muslCC}/bin/${muslCC.targetPrefix}cc";
    AR_x86_64_unknown_linux_musl = "${muslCC}/bin/${muslCC.targetPrefix}ar";
    CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER = "${muslCC}/bin/${muslCC.targetPrefix}cc";

    # OpenSSL vendored build needs to find the musl headers
    # The cc crate will use the CC env var above, which points at musl-gcc
  };

  # Common arguments shared between deps-only and final build
  commonArgs = {
    inherit src version;
    pname = if buildCli then "feos-cli" else "feos";
    strictDeps = true;

    nativeBuildInputs = [
      protobuf # protoc for tonic-build
      perl # for openssl vendored build
      pkg-config
      git # for build.rs git hash
    ]
    ++ lib.optional (!buildCli) muslCC;

    # Environment variables
    PROTOC = "${protobuf}/bin/protoc";
    SQLX_OFFLINE = "true";
  }
  // muslEnv;

  # Build dependencies only (for caching)
  cargoArtifacts = craneLib.buildDepsOnly (
    commonArgs
    // {
      doCheck = false;
    }
  );

in
craneLib.buildPackage (
  commonArgs
  // {
    inherit cargoArtifacts;

    cargoExtraArgs = if buildCli then "--package feos-cli" else "--package feos";

    doCheck = false;

    passthru = {
      inherit commonArgs cargoArtifacts src;
    };

    meta = {
      description =
        if buildCli then
          "CLI client for FeOS init system"
        else
          "Minimal Linux init system for hypervisors and container hosts";
      homepage = "https://github.com/ironcore-dev/FeOS";
      license = lib.licenses.asl20;
      platforms = [ "x86_64-linux" ];
      mainProgram = if buildCli then "feos-cli" else "feos";
    };
  }
)
