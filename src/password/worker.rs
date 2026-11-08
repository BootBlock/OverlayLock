//! The match thread. It owns the [`PasswordMatcher`] and takes typed units from the keyboard hook
//! and the overlay windows through a bounded queue, so a hashed check never runs inside a hook
//! (which Windows removes when it is slow) or a window procedure (OL6).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::thread::{self, JoinHandle};

use windows::Win32::Foundation::E_FAIL;

use super::{Password, PasswordMatcher};

/// How many typed units may wait for the match thread. A burst beyond it, which only a flood of
/// injected input could cause, loses units and restarts the match (see [`UnitSender::push`]).
const QUEUE_CAPACITY: usize = 1_024;

/// One message to the match thread.
enum Message {
    /// A typed unit. `after_gap` is `true` if units were lost or cleared before it, so the match
    /// must start again from this unit.
    Unit { unit: u16, after_gap: bool },
    /// Ends the thread.
    Stop,
}

/// Sends typed units to the match thread. Cheap to clone; every clone feeds the same matcher.
#[derive(Clone)]
pub struct UnitSender {
    queue: SyncSender<Message>,
    /// `true` while the next unit must restart the match.
    gap: Arc<AtomicBool>,
}

impl UnitSender {
    /// Queues one typed unit. Never blocks and never allocates, so the keyboard hook may call it.
    ///
    /// Returns `true` if the unit was queued. A unit that does not fit is lost, and the next
    /// queued unit restarts the match, so units on either side of the loss never join into the
    /// password.
    pub fn push(&self, unit: u16) -> bool {
        let after_gap = self.gap.swap(false, Ordering::AcqRel);
        let queued = self
            .queue
            .try_send(Message::Unit { unit, after_gap })
            .is_ok();
        if !queued {
            self.gap.store(true, Ordering::Release);
        }
        queued
    }

    /// Makes the next unit pushed restart the match, so no unit pushed before this call joins a
    /// unit pushed after it. Units already queued are still matched, in order.
    pub fn clear(&self) {
        self.gap.store(true, Ordering::Release);
    }
}

/// The running match thread.
pub struct MatchThread {
    units: UnitSender,
    stop: Arc<AtomicBool>,
    handle: JoinHandle<()>,
}

impl MatchThread {
    /// Starts the match thread for `password`, and waits until its matcher is ready. The thread
    /// calls `on_match` each time the typed units are the password.
    pub fn start(
        password: Password,
        on_match: impl FnMut() + Send + 'static,
    ) -> windows::core::Result<Self> {
        let (queue, receiver) = mpsc::sync_channel(QUEUE_CAPACITY);
        let (report, ready) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop);
        let handle = thread::Builder::new()
            .name("overlay-lock-match".to_owned())
            .spawn(move || match PasswordMatcher::new(password) {
                Ok(matcher) => {
                    let _ = report.send(Ok(()));
                    run(matcher, &receiver, &thread_stop, on_match);
                }
                Err(error) => {
                    let _ = report.send(Err(error));
                }
            })
            .map_err(|error| windows::core::Error::new(E_FAIL, error.to_string()))?;
        let started = ready.recv().unwrap_or_else(|_| {
            Err(windows::core::Error::new(
                E_FAIL,
                "the password match thread ended before it was ready",
            ))
        });
        if let Err(error) = started {
            let _ = handle.join();
            return Err(error);
        }
        Ok(Self {
            units: UnitSender {
                queue,
                gap: Arc::new(AtomicBool::new(false)),
            },
            stop,
            handle,
        })
    }

    /// Gets the sender that feeds this thread.
    pub fn units(&self) -> &UnitSender {
        &self.units
    }

    /// Ends the thread without matching the units still queued, and waits for it, which takes at
    /// most one check.
    pub fn stop(self) {
        self.stop.store(true, Ordering::Release);
        // The thread reads the flag before each message, so this send waits at most one check
        // for room, and fails at once if the thread is gone.
        let _ = self.units.queue.send(Message::Stop);
        let _ = self.handle.join();
    }
}

/// The match thread's loop: matches each unit until `Stop`, the stop flag or every sender is gone.
fn run(
    mut matcher: PasswordMatcher,
    receiver: &Receiver<Message>,
    stop: &AtomicBool,
    mut on_match: impl FnMut(),
) {
    while let Ok(message) = receiver.recv() {
        if stop.load(Ordering::Acquire) {
            return;
        }
        match message {
            Message::Stop => return,
            Message::Unit { unit, after_gap } => {
                if after_gap {
                    matcher.clear();
                }
                if matcher.push(unit) {
                    on_match();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;
    use std::time::Duration;

    use super::MatchThread;
    use crate::password::Password;
    use crate::password::hash::PasswordHash;

    const WAIT: Duration = Duration::from_secs(5);

    fn utf16(text: &str) -> Vec<u16> {
        text.encode_utf16().collect()
    }

    /// Starts a match thread that reports each match on the returned receiver.
    fn start(password: Password) -> (MatchThread, mpsc::Receiver<()>) {
        let (report, matches) = mpsc::channel();
        let thread = MatchThread::start(password, move || {
            let _ = report.send(());
        })
        .expect("the match thread starts");
        (thread, matches)
    }

    fn type_text(thread: &MatchThread, text: &str) {
        for unit in text.encode_utf16() {
            assert!(thread.units().push(unit), "the queue has room");
        }
    }

    #[test]
    fn a_hashed_password_typed_after_noise_matches_once() {
        let hash = PasswordHash::make(&utf16("go")).unwrap();
        let (thread, matches) = start(Password::Hash(hash));
        type_text(&thread, "xgxg");
        type_text(&thread, "o");
        matches.recv_timeout(WAIT).expect("the password matches");
        assert!(matches.try_recv().is_err(), "only one match");
        thread.stop();
    }

    #[test]
    fn clear_restarts_the_match() {
        let (thread, matches) = start(Password::Plain(utf16("go")));
        type_text(&thread, "g");
        thread.units().clear();
        type_text(&thread, "o");
        type_text(&thread, "go");
        matches.recv_timeout(WAIT).expect("the second go matches");
        assert!(
            matches.recv_timeout(Duration::from_millis(300)).is_err(),
            "the g before the clear and the o after it never match"
        );
        thread.stop();
    }

    /// A unit lost to a full queue restarts the match, so a "g" queued before the loss and an "o"
    /// queued after it never match "go".
    #[test]
    fn a_unit_lost_to_a_full_queue_restarts_the_match() {
        let (release, released) = mpsc::channel::<()>();
        let (report, matches) = mpsc::channel();
        let mut first = true;
        let thread = MatchThread::start(Password::Plain(utf16("go")), move || {
            let _ = report.send(());
            if std::mem::take(&mut first) {
                // Hold the thread, so the queue fills behind it.
                let _ = released.recv();
            }
        })
        .expect("the match thread starts");
        type_text(&thread, "go");
        matches.recv_timeout(WAIT).expect("the first go matches");
        // The thread is held: fill the queue with "g" until one is lost.
        assert!(thread.units().push(u16::from(b'g')));
        while thread.units().push(u16::from(b'g')) {}
        release.send(()).unwrap();
        while !thread.units().push(u16::from(b'o')) {}
        assert!(
            matches.recv_timeout(Duration::from_millis(500)).is_err(),
            "the g before the loss and the o after it never match"
        );
        type_text(&thread, "go");
        matches
            .recv_timeout(WAIT)
            .expect("a whole go still matches");
        thread.stop();
    }
}
