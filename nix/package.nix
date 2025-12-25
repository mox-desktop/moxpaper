{
  rustPlatform,
  lib,
  pkg-config,
  wayland,
  vulkan-loader,
  libGL,
  egl-wayland,
  openssl,
  httpSupport ? true,
  s3Support ? true,
}:
let
  cargoToml = builtins.fromTOML (builtins.readFile ../daemon/Cargo.toml);
in
rustPlatform.buildRustPackage {
  pname = "moxpaper";
  inherit (cargoToml.package) version;

  cargoLock = {
    lockFile = ../Cargo.lock;
    outputHashes = {
      "moxui-0.1.0" = "sha256-i2jwHkdoJW99mcb+DqCqhMfigNz4Bc1QmGcThWl2bRM=";
      "tvix-eval-0.1.0" = "sha256-2uNjqycyGa07RYDYfo7i6rk6zgC1pCfaAgoMTEoF6q0=";
    };
  };

  src = lib.cleanSourceWith {
    src = ../.;
    filter =
      path: type:
      let
        relPath = lib.removePrefix (toString ../. + "/") (toString path);
      in
      lib.any (p: lib.hasPrefix p relPath) [
        "daemon"
        "ctl"
        "libmoxpaper"
        "common"
        "Cargo.toml"
        "Cargo.lock"
      ];
  };

  nativeBuildInputs = [ pkg-config ];

  buildInputs = [
    wayland
    egl-wayland
  ]
  ++ lib.optionals (httpSupport || s3Support) [ openssl ];

  cargoFeatures = lib.optionals httpSupport [ "http" ] ++ lib.optionals s3Support [ "s3" ];

  doCheck = false;

  buildPhase = ''
    cargo build --release --workspace
  '';

  installPhase = ''
    install -Dm755 target/release/daemon $out/bin/moxpaperd
    install -Dm755 target/release/ctl $out/bin/moxpaper
    ln -s moxpaper $out/bin/moxpaperctl
  '';

  postFixup = ''
    patchelf \
      --add-needed "${libGL}/lib/libEGL.so.1" \
      --add-needed "${vulkan-loader}/lib/libvulkan.so.1" \
      $out/bin/moxpaperd
  '';

  dontPatchELF = false;

  meta = {
    mainProgram = "moxpaperd";
    description = "Mox desktop environment notification system";
    homepage = "https://github.com/mox-desktop/moxpaper";
    license = lib.licenses.mit;
    maintainers = builtins.attrValues { inherit (lib.maintainers) unixpariah; };
    platforms = lib.platforms.linux;
  };
}
