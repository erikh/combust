use anyhow::{Context, Result};

/// Sends a desktop notification.
pub fn send(title: &str, message: &str) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        send_linux(title, message)
    }
    #[cfg(target_os = "macos")]
    {
        send_macos(title, message)
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = (title, message);
        Ok(())
    }
}

#[cfg(target_os = "linux")]
fn send_linux(title: &str, message: &str) -> Result<()> {
    std::process::Command::new("notify-send")
        .arg(title)
        .arg(message)
        .status()
        .context("running notify-send")?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn send_macos(title: &str, message: &str) -> Result<()> {
    let script = format!(
        "display notification \"{}\" with title \"{}\"",
        message.replace('"', "\\\""),
        title.replace('"', "\\\"")
    );
    std::process::Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .status()
        .context("running osascript")?;
    Ok(())
}
