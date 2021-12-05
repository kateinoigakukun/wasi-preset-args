CC = ./tools/wasi-sdk-14.0/bin/clang
MKARGS = CLANG=$(CC) cargo run --bin wasi-mkargs --features=mkargs --

SYSROOT = ./tools/wasi-sdk-14.0/share/wasi-sysroot
TARGET = wasm32-unknown-wasi

OPTFLAGS =
CCFLAGS = -target $(TARGET) --sysroot $(SYSROOT) $(OPTFLAGS)
LDFLAGS = -target $(TARGET) --sysroot $(SYSROOT)

OBJS = main.c.o preset_args.o

PRESET_ARGS = foo bar baz

PROG = main.wasm

all: $(PROG)

clean:
	rm -rf $(PROG) $(OBJS)

test: $(PROG)
	wasmtime $(PROG)

.PHONY: preset_args.o
preset_args.o:
	$(MKARGS) --emit obj --program-name $(PROG) -o $@ -- $(PRESET_ARGS)

$(PROG): $(OBJS) $(LIB_WASI_VFS)
	$(CC) $(LDFLAGS) $(OBJS) $(LIB_WASI_VFS) -o $@

%.c.o: %.c $(HEADERS)
	$(CC) -c $(CCFLAGS) $< -o $@

.PHONY: all clean test
