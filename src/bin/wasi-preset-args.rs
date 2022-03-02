use clap::Parser;
use std::{ffi::OsString, path::PathBuf};

#[derive(Parser)]
pub struct Opt {
    /// .wasm file to process
    #[clap(name = "FILE")]
    file: PathBuf,

    /// The file path to write the output Wasm module to.
    #[clap(short, long, parse(from_os_str))]
    output: PathBuf,

    /// Program name used when runtime doesn't provide it.
    /// Defaults to the name of the .wasm file.
    #[clap(short, long)]
    program_name: Option<OsString>,

    /// Arguments to preset for the program
    #[clap(name = "ARGS", last = true)]
    args: Vec<OsString>,
}

fn main() -> anyhow::Result<()> {
    let opt = Opt::parse();
    let mut module_config = walrus::ModuleConfig::new();
    module_config.strict_validate(false);
    let mut module = module_config.parse_file(&opt.file)?;

    let program_name = if let Some(program_name) = opt.program_name {
        program_name
    } else {
        let file_name = opt
            .file
            .file_name()
            .ok_or_else(|| anyhow::anyhow!("no file name in path: {:?}", opt.file))?;
        file_name.to_owned()
    };
    let preset_args = wasi_preset_args::PresetArgs::new(program_name, opt.args);
    preset_args.run(&mut module)?;

    module.emit_wasm_file(opt.output)?;
    Ok(())
}
