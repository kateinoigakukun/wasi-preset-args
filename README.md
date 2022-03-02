[![CI](https://github.com/kateinoigakukun/wasi-preset-args/actions/workflows/main.yml/badge.svg)](https://github.com/kateinoigakukun/wasi-preset-args/actions/workflows/main.yml)
# wasi-preset-args

This project enables to preset command-line arguments to a WASI module.

## Installation

A typical installation from the release binaries might look like the following:

```console
$ export WASI_PRESET_ARGS_VERSION=0.1.0
$ curl -LO "https://github.com/kateinoigakukun/wasi-preset-args/releases/download/v${WASI_PRESET_ARGS_VERSION}/wasi-preset-args-x86_64-unknown-linux-gnu.zip"
$ unzip wasi-preset-args-x86_64-unknown-linux-gnu.zip
$ mv wasi-preset-args /usr/local/bin/wasi-preset-args
```

Or install via `cargo`:

```console
$ cargo install --git https://github.com/kateinoigakukun/wasi-preset-args.git --all-features
```

## Example Usage

First, prepare a WASI module you want to preset arguments to:

```c
#include <stdio.h>

int main(int argc, char **argv) {
  printf("argc = %d\n", argc);
  for (int i = 0; i < argc; i++) {
    printf("argv[%d] = %s\n", i, argv[i]);
  }
  return 0;
}
```

You can compile it for `wasm32-unknown-wasi` with [`wasi-sdk`](https://github.com/WebAssembly/wasi-sdk), and preset arguments to it:

```console
$ $WASI_SDK_PATH/bin/clang -target wasm32-unknown-wasi main.c -o main.wasm
$ wasi-preset-args main.wasm -o main.preset.wasm -- --foo --bar
```

Then, the WASI module behaves as if it was called with the preset arguments:

```console
$ wasmtime main.preset.wasm
argc = 3
argv[0] = main.preset.wasm
argv[1] = --foo
argv[2] = --bar

$ # Extra arguments are passed at the end
$ wasmtime main.preset.wasm -- --fizz file
argc = 5
argv[0] = main.preset.wasm
argv[1] = --foo
argv[2] = --bar
argv[3] = --fizz
argv[4] = file
```

## Testing

### End-to-end tests

To run e2e tests, you need to install the [`wasi-sdk`](https://github.com/WebAssembly/wasi-sdk) version 14.0 or later.

```console
$ export WASI_SDK_PATH=/path/to/wasi-sdk
$ ./tools/run-make-test.sh
```


## How does it work?

`wasi-preset-args` adds two WASI compatible functions (`$wasi_preset_args.args_sizes_get`, `$wasi_preset_args.args_get`) to the module.
They proxies the original WASI functions (`$wasi_snapshot_preview1.args_sizes_get`, `$wasi_snapshot_preview1.args_get`),  and adds the preset args to the front of the args list.

The preset args data is encoded in const instruction's immediates to avoid memory allocation.

See also doc comments in [`src/lib.rs`](src/lib.rs)
