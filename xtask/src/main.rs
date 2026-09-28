#![warn(clippy::all, clippy::pedantic, clippy::nursery)]

use std::{
	env,
	error::Error,
	path::{Path, PathBuf},
	process::Command,
};
#[cfg(not(target_os = "macos"))]
use std::{fs::File, io};

use walkdir::WalkDir;
#[cfg(not(target_os = "macos"))]
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

fn main() -> Result<(), Box<dyn Error>> {
	let task = env::args().nth(1);
	match task.as_deref() {
		Some("release") => release()?,
		_ => print_help(),
	}
	Ok(())
}

fn print_help() {
	println!("Tasks:");
	println!("	release	Build release binaries and package them");
}

fn release() -> Result<(), Box<dyn Error>> {
	let cargo = env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
	let status = Command::new(cargo).current_dir(project_root()).args(["build", "--release"]).status()?;
	if !status.success() {
		return Err("Cargo build failed".into());
	}
	let target_dir = project_root().join("target/release");
	let exe_name = if cfg!(windows) { "fedra.exe" } else { "fedra" };
	let exe_path = target_dir.join(exe_name);
	let readme_path = target_dir.join("readme.html");
	let sounds_dir = project_root().join("sounds");
	if !exe_path.exists() {
		return Err("Executable not found".into());
	}
	println!("Packaging binaries and docs...");
	#[cfg(target_os = "macos")]
	build_mac_dmg(&target_dir, &exe_path, &readme_path, &sounds_dir)?;
	#[cfg(not(target_os = "macos"))]
	{
		build_zip_package(&target_dir, &exe_path, &readme_path, &sounds_dir)?;
		if cfg!(windows) {
			build_windows_installer(&target_dir);
		}
	}
	Ok(())
}

fn project_root() -> PathBuf {
	Path::new(&env!("CARGO_MANIFEST_DIR")).ancestors().nth(1).unwrap().to_path_buf()
}

#[cfg(not(target_os = "macos"))]
fn arch_suffix() -> &'static str {
	match env::consts::ARCH {
		"aarch64" => "arm64",
		"x86_64" => "x64",
		other => other,
	}
}

#[cfg(not(target_os = "macos"))]
fn build_zip_package(
	target_dir: &Path,
	exe_path: &Path,
	readme_path: &Path,
	sounds_dir: &Path,
) -> Result<(), Box<dyn Error>> {
	let package_path = target_dir.join(format!("fedra-{}.zip", arch_suffix()));
	let file = File::create(&package_path)?;
	let mut zip = ZipWriter::new(file);
	let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
	let exe_filename = exe_path.file_name().unwrap();
	zip.start_file(exe_filename.to_string_lossy(), options)?;
	let mut f = File::open(exe_path)?;
	io::copy(&mut f, &mut zip)?;
	if readme_path.exists() {
		zip.start_file("readme.html", options)?;
		let mut f = File::open(readme_path)?;
		io::copy(&mut f, &mut zip)?;
	} else {
		println!("Warning: readme.html not found, skipping.");
	}
	if sounds_dir.exists() {
		for entry in WalkDir::new(sounds_dir) {
			let entry = entry?;
			let path = entry.path();
			let name = path.strip_prefix(sounds_dir.parent().unwrap())?;
			let name_str = name.to_string_lossy().replace('\\', "/");
			if path.is_file() {
				zip.start_file(name_str, options)?;
				let mut f = File::open(path)?;
				io::copy(&mut f, &mut zip)?;
			} else if !name.as_os_str().is_empty() {
				zip.add_directory(name_str, options)?;
			}
		}
	} else {
		println!("Warning: sounds directory not found, skipping.");
	}
	println!("Created zip: {}", package_path.display());
	Ok(())
}

/// Builds `Fedra.app` and a DMG that holds it beside an Applications link, so installing is the
/// usual drag. The binary is copied here rather than in build.rs, which runs before it's linked.
#[cfg(target_os = "macos")]
fn build_mac_dmg(
	target_dir: &Path,
	exe_path: &Path,
	readme_path: &Path,
	sounds_dir: &Path,
) -> Result<(), Box<dyn Error>> {
	use std::{fs, os::unix::fs::symlink};

	let bundle = target_dir.join("Fedra.app");
	let _ = fs::remove_dir_all(&bundle);
	let macos_dir = bundle.join("Contents/MacOS");
	let resources_dir = bundle.join("Contents/Resources");
	fs::create_dir_all(&macos_dir)?;
	fs::create_dir_all(&resources_dir)?;
	fs::write(bundle.join("Contents/Info.plist"), info_plist(&package_version()?))?;
	fs::copy(exe_path, macos_dir.join("fedra"))?;
	if readme_path.exists() {
		fs::copy(readme_path, resources_dir.join("readme.html"))?;
	} else {
		println!("Warning: readme.html not found, skipping.");
	}
	for entry in WalkDir::new(sounds_dir) {
		let entry = entry?;
		let destination = resources_dir.join(entry.path().strip_prefix(sounds_dir.parent().unwrap())?);
		if entry.file_type().is_dir() {
			fs::create_dir_all(&destination)?;
		} else {
			fs::copy(entry.path(), &destination)?;
		}
	}
	println!("Built app: {}", bundle.display());
	let staging = target_dir.join("dmg-staging");
	let _ = fs::remove_dir_all(&staging);
	fs::create_dir_all(&staging)?;
	if !Command::new("ditto").arg(&bundle).arg(staging.join("Fedra.app")).status()?.success() {
		return Err("ditto failed to copy Fedra.app".into());
	}
	symlink("/Applications", staging.join("Applications"))?;
	let dmg_path = target_dir.join("fedra.dmg");
	let status = Command::new("hdiutil")
		.args(["create", "-volname", "Fedra", "-format", "UDZO", "-ov", "-srcfolder"])
		.arg(&staging)
		.arg(&dmg_path)
		.status()?;
	if !status.success() {
		return Err("hdiutil create failed".into());
	}
	println!("Created DMG: {}", dmg_path.display());
	Ok(())
}

#[cfg(target_os = "macos")]
fn package_version() -> Result<String, Box<dyn Error>> {
	let manifest = std::fs::read_to_string(project_root().join("Cargo.toml"))?;
	manifest
		.lines()
		.find_map(|line| line.strip_prefix("version = \"")?.strip_suffix('"').map(str::to_string))
		.ok_or_else(|| "no version in Cargo.toml".into())
}

#[cfg(target_os = "macos")]
fn info_plist(version: &str) -> String {
	format!(
		r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>CFBundleName</key>
	<string>Fedra</string>
	<key>CFBundleDisplayName</key>
	<string>Fedra</string>
	<key>CFBundleIdentifier</key>
	<string>com.trypsynth.fedra</string>
	<key>CFBundleVersion</key>
	<string>{version}</string>
	<key>CFBundleShortVersionString</key>
	<string>{version}</string>
	<key>CFBundleExecutable</key>
	<string>fedra</string>
	<key>CFBundlePackageType</key>
	<string>APPL</string>
	<key>NSHighResolutionCapable</key>
	<true/>
</dict>
</plist>
"#
	)
}

#[cfg(not(target_os = "macos"))]
fn build_windows_installer(target_dir: &Path) {
	let iss_path = target_dir.join("fedra.iss");
	if !iss_path.exists() {
		println!("Skipping installer: fedra.iss not found.");
		return;
	}
	let status = Command::new("ISCC.exe").arg(&iss_path).status();
	match status {
		Ok(s) if s.success() => println!("Installer created successfully."),
		_ => println!("Failed to run Inno Setup (ISCC.exe). Is it in your PATH?"),
	}
}
