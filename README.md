# quikscore

[![Coverage Status](https://coveralls.io/repos/github/itscrystalline/quikscore/badge.svg)](https://coveralls.io/github/itscrystalline/quikscore)

Automatically scan and score answer sheets.

## Downloads

prebuilt versions of `quikscore` built by our CI are available [here](https://nightly.link/itscrystalline/quikscore/workflows/cd.yaml/main?preview)
or in the table below.

generally AVX support is preferred.

| Version                      | Explaination                                            | Link                                                                                                  |
| ---------------------------- | ------------------------------------------------------- | ----------------------------------------------------------------------------------------------------- |
| quikscore-linux-aarch64-avx  | for Linux on 64-bit ARM Devices, with AVX support       | https://nightly.link/itscrystalline/quikscore/workflows/cd.yaml/main/quikscore-linux-aarch64-avx.zip  |
| quikscore-linux-aarch64      | for Linux on 64-bit ARM Devices, without AVX support    | https://nightly.link/itscrystalline/quikscore/workflows/cd.yaml/main/quikscore-linux-aarch64.zip      |
| quikscore-linux-x86_64-avx   | for Linux on 64-bit x86 Devices, with AVX support       | https://nightly.link/itscrystalline/quikscore/workflows/cd.yaml/main/quikscore-linux-x86_64-avx.zip   |
| quikscore-linux-x86_64       | for Linux on 64-bit x86 Devices, without AVX support    | https://nightly.link/itscrystalline/quikscore/workflows/cd.yaml/main/quikscore-linux-x86_64.zip       |
| quikscore-macos-aarch64-avx  | for macOS on Apple Silicon Devices, with AVX support    | https://nightly.link/itscrystalline/quikscore/workflows/cd.yaml/main/quikscore-macos-aarch64-avx.zip  |
| quikscore-macos-aarch64      | for macOS on Apple Silicon Devices, without AVX support | https://nightly.link/itscrystalline/quikscore/workflows/cd.yaml/main/quikscore-macos-aarch64.zip      |
| quikscore-macos-x86_64-avx   | for macOS on 64-bit Intel Devices, with AVX support     | https://nightly.link/itscrystalline/quikscore/workflows/cd.yaml/main/quikscore-macos-x86_64-avx.zip   |
| quikscore-macos-x86_64       | for macOS on 64-bit Intel Devices, without AVX support  | https://nightly.link/itscrystalline/quikscore/workflows/cd.yaml/main/quikscore-macos-x86_64.zip       |
| quikscore-windows-x86_64-avx | for Windows on 64-bit x86 Devices, with AVX support     | https://nightly.link/itscrystalline/quikscore/workflows/cd.yaml/main/quikscore-windows-x86_64-avx.zip |
| quikscore-windows-x86_64     | for Windows on 64-bit x86 Devices, without AVX support  | https://nightly.link/itscrystalline/quikscore/workflows/cd.yaml/main/quikscore-windows-x86_64.zip     |

## Environment setup

install [node.js](https://nodejs.org/en/download), then install [yarn](https://yarnpkg.com/getting-started/install)

```shell
$ npm install -g corepack
```

next, install [rust](https://www.rust-lang.org/) from https://rustup.rs

then, in the project folder, run `yarn install` to install all the dependencies.

### OpenCV setup (Windows)

install [chocolatey](https://chocolatey.org/install) by running

```powershell
Set-ExecutionPolicy Bypass -Scope Process -Force; [System.Net.ServicePointManager]::SecurityProtocol = [System.Net.ServicePointManager]::SecurityProtocol -bor 3072; iex ((New-Object System.Net.WebClient).DownloadString('https://community.chocolatey.org/install.ps1'))
```

(if the command fails, run `Set-ExecutionPolicy Unrestricted` then rerun the above command.)

then, run

```powershell
choco install llvm opencv -y
```

to install OpenCV and LLVM libraries.

after that, confirm that `C:\tools\opencv` exists.
next, you need to define 3 environment variables.

`OPENCV_INCLUDE_PATHS`: Set to `C:\tools\opencv\build\include`
`OPENCV_LINK_LIBS`: Set to `+opencv_world4110`
`OPENCV_LINK_PATHS`: Set to `+C:\tools\opencv\build\x64\vc16\lib`

then add `C:\tools\opencv\build\x64\vc16\bin` to your `PATH`.

## Development

run `yarn tauri dev`. this will build the binary, and start the frontend server. after a bit you should be greeted with the application open.

## Building

run `yarn tauri build` to build the app.
