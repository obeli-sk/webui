use sha2::{Digest, Sha256};
use std::{env, fs};
use wit_component::DecodedWasm;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args_os().skip(1);
    let wasm_path = args.next().ok_or("expected WASM component path")?;
    let config_path = args.next().ok_or("expected output config path")?;
    if args.next().is_some() {
        return Err("expected exactly two arguments".into());
    }

    let wasm = fs::read(wasm_path)?;
    let DecodedWasm::Component(resolve, world_id) = wit_component::decode(&wasm)? else {
        return Err("input is not a WASM component".into());
    };
    let world = &resolve.worlds[world_id];
    let imports: Vec<_> = world
        .imports
        .keys()
        .map(|key| resolve.name_world_key(key))
        .collect();
    let exports: Vec<_> = world
        .exports
        .keys()
        .map(|key| resolve.name_world_key(key))
        .collect();
    let digest = format!("sha256:{:x}", Sha256::digest(&wasm));
    let config = serde_json::json!({
        "created": chrono::Utc::now(),
        "author": null,
        "architecture": "wasm",
        "os": "wasip2",
        "layerDigests": [digest],
        "component": { "exports": exports, "imports": imports, "target": null },
    });
    fs::write(config_path, serde_json::to_vec(&config)?)?;
    Ok(())
}
