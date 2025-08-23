{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };
  outputs =
    {
      self,
      nixpkgs,
      rust-overlay,
      ...
    }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
      ];
      overlays = [ (import rust-overlay) ];
      forAllSystems =
        function:
        nixpkgs.lib.genAttrs systems (
          system:
          let
            pkgs = import nixpkgs { inherit system overlays; };
          in
          function pkgs
        );
    in
    {
      devShells = forAllSystems (pkgs: {
        default =
          let
            buildInputs = [
              (pkgs.rust-bin.stable.latest.default.override {
                extensions = [
                  "rust-src"
                  "rustfmt"
                ];
              })
            ]
            ++ builtins.attrValues {
              inherit (pkgs)
                rust-analyzer-unwrapped
                nixd
                vulkan-loader
                vulkan-headers
                vulkan-validation-layers
                wgsl-analyzer
                wayland
                pkg-config
                lua5_4
                egl-wayland
                libGL
                ;
            };
          in
          pkgs.mkShell {
            inherit buildInputs;
            LD_LIBRARY_PATH = "${pkgs.lib.makeLibraryPath buildInputs}";
          };
      });

      packages = forAllSystems (pkgs: {
        moxpaper = pkgs.callPackage ./nix/package.nix {
          rustPlatform =
            let
              rust-bin = pkgs.rust-bin.stable.latest.default;
            in
            pkgs.makeRustPlatform {
              cargo = rust-bin;
              rustc = rust-bin;
            };
        };
        default = self.packages.${pkgs.system}.moxpaper;
      });

      formatter = forAllSystems (
        pkgs:
        pkgs.writeShellApplication {
          name = "nix3-fmt-wrapper";

          runtimeInputs = [
            pkgs.nixfmt-rfc-style
            pkgs.taplo
            pkgs.fd
            (pkgs.rust-bin.selectLatestNightlyWith (
              toolchain:
              toolchain.default.override {
                extensions = [ "rustfmt" ];
              }
            ))
          ];

          text = ''
            fd "$@" -t f -e nix -x nixfmt -q '{}'
            fd "$@" -t f -e toml -x taplo format '{}'
            cargo fmt
          '';
        }
      );

      homeManagerModules = {
        moxpaper = import ./nix/home-manager.nix;
        default = self.homeManagerModules.moxpaper;
      };
    };
}
