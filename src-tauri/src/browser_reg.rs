//! Registers Mix as a candidate default browser.

use crate::ipc::AppError;

const PROG_ID: &str = "MixHTML";
const CLIENT: &str = "Mix";

#[cfg(windows)]
fn write(root: &str, key: &str, value: Option<&str>) -> Result<(), AppError> {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    let current = RegKey::predef(HKEY_CURRENT_USER);
    let path = format!("{root}\\{key}");
    let (target, _) = current
        .create_subkey(&path)
        .map_err(|err| AppError::new("registry", err.to_string()))?;
    match value {
        Some(value) => target
            .set_value("", &value)
            .map_err(|err| AppError::new("registry", err.to_string())),
        None => Ok(()),
    }
}

/// Writes the associations Windows looks at, then opens the default apps page.
pub fn register() -> Result<String, AppError> {
    let exe = std::env::current_exe().map_err(|err| AppError::new("registry", err.to_string()))?;
    let exe = exe.display().to_string();

    #[cfg(windows)]
    {
        write(
            &format!("Software\\Classes\\{PROG_ID}\\shell\\open\\command"),
            "",
            Some(&format!("\"{exe}\" \"%1\"")),
        )?;
        write("Software\\Classes\\MixHTML\\DefaultIcon", "", Some(&format!("\"{exe}\",0")))?;
        write("Software\\Clients\\StartMenuInternet\\Mix\\shell\\open\\command", "", Some(&format!("\"{exe}\"")))?;
        write("Software\\Clients\\StartMenuInternet\\Mix\\Capabilities", "ApplicationName", Some(CLIENT))?;
        write("Software\\Clients\\StartMenuInternet\\Mix\\Capabilities", "ApplicationIcon", Some(&format!("\"{exe}\",0")))?;
        write("Software\\RegisteredApplications", CLIENT, Some(&format!("Software\\Clients\\StartMenuInternet\\{CLIENT}\\Capabilities")))?;
        crate::logging::info(&format!("registered as a browser candidate: {exe}"));
        return Ok(exe);
    }
    #[cfg(not(windows))]
    {
        let _ = exe;
        Err(AppError::new("registry", "only implemented on Windows"))
    }
}
