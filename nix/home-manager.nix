{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.services.moxpaper;
in
{
  options.services.moxpaper = {
    enable = lib.mkEnableOption "moxpaper";
    package = lib.mkPackageOption pkgs "moxpaper" { };
  };

  config = lib.mkIf cfg.enable {
    home.packages = [ cfg.package ];
    xdg.configFile."systemd/user/graphical-session.target.wants/moxpaper.service".source =
      "${cfg.package}/share/systemd/user/moxpaper.service";
  };
}
