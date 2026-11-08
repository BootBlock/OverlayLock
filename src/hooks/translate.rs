//! Turns low-level key events into UTF-16 code units with the hook thread's own key state.
//!
//! A swallowed key never updates the system key state, so the translator keeps its own 256-byte
//! state and passes it to `ToUnicodeEx` with flag `0x4`, which leaves the kernel's keyboard state
//! (and its dead-key buffer) alone. A dead key therefore gives no character in hook mode.

use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, GetKeyState, HKL, ToUnicodeEx, VIRTUAL_KEY, VK_CAPITAL, VK_CONTROL, VK_F4,
    VK_LCONTROL, VK_LMENU, VK_LSHIFT, VK_MENU, VK_NUMLOCK, VK_PACKET, VK_RCONTROL, VK_RMENU,
    VK_RSHIFT, VK_SHIFT,
};

/// `ToUnicodeEx` flag: do not change the kernel keyboard state (Windows 10 1607 and later).
const DO_NOT_CHANGE_KEYBOARD_STATE: u32 = 0x4;

/// The bit of a key state byte that means "down".
const DOWN: u8 = 0x80;

/// The bit of a key state byte that means "toggled on" (Caps Lock, Num Lock).
const TOGGLED: u8 = 0x01;

/// The most UTF-16 units one key can give.
pub const MAX_UNITS: usize = 8;

/// What one key event means for the lock.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyOutcome {
    /// Nothing to do.
    Nothing,
    /// Alt+F4 was pressed.
    AltF4,
    /// The key typed these UTF-16 units (the first `len` of `units`).
    Typed {
        /// The units, of which the first `len` are valid.
        units: [u16; MAX_UNITS],
        /// How many units are valid.
        len: usize,
    },
}

/// The hook thread's own keyboard state and layout.
pub struct KeyTranslator {
    state: [u8; 256],
    layout: HKL,
}

impl KeyTranslator {
    /// Initialises a new instance of the [`KeyTranslator`] type with every key up.
    pub fn new(layout: HKL) -> Self {
        Self {
            state: [0; 256],
            layout,
        }
    }

    /// Seeds the key state from the keys held down now and the Caps Lock and Num Lock toggles.
    pub fn seed_from_system(&mut self) {
        self.state = [0; 256];
        self.resync_held_keys();
        for toggle in [VK_CAPITAL, VK_NUMLOCK] {
            // SAFETY: plain query with no preconditions.
            if unsafe { GetKeyState(i32::from(toggle.0)) } & 1 != 0 {
                self.state[usize::from(toggle.0)] |= TOGGLED;
            }
        }
        self.sync_generic_modifiers();
    }

    /// Reloads which keys are held from the system, keeping the toggles.
    ///
    /// The key-ups of a key pressed before a visit to the secure desktop (Ctrl and Alt of
    /// Ctrl+Alt+Del) go to that desktop and never reach the hook, so they would stay down here.
    pub fn resync_held_keys(&mut self) {
        // SAFETY: plain query with no preconditions.
        self.resync_held_keys_from(|vk| unsafe { GetAsyncKeyState(i32::from(vk)) } < 0);
    }

    /// Reloads which keys are held from `held`, which says whether a virtual key is down,
    /// keeping the toggles.
    fn resync_held_keys_from(&mut self, held: impl Fn(u8) -> bool) {
        for vk in 1..=255u8 {
            let slot = &mut self.state[usize::from(vk)];
            *slot = (*slot & TOGGLED) | if held(vk) { DOWN } else { 0 };
        }
        self.sync_generic_modifiers();
    }

    /// Applies one key event and says what it means. It never allocates.
    ///
    /// A `VK_PACKET` event is one UTF-16 unit typed with `KEYEVENTF_UNICODE` (a Stream Deck text
    /// action, an on-screen keyboard): the unit is in the scan code, and no key changes state.
    pub fn key_event(&mut self, vk: u32, scan: u32, down: bool) -> KeyOutcome {
        if vk == u32::from(VK_PACKET.0) {
            let mut units = [0u16; MAX_UNITS];
            return match u16::try_from(scan & 0xFFFF) {
                Ok(unit) if down => {
                    units[0] = unit;
                    KeyOutcome::Typed { units, len: 1 }
                }
                _ => KeyOutcome::Nothing,
            };
        }
        let Some(index) = u8::try_from(vk).ok().map(usize::from) else {
            return KeyOutcome::Nothing;
        };
        if !down {
            self.state[index] &= !DOWN;
            self.sync_generic_modifiers();
            return KeyOutcome::Nothing;
        }
        let was_down = self.state[index] & DOWN != 0;
        self.state[index] |= DOWN;
        if !was_down && (index == key(VK_CAPITAL) || index == key(VK_NUMLOCK)) {
            self.state[index] ^= TOGGLED;
        }
        self.sync_generic_modifiers();

        let alt = self.is_down(VK_MENU);
        let ctrl = self.is_down(VK_CONTROL);
        if index == key(VK_F4) && alt {
            return KeyOutcome::AltF4;
        }
        // Alt without Ctrl is a system key: Windows gives `WM_SYSCHAR`, not `WM_CHAR`, so it types
        // nothing in focus mode either. Ctrl+Alt is AltGr and types.
        if alt && !ctrl {
            return KeyOutcome::Nothing;
        }
        let mut units = [0u16; MAX_UNITS];
        // SAFETY: the state is a full 256-byte array and the buffer is writable; flag 0x4 keeps
        // the kernel keyboard state unchanged.
        let result = unsafe {
            ToUnicodeEx(
                vk,
                scan,
                &self.state,
                &mut units,
                DO_NOT_CHANGE_KEYBOARD_STATE,
                Some(self.layout),
            )
        };
        match usize::try_from(result) {
            Ok(len) if len > 0 => KeyOutcome::Typed {
                units,
                len: len.min(MAX_UNITS),
            },
            // 0: no character. Negative: a dead key, which types nothing in hook mode.
            _ => KeyOutcome::Nothing,
        }
    }

    fn is_down(&self, vk: VIRTUAL_KEY) -> bool {
        self.state[key(vk)] & DOWN != 0
    }

    /// Keeps Shift, Ctrl and Alt in step with their left and right keys, as `ToUnicodeEx` reads
    /// the generic ones.
    fn sync_generic_modifiers(&mut self) {
        for (generic, left, right) in [
            (VK_SHIFT, VK_LSHIFT, VK_RSHIFT),
            (VK_CONTROL, VK_LCONTROL, VK_RCONTROL),
            (VK_MENU, VK_LMENU, VK_RMENU),
        ] {
            let down = self.is_down(left) || self.is_down(right);
            let slot = &mut self.state[key(generic)];
            *slot = if down { *slot | DOWN } else { *slot & !DOWN };
        }
    }
}

fn key(vk: VIRTUAL_KEY) -> usize {
    usize::from(vk.0)
}

#[cfg(test)]
mod tests {
    use super::{KeyOutcome, KeyTranslator};
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        KLF_NOTELLSHELL, LoadKeyboardLayoutW, MAPVK_VK_TO_VSC, MapVirtualKeyW, VIRTUAL_KEY,
        VK_CAPITAL, VK_F4, VK_LMENU, VK_LSHIFT,
    };
    use windows::core::w;

    fn us() -> KeyTranslator {
        // SAFETY: loads the US layout for this test only; KLF_NOTELLSHELL keeps the shell out.
        let layout = unsafe { LoadKeyboardLayoutW(w!("00000409"), KLF_NOTELLSHELL) }
            .expect("the US layout loads");
        KeyTranslator::new(layout)
    }

    fn press(translator: &mut KeyTranslator, vk: VIRTUAL_KEY, down: bool) -> KeyOutcome {
        // SAFETY: plain query with no preconditions.
        let scan = unsafe { MapVirtualKeyW(u32::from(vk.0), MAPVK_VK_TO_VSC) };
        translator.key_event(u32::from(vk.0), scan, down)
    }

    /// Presses and releases a key and returns the text its press typed.
    fn tap(translator: &mut KeyTranslator, vk: VIRTUAL_KEY) -> String {
        let outcome = press(translator, vk, true);
        press(translator, vk, false);
        match outcome {
            KeyOutcome::Typed { units, len } => String::from_utf16_lossy(&units[..len]),
            _ => String::new(),
        }
    }

    const A: VIRTUAL_KEY = VIRTUAL_KEY(b'A' as u16);
    const ONE: VIRTUAL_KEY = VIRTUAL_KEY(b'1' as u16);

    #[test]
    fn letter() {
        assert_eq!(tap(&mut us(), A), "a");
    }

    #[test]
    fn shifted_letter() {
        let mut translator = us();
        press(&mut translator, VK_LSHIFT, true);
        assert_eq!(tap(&mut translator, A), "A");
        press(&mut translator, VK_LSHIFT, false);
        assert_eq!(tap(&mut translator, A), "a");
    }

    #[test]
    fn caps_lock_then_letter() {
        let mut translator = us();
        tap(&mut translator, VK_CAPITAL);
        assert_eq!(tap(&mut translator, A), "A");
        tap(&mut translator, VK_CAPITAL);
        assert_eq!(tap(&mut translator, A), "a");
    }

    #[test]
    fn caps_lock_auto_repeat_toggles_once() {
        let mut translator = us();
        press(&mut translator, VK_CAPITAL, true);
        press(&mut translator, VK_CAPITAL, true);
        press(&mut translator, VK_CAPITAL, false);
        assert_eq!(tap(&mut translator, A), "A");
    }

    #[test]
    fn caps_lock_with_shift_gives_lower_case() {
        let mut translator = us();
        tap(&mut translator, VK_CAPITAL);
        press(&mut translator, VK_LSHIFT, true);
        assert_eq!(tap(&mut translator, A), "a");
    }

    #[test]
    fn shifted_digit() {
        let mut translator = us();
        press(&mut translator, VK_LSHIFT, true);
        assert_eq!(tap(&mut translator, ONE), "!");
    }

    #[test]
    fn alt_f4_is_detected() {
        let mut translator = us();
        press(&mut translator, VK_LMENU, true);
        assert_eq!(press(&mut translator, VK_F4, true), KeyOutcome::AltF4);
    }

    #[test]
    fn unicode_packet_types_its_unit() {
        let mut translator = us();
        let packet = u32::from(super::VK_PACKET.0);
        let outcome = translator.key_event(packet, u32::from('g'), true);
        assert!(
            matches!(outcome, KeyOutcome::Typed { units, len: 1 } if units[0] == u16::from(b'g'))
        );
        assert_eq!(
            translator.key_event(packet, u32::from('g'), false),
            KeyOutcome::Nothing
        );
    }

    #[test]
    fn resync_releases_keys_whose_key_up_was_missed() {
        let mut translator = us();
        press(&mut translator, VK_LMENU, true);
        tap(&mut translator, VK_CAPITAL);
        // The system reports every key up, as after a return from the secure desktop.
        translator.resync_held_keys_from(|_| false);
        assert_eq!(press(&mut translator, VK_F4, true), KeyOutcome::Nothing);
        assert_eq!(
            tap(&mut translator, A),
            "A",
            "Caps Lock must survive the resync"
        );
    }

    #[test]
    fn f4_alone_is_not_alt_f4() {
        assert_eq!(press(&mut us(), VK_F4, true), KeyOutcome::Nothing);
    }

    #[test]
    fn alt_released_then_f4_is_not_alt_f4() {
        let mut translator = us();
        press(&mut translator, VK_LMENU, true);
        press(&mut translator, VK_LMENU, false);
        assert_eq!(press(&mut translator, VK_F4, true), KeyOutcome::Nothing);
    }
}
