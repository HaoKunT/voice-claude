//! 录音开始 / 结束提示音。
//!
//! macOS：系统声音文件 afplay（不阻塞主流程，spawn 后丢掉 handle）
//! Windows：MessageBeep 系统声效（亚毫秒、异步播放，不 spawn 子进程）
//! 其他：noop

// Command 只在 macOS 分支（afplay）里用到；Windows 走 MessageBeep，
// 不加 cfg 的话 Windows 上是 unused import，clippy -D warnings 会挂。
#[cfg(target_os = "macos")]
use std::process::Command;

#[cfg(target_os = "windows")]
use windows::Win32::System::Diagnostics::Debug::MessageBeep;
#[cfg(target_os = "windows")]
use windows::Win32::UI::WindowsAndMessaging::MESSAGEBOX_STYLE;

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
        // 之前走 `powershell -Command "[System.Media.SystemSounds]::..."`:每次提示音
        // spawn 一个 PowerShell 进程(200-600ms),正好卡在「按键→开始录音」和
        // 「松开→处理」两个体感节点上。MessageBeep 直接调系统声效,亚毫秒、
        // 异步播放,语义一致(Asterisk=叮,简单 Beep=啵)。
        // MessageBeep(MESSAGEBOX_STYLE):0xFFFFFFFF = 简单蜂鸣;MB_ICONASTERISK(0x40) = 星号声
        const MB_ICONASTERISK: MESSAGEBOX_STYLE = MESSAGEBOX_STYLE(0x40);
        const SIMPLE_BEEP: MESSAGEBOX_STYLE = MESSAGEBOX_STYLE(0xFFFF_FFFF);
        let which_beep = match which {
            Sound::Start => MB_ICONASTERISK,
            Sound::Stop => SIMPLE_BEEP,
        };
        unsafe {
            let _ = MessageBeep(which_beep);
        }
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = which; // noop
    }
}
