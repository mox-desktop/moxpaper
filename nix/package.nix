{
  rustPlatform,
  lib,
  lua5_4,
  pkg-config,
  wayland,
  vulkan-loader,
}:
let
  cargoToml = builtins.fromTOML (builtins.readFile ../daemon/Cargo.toml);
in
rustPlatform.buildRustPackage rec {
  pname = "moxpaper";
  inherit (cargoToml.package) version;

  cargoLock = {
    lockFile = ../Cargo.lock;
    outputHashes = {
      "moxui-0.1.0" = "sha256-v/4a0+ljKu8vag9suBxZIi12CKwT7xorYy/Am03xtY0=";
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
        "common"
        "contrib"
        "Cargo.toml"
        "Cargo.lock"
      ];
  };

  nativeBuildInputs = [ pkg-config ];

  buildInputs = [
    wayland
    vulkan-loader
    lua5_4
  ];

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
    mkdir -p $out/share/systemd/user
    substitute $src/contrib/systemd/moxpaper.service.in $out/share/systemd/user/moxpaper.service --replace-fail '@bindir@' "$out/bin"
    chmod 0644 $out/share/systemd/user/moxpaper.service

    patchelf --set-rpath "${lib.makeLibraryPath buildInputs}" $out/bin/moxpaperd
  '';

  dontPatchELF = false;

  meta = with lib; {
    description = "Mox desktop environment notification system";
    homepage = "https://github.com/mox-desktop/moxpaper";
    license = licenses.mit;
    maintainers = [ maintainers.unixpariah ];
    platforms = platforms.linux;
    mainProgram = "moxpaper";
  };
}
