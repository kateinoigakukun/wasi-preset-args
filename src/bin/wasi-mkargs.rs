use std::path::PathBuf;

use structopt::StructOpt;

#[derive(StructOpt)]
pub struct Opt {
    #[structopt(long, value_name = "FILETYPE", possible_values = &["c", "obj"], default_value = "c")]
    emit: String,

    #[structopt(short = "o", parse(from_os_str))]
    output: PathBuf,

    #[structopt(long = "program-name")]
    default_arg0: String,

    #[structopt(name = "ARGS", last = true)]
    args: Vec<String>,
}

fn main() -> std::io::Result<()> {
    let opt = Opt::from_args();
    let c_src = wasi_preset_args::generate_c_source(&opt.default_arg0, &opt.args)?;

    match opt.emit.as_str() {
        "c" => {
            std::fs::write(opt.output, c_src)?;
        }
        "obj" => {
            let clang = std::env::var("CLANG").unwrap_or(String::from("clang"));
            let obj = wasi_preset_args::generate_obj(&c_src, &clang)?;
            std::fs::write(opt.output, obj)?;
        }
        _ => unreachable!("unexpected emit type: {}", opt.emit),
    }
    Ok(())
}
