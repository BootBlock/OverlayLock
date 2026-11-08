//! Process tests: the built executable locks, and unlocks by password, Alt+F4 and timeout.

mod common;

use std::thread::sleep;
use std::time::{Duration, Instant};

use common::{Lock, make_hash, post, post_text, serial};
use windows::Win32::UI::WindowsAndMessaging::{SC_CLOSE, WM_CLOSE, WM_SYSCOMMAND};

const SAFE: [&str; 6] = ["-p", "go", "--monitors", "cursor", "--timeout", "20s"];

fn args(extra: &[&'static str]) -> Vec<&'static str> {
    SAFE.iter().copied().chain(extra.iter().copied()).collect()
}

#[test]
fn password_after_noise_unlocks() {
    let _serial = serial();
    let mut lock = Lock::start(&SAFE);
    post_text(lock.overlay(), "xgo");
    assert_eq!(lock.exit_code_within(Duration::from_secs(2)), Some(0));
}

#[test]
fn wrong_order_stays_locked() {
    let _serial = serial();
    let mut lock = Lock::start(&SAFE);
    let overlay = lock.overlay();
    post_text(overlay, "gx");
    sleep(Duration::from_secs(1));
    assert!(lock.is_running(), "'gx' must not unlock 'go'");
    post_text(overlay, "go");
    assert_eq!(lock.exit_code_within(Duration::from_secs(2)), Some(0));
}

#[test]
fn hashed_password_after_noise_unlocks() {
    let _serial = serial();
    let hash = make_hash("go");
    let mut lock = Lock::start(&[
        "--password-hash",
        &hash,
        "--monitors",
        "cursor",
        "--timeout",
        "20s",
    ]);
    let overlay = lock.overlay();
    post_text(overlay, "gx");
    sleep(Duration::from_secs(1));
    assert!(lock.is_running(), "'gx' must not unlock the hash of 'go'");
    post_text(overlay, "xgo");
    assert_eq!(lock.exit_code_within(Duration::from_secs(2)), Some(0));
}

#[test]
fn alt_f4_unlocks() {
    let _serial = serial();
    let mut lock = Lock::start(&SAFE);
    post(lock.overlay(), WM_SYSCOMMAND, SC_CLOSE as usize);
    assert_eq!(lock.exit_code_within(Duration::from_secs(2)), Some(0));
}

#[test]
fn ignore_alt_f4_stays_locked() {
    let _serial = serial();
    let mut lock = Lock::start(&args(&["--ignore-alt-f4"]));
    let overlay = lock.overlay();
    post(overlay, WM_SYSCOMMAND, SC_CLOSE as usize);
    post(overlay, WM_CLOSE, 0);
    sleep(Duration::from_secs(1));
    assert!(
        lock.is_running(),
        "Alt+F4 and WM_CLOSE must not unlock with --ignore-alt-f4"
    );
    post_text(overlay, "go");
    assert_eq!(lock.exit_code_within(Duration::from_secs(2)), Some(0));
}

#[test]
fn timeout_unlocks() {
    let _serial = serial();
    let started = Instant::now();
    let mut lock = Lock::start(&["-p", "go", "--monitors", "cursor", "--timeout", "2s"]);
    lock.overlay();
    assert_eq!(lock.exit_code_within(Duration::from_secs(4)), Some(0));
    let elapsed = started.elapsed();
    assert!(
        elapsed >= Duration::from_secs(2),
        "unlocked after only {elapsed:?}"
    );
}

#[test]
fn second_instance_exits_3() {
    let _serial = serial();
    let mut first = Lock::start(&SAFE);
    let overlay = first.overlay();
    let mut second = Lock::start(&SAFE);
    assert_eq!(second.exit_code_within(Duration::from_secs(1)), Some(3));
    assert!(first.is_running(), "the first instance must stay locked");
    post_text(overlay, "go");
    assert_eq!(first.exit_code_within(Duration::from_secs(2)), Some(0));
}
