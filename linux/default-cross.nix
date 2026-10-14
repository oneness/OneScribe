let
  pkgs = import <nixpkgs> {};
  crossPkgs = pkgs.pkgsCross.aarch64-multiplatform;
in
crossPkgs.rustPlatform.buildRustPackage {
  pname = "onescribe-linux-core";
  version = "0.1.0";
  src = ./.;
  cargoLock.lockFile = ./Cargo.lock;
  # transcribe-cpp-sys compiles the native transcribe.cpp library via CMake.
  nativeBuildInputs = [ pkgs.buildPackages.cmake ];
  postPatch = ''
    # Replace relative paths to OneScribe assets with local assets dir
    substituteInPlace src/platform.rs \
      --replace-fail '"../OneScribe/Resources/Assets.xcassets/AppIcon.appiconset/icon_128x128.png"' '"assets/icon_128x128.png"' \
      --replace-fail '"../OneScribe/Resources/Assets.xcassets/AppIcon.appiconset/icon_256x256.png"' '"assets/icon_256x256.png"' \
      --replace-fail '"../OneScribe/Resources/Assets.xcassets/AppIcon.appiconset/icon_512x512.png"' '"assets/icon_512x512.png"'
  '';
  postInstall = ''
    mkdir -p $out/bin/assets/icons/hicolor/scalable/status

    cp ${./assets/completion.oga} $out/bin/assets/completion.oga
    cp ${./assets/icons/hicolor/scalable/status/onescribe-symbolic.svg} $out/bin/assets/icons/hicolor/scalable/status/
    cp ${./assets/icons/hicolor/scalable/status/onescribe-recording-symbolic.svg} $out/bin/assets/icons/hicolor/scalable/status/
    cp ${./assets/icons/hicolor/scalable/status/onescribe-transcribing-symbolic.svg} $out/bin/assets/icons/hicolor/scalable/status/
    cp ${../OneScribe/Resources/Assets.xcassets/AppIcon.appiconset/icon_128x128.png} $out/bin/assets/
    cp ${../OneScribe/Resources/Assets.xcassets/AppIcon.appiconset/icon_256x256.png} $out/bin/assets/
    cp ${../OneScribe/Resources/Assets.xcassets/AppIcon.appiconset/icon_512x512.png} $out/bin/assets/

    # Fix Nix store hashed filenames
    cd $out/bin/assets
    for f in *-icon_*.png; do
      [ -f "$f" ] && mv "$f" "''${f#*-}"
    done
    cd $out/bin/assets/icons/hicolor/scalable/status
    for f in *-onescribe-*.svg; do
      [ -f "$f" ] && mv "$f" "''${f#*-}"
    done

    # Patch ELF interpreter for non-NixOS targets
    chmod +w $out/bin/onescribe-linux-core
    ${pkgs.patchelf}/bin/patchelf --set-interpreter /lib/ld-linux-aarch64.so.1 $out/bin/onescribe-linux-core
  '';
}
