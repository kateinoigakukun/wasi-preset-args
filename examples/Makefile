CC = $(WASI_SDK_PATH)/bin/clang
WASI_PRESET_ARGS = cargo run -p wasi-preset-args --all-features --
TARGET = wasm32-unknown-wasi
CCFLAGS = -target $(TARGET)

PRESET_ARGS = foo bar baz
PROG = main.wasm

check:
	$(CC) $(CCFLAGS) main.c -o main.wasm
	$(WASI_PRESET_ARGS) main.wasm --program-name $(PROG) -o $(PROG) -- $(PRESET_ARGS)
	wasmtime $(PROG)

.PHONY: check
