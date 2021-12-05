use std::{
    io::{Read, Write},
    process::{Command, Stdio},
};

pub fn generate_c_source(default_arg0: &str, args: &[String]) -> std::io::Result<String> {
    let mut output = String::new();

    let prelude = r#"
#include <stdint.h>
#include <stddef.h>

#define __WASI_ERRNO_SUCCESS 0

// When host doen't provide arg0 and any trailing args, this returns [program_name, arg1, arg2, ..]
// When host provides arg0 and optional trailing args (x_arg1, x_arg2, ..), this returns [arg0, arg1, arg2, .., x_arg1, x_arg2]

int32_t __original__imported_wasi_snapshot_preview1_args_sizes_get(size_t *argc, size_t *argv_buf_size) __attribute__((
    __import_module__("wasi_snapshot_preview1"),
    __import_name__("args_sizes_get")
));

int32_t __original__imported_wasi_snapshot_preview1_args_get(char **argv, char *argv_buf) __attribute__((
    __import_module__("wasi_snapshot_preview1"),
    __import_name__("args_get")
));

"#;

    output.push_str(prelude);
    // Define the given args
    {
        output.push_str(&format!(
            "static size_t preset_argc = {};\n",
            args.len() + 1
        ));
        output.push_str(&format!(
            "static char default_arg0[] = \"{}\";\n",
            default_arg0
        ));

        for (i, arg) in args.iter().enumerate() {
            output.push_str(&format!("static char arg{}[] = \"{}\";\n", i + 1, arg));
        }
    }
    output.push_str(
        r#"
int32_t __imported_wasi_snapshot_preview1_args_sizes_get(size_t *argc, size_t *argv_buf_size) {
    int32_t err;
    err = __original__imported_wasi_snapshot_preview1_args_sizes_get(argc, argv_buf_size);
    if (err != __WASI_ERRNO_SUCCESS) {
      return err;
    }
    if (*argc == 0) {
      *argc = preset_argc;
    } else {
      *argc += preset_argc - 1;
    }
    return __WASI_ERRNO_SUCCESS;
}
"#,
    );

    {
        output.push_str(
            r#"
int32_t __imported_wasi_snapshot_preview1_args_get(char **argv, char *argv_buf) {
    int32_t err;
    size_t argc, argv_buf_size;
    err = __original__imported_wasi_snapshot_preview1_args_sizes_get(&argc, &argv_buf_size);
    if (err != __WASI_ERRNO_SUCCESS) {
      return err;
    }
  
    if (argc == 0) {
      argv[0] = default_arg0;
    } else {
      char **extra_argv = argv + preset_argc - 1;
      err = __original__imported_wasi_snapshot_preview1_args_get(extra_argv, argv_buf);
      if (err != __WASI_ERRNO_SUCCESS) {
        return err;
      }
      argv[0] = extra_argv[0];
    }
"#,
        );

        for (i, _) in args.iter().enumerate() {
            output.push_str(&format!("    argv[{}] = arg{};\n", i + 1, i + 1));
        }

        output.push_str(
            r#"
    return __WASI_ERRNO_SUCCESS;
}    
"#,
        );
    }

    Ok(output)
}

pub fn generate_obj(c_src: &str, clang: &str) -> std::io::Result<Vec<u8>> {
    let process = Command::new(clang)
        .args([
            "-x",
            "c",
            "-c",
            "-target",
            "wasm32-unknown-unknown",
            "-",
            "-o",
            "-",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;
    process.stdin.unwrap().write_all(c_src.as_bytes())?;

    let mut obj_buf = vec![];
    process.stdout.unwrap().read_to_end(&mut obj_buf)?;
    Ok(obj_buf)
}
