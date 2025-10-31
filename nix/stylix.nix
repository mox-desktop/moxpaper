{ config, lib, ... }:
let
  cfg = config.stylix.targets.moxpaper;
in
{
  options.stylix.targets.moxpaper.enable = config.lib.stylix.mkEnableTarget "Moxpaper" (
    config.stylix.image != null
  );

  config = lib.mkIf (config.stylix.enable && cfg.enable) {
    services.moxpaper.settings = {
      wallpaper.any.path = "${config.stylix.image}";
    };
  };
}
