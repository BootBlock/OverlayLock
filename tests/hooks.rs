//! Hook tests: real `SendInput` keyboard and mouse input against `--block-keys` and
//! `--block-mouse`. They take over the desktop input for a few seconds each, so they are
//! `#[ignore]` and run by hand: `cargo test --test hooks -- --ignored --test-threads=1`.

mod common;

use std::thread::sleep;
use std::time::Duration;

use common::input::{
    EditWindow, click_and_scroll, cursor, move_mouse, send_key, type_keys, type_unicode,
};
use common::{Lock, make_hash, post_text, serial};
use windows::Win32::UI::Input::KeyboardAndMouse::{VIRTUAL_KEY, VK_F4, VK_LMENU};

const G: VIRTUAL_KEY = VIRTUAL_KEY(b'G' as u16);
const O: VIRTUAL_KEY = VIRTUAL_KEY(b'O' as u16);
const X: VIRTUAL_KEY = VIRTUAL_KEY(b'X' as u16);

const KEYS: [&str; 7] = [
    "-p",
    "go",
    "--block-keys",
    "--monitors",
    "cursor",
    "--timeout",
    "20s",
];

fn press_alt_f4() {
    send_key(VK_LMENU, true);
    send_key(VK_F4, true);
    send_key(VK_F4, false);
    send_key(VK_LMENU, false);
}

#[test]
#[ignore = "takes over the keyboard; run by hand"]
fn typed_password_unlocks() {
    let _serial = serial();
    let mut lock = Lock::start(&KEYS);
    lock.overlay();
    type_keys(&[X, G, O]);
    assert_eq!(lock.exit_code_within(Duration::from_secs(2)), Some(0));
}

#[test]
#[ignore = "takes over the keyboard; run by hand"]
fn typed_password_unlocks_a_hashed_lock() {
    let _serial = serial();
    let hash = make_hash("go");
    let mut lock = Lock::start(&[
        "--password-hash",
        &hash,
        "--block-keys",
        "--monitors",
        "cursor",
        "--timeout",
        "20s",
    ]);
    lock.overlay();
    type_keys(&[G, X, G]);
    sleep(Duration::from_millis(500));
    assert!(lock.is_running(), "'gxg' must not unlock the hash of 'go'");
    type_keys(&[O]);
    assert_eq!(lock.exit_code_within(Duration::from_secs(2)), Some(0));
}

#[test]
#[ignore = "takes over the keyboard; run by hand"]
fn unicode_packets_unlock() {
    let _serial = serial();
    let mut lock = Lock::start(&KEYS);
    lock.overlay();
    type_unicode("xgo");
    assert_eq!(lock.exit_code_within(Duration::from_secs(2)), Some(0));
}

#[test]
#[ignore = "takes over the keyboard; run by hand"]
fn focused_window_receives_no_keys() {
    let _serial = serial();
    let edit = EditWindow::open();
    let mut lock = Lock::start(&KEYS);
    lock.overlay();
    // Wait until the app's focus retry (at most 2 s, OL10) has ended, or it takes the focus back.
    sleep(Duration::from_millis(2500));
    assert!(edit.take_focus(), "the test window did not get the focus");
    type_keys(&[X, G, O]);
    assert_eq!(lock.exit_code_within(Duration::from_secs(2)), Some(0));
    assert_eq!(
        edit.text(),
        "",
        "the focused window received swallowed keys"
    );
}

#[test]
#[ignore = "takes over the keyboard; run by hand"]
fn alt_f4_unlocks_in_hook_mode() {
    let _serial = serial();
    let mut lock = Lock::start(&KEYS);
    lock.overlay();
    press_alt_f4();
    assert_eq!(lock.exit_code_within(Duration::from_secs(2)), Some(0));
}

#[test]
#[ignore = "takes over the keyboard; run by hand"]
fn ignore_alt_f4_in_hook_mode() {
    let _serial = serial();
    let args: Vec<&str> = KEYS.iter().copied().chain(["--ignore-alt-f4"]).collect();
    let mut lock = Lock::start(&args);
    lock.overlay();
    press_alt_f4();
    sleep(Duration::from_secs(1));
    assert!(
        lock.is_running(),
        "Alt+F4 must not unlock with --ignore-alt-f4"
    );
    type_keys(&[G, O]);
    assert_eq!(lock.exit_code_within(Duration::from_secs(2)), Some(0));
}

#[test]
#[ignore = "takes over the mouse; run by hand"]
fn mouse_is_swallowed_until_unlock() {
    let _serial = serial();
    let mut lock = Lock::start(&[
        "-p",
        "go",
        "--block-mouse",
        "--monitors",
        "cursor",
        "--timeout",
        "20s",
    ]);
    let overlay = lock.overlay();
    sleep(Duration::from_millis(300));
    let before = cursor();
    move_mouse(200, 0);
    sleep(Duration::from_millis(200));
    assert_eq!(
        cursor(),
        before,
        "the cursor moved while the mouse was blocked"
    );
    post_text(overlay, "go");
    assert_eq!(lock.exit_code_within(Duration::from_secs(2)), Some(0));
    // Move one way and back the other on both axes: an edge stops at most one direction of each.
    move_mouse(-200, -200);
    move_mouse(400, 400);
    sleep(Duration::from_millis(200));
    assert_ne!(cursor(), before, "the cursor did not move after the unlock");
}

#[test]
#[ignore = "takes over the mouse; run by hand"]
fn typed_password_unlocks_after_swallowed_clicks() {
    let _serial = serial();
    let mut lock = Lock::start(&[
        "-p",
        "go",
        "--block-mouse",
        "--monitors",
        "cursor",
        "--timeout",
        "20s",
    ]);
    lock.overlay();
    move_mouse(300, 0);
    click_and_scroll();
    move_mouse(-300, 0);
    click_and_scroll();
    sleep(Duration::from_millis(300));
    type_keys(&[X, G, O]);
    assert_eq!(lock.exit_code_within(Duration::from_secs(2)), Some(0));
}
