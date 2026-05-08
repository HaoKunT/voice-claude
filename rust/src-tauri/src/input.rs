//! 键盘模拟输入。
//! 对应 Go 版的 input.go。

use anyhow::Result;
use enigo::{Enigo, Keyboard, Key, Settings, Direction};
use parking_lot::Mutex;
use std::sync::OnceLock;

static ENIGO: OnceLock<Mutex<Enigo>> = OnceLock::new();

fn enigo() -> &'static Mutex<Enigo> {
    ENIGO.get_or_init(|| Mutex::new(Enigo::new(&Settings::default()).expect("init enigo")))
}

/// 把文字模拟键盘输入到当前焦点窗口。
pub fn type_text(text: &str) -> Result<()> {
    enigo().lock().text(text)?;
    Ok(())
}

/// 按 n 次退格键，用于删除之前输入的中间结果。
pub fn delete_chars(n: usize) -> Result<()> {
    let mut e = enigo().lock();
    for _ in 0..n {
        e.key(Key::Backspace, Direction::Click)?;
    }
    Ok(())
}
