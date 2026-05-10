//! 录音开始 / 结束提示音。
//!
//! macOS：系统声音文件 afplay（不阻塞主流程，spawn 后丢掉 handle）
//! Windows：PowerShell 的 `[System.Media.SystemSounds]::Asterisk.Play()`
//! 其他：noop

use std::process::Command;

pub fn start() {
    play_sound(Sound::Start);
}

pub fn stop() {
    play_sound(Sound::Stop);
}

#[derive(Copy, Clone)]
enum Sound {
    Start,
    Stop,
}

fn play_sound(which: Sound) {
    #[cfg(target_os = "macos")]
    {
        let path = match which {
            Sound::Start => "/System/Library/Sounds/Tink.aiff",
            Sound::Stop => "/System/Library/Sounds/Pop.aiff",
        };
        let _ = Command::new("afplay").arg(path).spawn();
    }

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;

        const CREATE_NO_WINDOW: u32 = 0x08000000;
        let ps = match which {
            Sound::Start => "[System.Media.SystemSounds]::Asterisk.Play()",
            Sound::Stop => "[System.Media.SystemSounds]::Beep.Play()",
        };
        let _ = Command::new("powershell")
            .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", ps])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn();
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = which; // noop
    }
}
