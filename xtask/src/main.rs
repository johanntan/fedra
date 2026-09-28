#![warn(clippy::all, clippy::pedantic, clippy::nursery)]

use std::{
	env,
	error::Error,
	path::{Path, PathBuf},
};

use shipfitter::package::cargo_build_release;

fn main() -> Result<(), Box<dyn Error>> {
	if env::args().nth(1).as_deref() == Some("release") {
		return release();
	}
	println!("Tasks:");
	println!("	release	Build release binaries and package them");
	Ok(())
}

fn project_root() -> PathBuf {
	Path::new(env!("CARGO_MANIFEST_DIR")).ancestors().nth(1).unwrap().to_path_buf()
}

fn release() -> Result<(), Box<dyn Error>> {
	let root = project_root();
	cargo_build_release(&root, &[])?;
	let target_dir = root.join("target/release");
	let exe_path = target_dir.join(if cfg!(windows) { "fedra.exe" } else { "fedra" });
	if !exe_path.exists() {
		return Err("Executable not found".into());
	}
	println!("Packaging binaries and docs...");
	package(&root, &target_dir, &exe_path)
}

#[cfg(not(target_os = "macos"))]
fn package(root: &Path, target_dir: &Path, exe_path: &Path) -> Result<(), Box<dyn Error>> {
	use shipfitter::{
		host_arch_suffix,
		package::{Zip, inno_setup},
	};

	let zip_path = target_dir.join(format!("fedra-{}.zip", host_arch_suffix()));
	let mut zip = Zip::create(&zip_path)?;
	zip.file(exe_path, &exe_path.file_name().unwrap().to_string_lossy())?;
	let readme_path = target_dir.join("readme.html");
	if readme_path.exists() {
		zip.file(&readme_path, "readme.html")?;
	} else {
		println!("Warning: readme.html not found, skipping.");
	}
	zip.dir(&root.join("sounds"), "sounds")?;
	zip.finish()?;
	println!("Created zip: {}", zip_path.display());
	if cfg!(windows) && inno_setup(&target_dir.join("fedra.iss"))? {
		println!("Installer created successfully.");
	}
	Ok(())
}

#[cfg(target_os = "macos")]
fn package(root: &Path, target_dir: &Path, exe_path: &Path) -> Result<(), Box<dyn Error>> {
	use shipfitter::{
		host_arch_suffix,
		macos::{MacApp, dmg, sign},
	};

	let version = package_version(root)?;
	let app = MacApp {
		name: "Fedra",
		identifier: "com.trypsynth.fedra",
		executable: "fedra",
		version: &version,
		..MacApp::default()
	};
	let readme_path = target_dir.join("readme.html");
	let sounds_path = root.join("sounds");
	let mut resources = vec![sounds_path.as_path()];
	if readme_path.exists() {
		resources.push(&readme_path);
	} else {
		println!("Warning: readme.html not found, skipping.");
	}
	let bundle = app.bundle(target_dir, exe_path, &[], &resources)?;
	sign(&bundle, &[])?;
	println!("Built app: {}", bundle.display());
	let dmg_path = target_dir.join(format!("fedra-{}.dmg", host_arch_suffix()));
	dmg(&bundle, &dmg_path)?;
	println!("Created DMG: {}", dmg_path.display());
	Ok(())
}

#[cfg(target_os = "macos")]
fn package_version(root: &Path) -> Result<String, Box<dyn Error>> {
	let manifest = std::fs::read_to_string(root.join("Cargo.toml"))?;
	manifest
		.lines()
		.find_map(|line| line.strip_prefix("version = \"")?.strip_suffix('"').map(str::to_string))
		.ok_or_else(|| "no version in Cargo.toml".into())
}
