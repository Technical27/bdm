{ sources ? import ./sources.nix }:

let
  pkgs = import sources.nixpkgs {
    overlays = [ (import sources.nixpkgs-mozilla) ];
  };
  channel = pkgs.latest.rustChannels.stable.rust.override {
    targets = [ "x86_64-unknown-linux-gnu" ];
    extensions = [ "rust-src" ];
  };
in channel
