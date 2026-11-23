use windows::{
    Win32::{Foundation::*, System::Registry::*},
    core::*,
};
struct Key(HKEY);
impl Drop for Key {
    fn drop(&mut self) {
        unsafe {
            let _ = RegCloseKey(self.0);
        }
    }
}
fn registry(enabled: bool) -> Result<()> {
    unsafe {
        let mut key = HKEY::default();
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run"),
            None,
            PCWSTR::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            None,
            &mut key,
            None,
        )
        .ok()?;
        let key = Key(key);
        if enabled {
            let exe = std::env::current_exe().map_err(|e| Error::new(E_FAIL, e.to_string()))?;
            let command = format!("\"{}\" --autostart", exe.display());
            let text: Vec<u16> = command.encode_utf16().chain(Some(0)).collect();
            let bytes = std::slice::from_raw_parts(text.as_ptr().cast::<u8>(), text.len() * 2);
            RegSetValueExW(key.0, w!("Horizon"), None, REG_SZ, Some(bytes)).ok()
        } else {
            let result = RegDeleteValueW(key.0, w!("Horizon"));
            if result == ERROR_FILE_NOT_FOUND {
                Ok(())
            } else {
                result.ok()
            }
        }
    }
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
fn task_name(owner: &str) -> String {
    let hash = owner.bytes().fold(0xcbf29ce484222325u64, |hash, b| {
        (hash ^ u64::from(b)).wrapping_mul(0x100000001b3)
    });
    format!("Horizon-{hash:016x}")
}
fn task_xml(exe: &std::path::Path, owner: &str) -> String {
    let command = xml_escape(&exe.to_string_lossy());
    let folder = xml_escape(
        &exe.parent()
            .unwrap_or_else(|| std::path::Path::new("."))
            .to_string_lossy(),
    );
    let owner = xml_escape(owner);
    format!(
        r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
<RegistrationInfo><Description>Start Horizon when this user signs in.</Description></RegistrationInfo>
<Triggers><LogonTrigger><Enabled>true</Enabled><UserId>{owner}</UserId><Delay>PT0S</Delay></LogonTrigger></Triggers>
<Principals><Principal id="HorizonUser"><UserId>{owner}</UserId><LogonType>InteractiveToken</LogonType><RunLevel>LeastPrivilege</RunLevel></Principal></Principals>
<Settings><MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy><DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries><StopIfGoingOnBatteries>false</StopIfGoingOnBatteries><StartWhenAvailable>true</StartWhenAvailable><Enabled>true</Enabled><ExecutionTimeLimit>PT0S</ExecutionTimeLimit><Priority>5</Priority></Settings>
<Actions Context="HorizonUser"><Exec><Command>{command}</Command><Arguments>--autostart</Arguments><WorkingDirectory>{folder}</WorkingDirectory></Exec></Actions>
</Task>"#
    )
}
fn scheduled(enabled: bool) -> Result<()> {
    use windows::Win32::System::{Com::*, TaskScheduler::*, Variant::VARIANT};
    unsafe {
        let service: ITaskService = CoCreateInstance(&TaskScheduler, None, CLSCTX_INPROC_SERVER)?;
        let empty = VARIANT::default();
        service.Connect(&empty, &empty, &empty, &empty)?;
        let user = service.ConnectedUser()?.to_string();
        let domain = service.ConnectedDomain()?.to_string();
        let owner = if domain.is_empty() {
            user
        } else {
            format!("{domain}\\{user}")
        };
        let name = BSTR::from(task_name(&owner));
        let folder = service.GetFolder(&BSTR::from("\\"))?;
        if !enabled {
            if folder.GetTask(&name).is_ok() {
                folder.DeleteTask(&name, 0)?;
            }
            return Ok(());
        }
        let exe = std::env::current_exe().map_err(|e| Error::new(E_FAIL, e.to_string()))?;
        let command = format!("<Command>{}</Command>", xml_escape(&exe.to_string_lossy()));
        if let Ok(existing) = folder.GetTask(&name) {
            let xml = existing.Xml()?.to_string();
            if existing.Enabled()?.as_bool()
                && xml.contains(&command)
                && xml.contains("--autostart")
                && xml.contains("<LogonTrigger>")
                && xml.contains("InteractiveToken")
                && !xml.contains("HighestAvailable")
                && (!xml.contains("<Delay>") || xml.contains("<Delay>PT0S</Delay>"))
            {
                registry(false)?;
                return Ok(());
            }
        }
        folder.RegisterTask(
            &name,
            &BSTR::from(task_xml(&exe, &owner)),
            TASK_CREATE_OR_UPDATE.0,
            &empty,
            &empty,
            TASK_LOGON_INTERACTIVE_TOKEN,
            &empty,
        )?;
        if let Err(error) = registry(false) {
            let _ = folder.DeleteTask(&name, 0);
            return Err(error);
        }
        Ok(())
    }
}
pub fn apply(enabled: bool) -> Result<()> {
    if !enabled {
        scheduled(false)?;
        return registry(false);
    }
    scheduled(true).or_else(|_| registry(true))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn login_task_is_immediate_per_user_non_elevated_and_battery_friendly() {
        let xml = task_xml(
            std::path::Path::new(r#"C:\Apps & Tools\Horizon.exe"#),
            "PC\\A&B",
        );
        for text in [
            "<Delay>PT0S</Delay>",
            "<RunLevel>LeastPrivilege</RunLevel>",
            "InteractiveToken",
            "<DisallowStartIfOnBatteries>false",
            "--autostart",
            "Apps &amp; Tools",
            "PC\\A&amp;B",
        ] {
            assert!(xml.contains(text), "Missing {text}");
        }
        assert_ne!(task_name("PC\\A"), task_name("PC\\B"));
    }
}
