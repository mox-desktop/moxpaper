{
  self,
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.services.moxpaper;
  inherit (lib) types;
in
{
  imports = [ ./stylix.nix ];

  options.services.moxpaper = {
    enable = lib.mkEnableOption "moxpaper";
    package = lib.mkOption {
      type = types.package;
      default = self.packages.${pkgs.hostPlatform.system}.moxpaper;
    };
    settings = lib.mkOption {
      type = lib.types.attrs;
      default = { };
      description = "Configuration for moxpaper";
    };
  };

  config = lib.mkIf cfg.enable {
    xdg.configFile = {
      "mox/moxpaper.nix" = lib.mkIf (cfg.settings != { }) {
        text = lib.generators.toPretty { } cfg.settings;
      };
    };

    systemd.user.services.moxpaper = {
      Install = {
        WantedBy = [ config.wayland.systemd.target ];
      };
      Unit = {
        Description = "Wallpaper daemon with fully customizable animations";
        PartOf = [ config.wayland.systemd.target ];
        After = [ config.wayland.systemd.target ];
        ConditionEnvironment = "WAYLAND_DISPLAY";
      };
      Service = {
        ExecStart = "${lib.getExe cfg.package}";
        Restart = "always";
        RestartSec = "10";
      };
    };

    home.packages = [ cfg.package ];
  };
}
