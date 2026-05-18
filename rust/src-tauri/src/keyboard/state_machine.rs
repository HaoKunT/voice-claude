//! 触发模式状态机(纯逻辑,无 IO,可注入假事件源单测)。
//!
//! 两条路径:
//! - **HotkeyManager 路径**(Toggle / PushToTalk):`ManagerStateMachine`,输入
//!   `HotkeyKind`(主热键 / ESC) + `HotkeyState`(Pressed / Released)
//! - **KeyboardListener 路径**(DoubleTapHold):`DoubleTapStateMachine`,输入
//!   raw `KeyEvent` + 当前 `Instant`(假时钟便于单测 350ms 超时)
//!
//! "哪个 id / 哪个 modifier 对应啥" 的判断在 `backend.rs` 里做,本模块只关心
//! 状态机逻辑。

use std::time::{Duration, Instant};

use handy_keys::{HotkeyState, Key, KeyEvent, Modifiers};

/// DoubleTapHold 双击窗口。350ms 跟 macOS 系统 dictation "双击 Fn" 风格保持
/// 接近 —— 太短(<300ms)正常用户跟不上,太长(>500ms)误触率高。
pub const DOUBLE_TAP_WINDOW: Duration = Duration::from_millis(350);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerMode {
    Toggle,
    PushToTalk,
    /// 占位,P2 实现
    DoubleTapHold,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerEvent {
    Start,
    Stop,
    Cancel,
}

/// `backend.rs` 在拿到 `HotkeyEvent` 后,按 id 分类成这个枚举喂给状态机。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyKind {
    Main,
    Escape,
}

/// HotkeyManager 路径(Toggle / PushToTalk)的状态机。
pub struct ManagerStateMachine {
    mode: TriggerMode,
    /// Toggle 模式下当前是否在录音中。Pressed 翻转。
    /// 跨"录音真实是否在跑"是 best-effort —— 真实状态在 recorder 里,
    /// 这里只是给"再按一下停"的判断用。Cancel 时会被 reset 回 false,
    /// 让用户按 ESC 取消后再按主热键能立即开新一轮录音。
    toggle_recording: bool,
}

impl ManagerStateMachine {
    pub fn new(mode: TriggerMode) -> Self {
        Self {
            mode,
            toggle_recording: false,
        }
    }

    /// 处理一个分类完成的事件。
    pub fn handle(&mut self, kind: HotkeyKind, state: HotkeyState) -> Option<TriggerEvent> {
        // ESC Pressed:任何模式立即 Cancel,同步把 toggle 状态 reset。
        if kind == HotkeyKind::Escape {
            return match state {
                HotkeyState::Pressed => {
                    self.toggle_recording = false;
                    Some(TriggerEvent::Cancel)
                }
                HotkeyState::Released => None,
            };
        }

        // Main hotkey
        match (self.mode, state) {
            (TriggerMode::Toggle, HotkeyState::Pressed) => {
                if self.toggle_recording {
                    self.toggle_recording = false;
                    Some(TriggerEvent::Stop)
                } else {
                    self.toggle_recording = true;
                    Some(TriggerEvent::Start)
                }
            }
            (TriggerMode::Toggle, HotkeyState::Released) => None,
            (TriggerMode::PushToTalk, HotkeyState::Pressed) => Some(TriggerEvent::Start),
            (TriggerMode::PushToTalk, HotkeyState::Released) => Some(TriggerEvent::Stop),
            (TriggerMode::DoubleTapHold, _) => None,
        }
    }
}

/// DoubleTapHold 状态机内部状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DthState {
    /// 等第一次按下
    Idle,
    /// 第一次按下了,等松开
    FirstHeld,
    /// 松开后等 350ms 内再按下
    WaitSecond { until: Instant },
    /// 第二次按下后录音中,等松开
    Recording,
}

/// DoubleTapHold 路径的状态机。基于 raw `KeyEvent` 驱动。
///
/// 关键不变量:
/// - FirstDown 不立即 emit Start —— 必须等 FirstUp + SecondDown 才确认是双击 hold
/// - WaitSecond 期间按其他主键 / 其他 modifier → reset Idle(用户放弃了)
/// - WaitSecond 超时 → reset(下一帧 `tick` 自动触发,或下次事件来时检查)
/// - Recording 中按 ESC down → emit Cancel
/// - Recording 中松开目标 modifier → emit Stop
///
/// supervisor 在两次 listener.recv 之间会调 `tick()` 让 WaitSecond 超时及时
/// reset,即使用户没再按任何键。
pub struct DoubleTapStateMachine {
    target: Modifiers,
    state: DthState,
}

impl DoubleTapStateMachine {
    pub fn new(target: Modifiers) -> Self {
        Self {
            target,
            state: DthState::Idle,
        }
    }

    /// 周期性 tick:WaitSecond 超时则 reset Idle。supervisor 应在每次 sleep
    /// 之后调一下,确保用户单击后没第二次按下时也能及时回 Idle。
    pub fn tick(&mut self, now: Instant) {
        if let DthState::WaitSecond { until } = self.state {
            if now >= until {
                self.state = DthState::Idle;
            }
        }
    }

    pub fn handle(&mut self, ev: &KeyEvent, now: Instant) -> Option<TriggerEvent> {
        // Recording 中按 ESC down → Cancel(优先级最高)
        if matches!(self.state, DthState::Recording)
            && ev.is_key_down
            && ev.key == Some(Key::Escape)
        {
            self.state = DthState::Idle;
            return Some(TriggerEvent::Cancel);
        }

        // 非 Recording 状态下,任何主键(非纯 modifier)按下都 reset:
        // - WaitSecond 阶段被主键打断 → 用户在用快捷键,放弃双击
        // - FirstHeld 阶段如果用户先按 modifier 再按主键(像 ⌥+S),也 reset
        // Recording 中按主键不影响(用户可能边录音边操作其他)
        if !matches!(self.state, DthState::Recording) && ev.is_key_down && ev.key.is_some() {
            self.state = DthState::Idle;
            return None;
        }

        // 仅关心 target modifier 的变化。
        // 如果是其他 modifier 改变(用户在 WaitSecond 期间按了别的修饰键),也 reset。
        if ev.changed_modifier != Some(self.target) {
            if matches!(self.state, DthState::WaitSecond { .. }) && ev.changed_modifier.is_some() {
                self.state = DthState::Idle;
            }
            return None;
        }

        // 现在 changed_modifier == target,is_key_down 表示这次 modifier 是按下还是松开
        match (self.state, ev.is_key_down) {
            (DthState::Idle, true) => {
                self.state = DthState::FirstHeld;
                None
            }
            (DthState::FirstHeld, false) => {
                self.state = DthState::WaitSecond {
                    until: now + DOUBLE_TAP_WINDOW,
                };
                None
            }
            (DthState::WaitSecond { until }, true) => {
                if now < until {
                    self.state = DthState::Recording;
                    Some(TriggerEvent::Start)
                } else {
                    // 超时但用户重新按下 —— 当作新一轮单击的开始
                    self.state = DthState::FirstHeld;
                    None
                }
            }
            (DthState::Recording, false) => {
                self.state = DthState::Idle;
                Some(TriggerEvent::Stop)
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggle_pressed_alternates_start_stop() {
        let mut sm = ManagerStateMachine::new(TriggerMode::Toggle);
        assert_eq!(
            sm.handle(HotkeyKind::Main, HotkeyState::Pressed),
            Some(TriggerEvent::Start)
        );
        assert_eq!(sm.handle(HotkeyKind::Main, HotkeyState::Released), None);
        assert_eq!(
            sm.handle(HotkeyKind::Main, HotkeyState::Pressed),
            Some(TriggerEvent::Stop)
        );
        assert_eq!(
            sm.handle(HotkeyKind::Main, HotkeyState::Pressed),
            Some(TriggerEvent::Start)
        );
    }

    #[test]
    fn ptt_pressed_starts_released_stops() {
        let mut sm = ManagerStateMachine::new(TriggerMode::PushToTalk);
        assert_eq!(
            sm.handle(HotkeyKind::Main, HotkeyState::Pressed),
            Some(TriggerEvent::Start)
        );
        assert_eq!(
            sm.handle(HotkeyKind::Main, HotkeyState::Released),
            Some(TriggerEvent::Stop)
        );
    }

    #[test]
    fn esc_cancels_and_resets_toggle() {
        let mut sm = ManagerStateMachine::new(TriggerMode::Toggle);
        assert_eq!(
            sm.handle(HotkeyKind::Main, HotkeyState::Pressed),
            Some(TriggerEvent::Start)
        );
        assert_eq!(
            sm.handle(HotkeyKind::Escape, HotkeyState::Pressed),
            Some(TriggerEvent::Cancel)
        );
        // 关键:Cancel 后再按主热键应当 Start 而非 Stop —— toggle_recording 已 reset
        assert_eq!(
            sm.handle(HotkeyKind::Main, HotkeyState::Pressed),
            Some(TriggerEvent::Start)
        );
    }

    #[test]
    fn esc_cancels_in_ptt() {
        let mut sm = ManagerStateMachine::new(TriggerMode::PushToTalk);
        sm.handle(HotkeyKind::Main, HotkeyState::Pressed);
        assert_eq!(
            sm.handle(HotkeyKind::Escape, HotkeyState::Pressed),
            Some(TriggerEvent::Cancel)
        );
    }

    #[test]
    fn esc_released_ignored() {
        let mut sm = ManagerStateMachine::new(TriggerMode::Toggle);
        assert_eq!(sm.handle(HotkeyKind::Escape, HotkeyState::Released), None);
    }

    #[test]
    fn double_tap_hold_via_manager_path_is_noop() {
        // DoubleTapHold 不应经由 HotkeyManager(它走 KeyboardListener),
        // 这里的状态机收到也安全 no-op
        let mut sm = ManagerStateMachine::new(TriggerMode::DoubleTapHold);
        assert_eq!(sm.handle(HotkeyKind::Main, HotkeyState::Pressed), None);
        assert_eq!(sm.handle(HotkeyKind::Main, HotkeyState::Released), None);
    }

    // ── DoubleTapStateMachine 单测 ───────────────────────────────────

    fn modifier_event(changed: Modifiers, is_down: bool) -> KeyEvent {
        KeyEvent {
            modifiers: if is_down { changed } else { Modifiers::empty() },
            key: None,
            is_key_down: is_down,
            changed_modifier: Some(changed),
        }
    }

    fn key_event(key: Key, is_down: bool) -> KeyEvent {
        KeyEvent {
            modifiers: Modifiers::empty(),
            key: Some(key),
            is_key_down: is_down,
            changed_modifier: None,
        }
    }

    #[test]
    fn dth_full_path_emits_start_and_stop() {
        let mut sm = DoubleTapStateMachine::new(Modifiers::OPT_RIGHT);
        let t0 = Instant::now();
        // Idle → FirstHeld
        assert_eq!(
            sm.handle(&modifier_event(Modifiers::OPT_RIGHT, true), t0),
            None
        );
        // FirstHeld → WaitSecond
        assert_eq!(
            sm.handle(
                &modifier_event(Modifiers::OPT_RIGHT, false),
                t0 + Duration::from_millis(50)
            ),
            None
        );
        // WaitSecond + (now < until) + SecondDown → Recording, emit Start
        assert_eq!(
            sm.handle(
                &modifier_event(Modifiers::OPT_RIGHT, true),
                t0 + Duration::from_millis(150)
            ),
            Some(TriggerEvent::Start)
        );
        // Recording + SecondUp → Idle, emit Stop
        assert_eq!(
            sm.handle(
                &modifier_event(Modifiers::OPT_RIGHT, false),
                t0 + Duration::from_millis(800)
            ),
            Some(TriggerEvent::Stop)
        );
    }

    #[test]
    fn dth_wait_second_timeout_resets_via_tick() {
        let mut sm = DoubleTapStateMachine::new(Modifiers::OPT_RIGHT);
        let t0 = Instant::now();
        sm.handle(&modifier_event(Modifiers::OPT_RIGHT, true), t0);
        sm.handle(
            &modifier_event(Modifiers::OPT_RIGHT, false),
            t0 + Duration::from_millis(50),
        );
        // 超过 350ms 还没第二次按下 → tick 把 state 拉回 Idle
        sm.tick(t0 + Duration::from_millis(500));
        // 此后再按下,应该是新的 FirstHeld(不是 SecondDown 进 Recording)
        assert_eq!(
            sm.handle(
                &modifier_event(Modifiers::OPT_RIGHT, true),
                t0 + Duration::from_millis(600)
            ),
            None
        );
    }

    #[test]
    fn dth_main_key_during_wait_second_resets() {
        let mut sm = DoubleTapStateMachine::new(Modifiers::OPT_RIGHT);
        let t0 = Instant::now();
        sm.handle(&modifier_event(Modifiers::OPT_RIGHT, true), t0);
        sm.handle(
            &modifier_event(Modifiers::OPT_RIGHT, false),
            t0 + Duration::from_millis(50),
        );
        // WaitSecond 期间按主键 → reset Idle
        sm.handle(&key_event(Key::A, true), t0 + Duration::from_millis(100));
        // 接下来再按 modifier 不应进 Recording(应进 FirstHeld)
        assert_eq!(
            sm.handle(
                &modifier_event(Modifiers::OPT_RIGHT, true),
                t0 + Duration::from_millis(150)
            ),
            None
        );
    }

    #[test]
    fn dth_other_modifier_during_wait_second_resets() {
        let mut sm = DoubleTapStateMachine::new(Modifiers::OPT_RIGHT);
        let t0 = Instant::now();
        sm.handle(&modifier_event(Modifiers::OPT_RIGHT, true), t0);
        sm.handle(
            &modifier_event(Modifiers::OPT_RIGHT, false),
            t0 + Duration::from_millis(50),
        );
        // 按了别的 modifier(比如 left shift)→ reset
        sm.handle(
            &modifier_event(Modifiers::SHIFT_LEFT, true),
            t0 + Duration::from_millis(100),
        );
        // 再按 target 不应进 Recording
        assert_eq!(
            sm.handle(
                &modifier_event(Modifiers::OPT_RIGHT, true),
                t0 + Duration::from_millis(150)
            ),
            None
        );
    }

    #[test]
    fn dth_esc_in_recording_cancels() {
        let mut sm = DoubleTapStateMachine::new(Modifiers::OPT_RIGHT);
        let t0 = Instant::now();
        sm.handle(&modifier_event(Modifiers::OPT_RIGHT, true), t0);
        sm.handle(
            &modifier_event(Modifiers::OPT_RIGHT, false),
            t0 + Duration::from_millis(50),
        );
        sm.handle(
            &modifier_event(Modifiers::OPT_RIGHT, true),
            t0 + Duration::from_millis(100),
        );
        // 现在在 Recording,按 ESC down
        assert_eq!(
            sm.handle(
                &key_event(Key::Escape, true),
                t0 + Duration::from_millis(200)
            ),
            Some(TriggerEvent::Cancel)
        );
        // ESC 之后即使松开 modifier 也不再 emit Stop(state 已 Idle)
        assert_eq!(
            sm.handle(
                &modifier_event(Modifiers::OPT_RIGHT, false),
                t0 + Duration::from_millis(300)
            ),
            None
        );
    }

    #[test]
    fn dth_target_modifier_left_vs_right_distinct() {
        // 配置 right_option,只对 OPT_RIGHT 响应,OPT_LEFT 应忽略
        let mut sm = DoubleTapStateMachine::new(Modifiers::OPT_RIGHT);
        let t0 = Instant::now();
        // 按左 ⌥ 双击 — 不应进 Recording
        sm.handle(&modifier_event(Modifiers::OPT_LEFT, true), t0);
        sm.handle(
            &modifier_event(Modifiers::OPT_LEFT, false),
            t0 + Duration::from_millis(50),
        );
        assert_eq!(
            sm.handle(
                &modifier_event(Modifiers::OPT_LEFT, true),
                t0 + Duration::from_millis(100)
            ),
            None
        );
    }

    #[test]
    fn dth_other_key_in_recording_does_not_stop() {
        let mut sm = DoubleTapStateMachine::new(Modifiers::OPT_RIGHT);
        let t0 = Instant::now();
        sm.handle(&modifier_event(Modifiers::OPT_RIGHT, true), t0);
        sm.handle(
            &modifier_event(Modifiers::OPT_RIGHT, false),
            t0 + Duration::from_millis(50),
        );
        sm.handle(
            &modifier_event(Modifiers::OPT_RIGHT, true),
            t0 + Duration::from_millis(100),
        );
        // 现在 Recording 中,按主键不应 emit 任何东西
        assert_eq!(
            sm.handle(&key_event(Key::A, true), t0 + Duration::from_millis(200)),
            None
        );
        // 松开 modifier 仍正常 emit Stop
        assert_eq!(
            sm.handle(
                &modifier_event(Modifiers::OPT_RIGHT, false),
                t0 + Duration::from_millis(300)
            ),
            Some(TriggerEvent::Stop)
        );
    }
}
