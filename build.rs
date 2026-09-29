#![warn(clippy::all, clippy::pedantic, clippy::nursery)]

use std::{env, path::Path};

use shipfitter::{
	build::{configure_file, embed_commit_info, target_profile_dir},
	docs::{Page, convert},
	windows::{VersionInfo, embed_manifest},
};

fn main() {
	println!("cargo:rerun-if-changed=build.rs");
	println!("cargo:rerun-if-changed=Cargo.toml");
	println!("cargo:rerun-if-changed=sounds");
	println!("cargo:rerun-if-changed=doc");
	embed_commit_info("FEDRA");
	if let Some(target_dir) = target_profile_dir() {
		build_docs(&target_dir);
		if let Err(e) = configure_file(Path::new("fedra.iss.in"), &target_dir.join("fedra.iss"), &[]) {
			println!("cargo:warning=Failed to configure the installer script: {e}");
		}
	}
	if let Err(e) = embed_manifest("Fedra") {
		println!("cargo:warning=Failed to embed manifest: {e}");
	}
	let version_info = VersionInfo {
		product_name: "Fedra",
		company: "Quin Gillespie",
		copyright: "Copyright © 2026 Quin Gillespie",
		original_filename: "fedra.exe",
		..VersionInfo::default()
	};
	if let Err(e) = version_info.embed() {
		println!("cargo:warning=Failed to embed version info: {e}");
	}
	if env::var("CARGO_CFG_TARGET_OS").is_ok_and(|os| os == "windows") {
		delay_load_speech_dlls();
	}
}

fn build_docs(target_dir: &Path) {
	let page = Page { title: "Fedra Documentation", ..Page::default() };
	if let Err(e) = convert(Path::new("doc/readme.md"), &target_dir.join("readme.html"), &page) {
		println!("cargo:warning=Failed to generate the readme: {e}");
	}
}

/// Delay loads the screen reader DLLs a statically linked prism imports.
///
/// Cargo applies a link argument only to the crate that emits it, so prism-sys publishes the
/// list instead and the final link has to act on it. Without this they're hard imports, and
/// Fedra refuses to start on any machine that doesn't have every one of them installed.
fn delay_load_speech_dlls() {
	let Ok(dlls) = env::var("DEP_PRISMER_DELAY_LOAD_DLLS") else {
		println!("cargo:warning=prismer published no delay-load list; speech DLLs will be hard imports");
		return;
	};
	// LNK4199 is "you asked to delay load something nothing imports", which is exactly what
	// happens here: prism's Windows list also carries its Orca and speech-dispatcher bridges,
	// which are Linux. Delay loading a DLL with no imports does nothing, so the warning is noise.
	println!("cargo:rustc-link-arg=/IGNORE:4199");
	for dll in dlls.split(';').filter(|dll| !dll.is_empty()) {
		println!("cargo:rustc-link-arg=/DELAYLOAD:{dll}");
	}
}
