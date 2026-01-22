{
  config,
  pkgs,
  lib,
  ...
}:
let
  cfg = config.services.nix-cache-overlay;
in
{
  options.services.nix-cache-overlay = {
    enable = lib.mkEnableOption "nix-cache-overlay";
    package = lib.mkPackageOption pkgs "nix-cache-overlay" { };
    listen = lib.mkOption {
      type = lib.types.str;
      default = "[::]:8080";
      description = ''
        Socket address to listen on.
      '';
    };
    log = lib.mkOption {
      type = lib.types.str;
      default = "nix_cache_overlay=info";
      description = ''
        Log configuration in RUST_LOG format.
      '';
    };
    endpoint = lib.mkOption {
      type = lib.types.str;
      description = ''
        The endpoint of the underlying cache storage.
      '';
    };
    extraArgs = lib.mkOption {
      type = with lib.types; listOf str;
      default = [ ];
      description = ''
        Extra command-line arguments pass to nix-cache-overlay.
      '';
    };
    environmentFile = lib.mkOption {
      type = lib.types.path;
      default = null;
      description = ''
        Path to an environment file to provide token and AWS credentials.
      '';
    };
  };
  config = lib.mkIf cfg.enable {
    systemd.services.nix-cache-overlay = {
      script = ''
        ${cfg.package}/bin/nix-cache-overlay \
          --listen "${cfg.listen}" \
          --endpoint "${cfg.endpoint}" \
          --logging-method=journald \
          ${lib.escapeShellArgs cfg.extraArgs}
      '';
      serviceConfig = {
        DynamicUser = true;
        EnvironmentFile = if cfg.environmentFile != null then [ cfg.environmentFile ] else [ ];
      };
      environment.RUST_LOG = cfg.log;
      wantedBy = [ "multi-user.target" ];
    };
  };
}
