{
  description = "Dev shell for pebble-bounce (Cloudflare Worker)";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = {
    self,
    nixpkgs,
  }: let
    systems = ["x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin"];
    forAllSystems = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});
  in {
    devShells = forAllSystems (pkgs: {
      default = pkgs.mkShell {
        packages = with pkgs; [
          wrangler
          worker-build
          nodejs
          openssl
          pkg-config
        ];

        # Where wrangler/workerd and anything they spawn look for libssl at runtime.
        LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath [pkgs.openssl];

        # Where build tooling (openssl-sys, worker-build, node-gyp, ...) looks for
        # the headers and libs, since there is no /usr/include on NixOS.
        OPENSSL_DIR = "${pkgs.openssl.dev}";
        OPENSSL_LIB_DIR = "${pkgs.lib.getLib pkgs.openssl}/lib";
        OPENSSL_INCLUDE_DIR = "${pkgs.openssl.dev}/include";
        OPENSSL_NO_VENDOR = "1";

        shellHook = ''
          export PKG_CONFIG_PATH="${pkgs.openssl.dev}/lib/pkgconfig''${PKG_CONFIG_PATH:+:$PKG_CONFIG_PATH}"
          # Keep wrangler's state inside the repo instead of $HOME.
          export WRANGLER_HOME="$PWD/.wrangler"
        '';
      };
    });
  };
}
