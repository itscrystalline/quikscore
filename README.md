# quikscore

[![Coverage Status](https://coveralls.io/repos/github/itscrystalline/quikscore/badge.svg)](https://coveralls.io/github/itscrystalline/quikscore)

Automatically scan and score answer sheets.

## Environment setup

install [node.js](https://nodejs.org/en/download), then install [yarn](https://yarnpkg.com/getting-started/install)

```shell
$ npm install -g corepack
```

next, install [rust](https://www.rust-lang.org/) from https://rustup.rs

then, in the project folder, run `yarn install` to install all the dependencies.

## Development

run `yarn tauri dev`. this will build the binary, and start the frontend server. after a bit you should be greeted with the application open.

## Building

run `yarn tauri build` to build the app.
