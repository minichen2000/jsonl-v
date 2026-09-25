//! Windows 资源管理器右键菜单注册（HKCU，无需管理员）。
//! 用系统自带 reg.exe 读写，不引入额外依赖。
//! 所有 spawn 都带 CREATE_NO_WINDOW，避免控制台窗口闪现。
//! 非 Windows 平台为桩实现（不提供此功能）。

#[cfg(windows)]
mod platform {
    use std::os::windows::process::CommandExt;
    use std::process::Command;

    const KEY_PATH: &str = r"HKCU\Software\Classes\*\shell\jsonl-v";
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    fn reg_cmd() -> Command {
        let mut c = Command::new("reg");
        c.creation_flags(CREATE_NO_WINDOW);
        c
    }

    pub fn is_registered() -> bool {
        reg_cmd()
            .args(["query", KEY_PATH])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    pub fn register(display_name: &str) -> Result<(), String> {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let exe = exe.display().to_string();
        run(reg_add(KEY_PATH, None, display_name))?;
        run(reg_add(KEY_PATH, Some("Icon"), &exe))?;
        let command_key = format!(r"{KEY_PATH}\command");
        run(reg_add(&command_key, None, &format!("\"{exe}\" \"%1\"")))?;
        Ok(())
    }

    pub fn unregister() -> Result<(), String> {
        let out = reg_cmd()
            .args(["delete", KEY_PATH, "/f"])
            .output()
            .map_err(|e| e.to_string())?;
        if out.status.success() {
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
        }
    }

    fn reg_add(key: &str, value_name: Option<&str>, data: &str) -> Vec<String> {
        let mut args = vec![
            "add".to_string(),
            key.to_string(),
            "/f".to_string(),
            "/d".to_string(),
            data.to_string(),
        ];
        match value_name {
            Some(name) => {
                args.push("/v".to_string());
                args.push(name.to_string());
            }
            None => args.push("/ve".to_string()),
        }
        args
    }

    fn run(args: Vec<String>) -> Result<(), String> {
        let out = reg_cmd()
            .args(&args)
            .output()
            .map_err(|e| e.to_string())?;
        if out.status.success() {
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
        }
    }
}

#[cfg(not(windows))]
mod platform {
    pub fn is_registered() -> bool {
        false
    }

    pub fn register(_display_name: &str) -> Result<(), String> {
        Err("Explorer context menu registration is only supported on Windows".into())
    }

    pub fn unregister() -> Result<(), String> {
        Err("Explorer context menu registration is only supported on Windows".into())
    }
}

pub use platform::{is_registered, register, unregister};

/// 在后台线程查询注册状态（不阻塞 UI 首帧）。
pub fn is_registered_async() -> std::sync::mpsc::Receiver<bool> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(is_registered());
    });
    rx
}
