# Native build of the OneScribe Linux core.
#
#   nix-build default.nix --out-link ~/.local/opt/onescribe
#   nix-build default.nix --arg vulkan false        # CPU-only build
#
# Installing to a stable out-link (rather than running `target/release/...`)
# keeps the daemon off build artifacts and puts the bundled assets next to the
# binary, where release builds resolve them from.
{
  pkgs ? import <nixpkgs> { },
  vulkan ? true,
}:
let
  inherit (pkgs) lib;
in
pkgs.rustPlatform.buildRustPackage {
  pname = "onescribe-linux-core";
  version = "0.1.0";
  # Ship only real build inputs. `target/` alone reaches tens of GB here and
  # copying it into the store can exhaust the disk; a whitelist also keeps doc
  # edits from invalidating the build.
  src = lib.cleanSourceWith {
    src = ./.;
    filter =
      path: _type:
      if toString path == toString ./. then
        true
      else
        builtins.elem (builtins.head (lib.splitString "/" (lib.removePrefix (toString ./. + "/") (toString path)))) [
          "src"
          "assets"
          "Cargo.toml"
          "Cargo.lock"
        ];
  };
  cargoLock.lockFile = ./Cargo.lock;

  # transcribe-cpp-sys compiles the native transcribe.cpp library via CMake.
  # The Vulkan backend additionally needs the loader, headers and glslc.
  nativeBuildInputs = [
    pkgs.cmake
    pkgs.pkg-config
  ]
  ++ lib.optionals vulkan [
    pkgs.vulkan-headers
    pkgs.vulkan-loader
    pkgs.shaderc
    pkgs.spirv-headers
  ];
  buildInputs = lib.optionals vulkan [
    pkgs.vulkan-loader
    # ggml-vulkan includes <spirv/unified1/spirv.hpp> directly; putting the
    # package in buildInputs lets the cc-wrapper add the include dir.
    pkgs.spirv-headers
  ];

  # CMake's FindVulkan looks for VULKAN_SDK rather than NIX_CFLAGS_COMPILE, and
  # ggml-vulkan needs glslc to compile its shaders.
  VULKAN_SDK = lib.optionalString vulkan "${pkgs.vulkan-headers}";
  Vulkan_GLSLC_EXECUTABLE = lib.optionalString vulkan "${pkgs.shaderc}/bin/glslc";

  # ggml-vulkan does `find_package(SPIRV-Headers CONFIG REQUIRED)`. The cmake
  # crate drives CMake directly and passes whatever CMAKE_PREFIX_PATH holds (the
  # nixpkgs setup hook's value does not reach it), so set it here.
  CMAKE_PREFIX_PATH = lib.optionalString vulkan (
    lib.makeSearchPath "" [
      pkgs.spirv-headers
      pkgs.vulkan-headers
      pkgs.vulkan-loader
    ]
  );

  # `vulkan` is a default cargo feature (the default model needs the GPU to keep
  # up); with vulkan = false drop default features to build CPU-only. The backend
  # is chosen at runtime, so a Vulkan build still works without a GPU.
  buildNoDefaultFeatures = !vulkan;

  postPatch = ''
    # Replace relative paths to OneScribe assets with the bundled assets dir
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
  '';

  meta = with lib; {
    description = "OneScribe system-wide speech-to-text daemon (Linux)";
    homepage = "https://github.com/oneness/OneScribe";
    license = licenses.mit;
    mainProgram = "onescribe-linux-core";
  };
}
