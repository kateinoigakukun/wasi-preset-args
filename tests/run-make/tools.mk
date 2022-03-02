CC = $(WASI_SDK_PATH)/bin/clang
WASI_PRESET_ARGS = cargo run -p wasi-preset-args --all-features --
WASI_RUN = wasmtime
NODE = node --experimental-wasi-unstable-preview1

TARGET = wasm32-unknown-wasi

OPTFLAGS ?=
CCFLAGS = -target $(TARGET) $(OPTFLAGS)

TMPDIR = $(shell mkdir -p .tmp && echo .tmp)
