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
      overlays = [
        (import rust-overlay)
        (self: super: {
          rustToolchain = super.rust-bin.selectLatestNightlyWith (
            toolchain:
            toolchain.default.override {
              extensions = [
                "rustc-codegen-cranelift-preview"
                "rust-src"
                "rustfmt"
              ];
            }
          );
        })
      ];

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
        default = pkgs.mkShell (
          pkgs.lib.fix (finalAttrs: {
            buildInputs = builtins.attrValues {
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
            LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath finalAttrs.buildInputs;
            RUST_SRC_PATH = "${pkgs.rustToolchain}/lib/rustlib/src/rust/library";
          })
        );

      });

      formatter = forAllSystems (
        pkgs:
        pkgs.writeShellApplication {
          name = "nix3-fmt-wrapper";

          runtimeInputs = builtins.attrValues {
            inherit (pkgs)
              rustToolchain
              nixfmt-rfc-style
              taplo
              fd
              ;
          };

          text = ''
            fd "$@" -t f -e nix -x nixfmt -q '{}'
            fd "$@" -t f -e toml -x taplo format '{}'
            cargo fmt
          '';
        }
      );

      packages = forAllSystems (pkgs: {
        moxpaper = pkgs.callPackage ./nix/package.nix {
          rustPlatform = pkgs.makeRustPlatform {
            cargo = pkgs.rustToolchain;
            rustc = pkgs.rustToolchain;
          };
        };
        default = self.packages.${pkgs.stdenv.hostPlatform.system}.moxpaper;
      });

      homeManagerModules = {
        moxpaper = import ./nix/home-manager.nix;
        default = self.homeManagerModules.moxpaper;
      };
    };
}
