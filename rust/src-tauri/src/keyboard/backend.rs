//! supervisor 线程:跑 handy-keys 实例 + 状态机循环 + 热重载。
//!
//! 两条路径,根据 `cfg.mode` 选:
//! - **HotkeyManager 路径**(Toggle / PushToTalk):注册主热键 + 录音中临时
//!   注册 ESC,通过 HotkeyEvent 驱动 `ManagerStateMachine`
//! - **KeyboardListener 路径**(DoubleTapHold):流式拿所有 KeyEvent,驱动
//!   `DoubleTapStateMachine`,ESC 在状态机内部识别(无需额外注册)
//!
//! 公开 API:
//! - `KeyboardBackend::start(app, cfg)` 启动 supervisor 线程
//! - `KeyboardBackend::reload(cfg)` 热更新(发 Control::Reload,supervisor break
//!   内层循环重建 manager/listener)
//! - `Drop`:发 Control::Shutdown 并 join supervisor 线程

use std::sync::mpsc::{self, Sender, TryRecvError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use handy_keys::{Hotkey, HotkeyId, HotkeyManager, Key, KeyboardListener, Modifiers};
use tauri::AppHandle;

use super::state_machine::{
    DoubleTapStateMachine, HotkeyKind, ManagerStateMachine, TriggerEvent, TriggerMode,
};
use crate::recorder;

/// supervisor 线程外部输入。
pub enum Control {
    Reload(BackendConfig),
    Shutdown,
}

/// 给 supervisor 的运行时配置。从 `crate::config::Config` 转换得到。
#[derive(Clone)]
pub struct BackendConfig {
    pub mode: TriggerMode,
    /// Toggle/PTT 时是用户主热键,DoubleTapHold 时为 None。
    pub hotkey: Option<Hotkey>,
    /// DoubleTapHold 模式下要双击的 modifier。
    pub double_tap_modifier: Option<Modifiers>,
}

pub struct KeyboardBackend {
    ctrl_tx: Sender<Control>,
    handle: Option<JoinHandle<()>>,
}

impl KeyboardBackend {
    /// 启动 supervisor 线程并 probe 一遍 cfg(确保 macOS Accessibility 已授权 +
    /// 热键能注册成功),失败立即返回 Err 而不是异步报回来。
    pub fn start(app: AppHandle, cfg: BackendConfig) -> Result<Self> {
        probe(&cfg)?;

        let (ctrl_tx, ctrl_rx) = mpsc::channel();
        let handle = thread::Builder::new()
            .name("voice-claude-keyboard".into())
            .spawn(move || run_supervisor(app, cfg, ctrl_rx))
            .map_err(|e| anyhow!("启动 keyboard backend 线程失败: {}", e))?;

        Ok(Self {
            ctrl_tx,
            handle: Some(handle),
        })
    }

    /// 热重载 cfg。supervisor 收到后 break 内层循环重建 manager/listener。
    pub fn reload(&self, cfg: BackendConfig) -> Result<()> {
        self.ctrl_tx
            .send(Control::Reload(cfg))
            .map_err(|_| anyhow!("keyboard backend 已退出,无法 reload"))
    }
}

impl Drop for KeyboardBackend {
    fn drop(&mut self) {
        let _ = self.ctrl_tx.send(Control::Shutdown);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

/// 同步 probe 一遍配置 —— 失败立即返回,不让 supervisor 异步报错。
fn probe(cfg: &BackendConfig) -> Result<()> {
    match cfg.mode {
        TriggerMode::Toggle | TriggerMode::PushToTalk => {
            // 必须用 new_with_blocking:默认 new() 只观察事件不 block,主热键
            // 按键会透传到当前焦点 app(尤其 Space/字母键被实际输入,体感坏)。
            let manager = HotkeyManager::new_with_blocking()
                .map_err(|e| anyhow!("无法创建 HotkeyManager(macOS 检查辅助功能权限): {}", e))?;
            let hk = cfg
                .hotkey
                .ok_or_else(|| anyhow!("Toggle/PTT 模式需要主热键"))?;
            manager
                .register(hk)
                .map_err(|e| anyhow!("注册主热键失败: {}", e))?;
            // Drop 自动 unregister
        }
        TriggerMode::DoubleTapHold => {
            let _listener = KeyboardListener::new()
                .map_err(|e| anyhow!("无法创建 KeyboardListener(macOS 检查辅助功能权限): {}", e))?;
            if cfg.double_tap_modifier.is_none() {
                return Err(anyhow!("DoubleTapHold 模式需要 double_tap_modifier"));
            }
        }
    }
    Ok(())
}

/// 录音中临时注册的 ESC 取消热键(无 modifier,纯 Escape 键)。
fn esc_hotkey() -> Result<Hotkey> {
    Hotkey::new(Modifiers::empty(), Some(Key::Escape))
        .map_err(|e| anyhow!("构造 ESC 热键失败: {}", e))
}

fn run_supervisor(app: AppHandle, mut cfg: BackendConfig, ctrl_rx: mpsc::Receiver<Control>) {
    'reload: loop {
        match cfg.mode {
            TriggerMode::Toggle | TriggerMode::PushToTalk => {
                match run_manager_path(&app, &cfg, &ctrl_rx) {
                    PathExit::Reload(new_cfg) => {
                        cfg = new_cfg;
                        continue 'reload;
                    }
                    PathExit::Shutdown => return,
                    PathExit::FailedWaitControl => {
                        if !wait_for_control(&ctrl_rx, &mut cfg) {
                            return;
                        }
                        continue 'reload;
                    }
                }
            }
            TriggerMode::DoubleTapHold => match run_listener_path(&app, &cfg, &ctrl_rx) {
                PathExit::Reload(new_cfg) => {
                    cfg = new_cfg;
                    continue 'reload;
                }
                PathExit::Shutdown => return,
                PathExit::FailedWaitControl => {
                    if !wait_for_control(&ctrl_rx, &mut cfg) {
                        return;
                    }
                    continue 'reload;
                }
            },
        }
    }
}

/// 路径退出原因。reload / shutdown 是正常退出,FailedWaitControl 是构造失败后
/// 阻塞等下一次控制消息(避免空转 + 让 reload 能修)。
enum PathExit {
    Reload(BackendConfig),
    Shutdown,
    FailedWaitControl,
}

fn run_manager_path(
    app: &AppHandle,
    cfg: &BackendConfig,
    ctrl_rx: &mpsc::Receiver<Control>,
) -> PathExit {
    // new_with_blocking:把已注册的热键事件从 OS 事件流里拦下来,不再透传给
    // 焦点 app。普通 new() 模式 Space/字母键会被实际打到当前窗口里。
    let manager = match HotkeyManager::new_with_blocking() {
        Ok(m) => m,
        Err(e) => {
            tracing::error!(error = %e, "keyboard supervisor: 创建 HotkeyManager 失败");
            return PathExit::FailedWaitControl;
        }
    };

    let main_id = match cfg.hotkey {
        Some(hk) => match manager.register(hk) {
            Ok(id) => id,
            Err(e) => {
                tracing::error!(error = %e, "keyboard supervisor: 注册主热键失败");
                return PathExit::FailedWaitControl;
            }
        },
        None => {
            tracing::warn!("keyboard supervisor(manager): cfg.hotkey 为 None");
            return PathExit::FailedWaitControl;
        }
    };

    let mut state = ManagerStateMachine::new(cfg.mode);
    let mut esc_id: Option<HotkeyId> = None;
    let mut last_heartbeat = Instant::now();

    tracing::info!(mode = ?cfg.mode, "keyboard supervisor: 已就位(manager 路径)");

    loop {
        match ctrl_rx.try_recv() {
            Ok(Control::Shutdown) => return PathExit::Shutdown,
            Ok(Control::Reload(new_cfg)) => return PathExit::Reload(new_cfg),
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => return PathExit::Shutdown,
        }

        // Heartbeat:supervisor 主循环每 30s emit 一条 INFO log。诊断用 ——
        // 用户报"wake 后系统键鼠卡死"时,看日志最后 heartbeat 时间戳能直接确认
        // supervisor 何时停止响应(若长期没续上 = supervisor 线程 / runtime 卡了;
        // 若一直续 = 卡的不是 supervisor,得换方向查)。
        if last_heartbeat.elapsed() >= Duration::from_secs(30) {
            tracing::debug!("keyboard supervisor heartbeat (manager 路径)");
            last_heartbeat = Instant::now();
        }

        if let Some(ev) = manager.try_recv() {
            let kind = if ev.id == main_id {
                Some(HotkeyKind::Main)
            } else if Some(ev.id) == esc_id {
                Some(HotkeyKind::Escape)
            } else {
                None
            };
            if let Some(kind) = kind {
                if let Some(trig) = state.handle(kind, ev.state) {
                    dispatch_trigger(app, trig);
                    manage_esc(&manager, &mut esc_id, trig);
                }
            }
        } else {
            thread::sleep(Duration::from_millis(50));
        }
    }
}

fn run_listener_path(
    app: &AppHandle,
    cfg: &BackendConfig,
    ctrl_rx: &mpsc::Receiver<Control>,
) -> PathExit {
    let listener = match KeyboardListener::new() {
        Ok(l) => l,
        Err(e) => {
            tracing::error!(error = %e, "keyboard supervisor: 创建 KeyboardListener 失败");
            return PathExit::FailedWaitControl;
        }
    };

    let target = match cfg.double_tap_modifier {
        Some(m) => m,
        None => {
            tracing::warn!("keyboard supervisor(listener): double_tap_modifier 为 None");
            return PathExit::FailedWaitControl;
        }
    };

    let mut state = DoubleTapStateMachine::new(target);
    let mut last_heartbeat = Instant::now();

    tracing::info!(target = ?target, "keyboard supervisor: 已就位(listener 路径)");

    loop {
        match ctrl_rx.try_recv() {
            Ok(Control::Shutdown) => return PathExit::Shutdown,
            Ok(Control::Reload(new_cfg)) => return PathExit::Reload(new_cfg),
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => return PathExit::Shutdown,
        }

        // Heartbeat,见 manager 路径同条注释
        if last_heartbeat.elapsed() >= Duration::from_secs(30) {
            tracing::debug!("keyboard supervisor heartbeat (listener 路径)");
            last_heartbeat = Instant::now();
        }

        // 50ms 阻塞拿事件,期间也走 tick 检查 350ms 超时
        match listener.recv_timeout(Duration::from_millis(50)) {
            Ok(ev) => {
                let now = Instant::now();
                if let Some(trig) = state.handle(&ev, now) {
                    dispatch_trigger(app, trig);
                }
            }
            Err(handy_keys::Error::Timeout) => {
                state.tick(Instant::now());
            }
            Err(e) => {
                tracing::error!(error = %e, "listener.recv_timeout 失败,退出 listener 路径");
                return PathExit::FailedWaitControl;
            }
        }
    }
}

/// 出错时阻塞等下一条 Control 消息,期间不空转 CPU。返回 true 表示该 reload,
/// false 表示 shutdown 或 channel 断开应退出。
fn wait_for_control(ctrl_rx: &mpsc::Receiver<Control>, cfg: &mut BackendConfig) -> bool {
    match ctrl_rx.recv() {
        Ok(Control::Shutdown) => false,
        Ok(Control::Reload(new_cfg)) => {
            *cfg = new_cfg;
            true
        }
        Err(_) => false,
    }
}

/// 按 TriggerEvent 调度到 Tauri 主线程上对 recorder 操作。
fn dispatch_trigger(app: &AppHandle, trig: TriggerEvent) {
    let app_clone = app.clone();
    let _ = app.run_on_main_thread(move || {
        use tauri::{Emitter, Manager};
        let state = app_clone.state::<crate::AppState>();
        let cfg = state.snapshot();
        match trig {
            TriggerEvent::Start => recorder::start(app_clone.clone(), cfg),
            TriggerEvent::Stop => recorder::stop(),
            TriggerEvent::Cancel => {
                let _ = app_clone.emit("recording-cancelled", ());
                recorder::cancel();
            }
        }
    });
}

/// HotkeyManager 路径下,Start 时挂上临时 ESC、Stop/Cancel 时卸掉。register 失败
/// 只 warn 不阻塞录音主路径(用户仍可通过 indicator 上的 ✕ 按钮取消)。
///
/// DoubleTapHold 路径走 KeyboardListener 不在这里,ESC 由状态机直接识别。
fn manage_esc(manager: &HotkeyManager, esc_id: &mut Option<HotkeyId>, trig: TriggerEvent) {
    match trig {
        TriggerEvent::Start => {
            if let Some(id) = esc_id.take() {
                // 异常情况(连续两次 Start 没 Stop):先卸再注册保持一致
                let _ = manager.unregister(id);
            }
            match esc_hotkey().and_then(|hk| manager.register(hk).map_err(|e| anyhow!("{}", e))) {
                Ok(id) => *esc_id = Some(id),
                Err(e) => tracing::warn!(error = %e, "注册 ESC 取消热键失败,只能用 ✕ 按钮取消"),
            }
        }
        TriggerEvent::Stop | TriggerEvent::Cancel => {
            if let Some(id) = esc_id.take() {
                if let Err(e) = manager.unregister(id) {
                    tracing::debug!(error = %e, "注销 ESC 热键失败,可能已自动卸");
                }
            }
        }
    }
}
