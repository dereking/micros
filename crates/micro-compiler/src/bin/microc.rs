use std::path::PathBuf;

fn main() {
    if let Err(code) = run() {
        std::process::exit(code);
    }
}

fn run() -> Result<(), i32> {
    let arguments: Vec<_> = std::env::args_os().collect();
    if arguments.len() != 3 {
        eprintln!("usage: microc <input.ts> <output.mbc>");
        return Err(1);
    }
    let input = PathBuf::from(&arguments[1]);
    let output = PathBuf::from(&arguments[2]);
    let source = std::fs::read_to_string(&input).map_err(|error| {
        eprintln!("microc: cannot read {}: {error}", input.display());
        1
    })?;
    let image =
        micro_compiler::compile_source(&input.to_string_lossy(), &source).map_err(|errors| {
            for error in errors {
                eprintln!("{error}");
            }
            2
        })?;
    let bytes = micro_ir::encode(&image).map_err(|error| {
        eprintln!("microc: cannot encode MBC: {error}");
        1
    })?;
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent).map_err(|error| {
            eprintln!("microc: cannot create {}: {error}", parent.display());
            1
        })?;
    }
    std::fs::write(&output, bytes).map_err(|error| {
        eprintln!("microc: cannot write {}: {error}", output.display());
        1
    })?;
    println!("compiled {} -> {}", input.display(), output.display());
    Ok(())
}
