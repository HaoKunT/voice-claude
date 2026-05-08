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

/// 把文字模拟键盘输入到当前焦点窗口。
pub fn type_text(text: &str) -> Result<()> {
    let mut e = new_enigo()?;
    e.text(text)?;
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
