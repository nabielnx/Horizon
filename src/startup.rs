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
pub fn apply(enabled: bool) -> Result<()> {
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
            let command = format!("\"{}\"", exe.display());
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
