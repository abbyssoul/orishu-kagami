//! Developer tool: package separately compiled reference Components using public
//! release/bundle contracts. No installation or scientific execution occurs.
use orishu_plugin::{ArtifactDigest, ExecutionContractId, Limits, bundle};
use orishu_runtime::{Sandbox, SandboxLimits};
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};
#[allow(dead_code)] // Channel bindings are consumed by tests/guests, not packaging.
#[path = "../../../plugins/reference/declarations.rs"]
mod declarations;

fn read_component(path: &Path) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let max = SandboxLimits::default().component_bytes as u64;
    let file = fs::File::open(path)?;
    if !file.metadata()?.is_file() || file.metadata()?.len() > max {
        return Err("expected bounded regular Component file".into());
    }
    let mut bytes = Vec::new();
    file.take(max + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > max {
        return Err("Component grew beyond byte budget".into());
    }
    Ok(bytes)
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let gravity = PathBuf::from(args.next().ok_or("expected Newtonian Component path")?);
    let euler = PathBuf::from(args.next().ok_or("expected Euler Component path")?);
    let directory = PathBuf::from(args.next().ok_or("expected existing output directory")?);
    if args.next().is_some() {
        return Err("unexpected argument".into());
    }
    if !directory.is_dir() {
        return Err("output directory must already exist".into());
    }
    let gravity = read_component(&gravity)?;
    let euler = read_component(&euler)?;
    let host = Sandbox::new(SandboxLimits::default())?;
    host.compile(
        &gravity,
        ArtifactDigest::sha256_of(&gravity),
        ExecutionContractId::Field,
    )?;
    host.compile(
        &euler,
        ArtifactDigest::sha256_of(&euler),
        ExecutionContractId::Dynamics,
    )?;
    let mut packages = Vec::new();
    for (name, payloads, code) in [
        ("vocabulary", declarations::vocabulary(), vec![]),
        (
            "solvers",
            vec![
                (
                    "newtonian".parse()?,
                    declarations::newtonian(ArtifactDigest::sha256_of(&gravity)),
                ),
                (
                    "euler".parse()?,
                    declarations::euler(ArtifactDigest::sha256_of(&euler)),
                ),
            ],
            vec![gravity.as_slice(), euler.as_slice()],
        ),
    ] {
        let (release, blobs) =
            declarations::release(&format!("org.orishu.reference.{name}"), payloads, &code)?;
        let borrowed: BTreeMap<_, _> = blobs
            .iter()
            .map(|(id, bytes)| (*id, bytes.as_slice()))
            .collect();
        let bytes = bundle::pack(&release, &borrowed, &Limits::default(), Default::default())?;
        let verified = bundle::read(&bytes, &Limits::default(), Default::default())?;
        let path = directory.join(format!("{name}.okplugin"));
        let id = verified.release().id();
        packages.push((path, id, bytes));
    }
    // All input/ABI/package checks precede output. Never replace an existing file.
    // Separate outputs are not a multi-file transaction; an IO failure can leave a
    // completed first package. Report only files that were fully written/synced.
    for (path, id, bytes) in packages {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        println!("{} {}", id, path.display());
    }
    Ok(())
}
