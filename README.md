# quikscore

[![Coverage Status](https://coveralls.io/repos/github/itscrystalline/quikscore/badge.svg)](https://coveralls.io/github/itscrystalline/quikscore)

Automatically scan and score answer sheets.

## Downloads

prebuilt versions of `quikscore` built by our CI are available
[here](https://nightly.link/itscrystalline/quikscore/workflows/cd.yaml/main?preview)
or in the table below.

| Version                             | Explaination                                 | Link                                                                                                         |
| ----------------------------------- | -------------------------------------------- | ------------------------------------------------------------------------------------------------------------ |
| quikscore-linux-aarch64             | for Linux on 64-bit ARM Devices              | https://nightly.link/itscrystalline/quikscore/workflows/cd.yaml/main/quikscore-linux-aarch64.zip             |
| quikscore-linux-x86_64              | for Linux on 64-bit x86 Devices              | https://nightly.link/itscrystalline/quikscore/workflows/cd.yaml/main/quikscore-linux-x86_64.zip              |
| quikscore-macos-aarch64             | for macOS on Apple Silicon Devices           | https://nightly.link/itscrystalline/quikscore/workflows/cd.yaml/main/quikscore-macos-aarch64.zip             |
| quikscore-macos-x86_64              | for macOS on 64-bit Intel Devices            | https://nightly.link/itscrystalline/quikscore/workflows/cd.yaml/main/quikscore-macos-x86_64.zip              |
| quikscore-windows-x86_64            | for Windows on 64-bit x86 Devices            | https://nightly.link/itscrystalline/quikscore/workflows/cd.yaml/main/quikscore-windows-x86_64.zip            |
| quikscore-windows-x86_64-installers | Installers for Windows on 64-bit x86 Devices | https://nightly.link/itscrystalline/quikscore/workflows/cd.yaml/main/quikscore-windows-x86_64-installers.zip |

## Environment setup

> [!WARNING]
> These instructions are non-exhaustive. For full instructions, refer to the
> [DEV_MANUAL.pdf](https://github.com/itscrystalline/quikscore/blob/main/DEV_MANUAL.pdf)
> file at the root of the repository.

install [node.js](https://nodejs.org/en/download), then install
[yarn](https://yarnpkg.com/getting-started/install)

```shell
npm install -g corepack
```

next, install [rust](https://www.rust-lang.org/) from https://rustup.rs

then, in the project folder, run `yarn install` to install all the dependencies.

### OpenCV & Tesseract setup (Windows)

install [chocolatey](https://chocolatey.org/install) by running

```powershell
Set-ExecutionPolicy Bypass -Scope Process -Force; [System.Net.ServicePointManager]::SecurityProtocol = [System.Net.ServicePointManager]::SecurityProtocol -bor 3072; iex ((New-Object System.Net.WebClient).DownloadString('https://community.chocolatey.org/install.ps1'))
```

(if the command fails, run `Set-ExecutionPolicy Unrestricted` then rerun the
above command.)

then, run

```powershell
choco install llvm opencv -y
```

to install OpenCV and LLVM libraries.

next, install [msys2](https://www.msys2.org/).

> [!NOTE]
> You can also download the latest nightly directly from MSYS2's
> [github actions](https://github.com/msys2/msys2-installer/releases/latest).
>
> [direct link for x64 windows](https://github.com/msys2/msys2-installer/releases/download/nightly-x86_64/msys2-x86_64-latest.exe)
>
> if you use `winget`, get it from `winget install MSYS2.MSYS2`.

next, open the **UCRT64** version of msys2. it's the yellow icon. when you open
it up, somewhere in the shell should day `UCRT64`.

then, run the following to update the system and install the dependencies.

```shell
pacman -Syu --noconfirm
```

then, msys2 will restart. start it back up, then run

```shell
pacman -Syu --noconfirm # just to make sure everything is updated
pacman -S mingw-w64-ucrt-x86_64-tesseract-ocr mingw-w64-ucrt-x86_64-openssl --noconfirm
```

when pacman asks to confirm, press `enter`.

after that, you need to define 12 environment variables. if you use powershell,
paste the script below. if not, create them manually.

> [!WARNING]
> The instructions below assume the default installation path for MSYS2:
> `C:\msys64`. If you have installed MSYS2 to a different location, change
> `C:\msys64` to your install location.

```powershell
$env:MSYS2="C:\msys64"
[Environment]::SetEnvironmentVariable("OPENCV_INCLUDE_PATHS", "C:\tools\opencv\build\include", "User")
[Environment]::SetEnvironmentVariable("OPENCV_LINK_PATHS", "C:\tools\opencv\build\x64\vc16\lib", "User")
[Environment]::SetEnvironmentVariable("OPENCV_DLL_PATH", "C:\tools\opencv\build\x64\vc16\bin", "User")
[Environment]::SetEnvironmentVariable("OPENCV_LINK_LIBS", "opencv_world4110", "User")
[Environment]::SetEnvironmentVariable("LEPTONICA_INCLUDE_PATH", "$env:MSYS2\ucrt64\include", "User")
[Environment]::SetEnvironmentVariable("LEPTONICA_LINK_PATHS", "$env:MSYS2\ucrt64\lib", "User")
[Environment]::SetEnvironmentVariable("LEPTONICA_DLL_PATH", "$env:MSYS2\ucrt64\bin", "User")
[Environment]::SetEnvironmentVariable("LEPTONICA_LINK_LIBS", "leptonica", "User")
[Environment]::SetEnvironmentVariable("TESSERACT_INCLUDE_PATHS", "$env:MSYS2\ucrt64\include", "User")
[Environment]::SetEnvironmentVariable("TESSERACT_LINK_PATHS", "$env:MSYS2\ucrt64\lib", "User")
[Environment]::SetEnvironmentVariable("TESSERACT_DLL_PATH", "$env:MSYS2\ucrt64\bin", "User")
[Environment]::SetEnvironmentVariable("TESSERACT_LINK_LIBS", "tesseract", "User")
```

<details>
<summary> Explaination </summary>

`OPENCV_INCLUDE_PATHS`: Set to `C:\tools\opencv\build\include`

> This tells the compiler where to find OpenCV header files (`.h` / `.hpp`) when
> building.

`OPENCV_LINK_PATHS`: Set to `C:\tools\opencv\build\x64\vc16\lib`

> This tells the linker where to find OpenCV static or import libraries (`.lib`)
> for linking.

`OPENCV_DLL_PATH`: Set to `C:\tools\opencv\build\x64\vc16\bin`

> This points to the directory containing OpenCV dynamic libraries (`.dll`) to
> be bundled with the app.

`OPENCV_LINK_LIBS`: Set to `opencv_world4110`

> The actual OpenCV library name to link against. Use the base name without
> `lib` prefix or extension.

`LEPTONICA_INCLUDE_PATH`: Set to `C:\msys64\ucrt64\include`

> Location of Leptonica header files. Required for compilation of code using
> Leptonica.

`LEPTONICA_LINK_PATHS`: Set to `C:\msys64\ucrt64\lib`

> Directory containing Leptonica import libraries (`.a` or `.lib`) for linking.

`LEPTONICA_DLL_PATH`: Set to `C:\msys64\ucrt64\bin`

> Directory containing the Leptonica DLLs to be bundled with the app.

`LEPTONICA_LINK_LIBS`: Set to `leptonica`

> Library name for linking. The build system will convert this into the
> appropriate linker flag.

`TESSERACT_INCLUDE_PATHS`: Set to `C:\msys64\ucrt64\include`

> Tesseract header file location. Needed for compilation.

`TESSERACT_LINK_PATHS`: Set to `C:\msys64\ucrt64\lib`

> Directory containing Tesseract import libraries for linking.

`TESSERACT_DLL_PATH`: Set to `C:\msys64\ucrt64\bin`

> Directory containing Tesseract DLLs to be bundled with the app.

`TESSERACT_LINK_LIBS`: Set to `tesseract`

> The library name used by the linker to resolve Tesseract symbols.

</details>

## Development

run `yarn tauri dev`. this will build the binary, and start the frontend server.
after a bit you should be greeted with the application open.

## Building

run `yarn tauri build` to build the app.
