use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::error::Error;
use crate::tool::run;

pub const GPU_ARCH: &str = "gfx1031";

pub fn hip(source: &Path) -> Result<PathBuf, Error> {
    let input_error = |error| Error::InputFile {
        path: source.to_owned(),
        source: error,
    };
    let file = fs::File::open(source).map_err(input_error)?;
    if !file.metadata().map_err(input_error)?.is_file() {
        return Err(Error::InputFormat {
            path: source.to_owned(),
        });
    }
    let arch = GPU_ARCH;
    let artifacts = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/hip");
    fs::create_dir_all(&artifacts).map_err(|source| Error::CreateDirectory {
        path: artifacts.clone(),
        source,
    })?;
    let filename = source.file_name().ok_or_else(|| Error::InputFormat {
        path: source.to_owned(),
    })?;
    let bundle = artifacts.join(filename).with_extension("bundle");
    let object = artifacts.join(filename).with_extension("hsaco");
    let bundler = Path::new("/opt/rocm/lib/llvm/bin/clang-offload-bundler");

    run(Command::new("/opt/rocm/bin/hipcc")
        .arg("--genco")
        .arg(format!("--offload-arch={arch}"))
        .arg("--rocm-path=/opt/rocm")
        .arg("--hip-path=/opt/rocm")
        .arg("-O1")
        .arg(source)
        .arg("-o")
        .arg(&bundle))?;

    let targets = run(Command::new(bundler)
        .args(["-type=o", "-list"])
        .arg(format!("-input={}", bundle.display())))?;
    let matching: Vec<_> = targets
        .lines()
        .map(str::trim)
        .filter(|target| target.starts_with("hip") && target.ends_with(&format!("-{arch}")))
        .collect();
    let [target] = matching.as_slice() else {
        return Err(Error::HipTarget {
            arch: arch.into(),
            targets: targets.lines().map(str::to_owned).collect(),
        });
    };

    run(Command::new(bundler)
        .args(["-type=o", "-unbundle"])
        .arg(format!("-targets={target}"))
        .arg(format!("-input={}", bundle.display()))
        .arg(format!("-output={}", object.display())))?;

    Ok(object)
}
