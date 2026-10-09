use super::*;
use serde_json::{Value, json};

fn read(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}
fn write(path: &Path, value: &Value) {
    fs::write(path, serde_json::to_vec(value).unwrap()).unwrap();
}

// Begin with the independently exact v1 fixture, then replace only authoring
// references. Expected bundle bytes are computed before any source is changed.
fn symbolic(path: &Path) -> Package {
    let expected = source(path);
    let contracts: Vec<_> = declarations()
        .into_iter()
        .map(|(id, p)| {
            (
                id,
                serde_json::to_value(p.contract_ref(&Limits::default()).unwrap()).unwrap(),
            )
        })
        .collect();
    let mut manifest = read(&path.join("plugin.json"));
    manifest["apiVersion"] = json!("orishu.plugin-source/v2");
    for artifact in manifest["artifacts"].as_array_mut().unwrap() {
        artifact["localId"] = json!(artifact["path"].as_str().unwrap().trim_end_matches(".wasm"));
    }
    for contribution in manifest["contributions"].as_array().unwrap() {
        let file = path.join(contribution["path"].as_str().unwrap());
        let mut payload = read(&file);
        for req in payload["scientific"]["requirements"]
            .as_array_mut()
            .unwrap()
        {
            let local = contracts
                .iter()
                .find(|(_, exact)| *exact == req["contract"])
                .unwrap()
                .0;
            req["contract"] = json!({"localContribution":local});
        }
        if let Some(kernel) = payload["scientific"].get_mut("kernel") {
            let local = if *kernel == json!(ArtifactDigest::sha256_of(b"euler-kernel-fixture")) {
                "euler"
            } else {
                "classical"
            };
            *kernel = json!({"localArtifact":local});
        }
        write(&file, &payload);
    }
    manifest["contributions"].as_array_mut().unwrap().reverse();
    write(&path.join("plugin.json"), &manifest);
    expected
}

#[test]
fn symbolic_source_pack_is_identical_to_exact_v1_and_independent_of_manifest_order() {
    let dir = tempfile::tempdir().unwrap();
    let exact = symbolic(dir.path());
    let symbolic = Package::load(dir.path()).unwrap();
    assert_eq!(exact.release().id(), symbolic.release().id());
    assert_eq!(exact.pack().unwrap(), symbolic.pack().unwrap());
    let mut manifest = read(&dir.path().join("plugin.json"));
    manifest["contributions"].as_array_mut().unwrap().reverse();
    manifest["artifacts"].as_array_mut().unwrap().reverse();
    // Artifact aliases/paths are build inputs, never scientific identity.
    manifest["artifacts"][0]["localId"] = json!("renamed-code");
    let old_path = manifest["artifacts"][0]["path"]
        .as_str()
        .unwrap()
        .to_owned();
    fs::rename(dir.path().join(&old_path), dir.path().join("renamed.wasm")).unwrap();
    manifest["artifacts"][0]["path"] = json!("renamed.wasm");
    let old_alias = old_path.trim_end_matches(".wasm");
    for contribution in manifest["contributions"].as_array().unwrap() {
        let file = dir.path().join(contribution["path"].as_str().unwrap());
        let mut payload = read(&file);
        if payload["scientific"]["kernel"]["localArtifact"] == old_alias {
            payload["scientific"]["kernel"]["localArtifact"] = json!("renamed-code");
            write(&file, &payload);
        }
    }
    write(&dir.path().join("plugin.json"), &manifest);
    assert_eq!(
        exact.pack().unwrap(),
        Package::load(dir.path()).unwrap().pack().unwrap()
    );
}

#[test]
fn symbolic_source_refuses_cycles_missing_aliases_duplicate_aliases_and_v1_markers() {
    for case in [
        "cycle",
        "two-node-cycle",
        "missing",
        "opaque",
        "duplicate-artifact",
        "duplicate-contribution",
        "v1",
        "missing-kernel",
    ] {
        let dir = tempfile::tempdir().unwrap();
        symbolic(dir.path());
        let mut manifest = read(&dir.path().join("plugin.json"));
        let id = manifest["contributions"][0]["localId"]
            .as_str()
            .unwrap()
            .to_owned();
        let file = dir
            .path()
            .join(manifest["contributions"][0]["path"].as_str().unwrap());
        let mut payload = read(&file);
        match case {
            "cycle" | "two-node-cycle" | "missing" | "opaque" => {
                let second = manifest["contributions"][1]["localId"].as_str().unwrap();
                let target = if case == "cycle" {
                    id.as_str()
                } else if case == "two-node-cycle" {
                    second
                } else {
                    "absent"
                };
                payload["scientific"]["requirements"] =
                    json!([{"slot":"loop","contract":{"localContribution":target}}]);
                if case == "two-node-cycle" {
                    let file = dir
                        .path()
                        .join(manifest["contributions"][1]["path"].as_str().unwrap());
                    let mut other = read(&file);
                    other["scientific"]["requirements"] =
                        json!([{"slot":"loop","contract":{"localContribution":id}}]);
                    write(&file, &other);
                }
                if case == "opaque" {
                    fs::write(dir.path().join("opaque.bin"), b"opaque").unwrap();
                    manifest["contributions"].as_array_mut().unwrap().push(json!({"localId":"absent","extensionPoint":"org.example.future/v1","path":"opaque.bin"}));
                }
            }
            "duplicate-artifact" => {
                manifest["artifacts"][1]["localId"] = manifest["artifacts"][0]["localId"].clone()
            }
            "duplicate-contribution" => manifest["contributions"][1]["localId"] = json!(id),
            "v1" => manifest["apiVersion"] = json!("orishu.plugin-source/v1"),
            "missing-kernel" => {
                let integrator = manifest["contributions"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|c| c["extensionPoint"] == KnownPoint::Integrators.as_str())
                    .unwrap();
                let file = dir.path().join(integrator["path"].as_str().unwrap());
                let mut integrator = read(&file);
                integrator["scientific"]["kernel"] = json!({"localArtifact":"absent"});
                write(&file, &integrator);
            }
            _ => unreachable!(),
        }
        if case != "missing-kernel" {
            write(&file, &payload);
        }
        write(&dir.path().join("plugin.json"), &manifest);
        let expected = if matches!(case, "cycle" | "two-node-cycle" | "missing" | "opaque") {
            Code::InvalidSelection
        } else {
            Code::Malformed
        };
        assert_eq!(
            Package::load(dir.path()).unwrap_err().code,
            expected,
            "{case}"
        );
    }
}

#[test]
fn symbolic_source_propagates_changed_contract_and_code_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let original = symbolic(dir.path());
    let mut manifest = read(&dir.path().join("plugin.json"));
    let component = manifest["contributions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["localId"] == "mass")
        .unwrap();
    let file = dir.path().join(component["path"].as_str().unwrap());
    let mut mass = read(&file);
    mass["scientific"]["properties"][0]["schema"]["defaultExpression"] = json!("3 kg");
    write(&file, &mass);
    let changed = Package::load(dir.path()).unwrap();
    assert_ne!(changed.release().id(), original.release().id());
    let mass_contract = changed.release().payloads()[&"mass".parse().unwrap()]
        .payload()
        .unwrap()
        .contract_ref(&Limits::default())
        .unwrap();
    let model = changed
        .release()
        .payloads()
        .values()
        .filter_map(|p| p.payload())
        .find(|p| p.point() == KnownPoint::FieldModels)
        .unwrap();
    assert_eq!(
        model
            .requirements()
            .iter()
            .find(|r| r.slot.as_str() == "mass")
            .unwrap()
            .contract,
        mass_contract
    );
    fs::write(
        dir.path().join("classical.wasm"),
        b"changed kernel; never executed by packing",
    )
    .unwrap();
    let code_changed = Package::load(dir.path()).unwrap();
    assert_ne!(changed.release().id(), code_changed.release().id());
    let model = code_changed
        .release()
        .payloads()
        .values()
        .filter_map(|p| p.payload())
        .find(|p| p.point() == KnownPoint::FieldModels)
        .unwrap();
    let Payload::FieldModels(model) = model else {
        unreachable!()
    };
    assert_eq!(
        model.scientific.kernel,
        ArtifactDigest::sha256_of(b"changed kernel; never executed by packing")
    );
    // Unknown points remain uninterpreted, including text resembling a marker.
    fs::write(
        dir.path().join("opaque.bin"),
        br#"{"localContribution":"missing"}"#,
    )
    .unwrap();
    manifest["contributions"].as_array_mut().unwrap().push(
        json!({"localId":"future","extensionPoint":"org.example.future/v1","path":"opaque.bin"}),
    );
    write(&dir.path().join("plugin.json"), &manifest);
    assert!(Package::load(dir.path()).is_ok());
}

#[test]
fn cli_packs_validates_and_installs_symbolic_source_without_ambient_inventory_lookup() {
    let _process_isolation = isolate_process_spawns();
    let dir = tempfile::tempdir().unwrap();
    let expected = symbolic(dir.path());
    let output = dir.path().join("output.okplugin");
    let inventory = dir.path().join("inventory");
    let run = |args: &[&std::ffi::OsStr]| {
        let result = Process::new(env!("CARGO_BIN_EXE_kagami"))
            .args(["plugin", "--json", "--directory"])
            .arg(&inventory)
            .args(args)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stdout)
        );
        serde_json::from_slice::<Value>(&result.stdout).unwrap()
    };
    let report = run(&["validate".as_ref(), dir.path().as_os_str()]);
    assert_eq!(report["result"]["release"], json!(expected.release().id()));
    assert!(!inventory.exists());
    run(&[
        "pack".as_ref(),
        dir.path().as_os_str(),
        "--output".as_ref(),
        output.as_os_str(),
    ]);
    assert_eq!(fs::read(&output).unwrap(), expected.pack().unwrap());
    assert!(!inventory.exists());
    run(&["install".as_ref(), output.as_os_str()]);
    assert!(inventory.exists());
    let before = run(&["list".as_ref(), "--all-releases".as_ref()]);
    let mut manifest = read(&dir.path().join("plugin.json"));
    manifest["artifacts"][1]["localId"] = manifest["artifacts"][0]["localId"].clone();
    write(&dir.path().join("plugin.json"), &manifest);
    let refused_output = dir.path().join("refused.okplugin");
    let result = Process::new(env!("CARGO_BIN_EXE_kagami"))
        .args(["plugin", "--json", "--directory"])
        .arg(&inventory)
        .arg("pack")
        .arg(dir.path())
        .arg("--output")
        .arg(&refused_output)
        .output()
        .unwrap();
    assert!(!result.status.success());
    let report: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(report["error"]["code"], "Malformed");
    assert!(!refused_output.exists());
    assert_eq!(run(&["list".as_ref(), "--all-releases".as_ref()]), before);
}
