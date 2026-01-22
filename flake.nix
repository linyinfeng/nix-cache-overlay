{
  inputs = {
    flake-parts.url = "github:hercules-ci/flake-parts";
    flake-parts.inputs.nixpkgs-lib.follows = "nixpkgs";

    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";

    treefmt-nix.url = "github:numtide/treefmt-nix";
    treefmt-nix.inputs.nixpkgs.follows = "nixpkgs";

    crane.url = "github:ipetkov/crane";
  };

  outputs =
    inputs@{ flake-parts, ... }:
    flake-parts.lib.mkFlake { inherit inputs; } (
      {
        self,
        inputs,
        lib,
        ...
      }:
      {
        systems = [
          "x86_64-linux"
          "aarch64-linux"
        ];
        imports = [
          inputs.flake-parts.flakeModules.easyOverlay
          inputs.treefmt-nix.flakeModule
        ];
        flake = {
          nixosModules.nix-cache-overlay = ./nixos/module.nix;
        };
        perSystem =
          {
            config,
            self',
            pkgs,
            system,
            ...
          }:
          let
            craneLib = inputs.crane.mkLib pkgs;
            src = craneLib.cleanCargoSource (craneLib.path ./.);
            bareCommonArgs = {
              inherit src;
              nativeBuildInputs = with pkgs; [
                pkg-config
                installShellFiles
              ];
              buildInputs = with pkgs; [
                openssl
                sqlite
              ];
            };
            cargoArtifacts = craneLib.buildDepsOnly bareCommonArgs;
            commonArgs = bareCommonArgs // {
              inherit cargoArtifacts;
            };
          in
          {
            packages = {
              nix-cache-overlay = craneLib.buildPackage commonArgs;
              default = config.packages.nix-cache-overlay;
            };
            overlayAttrs.nix-cache-overlay = config.packages.nix-cache-overlay;
            checks = {
              inherit (self'.packages) nix-cache-overlay;
              # doc = craneLib.cargoDoc commonArgs;
              # fmt = craneLib.cargoFmt { inherit src; };
              nextest = craneLib.cargoNextest (
                commonArgs
                // {
                  cargoNextestExtraArgs = "--no-tests warn";
                }
              );
              clippy = craneLib.cargoClippy (
                commonArgs
                // {
                  cargoClippyExtraArgs = "--all-targets -- --deny warnings";
                }
              );
            };
            treefmt = {
              projectRootFile = "flake.nix";
              programs = {
                nixfmt.enable = true;
                rustfmt.enable = true;
                shfmt.enable = true;
                prettier.enable = true;
              };
            };
            devShells.default = pkgs.mkShell {
              inputsFrom = lib.attrValues self'.checks;
              packages = with pkgs; [
                rustup
                rust-analyzer
              ];
            };
          };
      }
    );
}
