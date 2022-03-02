-include ../tools.mk

check:
	$(CC) $(CCFLAGS) main.c -o $(TMPDIR)/main.wasm
	$(WASI_PRESET_ARGS) $(TMPDIR)/main.wasm --program-name default.wasm -o $(TMPDIR)/foo_bar.wasm -- \
	  FOO BAR XXX

	$(WASI_RUN) $(TMPDIR)/foo_bar.wasm > $(TMPDIR)/no_args.txt
	diff $(TMPDIR)/no_args.txt expected/no_args.txt

	$(WASI_RUN) $(TMPDIR)/foo_bar.wasm -- FIZZ BUZZ > $(TMPDIR)/extra_args.txt
	diff $(TMPDIR)/extra_args.txt expected/extra_args.txt

	$(NODE) ./harness.js $(TMPDIR)/foo_bar.wasm > $(TMPDIR)/no_arg0.txt
	diff $(TMPDIR)/no_arg0.txt expected/no_arg0.txt

	# Split a long string into chunks
	$(WASI_PRESET_ARGS) $(TMPDIR)/main.wasm --program-name default.wasm -o $(TMPDIR)/long_str.wasm -- \
	  FOO_BAR_FIZZ_BUZZ_XXX_YYY_ZZZ
	$(WASI_RUN) $(TMPDIR)/long_str.wasm > $(TMPDIR)/long_str.txt
	diff $(TMPDIR)/long_str.txt expected/long_str.txt
