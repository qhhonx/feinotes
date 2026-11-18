//! Embed the files under assets/ that the app reads into the binary (see
//! src/assets.rs); SKIP lists the rest.

use std::fmt::Write;
use std::path::Path;

fn main() {
    let manifest = std::path::PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let root = if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("ios") {
        manifest.join("../..").canonicalize().expect("find Fei Notes project root")
    } else { manifest };
    let linux = matches!(std::env::var("CARGO_CFG_TARGET_OS").as_deref(), Ok("linux" | "freebsd"));
    let skip: Vec<&str> = SKIP.iter().copied().chain((!linux).then_some("fonts/emoji/")).collect();
    embed(&root, "assets", "assets.rs", &skip);
    embed(&root, "shared-mock/library", "guides.rs", &[]);
    println!("cargo:rustc-env=FEI_TESTFLIGHT_BETA=0");
    println!("cargo:rustc-env=FEI_UPSTREAM_PREVIEW=0");
    let revision = std::process::Command::new("git").arg("-C").arg(&root).args(["rev-parse", "--short=12", "HEAD"])
        .output().ok().filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned()).unwrap_or_else(|| "unknown".into());
    println!("cargo:rustc-env=FEI_SOURCE_REVISION={revision}");
    // A candidate may advance without changing Rust files. Do not cache its old revision.
    let branch = std::process::Command::new("git").arg("-C").arg(&root).args(["symbolic-ref", "--quiet", "HEAD"])
        .output().ok().filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned());
    for name in [Some("HEAD"), Some("packed-refs"), branch.as_deref()].into_iter().flatten() {
        if let Ok(output) = std::process::Command::new("git").arg("-C").arg(&root)
            .args(["rev-parse", "--path-format=absolute", "--git-path", name]).output() && output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout);
            if Path::new(path.trim()).exists() { println!("cargo:rerun-if-changed={}", path.trim()); }
        }
    }
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("ios")
        && std::env::var_os("CARGO_FEATURE_CLOUDKIT_SYNC").is_some() {
        println!("cargo:rerun-if-env-changed=FEI_IOS_PRODUCT_CLOUD");
        println!("cargo:rerun-if-env-changed=FEI_TESTFLIGHT_BETA");
        println!("cargo:rerun-if-env-changed=FEI_IOS_UPSTREAM_PREVIEW");
        let product = std::env::var("FEI_IOS_PRODUCT_CLOUD").as_deref() == Ok("1");
        let preview = std::env::var("FEI_IOS_UPSTREAM_PREVIEW").as_deref() == Ok("1");
        let beta = std::env::var("FEI_TESTFLIGHT_BETA").unwrap_or_else(|_| "0".into());
        assert!(matches!(beta.as_str(), "0" | "1"), "Invalid FEI_TESTFLIGHT_BETA");
        assert!(beta != "1" || (product && std::env::var("PROFILE").as_deref() == Ok("release")
            && std::env::var_os("CARGO_FEATURE_DEVTOOLS").is_none()
            && std::env::var_os("CARGO_FEATURE_MOBILE_QA").is_none()), "TestFlight requires a Production release without QA");
        println!("cargo:rustc-env=FEI_TESTFLIGHT_BETA={beta}");
        assert!(!preview || (!product && beta == "0"), "Upstream previews cannot use the production or TestFlight configuration");
        println!("cargo:rustc-env=FEI_UPSTREAM_PREVIEW={}", if preview { "1" } else { "0" });
        println!("cargo:rustc-env=FEI_CLOUD_NAMESPACE={}", if preview { "Development:iCloud.local.feinotes.upstream-preview" }
            else if product { "Production:iCloud.app.feinotes.mac" } else { "Development:iCloud.app.quietmd.cloudlab" });
    }
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") { windows_resources(); }
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos")
        && std::env::var_os("CARGO_FEATURE_CLOUDKIT_RESTORE").is_some() {
        cloudkit_restore();
    }
}

fn cloudkit_restore() {
    let source = "packaging/macos/CloudRestore.swift";
    let sync = "packaging/macos/CloudSync.swift";
    let share = "packaging/macos/CloudShare.swift";
    let subscription = "packaging/macos/CloudSubscription.swift";
    let trial = "packaging/macos/CloudTrial.swift";
    let testflight_install = "packaging/macos/TestFlightInstall.swift";
    let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    let arch = match std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() {
        Ok("aarch64") => "arm64",
        Ok("x86_64") => "x86_64",
        _ => panic!("Unsupported macOS architecture"),
    };
    println!("cargo:rerun-if-changed={source}");
    println!("cargo:rerun-if-changed={sync}");
    println!("cargo:rerun-if-changed={share}");
    println!("cargo:rerun-if-changed={subscription}");
    println!("cargo:rerun-if-changed={trial}");
    println!("cargo:rerun-if-changed={testflight_install}");
    println!("cargo:rerun-if-env-changed=FEI_CLOUD_CONTAINER");
    println!("cargo:rerun-if-env-changed=FEI_CLOUD_ENVIRONMENT");
    println!("cargo:rerun-if-env-changed=FEI_BUNDLE_ID");
    println!("cargo:rerun-if-env-changed=FEI_TESTFLIGHT_BETA");
    let container = std::env::var("FEI_CLOUD_CONTAINER").unwrap_or_else(|_| "iCloud.app.quietmd.cloudlab".into());
    let environment = std::env::var("FEI_CLOUD_ENVIRONMENT").unwrap_or_else(|_| "Development".into());
    let bundle = std::env::var("FEI_BUNDLE_ID").unwrap_or_else(|_| "app.quietmd.cloudlab".into());
    let identifier = |name: &str| name.split('.').all(|part| !part.is_empty()
        && part.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'-'));
    assert!(container.strip_prefix("iCloud.").is_some_and(identifier), "Invalid FEI_CLOUD_CONTAINER");
    assert!(bundle.contains('.') && identifier(&bundle), "Invalid FEI_BUNDLE_ID");
    assert!(matches!(environment.as_str(), "Development" | "Production"), "Invalid FEI_CLOUD_ENVIRONMENT");
    let qa = std::env::var_os("CARGO_FEATURE_DEVTOOLS").is_some()
        || std::env::var_os("CARGO_FEATURE_CLOUDKIT_SYNC").is_none();
    assert!(environment != "Production" || (!qa && container != "iCloud.app.quietmd.cloudlab"),
        "Production requires a release container and a build without QA");
    let testflight = match std::env::var("FEI_TESTFLIGHT_BETA").as_deref() {
        Err(std::env::VarError::NotPresent) => false,
        Ok("1") => true,
        _ => panic!("Invalid FEI_TESTFLIGHT_BETA"),
    };
    assert!(!testflight || (!qa && environment == "Production"
        && bundle == "app.feinotes.mac" && container == "iCloud.app.feinotes.mac"),
        "TestFlight requires the Fei Notes production bundle and container without QA");
    println!("cargo:rustc-env=FEI_TESTFLIGHT_BETA={}", if testflight { "1" } else { "0" });
    println!("cargo:rustc-env=FEI_CLOUD_NAMESPACE={environment}:{container}");
    println!("cargo:rustc-env=FEI_BUNDLE_ID={bundle}");
    let config = out.join("FeiCloudConfig.swift");
    std::fs::write(&config, format!("let containerID = \"{container}\"\nlet cloudEnvironment = \"{environment}\"\n"))
        .expect("write CloudKit build settings");
    let mut command = std::process::Command::new("xcrun");
    command.args([
        "swiftc", "-swift-version", "6", "-O", "-emit-library", "-module-name", "QuietCloudRestore",
        "-target", &format!("{arch}-apple-macos14.0"), "-module-cache-path", &out.join("swift-cache").to_string_lossy(),
        "-Xlinker", "-install_name", "-Xlinker", "@rpath/libQuietCloudRestore.dylib",
        source, sync, share, subscription, trial, testflight_install,
    ]).arg(config);
    if qa { command.arg("-DFEI_CLOUD_QA"); }
    if testflight { command.arg("-DFEI_TESTFLIGHT_BETA"); }
    let status = command.arg("-o").arg(out.join("libQuietCloudRestore.dylib"))
        .status().expect("swiftc is required for cloudkit-restore");
    assert!(status.success(), "CloudKit restore bridge failed to compile");
    println!("cargo:rustc-link-search=native={}", out.display());
    println!("cargo:rustc-link-lib=dylib=QuietCloudRestore");
    println!("cargo:rustc-link-arg-bin=quiet_md=-Wl,-rpath,@executable_path/../Frameworks");
}

fn windows_resources() {
    let root = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let icon = root.join("assets/branding/app-icon.ico").to_string_lossy().replace('\\', "/");
    let version = std::env::var("CARGO_PKG_VERSION").unwrap();
    let numbers = format!("{},0", version.replace('.', ","));
    let rc = format!(r#"1 ICON "{icon}"
1 VERSIONINFO
FILEVERSION {numbers}
PRODUCTVERSION {numbers}
FILEOS 0x40004
FILETYPE 0x1
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040904B0"
    BEGIN
      VALUE "FileDescription", "Quiet.md\0"
      VALUE "FileVersion", "{version}\0"
      VALUE "ProductName", "Quiet.md\0"
      VALUE "ProductVersion", "{version}\0"
      VALUE "OriginalFilename", "quiet_md.exe\0"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x409, 1200
  END
END
"#);
    let source = out.join("quiet-md.rc");
    let resource = out.join("quiet-md.res");
    std::fs::write(&source, rc).unwrap();
    let status = std::process::Command::new("rc.exe").arg("/nologo").arg("/fo").arg(&resource).arg(&source).status().expect("Windows SDK rc.exe is required; build from a Visual Studio developer shell");
    assert!(status.success(), "Windows resource compilation failed");
    println!("cargo:rustc-link-arg-bin=quiet_md={}", resource.display());
}

/// Files under assets/ the binary never reads: source art, the preview's dev
/// icon (scripts/preview.sh), the Windows icon (linked as a resource below and
/// used by the installer), themes (`include_str!` in src/themes.rs) and notes.
const SKIP: &[&str] = &["branding/hamster-source.png", "branding/quiet-q/", "branding/app-icon-dev.png", "branding/app-icon.ico", "themes/", "onboarding/README.md"];

fn embed(project: &Path, source: &str, output: &str, skip: &[&str]) {
    let root = project.join(source);
    println!("cargo:rerun-if-changed={}", root.display());
    let mut files = vec![];
    collect(&root, &root, &mut files);
    let fei_notes = matches!(std::env::var("CARGO_CFG_TARGET_OS").as_deref(), Ok("macos" | "ios"))
        && std::env::var_os("CARGO_FEATURE_CLOUDKIT_SYNC").is_some();
    if source == "assets" && fei_notes {
        for (path, file) in files.iter().filter(|(path, _)| path.starts_with("icons/")) {
            assert!(path != "icons/LICENSE-nucleo" && (!path.ends_with(".svg")
                || !std::fs::read_to_string(file).unwrap().to_ascii_lowercase().contains("nucleo")),
                "Replace the upstream Nucleo resource before bundling Fei Notes: {path}");
        }
        let icon = root.join("../packaging/macos/fei-notes.png");
        println!("cargo:rerun-if-changed={}", icon.display());
        files.retain(|(path, _)| !path.starts_with("branding/"));
        files.push(("branding/app-icon.png".into(), icon.to_string_lossy().into_owned()));
    }
    files.retain(|(rel, _)| !skip.iter().any(|s| if s.ends_with('/') { rel.starts_with(s) } else { rel == s }));
    files.sort();
    let mut out = String::new();
    if source == "shared-mock/library" && fei_notes {
        // The old guide uses Quiet.md branding and screenshots. Fei Notes starts
        // with an empty SQLite library; retain the legacy fixtures only for tests.
        out.push_str("#[cfg(not(test))]\npub static FILES: &[(&str, &[u8])] = &[];\n#[cfg(test)]\n");
    }
    out.push_str("pub static FILES: &[(&str, &[u8])] = &[\n");
    for (rel, abs) in files {
        writeln!(out, "    ({rel:?}, include_bytes!({abs:?})),").unwrap();
    }
    out.push_str("];\n");
    let dest = Path::new(&std::env::var("OUT_DIR").unwrap()).join(output);
    std::fs::write(dest, out).unwrap();
}

fn collect(root: &Path, dir: &Path, out: &mut Vec<(String, String)>) {
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() {
            collect(root, &p, out);
        } else {
            let rel = p.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/");
            out.push((rel, p.to_string_lossy().into_owned()));
        }
    }
}
