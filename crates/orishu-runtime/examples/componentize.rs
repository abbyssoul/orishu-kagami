//! External-build helper: wrap a core module containing embedded WIT metadata.
//! This executes no guest. The runtime independently verifies the resulting bytes.
use std::{
    io::{Read, Write},
    path::PathBuf,
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let input = PathBuf::from(args.next().ok_or("expected core module path")?);
    let output = PathBuf::from(args.next().ok_or("expected new component output path")?);
    if args.next().is_some() {
        return Err("unexpected argument".into());
    }
    const MAX: u64 = 32 * 1024 * 1024;
    let file = std::fs::File::open(input)?;
    if !file.metadata()?.is_file() || file.metadata()?.len() > MAX {
        return Err("core module input exceeds file/byte policy".into());
    }
    let mut bytes = Vec::new();
    file.take(MAX + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX {
        return Err("core module grew beyond byte policy".into());
    }
    let component = wit_component::ComponentEncoder::default()
        .module(&bytes)?
        .validate(true)
        .encode()?;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)?;
    file.write_all(&component)?;
    file.sync_all()?;
    Ok(())
}
