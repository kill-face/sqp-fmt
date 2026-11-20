{
  description = "sqp-fmt: SQL formatter for the dialects sqlparser-rs parses";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    # The parser fork. Cargo.toml depends on it by path (../sqp-rs) so the two
    # repos can be developed side by side; the package puts this input there.
    sqp-rs = {
      url = "github:kill-face/sqp-rs/main";
      flake = false;
    };
  };

  outputs = {
    self,
    nixpkgs,
    sqp-rs,
  }: let
    system = "x86_64-linux";
    pkgs = nixpkgs.legacyPackages.${system};
    sqp-fmt = pkgs.rustPlatform.buildRustPackage {
      pname = "sqp-fmt";
      version = "0.1.0-${self.shortRev or self.dirtyShortRev or "dirty"}";
      src = self;
      cargoLock.lockFile = ./Cargo.lock;
      # The path dependency ../sqp-rs, beside the unpacked source.
      postUnpack = ''
        cp -r ${sqp-rs} sqp-rs
        chmod -R u+w sqp-rs
      '';
      # Unit, CLI and golden tests; the corpus runs need files outside the repo.
      doCheck = true;
      meta = {
        description = "SQL formatter that only changes whitespace and keyword case";
        homepage = "https://github.com/kill-face/sqp-fmt";
        license = pkgs.lib.licenses.mit;
        mainProgram = "sqp-fmt";
      };
    };
  in {
    packages.${system} = {
      inherit sqp-fmt;
      default = sqp-fmt;
    };

    devShells.${system}.default = pkgs.mkShell {
      packages = with pkgs; [cargo rustc clippy rustfmt rust-analyzer gcc];
    };
  };
}
