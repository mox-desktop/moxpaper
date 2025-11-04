{
  inputs.tooling.url = "github:mox-desktop/tooling";

  outputs =
    { self, tooling, ... }:
    tooling.lib.mkMoxFlake {
      devShells = tooling.lib.forAllSystems (pkgs: {
        default = pkgs.mkShell (
          pkgs.lib.fix (finalAttrs: {
            buildInputs = builtins.attrValues {
              inherit (pkgs)
                rustToolchain
                rust-analyzer-unwrapped
                nixd
                vulkan-loader
                vulkan-headers
                vulkan-validation-layers
                wgsl-analyzer
                wayland
                pkg-config
                egl-wayland
                libGL
                openssl
                garage
                minio-client
                ;
            };
            LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath finalAttrs.buildInputs;
            RUST_SRC_PATH = "${pkgs.rustToolchain}/lib/rustlib/src/rust/library";
            shellHook = ''
              export MOXPAPER_S3_ENDPOINT="http://localhost:3900"
              export GARAGE_CONFIG_FILE="/tmp/garage-moxpaper/garage.toml"
              export MOXPAPER_S3_REGION="garage"
              export MOXPAPER_S3_ACCESS_KEY_ID_FILE="/tmp/garage-moxpaper/access-key-id"
              export MOXPAPER_S3_SECRET_ACCESS_KEY_FILE="/tmp/garage-moxpaper/secret-access-key"

              if [ ! -f /tmp/garage-moxpaper/.garage-secrets ]; then
                RPC_SECRET=$(openssl rand -hex 32)
                ADMIN_TOKEN=$(openssl rand -base64 32)
                METRICS_TOKEN=$(openssl rand -base64 32)
                echo "RPC_SECRET=$RPC_SECRET" > /tmp/garage-moxpaper/.garage-secrets
                echo "ADMIN_TOKEN=$ADMIN_TOKEN" >> /tmp/garage-moxpaper/.garage-secrets
                echo "METRICS_TOKEN=$METRICS_TOKEN" >> /tmp/garage-moxpaper/.garage-secrets
              else
                source /tmp/garage-moxpaper/.garage-secrets
              fi

              mkdir -p /tmp/garage-moxpaper/{meta,data} && cat > $GARAGE_CONFIG_FILE << EOF
              metadata_dir = "/tmp/meta"
              data_dir = "/tmp/data"
              db_engine = "sqlite"

              replication_factor = 1

              rpc_bind_addr = "[::]:3901"
              rpc_public_addr = "127.0.0.1:3901"
              rpc_secret = "$RPC_SECRET"

              [s3_api]
              s3_region = "garage"
              api_bind_addr = "[::]:3900"
              root_domain = ".s3.garage.localhost"

              [s3_web]
              bind_addr = "[::]:3902"
              root_domain = ".web.garage.localhost"
              index = "index.html"

              [k2v_api]
              api_bind_addr = "[::]:3904"

              [admin]
              api_bind_addr = "[::]:3903"
              admin_token = "$ADMIN_TOKEN"
              metrics_token = "$METRICS_TOKEN"
              EOF
            '';
          })
        );
      });

      packages = tooling.lib.forAllSystems (pkgs: {
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
