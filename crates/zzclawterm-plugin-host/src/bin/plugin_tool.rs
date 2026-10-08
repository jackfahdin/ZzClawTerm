use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use zzclawterm_plugin_host::manager::PluginManager;
use zzclawterm_plugin_host::package::{StagingDirectory, load, snapshot};
use zzclawterm_plugin_host::runtime::RuntimeLimits;

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    match arguments.as_slice() {
        [operation, input, output] if operation == "component" => {
            let module = fs::read(input)?;
            let component = wit_component::ComponentEncoder::default().module(&module)?.validate(true).encode()?;
            zzclawterm_plugin_host::runtime::validate_api_marker(&component)?;
            fs::write(output, component)?;
            println!("Built component: {output}");
        }
        [operation, input] if operation == "validate" => validate(Path::new(input))?,
        [operation, input, output] if operation == "pack" => {
            validate(Path::new(input))?;
            let temporary = temporary_root()?;
            let result = (|| {
                let stage = StagingDirectory::new(&temporary)?;
                snapshot(Path::new(input), &stage.path)?;
                let mut writer = zip::ZipWriter::new(fs::File::create(output)?);
                write_archive(&stage.path, &stage.path, &mut writer)?;
                writer.finish()?;
                Ok::<_, Box<dyn std::error::Error>>(())
            })();
            fs::remove_dir_all(&temporary)?;
            result?;
            println!("Packaged plugin: {output}");
        }
        _ => return Err("Usage: zzclawterm-plugin-tool component <core.wasm> <component.wasm> | validate <directory-or.zip> | pack <directory-or.zip> <output.zip>".into()),
    }
    Ok(())
}

fn temporary_root() -> Result<PathBuf, std::io::Error> {
    let root =
        std::env::temp_dir().join(format!("zzclawterm-plugin-tool-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&root)?;
    Ok(root)
}

fn validate(input: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let root = temporary_root()?;
    let result = (|| {
        let mut manager = PluginManager::open(
            root.join("host"),
            env!("CARGO_PKG_VERSION"),
            RuntimeLimits::default(),
        )?;
        let id = manager.install(input, false)?;
        let manifest = load(
            &root.join("host/installed").join(id),
            env!("CARGO_PKG_VERSION"),
        )?
        .manifest;
        println!(
            "Validated {} {} (API {}, {} actions)",
            manifest.id,
            manifest.version,
            manifest.api_version,
            manifest.actions.len()
        );
        Ok::<_, Box<dyn std::error::Error>>(())
    })();
    fs::remove_dir_all(&root)?;
    result
}

fn write_archive(
    root: &Path,
    path: &Path,
    writer: &mut zip::ZipWriter<fs::File>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut entries = fs::read_dir(path)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            write_archive(root, &path, writer)?;
        } else {
            let name = path
                .strip_prefix(root)?
                .to_str()
                .ok_or("Non-UTF8 resource path")?
                .replace('\\', "/");
            writer.start_file(
                name,
                zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Deflated),
            )?;
            writer.write_all(&fs::read(path)?)?;
        }
    }
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
