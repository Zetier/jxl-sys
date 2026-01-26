use anyhow::{Result, anyhow, bail};
use cmake::Config;
use regex::Regex;
use semver::Version;
use std::{env, ffi::OsStr, fs, path::PathBuf, thread};

const PREFIX: &str = "libjxl-";

fn validate_version() -> Result<()> {
    let crate_ver = Version::parse(&env::var("CARGO_PKG_VERSION")?)?;

    if !crate_ver.build.starts_with(PREFIX) {
        bail!(
            "expected build-metadata `+libjxl-x.y.z`, got `+{}`",
            crate_ver.build
        );
    }
    let libjxl_meta_str = &crate_ver.build[PREFIX.len()..];
    let libjxl_meta = Version::parse(libjxl_meta_str)?;

    let cmake_txt = fs::read_to_string("libjxl/lib/CMakeLists.txt")?;
    let cap = |var: &str| {
        Regex::new(&format!(r#"set\({var}\s+(\d+)\)"#))?
            .captures(&cmake_txt)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str())
            .ok_or(anyhow!("`{}` not found in CMakeLists.txt", var))
    };
    let upstream_str = format!(
        "{}.{}.{}",
        cap("JPEGXL_MAJOR_VERSION")?,
        cap("JPEGXL_MINOR_VERSION")?,
        cap("JPEGXL_PATCH_VERSION")?,
    );
    let upstream = Version::parse(&upstream_str)?;

    if libjxl_meta != upstream {
        bail!(
            "version mismatch: crate metadata says libjxl={} but upstream is {}",
            libjxl_meta,
            upstream
        );
    }

    Ok(())
}

fn main() -> Result<()> {
    println!("cargo:rerun-if-changed=libjxl/");
    println!("cargo:rustc-link-lib=static=jxl");
    println!("cargo:rustc-link-lib=static=jxl_cms");
    println!("cargo:rustc-link-lib=static=jxl_threads");
    println!("cargo:rustc-link-lib=static=hwy");
    println!("cargo:rustc-link-lib=static=brotlidec");
    println!("cargo:rustc-link-lib=static=brotlienc");
    println!("cargo:rustc-link-lib=static=brotlicommon");

    #[cfg(target_os = "macos")]
    println!("cargo:rustc-link-lib=c++");
    #[cfg(target_os = "linux")]
    println!("cargo:rustc-link-lib=dylib=stdc++");

    validate_version()?;

    let mut cfg = Config::new("libjxl");
    if cfg!(all(target_os = "windows", target_env = "msvc")) {
        // Force Release for MSVC multi-config builds to avoid unoptimized RelWithDebInfo output.
        let profile = env::var("PROFILE").unwrap_or_default();
        let cmake_profile = if profile == "debug" {
            "Debug"
        } else {
            "Release"
        };
        cfg.profile(cmake_profile);
        cfg.define("CMAKE_C_FLAGS_RELEASE", "/O2 /Ob2 /DNDEBUG");
        cfg.define("CMAKE_CXX_FLAGS_RELEASE", "/O2 /Ob2 /DNDEBUG");
    }

    let dst = cfg
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
    println!(
        "cargo:rustc-link-search=native={}",
        dst.join("lib").display()
    );
    println!(
        "cargo:rustc-link-search=native={}",
        dst.join("lib64").display()
    );

    let include_jxl = include.join("jxl");
    let mut bindings = bindgen::Builder::default()
        .allowlist_item("Jxl.*")
        .clang_arg(format!("-I{}", include.display()))
        .default_enum_style(bindgen::EnumVariation::Rust {
            non_exhaustive: true,
        })
        .derive_default(true)
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
        .write_to_file(PathBuf::from(env::var("OUT_DIR")?).join("bindings.rs"))?;

    Ok(())
}
