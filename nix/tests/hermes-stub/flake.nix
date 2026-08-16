{
  description = "Hermes package stub for ArcOS Nix evaluation";

  inputs.nixpkgs.url = "nixpkgs";

  outputs =
    { nixpkgs, ... }:
    let
      system = "x86_64-linux";
      pkgs = nixpkgs.legacyPackages.${system};
    in
    {
      packages.${system}.default = pkgs.writeShellScriptBin "hermes" ''
        echo "Hermes CI evaluation stub: do not use at runtime" >&2
        exit 1
      '';
    };
}
