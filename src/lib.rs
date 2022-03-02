//! This crate provides a way to preset the arguments of a WASI program by modifying
//! the Wasm module.
//!
//! ## Example
//!
//! ```no_run
//! use wasi_preset_args::PresetArgs;
//!
//! let preset_args = PresetArgs::new("my_program".into(), vec!["--arg1".into(), "--arg2".into()]);
//! preset_args.run(&mut module)?;
//! ```
//! Then, the result program behaves as if "--arg1" and "--arg2" were passed to it, and
//! the rest of the arguments are passed at the last.
//! If a runtime does not provide a program name, "my_program" will be used as argv[0].
//!
//! ```console
//! $ wasmtime run ./my_program.wasm --arg3 # --arg1 --arg2 --arg3 is passed to the program
//! ```

use std::{collections::HashMap, ffi::OsString};

use walrus::{
    ir::{BinaryOp, LoadKind, MemArg, StoreKind, UnaryOp, Value},
    FunctionBuilder, FunctionId, GlobalId, InitExpr, InstrSeqBuilder, LocalId, MemoryId, Module,
    ValType,
};

mod call_graph;

pub struct PresetArgs {
    program_name: OsString,
    args: Vec<Vec<u8>>,
    wasi_module_name: String,
}

impl PresetArgs {
    pub fn new(program_name: OsString, args: Vec<OsString>) -> Self {
        let args = args
            .into_iter()
            .map(|arg| arg.to_string_lossy().as_bytes().to_vec())
            .collect::<Vec<_>>();
        Self {
            program_name,
            args,
            wasi_module_name: "wasi_snapshot_preview1".to_string(),
        }
    }

    /// Instrument the input Wasm so that it can override WASI args_get and args_sizes.
    ///
    /// ## Code Shape
    ///
    /// This function will adds two WASI compatible args_* functions to the module.
    /// They proxies the original functions and adds the preset args to the front of the args list.
    /// The preset args data is encoded in const instruction's immediates to avoid memory allocation.
    /// (Adding a new data segment in a linked module would break memory layout, so we can't use memory)
    ///
    /// For example, given this input Wasm module:
    ///
    /// ```wat
    /// (module
    ///  (import "wasi_snapshot_preview1" "args_sizes_get" (func (param i32 i32) (result i32)))
    ///  (import "wasi_snapshot_preview1" "args_get" (func (param i32 i32) (result i32)))
    ///  # pseudo-code
    ///  (func $__main_void (result i32)
    ///   (call $wasi_snapshot_preview1.args_sizes_get ...)
    ///   (call $wasi_snapshot_preview1.args_get ...)
    ///  )
    /// )
    /// ```
    ///
    /// this function will produce this output Wasm module:
    ///
    /// ```wat
    /// (module
    ///  (import "wasi_snapshot_preview1" "args_sizes_get" (func (param i32 i32) (result i32)))
    ///  (import "wasi_snapshot_preview1" "args_get" (func (param i32 i32) (result i32)))
    ///
    ///  (global $saved_original_argc (mut i32) (i32.const 0))
    ///
    ///  # pseudo-code
    ///  (func $__main_void (result i32)
    ///   (call $wasi_preset_args.args_sizes_get ...)
    ///   (call $wasi_preset_args.args_get ...)
    ///  )
    ///  (func $wasi_preset_args.args_sizes_get (size_t *argc_ptr, size_t *argv_buf_size_ptr) (result i32)
    ///     i32 err = $wasi_snapshot_preview1.args_sizes_get(argc_ptr, argv_buf_size);
    ///     if (err == __WASI_ERRNO_SUCCESS) {
    ///       i32 argc = *argc_ptr;
    ///       $saved_original_argc = argc;
    ///       if (argc == 0) {
    ///         *argc_ptr = 1 /* program name */ + PRESET_ARGS_LEN();
    ///         *argv_buf_size_ptr = PROGRAM_NAME_SIZE() + PRESET_ARGS_SIZE();
    ///       } else {
    ///         *argc_ptr = argc + PRESET_ARGS_LEN();
    ///         *argv_buf_size_ptr += PRESET_ARGS_SIZE();
    ///       }
    ///       return __WASI_ERRNO_SUCCESS;
    ///     } else {
    ///       return err;
    ///     }
    ///  )
    ///  (func $wasi_preset_args.args_get (char **argv, char *argv_buf) (result i32)
    ///     if ($saved_original_argc == 0) {
    ///       char *program_name = argv_buf + PRESET_ARGS_SIZE();
    ///       memcpy(program_name, PROGRAM_NAME_DATA(), PROGRAM_NAME_SIZE());
    ///       argv[0] = program_name;
    ///     } else {
    ///       char **extra_argv = argv + PRESET_ARGS_LEN();
    ///       err = $wasi_snapshot_preview1.args_get(extra_argv, argv_buf + PRESET_ARGS_SIZE());
    ///       if (err == __WASI_ERRNO_SUCCESS) {
    ///         argv[0] = extra_argv[0];
    ///       } else {
    ///         return err;
    ///       }
    ///     }
    ///
    ///     memcpy(argv_buf, PRESET_ARGS_DATA(), PRESET_ARGS_SIZE());
    ///
    ///     argv[1] = argv_buf + PRESET_ARGS_OFFSET(0);
    ///     argv[2] = argv_buf + PRESET_ARGS_OFFSET(1);
    ///     ...
    ///     argv[PRESET_ARGS_LEN()] = argv_buf + PRESET_ARGS_OFFSET(PRESET_ARGS_LEN() - 1);
    ///
    ///     return __WASI_ERRNO_SUCCESS;
    ///  )
    /// )
    /// ```
    ///
    /// ## Limitations
    ///
    /// This rewrite assumes that `args_get` is always called after `args_sizes_get` to save the
    /// original argc in a global variable, which is used to determine whether the runtime provides
    /// program name or not.
    ///
    pub fn run(&self, module: &mut Module) -> anyhow::Result<()> {
        // Add the global variable to store the original argc.
        let saved_original_argc =
            module
                .globals
                .add_local(ValType::I32, true, InitExpr::Value(Value::I32(0)));

        let original_args_sizes_get =
            get_import_function(module, &self.wasi_module_name, "args_sizes_get")?;
        let (dummy_args_sizes_get, dummy_args_sizes_get_import) = module.add_import_func(
            "wasi_preset_args",
            "args_sizes_get",
            module.funcs.get(original_args_sizes_get).ty(),
        );
        let original_args_get = get_import_function(module, &self.wasi_module_name, "args_get")?;
        let (dummy_args_get, dummy_args_get_import) = module.add_import_func(
            "wasi_preset_args",
            "args_get",
            module.funcs.get(original_args_get).ty(),
        );

        let mut call_graph = call_graph::CallGraph::build_from(module);
        {
            // Replace the use of the original `args_*` with dummy functions
            // to distinguish them from the use of them in our proxy functions.
            let mut map = HashMap::new();
            map.insert(original_args_sizes_get, dummy_args_sizes_get);
            map.insert(original_args_get, dummy_args_get);

            call_graph::replace_func_use(&map, module, &mut call_graph);
        }
        let new_args_sizes_get = self.add_args_sizes_get(module, saved_original_argc)?;
        let new_args_get = self.add_args_get(module, saved_original_argc)?;
        {
            // Replace the use of the dummy functions with the proxy functions.
            // This doesn't replace the use of the original functions in the proxy
            // functions, thanks to the dummy functions marking.
            let mut map = HashMap::new();
            map.insert(dummy_args_sizes_get, new_args_sizes_get);
            map.insert(dummy_args_get, new_args_get);
            call_graph::replace_func_use(&map, module, &mut call_graph);
        }

        module.imports.delete(dummy_args_sizes_get_import);
        module.imports.delete(dummy_args_get_import);

        Ok(())
    }

    fn preset_args_size(&self) -> usize {
        self.args.iter().map(|arg| arg.len() + 1).sum::<usize>()
    }
    fn argv_buf_size(&self) -> usize {
        self.program_name.len() + 1 + self.preset_args_size()
    }
    fn pointer_size(&self) -> usize {
        4
    }

    fn argv_buf_size_value(&self) -> walrus::ir::Value {
        walrus::ir::Value::I32(i32::from_le_bytes(
            (self.argv_buf_size() as u32).to_le_bytes(),
        ))
    }

    /// See the comment in `run` for the Code Shape.
    fn add_args_sizes_get(
        &self,
        module: &mut Module,
        saved_original_argc: GlobalId,
    ) -> anyhow::Result<FunctionId> {
        let original = get_import_function(module, &self.wasi_module_name, "args_sizes_get")?;
        let sig = module.types.get(module.funcs.get(original).ty()).clone();
        let mut builder = FunctionBuilder::new(&mut module.types, sig.params(), sig.results());

        // Arguments
        let argc_ptr = module.locals.add(ValType::I32);
        let argv_buf_size_ptr = module.locals.add(ValType::I32);
        // Locals
        let err = module.locals.add(ValType::I32);
        let argc = module.locals.add(ValType::I32);

        let memory = match module.memories.iter().next() {
            Some(m) => m,
            None => anyhow::bail!("no memory"),
        };

        builder.name("wasi_preset_args.args_sizes_get".to_string());

        let mut instr_builder = builder.func_body();

        // i32 err = $wasi_snapshot_preview1.args_sizes_get(argc_ptr, argv_buf_size);
        instr_builder
            .local_get(argc_ptr)
            .local_get(argv_buf_size_ptr)
            .call(original)
            .local_tee(err);

        // if (err != __WASI_ERRNO_SUCCESS) {
        instr_builder.unop(UnaryOp::I32Eqz).if_else(
            ValType::I32,
            |then| {
                // i32 argc = *argc_ptr;
                then.local_get(argc_ptr)
                    .load(
                        memory.id(),
                        LoadKind::I32 { atomic: false },
                        MemArg {
                            align: 1,
                            offset: 0,
                        },
                    )
                    .local_tee(argc);

                // saved_original_argc = argc;
                then.global_set(saved_original_argc);

                // if (argc == 0) {
                then.local_get(argc)
                    .unop(UnaryOp::I32Eqz)
                    .if_else(
                        None,
                        |then| {
                            // *argc_ptr = 1 /* program name */ + PRESET_ARGS_LEN();
                            then.local_get(argc_ptr)
                                .const_(usize_to_wasm_i32(1 + self.args.len()))
                                .store(
                                    memory.id(),
                                    StoreKind::I32 { atomic: false },
                                    MemArg {
                                        align: 1,
                                        offset: 0,
                                    },
                                );
                            // *argv_buf_size_ptr = PROGRAM_NAME_SIZE() + PRESET_ARGS_SIZE();
                            then.local_get(argv_buf_size_ptr)
                                .const_(usize_to_wasm_i32(self.argv_buf_size()))
                                .store(
                                    memory.id(),
                                    StoreKind::I32 { atomic: false },
                                    MemArg {
                                        align: 1,
                                        offset: 0,
                                    },
                                );
                        },
                        |_else| {
                            // *argc_ptr = argc + PRESET_ARGS_LEN();
                            _else
                                .local_get(argc_ptr)
                                .local_get(argc)
                                .const_(usize_to_wasm_i32(self.args.len()))
                                .binop(BinaryOp::I32Add)
                                .store(
                                    memory.id(),
                                    StoreKind::I32 { atomic: false },
                                    MemArg {
                                        align: 1,
                                        offset: 0,
                                    },
                                );
                            // *argv_buf_size_ptr += PRESET_ARGS_SIZE();
                            _else
                                .local_get(argv_buf_size_ptr)
                                .local_get(argv_buf_size_ptr)
                                .load(
                                    memory.id(),
                                    LoadKind::I32 { atomic: false },
                                    MemArg {
                                        align: 1,
                                        offset: 0,
                                    },
                                )
                                .const_(self.argv_buf_size_value())
                                .binop(BinaryOp::I32Add)
                                .store(
                                    memory.id(),
                                    StoreKind::I32 { atomic: false },
                                    MemArg {
                                        align: 1,
                                        offset: 0,
                                    },
                                );
                        },
                    )
                    .i32_const(__WASI_ERRNO_SUCCESS);
            },
            |else_| {
                else_.local_get(err);
            },
        );
        Ok(builder.finish(vec![argc_ptr, argv_buf_size_ptr], &mut module.funcs))
    }

    /// See the comment in `run` for the Code Shape.
    fn add_args_get(
        &self,
        module: &mut Module,
        saved_original_argc: GlobalId,
    ) -> anyhow::Result<FunctionId> {
        let original = get_import_function(module, &self.wasi_module_name, "args_get")?;
        let sig = module.types.get(module.funcs.get(original).ty()).clone();
        let mut builder = FunctionBuilder::new(&mut module.types, sig.params(), sig.results());

        let argv = module.locals.add(ValType::I32);
        let argv_buf = module.locals.add(ValType::I32);
        let err = module.locals.add(ValType::I32);
        let extra_argv = module.locals.add(ValType::I32);

        let memory = match module.memories.iter().next() {
            Some(m) => m,
            None => anyhow::bail!("no memory"),
        };

        builder.name("wasi_preset_args.args_get".to_string());
        let mut instr_builder = builder.func_body();

        // 1. Write argv[0], argv[1+args.len()...]
        let instr_builder = instr_builder
            .global_get(saved_original_argc)
            .unop(UnaryOp::I32Eqz)
            .if_else(
                None,
                |then| {
                    store_string_at(
                        then,
                        memory.id(),
                        self.program_name.to_string_lossy().as_bytes(),
                        argv_buf,
                        self.preset_args_size(),
                    );
                    then.local_get(argv)
                        .local_get(argv_buf)
                        .const_(usize_to_wasm_i32(self.preset_args_size()))
                        .binop(BinaryOp::I32Add)
                        .store(
                            memory.id(),
                            StoreKind::I32 { atomic: false },
                            MemArg {
                                align: 1,
                                offset: 0,
                            },
                        );
                },
                |else_| {
                    // 1. argv_buf ..< argv_buf + preset_buf_size: preset_buf
                    // 2. argv_buf + preset_buf_size ..< argv_buf + preset_buf_size + original_buf_size: original_buf

                    // write original argv[0] at argv[args.len()], and move it at argv[0]

                    // char **extra_argv = argv + PRESET_ARGS_LEN();
                    let else_ = else_
                        .local_get(argv)
                        .const_(usize_to_wasm_i32(self.args.len() * self.pointer_size()))
                        .binop(BinaryOp::I32Add)
                        .local_tee(extra_argv);

                    // err = $wasi_snapshot_preview1.args_get(extra_argv, argv_buf + PRESET_ARGS_SIZE());
                    let else_ = else_
                        .local_get(argv_buf)
                        .const_(self.argv_buf_size_value())
                        .binop(BinaryOp::I32Add)
                        .call(original)
                        .local_tee(err);

                    else_.unop(UnaryOp::I32Eqz).if_else(
                        None,
                        |then| {
                            // argv[0] = extra_argv[0];
                            then.local_get(argv)
                                .local_get(extra_argv)
                                .load(
                                    memory.id(),
                                    LoadKind::I32 { atomic: false },
                                    MemArg {
                                        align: 1,
                                        offset: 0,
                                    },
                                )
                                .store(
                                    memory.id(),
                                    StoreKind::I32 { atomic: false },
                                    MemArg {
                                        align: 1,
                                        offset: 0,
                                    },
                                );
                        },
                        |_else| {
                            _else.local_get(err).return_();
                        },
                    );
                },
            );

        // 2. Write argv[1..<1+args.len()]
        let mut offset = 0;
        for (i, arg) in self.args.iter().enumerate() {
            store_string_at(instr_builder, memory.id(), arg, argv_buf, offset);
            instr_builder
                .local_get(argv)
                .const_(usize_to_wasm_i32((i + 1) * self.pointer_size()))
                .binop(BinaryOp::I32Add)
                .local_get(argv_buf)
                .const_(usize_to_wasm_i32(offset))
                .binop(BinaryOp::I32Add)
                .store(
                    memory.id(),
                    StoreKind::I32 { atomic: false },
                    MemArg {
                        align: 1,
                        offset: 0,
                    },
                );
            offset += arg.len() + 1;
        }

        instr_builder.i32_const(__WASI_ERRNO_SUCCESS);
        Ok(builder.finish(vec![argv, argv_buf], &mut module.funcs))
    }
}

const __WASI_ERRNO_SUCCESS: i32 = 0;

fn get_import_function(m: &Module, module: &str, name: &str) -> anyhow::Result<FunctionId> {
    let original = match m.imports.find(module, name) {
        Some(f) => f,
        None => anyhow::bail!("{}.{} not found", module, name),
    };
    let original = m.imports.get(original);
    let original = match original.kind {
        walrus::ImportKind::Function(f) => f,
        _ => anyhow::bail!("{}.{} is not a function", module, name),
    };
    Ok(original)
}

fn usize_to_wasm_i32(x: usize) -> Value {
    Value::I32(i32::from_le_bytes((x as u32).to_le_bytes()))
}

fn store_string_at(
    builder: &mut InstrSeqBuilder,
    memory: MemoryId,
    s: &[u8],
    base: LocalId,
    offset: usize,
) {
    let mut written = 0;
    for chunk_size in [8, 4, 2, 1] {
        let chunk_count = (s.len() - written) / chunk_size;
        for _ in 0..chunk_count {
            let chunk = &s[written..written + chunk_size];
            let (v, kind) = match chunk_size {
                8 => (
                    Value::I64(i64::from_le_bytes(chunk.try_into().unwrap())),
                    StoreKind::I64 { atomic: false },
                ),
                4 => (
                    Value::I32(i32::from_le_bytes(chunk.try_into().unwrap())),
                    StoreKind::I32 { atomic: false },
                ),
                2 => (
                    Value::I32(i16::from_le_bytes(chunk.try_into().unwrap()) as i32),
                    StoreKind::I32_16 { atomic: false },
                ),
                1 => (
                    Value::I32(i8::from_le_bytes(chunk.try_into().unwrap()) as i32),
                    StoreKind::I32_8 { atomic: false },
                ),
                _ => unreachable!(),
            };
            builder
                .local_get(base)
                .const_(usize_to_wasm_i32(offset + written))
                .binop(BinaryOp::I32Add)
                .const_(v)
                .store(
                    memory,
                    kind,
                    MemArg {
                        align: 1,
                        offset: 0,
                    },
                );
            written += chunk_size;
        }
    }
}
