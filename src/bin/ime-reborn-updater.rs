#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use semver::Version;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    env,
    error::Error,
    ffi::OsStr,
    fs::{self, File},
    io::{Read, Write},
    os::windows::process::CommandExt,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};
use windows::{
    core::w,
    Win32::{
        Foundation::{CloseHandle, ERROR_ALREADY_EXISTS},
        System::Threading::CreateMutexW,
        UI::WindowsAndMessaging::{
            FindWindowW, MessageBoxW, MB_ICONERROR, MB_ICONINFORMATION, MB_OK,
        },
    },
};

const MANIFEST_URL: &str = "https://github.com/taskinoz/Impression-Eyes-Reborn/releases/latest/download/ime-reborn-update.json";
const EXPECTED_PREFIX: &str =
    "https://github.com/taskinoz/Impression-Eyes-Reborn/releases/download/v";
const EXPECTED_SUFFIX: &str = "/ime-reborn-windows-x86_64-setup.exe";
const TASK_NAME: &str = "ime-reborn Update Check";
const MAX_MANIFEST: usize = 64 * 1024;
const MAX_INSTALLER: u64 = 256 * 1024 * 1024;
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UpdateManifest {
    schema: u32,
    version: String,
    published: String,
    minimum_updater: String,
    installer_url: String,
    installer_sha256: String,
    notes_url: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Scheduled,
    Interactive,
}

fn main() {
    let argument = env::args().nth(1);
    let result = match argument.as_deref() {
        Some("--install-task") => install_task(),
        Some("--remove-task") => remove_task(),
        Some("--scheduled") => run_update(Mode::Scheduled),
        Some("--interactive") | None => run_update(Mode::Interactive),
        _ => Err("unknown command-line option".into()),
    };

    if let Err(error) = result {
        if argument.as_deref() != Some("--scheduled") {
            show_message("ime-reborn updater", &error.to_string(), true);
        }
        std::process::exit(1);
    }
}

fn install_task() -> Result<(), Box<dyn Error>> {
    let updater = env::current_exe()?;
    let task_command = format!("\"{}\" --scheduled", updater.display());
    let status = hidden_command("schtasks.exe")
        .args([
            "/Create",
            "/F",
            "/TN",
            TASK_NAME,
            "/TR",
            &task_command,
            "/SC",
            "DAILY",
            "/ST",
            "12:00",
            "/RL",
            "LIMITED",
        ])
        .status()?;
    if !status.success() {
        return Err(format!("scheduled task creation failed with {status}").into());
    }
    Ok(())
}

fn remove_task() -> Result<(), Box<dyn Error>> {
    let status = hidden_command("schtasks.exe")
        .args(["/Delete", "/F", "/TN", TASK_NAME])
        .status()?;
    // schtasks also fails when the task is already absent; uninstall may continue.
    if !status.success() {
        return Err(format!("scheduled task removal returned {status}").into());
    }
    Ok(())
}

fn run_update(mode: Mode) -> Result<(), Box<dyn Error>> {
    let _mutex = SingleInstance::acquire()?;
    let installed = installed_version()?;
    let agent = ureq::AgentBuilder::new()
        .redirects(5)
        .timeout(Duration::from_secs(120))
        .timeout_connect(Duration::from_secs(10))
        .user_agent(&format!("ime-reborn-updater/{}", env!("CARGO_PKG_VERSION")))
        .build();

    let manifest = fetch_manifest(&agent)?;
    validate_manifest(&manifest)?;
    let available = Version::parse(&manifest.version)?;
    let minimum = Version::parse(&manifest.minimum_updater)?;
    let updater = Version::parse(env!("CARGO_PKG_VERSION"))?;
    if updater < minimum {
        return Err(
            "this updater is too old; download the latest installer from ime-reborn.com".into(),
        );
    }
    if available <= installed {
        if mode == Mode::Interactive {
            show_message(
                "ime-reborn updater",
                &format!("You are up to date ({installed})."),
                false,
            );
        }
        return Ok(());
    }
    if viewer_is_open() {
        return Err("close ime-reborn before installing the available update".into());
    }

    let installer = download_installer(&agent, &manifest)?;
    if mode == Mode::Interactive {
        show_message(
            "ime-reborn updater",
            &format!(
                "Version {} is ready. The installer will now open.\n\nRelease notes: {}",
                manifest.version, manifest.notes_url
            ),
            false,
        );
    }
    launch_installer(&installer, mode)?;
    Ok(())
}

fn launch_installer(installer: &Path, mode: Mode) -> Result<(), Box<dyn Error>> {
    let mut command = Command::new(installer);
    if mode == Mode::Scheduled {
        command.arg("/S");
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
        .spawn()
        .map_err(|error| format!("failed to open {}: {error}", installer.display()))?;
    Ok(())
}

fn fetch_manifest(agent: &ureq::Agent) -> Result<UpdateManifest, Box<dyn Error>> {
    let response = agent.get(MANIFEST_URL).call()?;
    if response.get_url().starts_with("http://") {
        return Err("the update manifest redirected to an insecure URL".into());
    }
    let mut bytes = Vec::new();
    response
        .into_reader()
        .take((MAX_MANIFEST + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > MAX_MANIFEST {
        return Err("update manifest exceeds the size limit".into());
    }
    Ok(serde_json::from_slice(&bytes)?)
}

fn validate_manifest(manifest: &UpdateManifest) -> Result<(), Box<dyn Error>> {
    if manifest.schema != 1 {
        return Err("unsupported update manifest schema".into());
    }
    let version = Version::parse(&manifest.version)?;
    if !version.pre.is_empty() || !version.build.is_empty() {
        return Err("preview releases are not accepted by the stable updater".into());
    }
    let expected = format!("{EXPECTED_PREFIX}{}{EXPECTED_SUFFIX}", manifest.version);
    if manifest.installer_url != expected {
        return Err("the installer URL is not the expected pinned GitHub release asset".into());
    }
    if manifest.installer_sha256.len() != 64
        || !manifest
            .installer_sha256
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err("the installer SHA-256 is invalid".into());
    }
    if manifest.published.len() > 64
        || manifest.notes_url
            != format!(
                "https://github.com/taskinoz/Impression-Eyes-Reborn/releases/tag/v{}",
                manifest.version
            )
    {
        return Err("the release metadata is not from the expected GitHub repository".into());
    }
    Ok(())
}

fn download_installer(
    agent: &ureq::Agent,
    manifest: &UpdateManifest,
) -> Result<PathBuf, Box<dyn Error>> {
    let directory = update_directory()?;
    fs::create_dir_all(&directory)?;
    let final_path = directory.join(format!("ime-reborn-{}-setup.exe", manifest.version));
    let partial_path = final_path.with_extension("exe.part");
    let response = agent.get(&manifest.installer_url).call()?;
    if response.get_url().starts_with("http://") {
        return Err("an HTTPS download redirected to an insecure URL".into());
    }
    let mut reader = response.into_reader();
    let mut output = File::create(&partial_path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    let mut total = 0_u64;
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        total = total
            .checked_add(read as u64)
            .ok_or("installer size overflow")?;
        if total > MAX_INSTALLER {
            drop(output);
            let _ = fs::remove_file(&partial_path);
            return Err("installer exceeds the size limit".into());
        }
        hasher.update(&buffer[..read]);
        output.write_all(&buffer[..read])?;
    }
    output.sync_all()?;
    let actual = format!("{:x}", hasher.finalize());
    if !actual.eq_ignore_ascii_case(&manifest.installer_sha256) {
        drop(output);
        let _ = fs::remove_file(&partial_path);
        return Err("downloaded installer failed SHA-256 verification".into());
    }
    drop(output);
    fs::rename(&partial_path, &final_path)?;
    Ok(final_path)
}

fn installed_version() -> Result<Version, Box<dyn Error>> {
    let path = env::current_exe()?
        .parent()
        .ok_or("updater has no parent directory")?
        .join("current-version.txt");
    Ok(Version::parse(fs::read_to_string(path)?.trim())?)
}

fn update_directory() -> Result<PathBuf, Box<dyn Error>> {
    Ok(
        PathBuf::from(env::var_os("LOCALAPPDATA").ok_or("LOCALAPPDATA is unavailable")?)
            .join("ime-reborn")
            .join("updates"),
    )
}

fn viewer_is_open() -> bool {
    unsafe { FindWindowW(w!("ImpressionEyesRebornWindow"), None).is_ok() }
}

fn hidden_command<S: AsRef<OsStr>>(program: S) -> Command {
    let mut command = Command::new(program);
    command.creation_flags(CREATE_NO_WINDOW);
    command
}

fn show_message(title: &str, message: &str, error: bool) {
    let title: Vec<u16> = title.encode_utf16().chain(Some(0)).collect();
    let message: Vec<u16> = message.encode_utf16().chain(Some(0)).collect();
    let icon = if error {
        MB_ICONERROR
    } else {
        MB_ICONINFORMATION
    };
    unsafe {
        MessageBoxW(
            None,
            windows::core::PCWSTR(message.as_ptr()),
            windows::core::PCWSTR(title.as_ptr()),
            MB_OK | icon,
        );
    }
}

struct SingleInstance(windows::Win32::Foundation::HANDLE);

impl SingleInstance {
    fn acquire() -> Result<Self, Box<dyn Error>> {
        let handle = unsafe { CreateMutexW(None, true, w!("Local\\ime-reborn-updater"))? };
        if windows::core::Error::from_win32().code() == ERROR_ALREADY_EXISTS.to_hresult() {
            unsafe { CloseHandle(handle)? };
            return Err("another update check is already running".into());
        }
        Ok(Self(handle))
    }
}

impl Drop for SingleInstance {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_manifest() -> UpdateManifest {
        UpdateManifest {
            schema: 1,
            version: "1.2.3".into(),
            published: "2026-08-24T00:00:00Z".into(),
            minimum_updater: "0.1.0".into(),
            installer_url: "https://github.com/taskinoz/Impression-Eyes-Reborn/releases/download/v1.2.3/ime-reborn-windows-x86_64-setup.exe".into(),
            installer_sha256: "a".repeat(64),
            notes_url: "https://github.com/taskinoz/Impression-Eyes-Reborn/releases/tag/v1.2.3".into(),
        }
    }

    #[test]
    fn accepts_expected_stable_release() {
        assert!(validate_manifest(&valid_manifest()).is_ok());
    }

    #[test]
    fn rejects_changed_download_host_or_path() {
        let mut manifest = valid_manifest();
        manifest.installer_url = "https://example.com/setup.exe".into();
        assert!(validate_manifest(&manifest).is_err());
    }

    #[test]
    fn rejects_prerelease() {
        let mut manifest = valid_manifest();
        manifest.version = "1.2.3-beta.1".into();
        assert!(validate_manifest(&manifest).is_err());
    }
}
