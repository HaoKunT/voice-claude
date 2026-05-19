//! 键盘模拟输入。
//! 对应 Go 版的 input.go。
//!
//! Enigo 的 Settings 在 macOS 上握着线程锁，不能跨线程共享（非 Sync）。
//! 每次 type_text/delete_chars 新建一个 Enigo 实例，保证可以在任意 tokio 任务里调用。

use anyhow::Result;
use enigo::{Direction, Enigo, Key, Keyboard, Settings};

fn new_enigo() -> Result<Enigo> {
    Enigo::new(&Settings::default()).map_err(|e| anyhow::anyhow!("init enigo: {:?}", e))
}

/// 一次性给目标 app 注入的最大字符数。enigo macOS 走
/// `CGEventKeyboardSetUnicodeString` —— 单 event 塞太多 unicode 字符时,
/// 目标 app(尤其 Electron 类 / 中文 IME)同步处理会卡。线上踩坑:
/// 一段 180 字符中文一次性注入 Claude Code,目标 app 卡了 3 分钟。
/// 50 字符是经验值,大部分焦点 app 处理 < 100ms 无感。
const TYPE_CHUNK_CHARS: usize = 50;

/// 块间小睡,让目标 app run loop 跑一下处理上一段。20ms 用户感知不到
/// 但能给 IME / Electron renderer 喘口气避免 hang。
const TYPE_CHUNK_PAUSE_MS: u64 = 20;

/// 把文字模拟键盘输入到当前焦点窗口。
///
/// 长文本(> TYPE_CHUNK_CHARS)分段注入,中间 sleep 一段时间。
/// 一次性巨量 unicode event 会让目标 app 同步处理 hang(根因在目标 app
/// IME 实现,不在 voice-claude),分段是 client 侧能做的最实际缓解。
pub fn type_text(text: &str) -> Result<()> {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= TYPE_CHUNK_CHARS {
        // 短文本直接整段输入 —— 节省 enigo init 开销 + 块间 sleep
        let mut e = new_enigo()?;
        e.text(text)?;
        return Ok(());
    }
    let mut e = new_enigo()?;
    let pause = std::time::Duration::from_millis(TYPE_CHUNK_PAUSE_MS);
    for (i, chunk) in chars.chunks(TYPE_CHUNK_CHARS).enumerate() {
        if i > 0 {
            std::thread::sleep(pause);
        }
        let s: String = chunk.iter().collect();
        e.text(&s)?;
    }
    Ok(())
}

/// 按 n 次退格键，用于删除之前输入的中间结果。
pub fn delete_chars(n: usize) -> Result<()> {
    if n == 0 {
        return Ok(());
    }
    let mut e = new_enigo()?;
    for _ in 0..n {
        e.key(Key::Backspace, Direction::Click)?;
    }
    Ok(())
}
