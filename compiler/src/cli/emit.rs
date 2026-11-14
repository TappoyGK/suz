use super::{Failure, Result, assets, output::StagedDirectory};
use std::{fs, path::Path};

pub fn emit(c: &str, destination: &Path, environment: Option<&Path>) -> Result<()> {
    let failure = |error| {
        Failure::tool(
            2,
            "tool.publish-output",
            format!("{}: {error}", destination.display()),
        )
    };
    match fs::symlink_metadata(destination) {
        Ok(_) => {
            return Err(Failure::tool(
                2,
                "tool.output",
                format!("output path already exists: {}", destination.display()),
            ));
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(failure(e)),
    }
    let environment = environment
        .map(|path| {
            fs::read(path).map_err(|e| {
                Failure::tool(2, "tool.environment", format!("{}: {e}", path.display()))
            })
        })
        .transpose()?;
    let parent = destination
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let staged = StagedDirectory::new(parent).map_err(failure)?;
    unpack(c, staged.path(), environment.as_deref()).map_err(failure)?;
    staged.publish(destination).map_err(failure)
}

pub fn unpack(c: &str, directory: &Path, environment: Option<&[u8]>) -> std::io::Result<()> {
    for &(name, bytes) in assets::FILES {
        let target = directory.join(name);
        fs::create_dir_all(target.parent().unwrap())?;
        let bytes = if name == "runtime/suz_env.c" {
            environment.unwrap_or(bytes)
        } else {
            bytes
        };
        fs::write(&target, bytes)?;
        #[cfg(unix)]
        if name == "scripts/build-c.sh" {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&target, fs::Permissions::from_mode(0o755))?;
        }
    }
    fs::write(directory.join("program.c"), c)?;
    Ok(())
}
