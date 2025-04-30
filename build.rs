use anyhow::{Result, anyhow, bail, ensure};
use cmake::Config;
use flate2::read::GzDecoder;
use regex::Regex;
use semver::Version;
use std::{
    env,
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
    process::Command,
    thread,
};
use tar::Archive;

const PREFIX: &str = "libjxl-";

fn extract_crate_libjxl_version() -> Result<semver::Version> {
    let crate_ver = Version::parse(&env::var("CARGO_PKG_VERSION")?)?;

    if !crate_ver.build.starts_with(PREFIX) {
        bail!(
            "expected build-metadata `+libjxl-x.y.z`, got `+{}`",
            crate_ver.build
        );
    }
    let libjxl_meta_str = &crate_ver.build[PREFIX.len()..];
    Ok(Version::parse(libjxl_meta_str)?)
}

fn extract_downloaded_libjxl_version(source_path: &Path) -> Result<semver::Version> {
    let cmake_txt = fs::read_to_string(source_path.join("lib/CMakeLists.txt"))?;
    let cap = |var: &str| {
        Regex::new(&format!(r#"set\({}\s+(\d+)\)"#, var))?
            .captures(&cmake_txt)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str())
            .ok_or(anyhow!("`{}` not found in CMakeLists.txt", var))
    };
    let downloaded_version_str = format!(
        "{}.{}.{}",
        cap("JPEGXL_MAJOR_VERSION")?,
        cap("JPEGXL_MINOR_VERSION")?,
        cap("JPEGXL_PATCH_VERSION")?,
    );
    Ok(Version::parse(&downloaded_version_str)?)
}

fn get_libjxl_source(version: &semver::Version, out_dir: &Path) -> Result<PathBuf> {
    let url = format!(
        "https://github.com/libjxl/libjxl/archive/refs/tags/v{}.{}.{}.tar.gz",
        version.major, version.minor, version.patch
    );
    let mut resp = ureq::get(&url).call()?;
    let mut decoder = GzDecoder::new(resp.body_mut().as_reader());
    let mut archive = Archive::new(&mut decoder);
    archive.unpack(out_dir)?;
    let libjxl_dir = out_dir.join(format!(
        "libjxl-{}.{}.{}",
        version.major, version.minor, version.patch
    ));
    Command::new(libjxl_dir.join("deps.sh")).spawn()?.wait()?;

    Ok(libjxl_dir)
}

fn main() -> Result<()> {
    let out_dir = PathBuf::from(env::var("OUT_DIR")?);
    let libjxl_version = extract_crate_libjxl_version()?;
    let libjxl_source = get_libjxl_source(&libjxl_version, &out_dir)?;
    ensure!(
        extract_downloaded_libjxl_version(&libjxl_source)? == libjxl_version,
        "version mismatch between crate and downloaded libjxl"
    );

    println!("cargo:rerun-if-changed={}", libjxl_source.display());
    println!("cargo:rustc-link-lib=static=jxl");
    println!("cargo:rustc-link-lib=dylib=stdc++");

    let dst = Config::new(libjxl_source)
        .define("BUILD_TESTING", "OFF")
        .define("BUILD_SHARED_LIBS", "OFF")
        .define("JPEGXL_ENABLE_BENCHMARK", "OFF")
        .define("JPEGXL_ENABLE_EXAMPLES", "OFF")
        .define("JPEGXL_ENABLE_DOXYGEN", "OFF")
        .define("JPEGXL_ENABLE_OPENEXR", "OFF")
        .define("JPEGXL_BUNDLE_LIBPNG", "OFF")
        .define("JPEGXL_ENABLE_JNI", "OFF")
        .define("JPEGXL_ENABLE_JPEGLI", "OFF")
        .define("JPEGXL_ENABLE_MANPAGES", "OFF")
        .define("JPEGXL_ENABLE_SJPEG", "OFF")
        .define("JPEGXL_ENABLE_TOOLS", "OFF")
        .env(
            "CMAKE_BUILD_PARALLEL_LEVEL",
            format!("{}", thread::available_parallelism()?),
        )
        .build();

    let include = dst.join("include");
    let lib = dst.join("lib");
    println!("cargo:rustc-link-search=native={}", lib.display());

    let include_jxl = include.join("jxl");
    let mut bindings = bindgen::Builder::default()
        .allowlist_item("Jxl.*")
        .clang_arg(format!("-I{}", include.display()))
        .default_enum_style(bindgen::EnumVariation::Rust {
            non_exhaustive: true,
        })
        .generate_comments(true)
        .use_core();

    for entry in include_jxl.read_dir()? {
        let entry = entry?;
        let is_valid_c_header = entry.file_type()?.is_file()
            && entry.path().extension().and_then(OsStr::to_str) == Some("h")
            && !entry
                .path()
                .file_stem()
                .and_then(OsStr::to_str)
                .map(|stem| stem.ends_with("_cxx"))
                .unwrap_or(true);
        if is_valid_c_header {
            bindings = bindings.header(entry.path().to_string_lossy());
        }
    }

    bindings
        .generate()?
        .write_to_file(out_dir.join("bindings.rs"))?;

    Ok(())
}
