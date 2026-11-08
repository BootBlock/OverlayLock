//! The `WH_KEYBOARD_LL` procedure. It swallows every key event and feeds key-downs through the
//! hook thread's [`KeyTranslator`] into its [`PasswordMatcher`].
//!
//! The procedure never allocates and never blocks: Windows removes a hook silently when its
//! callback is slow. Its only cross-thread call is `PostMessageW` to the controller.

use std::cell::RefCell;

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, HC_ACTION, KBDLLHOOKSTRUCT, PostMessageW, WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN,
    WM_SYSKEYUP,
};

use super::translate::{KeyOutcome, KeyTranslator};
use crate::overlay::WM_APP_UNLOCK;
use crate::password::PasswordMatcher;

/// What the keyboard hook needs, moved onto the hook thread.
pub struct KeyboardConfig {
    /// The password matcher, taken out of the app for the life of the hook (OL6).
    pub matcher: PasswordMatcher,
    /// `true` if Alt+F4 unlocks; otherwise, `false`.
    pub alt_f4_unlocks: bool,
}

/// The keyboard hook's state on the hook thread.
pub struct KeyboardState {
    translator: KeyTranslator,
    matcher: PasswordMatcher,
    controller: HWND,
    alt_f4_unlocks: bool,
}

impl KeyboardState {
    /// Initialises a new instance of the [`KeyboardState`] type.
    pub fn new(config: KeyboardConfig, translator: KeyTranslator, controller: HWND) -> Self {
        Self {
            translator,
            matcher: config.matcher,
            controller,
            alt_f4_unlocks: config.alt_f4_unlocks,
        }
    }

    /// Reloads the held keys after a visit to another desktop.
    pub fn resync(&mut self) {
        self.translator.resync_held_keys();
    }

    /// Gives the password matcher back when the hook ends.
    pub fn into_matcher(self) -> PasswordMatcher {
        self.matcher
    }

    /// Applies one key event. While `paused`, it only keeps the key state in step.
    fn handle(&mut self, event: &KBDLLHOOKSTRUCT, down: bool, paused: bool) {
        let outcome = self
            .translator
            .key_event(event.vkCode, event.scanCode, down);
        if paused {
            return;
        }
        match outcome {
            KeyOutcome::Nothing => {}
            KeyOutcome::AltF4 => {
                if self.alt_f4_unlocks {
                    self.unlock();
                }
            }
            KeyOutcome::Typed { units, len } => {
                for &unit in &units[..len] {
                    if self.matcher.push(unit) {
                        self.unlock();
                    }
                }
            }
        }
    }

    fn unlock(&self) {
        // SAFETY: posting plain integers to the controller window never blocks.
        let _ = unsafe { PostMessageW(Some(self.controller), WM_APP_UNLOCK, WPARAM(0), LPARAM(0)) };
    }
}

thread_local! {
    /// The keyboard hook's state. The procedure has no user pointer, so the state lives here, on
    /// the hook thread, which is the only thread that runs the procedure.
    pub static KEYBOARD: RefCell<Option<KeyboardState>> = const { RefCell::new(None) };
}

/// The `WH_KEYBOARD_LL` procedure: swallows every key event (OL11), except while the session is
/// locked, when the lock screen on the same desktop needs the keys to sign in.
pub extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if u32::try_from(code) != Ok(HC_ACTION) {
        // SAFETY: passes a notification that is not an action on unchanged.
        return unsafe { CallNextHookEx(None, code, wparam, lparam) };
    }
    // SAFETY: for `HC_ACTION`, `lParam` points to the event's `KBDLLHOOKSTRUCT`.
    let event = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
    let down = match u32::try_from(wparam.0) {
        Ok(WM_KEYDOWN | WM_SYSKEYDOWN) => Some(true),
        Ok(WM_KEYUP | WM_SYSKEYUP) => Some(false),
        _ => None,
    };
    let paused = super::PAUSED.get();
    if let Some(down) = down {
        // Injected input (a Stream Deck text action, `SendInput`) counts like physical input.
        KEYBOARD.with_borrow_mut(|state| {
            if let Some(state) = state {
                state.handle(event, down, paused);
            }
        });
    }
    if paused {
        // SAFETY: passes the event on unchanged while the session is locked.
        return unsafe { CallNextHookEx(None, code, wparam, lparam) };
    }
    LRESULT(1)
}
