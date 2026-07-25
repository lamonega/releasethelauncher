use std::process::Command;

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Check if the system has at least `required_mb` megabytes of available memory.
#[must_use]
pub fn has_enough_memory(required_mb: u64) -> bool {
    #[cfg(target_os = "windows")]
    {
        let mut cmd = Command::new("powershell");
        cmd.args([
            "-NoProfile",
            "-Command",
            "(Get-CimInstance Win32_OperatingSystem | Select-Object FreePhysicalMemory).FreePhysicalMemory / 1024",
        ]);
        cmd.creation_flags(CREATE_NO_WINDOW);
        if let Ok(output) = cmd.output() {
            if let Ok(s) = String::from_utf8(output.stdout) {
                if let Ok(kb) = s.trim().parse::<u64>() {
                    return kb / 1024 >= required_mb;
                }
            }
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = required_mb;
    }
    true
}
