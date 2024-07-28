{ pkgs ? import <nixpkgs> {} }:
  pkgs.mkShell {
    # nativeBuildInputs is usually what you want -- tools you need to run
    nativeBuildInputs = with pkgs.buildPackages; [
      ccache
      cmake
      dfu-util
      ninja
      openssl
      libiconv
    ]
    ++ lib.optionals stdenv.isDarwin [pkgs.libiconv pkgs.darwin.apple_sdk.frameworks.Security pkgs.darwin.apple_sdk.frameworks.SystemConfiguration];
}