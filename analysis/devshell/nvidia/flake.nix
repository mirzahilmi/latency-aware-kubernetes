{
  description = "CUDA development environment";

  inputs.nixpkgs.url = "github:nixos/nixpkgs/nixos-25.05";

  outputs = {...} @ inputs: let
    system = "x86_64-linux";
    pkgs = import inputs.nixpkgs {
      inherit system;
      config.allowUnfree = true;
      config.cudaSupport = true;
      config.cudaVersion = "12";
    };
    nvidiaPackage = pkgs.linuxPackages.nvidiaPackages.stable;
  in {
    devShells.${system}.default = pkgs.mkShell {
      packages = with pkgs; [
        basedpyright
        binutils
        cudaPackages.cuda_cudart
        cudaPackages.cudnn
        cudatoolkit
        ffmpeg
        fmt.dev
        freeglut
        libGL
        libGLU
        ncurses
        nvidiaPackage
        python3
        ruff
        stdenv.cc
        uv
        xorg.libX11
        xorg.libXext
        xorg.libXi
        xorg.libXmu
        xorg.libXrandr
        xorg.libXv
        zlib
      ];

      shellHook = ''
        export LD_LIBRARY_PATH="${nvidiaPackage}/lib:$LD_LIBRARY_PATH"
        export CUDA_PATH=${pkgs.cudatoolkit}
        export EXTRA_LDFLAGS="-L/lib -L${nvidiaPackage}/lib"
        export EXTRA_CCFLAGS="-I/usr/include"
        export CMAKE_PREFIX_PATH="${pkgs.fmt.dev}:$CMAKE_PREFIX_PATH"
        export PKG_CONFIG_PATH="${pkgs.fmt.dev}/lib/pkgconfig:$PKG_CONFIG_PATH"
      '';
    };
  };
}
