//! Finds the display monitors, chooses which of them to cover, and works out how the set of
//! overlays must change when the displays change.

use std::collections::HashSet;

use windows::Win32::Foundation::{LPARAM, POINT, RECT};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITOR_DEFAULTTOPRIMARY, MONITORINFO,
    MONITORINFOEXW, MonitorFromPoint,
};
use windows::Win32::UI::WindowsAndMessaging::{GetCursorPos, MONITORINFOF_PRIMARY};
use windows::core::BOOL;

/// Which monitors the overlays cover.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Monitors {
    /// Every monitor.
    #[default]
    All,
    /// Only the monitor under the cursor at start, or the primary monitor if that one goes.
    Cursor,
}

/// The identity of a monitor: its `MONITORINFOEXW.szDevice` name, trimmed at the first NUL.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct MonitorId(Box<[u16]>);

impl MonitorId {
    /// Initialises a new instance of the [`MonitorId`] type from a device name buffer.
    pub fn from_device(device: &[u16]) -> Self {
        let len = device
            .iter()
            .position(|&unit| unit == 0)
            .unwrap_or(device.len());
        Self(device[..len].into())
    }
}

/// A rectangle in physical pixels, as `RECT` gives it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    /// The x coordinate of the left edge.
    pub left: i32,
    /// The y coordinate of the top edge.
    pub top: i32,
    /// The x coordinate one past the right edge.
    pub right: i32,
    /// The y coordinate one past the bottom edge.
    pub bottom: i32,
}

impl Rect {
    /// Gets the width in pixels.
    pub fn width(self) -> i32 {
        self.right - self.left
    }

    /// Gets the height in pixels.
    pub fn height(self) -> i32 {
        self.bottom - self.top
    }
}

impl From<RECT> for Rect {
    fn from(rect: RECT) -> Self {
        Self {
            left: rect.left,
            top: rect.top,
            right: rect.right,
            bottom: rect.bottom,
        }
    }
}

/// One display monitor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Monitor {
    /// The device name that identifies the monitor.
    pub id: MonitorId,
    /// The full monitor rectangle (`rcMonitor`, never the work area).
    pub rect: Rect,
    /// `true` if this is the primary monitor; otherwise, `false`.
    pub primary: bool,
}

/// One step that brings the overlay set in line with the wanted monitors.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Change {
    /// Make an overlay for a monitor that has none.
    Create(Monitor),
    /// Move and resize the overlay of a monitor whose rectangle changed.
    Move {
        /// The monitor whose overlay moves.
        id: MonitorId,
        /// The monitor's new rectangle.
        rect: Rect,
    },
    /// Destroy the overlay of a monitor that is no longer covered.
    Destroy(MonitorId),
}

/// Gets every display monitor, in the order `EnumDisplayMonitors` gives them.
pub fn enumerate() -> windows::core::Result<Vec<Monitor>> {
    let mut found: Vec<HMONITOR> = Vec::new();
    // SAFETY: `collect_monitor` runs only during this call and receives `found` as a valid,
    // exclusive `*mut Vec<HMONITOR>` through `LPARAM`.
    unsafe {
        EnumDisplayMonitors(
            None,
            None,
            Some(collect_monitor),
            LPARAM(std::ptr::from_mut(&mut found) as isize),
        )
    }
    .ok()?;
    found.into_iter().map(monitor_info).collect()
}

/// Gets the monitor under the cursor, or the primary monitor if the cursor position is unknown
/// (for example while the secure desktop shows).
pub fn monitor_at_cursor() -> windows::core::Result<MonitorId> {
    let mut point = POINT::default();
    // SAFETY: `point` is a valid, writable `POINT`.
    if unsafe { GetCursorPos(&raw mut point) }.is_err() {
        // The primary monitor's top-left corner is always (0, 0).
        point = POINT::default();
    }
    // SAFETY: `MonitorFromPoint` takes the point by value and has no other preconditions.
    let monitor = unsafe { MonitorFromPoint(point, MONITOR_DEFAULTTOPRIMARY) };
    Ok(monitor_info(monitor)?.id)
}

/// Chooses the monitors to cover.
///
/// With [`Monitors::All`] it returns every monitor. With [`Monitors::Cursor`] it returns the
/// `kept` monitor if it still exists, else the primary monitor (else the first, if no monitor is
/// marked primary). The caller keeps the returned monitor's ID as its new `kept` monitor.
pub fn select<'a>(
    all: &'a [Monitor],
    mode: Monitors,
    kept: Option<&MonitorId>,
) -> Vec<&'a Monitor> {
    match mode {
        Monitors::All => all.iter().collect(),
        Monitors::Cursor => kept
            .and_then(|id| all.iter().find(|monitor| &monitor.id == id))
            .or_else(|| all.iter().find(|monitor| monitor.primary))
            .or_else(|| all.first())
            .into_iter()
            .collect(),
    }
}

/// Works out the changes that turn the `current` overlays into one overlay per `wanted` monitor.
///
/// It emits every [`Change::Destroy`] first, then every [`Change::Move`] (only for a rectangle that
/// changed), then every [`Change::Create`], each group in input order.
pub fn reconcile(current: &[(MonitorId, Rect)], wanted: &[&Monitor]) -> Vec<Change> {
    let wanted_ids: HashSet<&MonitorId> = wanted.iter().map(|monitor| &monitor.id).collect();
    let destroys = current
        .iter()
        .filter(|(id, _)| !wanted_ids.contains(id))
        .map(|(id, _)| Change::Destroy(id.clone()));
    let moves = wanted.iter().filter_map(|monitor| {
        current
            .iter()
            .find(|(id, _)| *id == monitor.id)
            .filter(|(_, rect)| *rect != monitor.rect)
            .map(|_| Change::Move {
                id: monitor.id.clone(),
                rect: monitor.rect,
            })
    });
    let creates = wanted
        .iter()
        .filter(|monitor| current.iter().all(|(id, _)| *id != monitor.id))
        .map(|monitor| Change::Create((*monitor).clone()));
    destroys.chain(moves).chain(creates).collect()
}

/// Receives each monitor from `EnumDisplayMonitors` and pushes it into the `Vec` behind `data`.
extern "system" fn collect_monitor(monitor: HMONITOR, _: HDC, _: *mut RECT, data: LPARAM) -> BOOL {
    // SAFETY: `enumerate` passes a valid, exclusive `*mut Vec<HMONITOR>` that outlives the
    // enumeration, and nothing else touches it while the callback runs.
    let found = unsafe { &mut *(data.0 as *mut Vec<HMONITOR>) };
    found.push(monitor);
    BOOL::from(true)
}

/// Reads a monitor's device name, rectangle and primary flag.
fn monitor_info(monitor: HMONITOR) -> windows::core::Result<Monitor> {
    let mut info = MONITORINFOEXW::default();
    info.monitorInfo.cbSize = u32::try_from(size_of::<MONITORINFOEXW>()).expect("small struct");
    // SAFETY: `info` is a writable `MONITORINFOEXW` and its `cbSize` says so, which allows
    // `GetMonitorInfoW` to fill the extended structure through the `MONITORINFO` pointer.
    unsafe { GetMonitorInfoW(monitor, (&raw mut info).cast::<MONITORINFO>()) }.ok()?;
    Ok(Monitor {
        id: MonitorId::from_device(&info.szDevice),
        rect: info.monitorInfo.rcMonitor.into(),
        primary: info.monitorInfo.dwFlags & MONITORINFOF_PRIMARY != 0,
    })
}

#[cfg(test)]
mod tests {
    use super::{Change, Monitor, MonitorId, Monitors, Rect, enumerate, reconcile, select};

    fn id(name: &str) -> MonitorId {
        let units: Vec<u16> = name.encode_utf16().collect();
        MonitorId::from_device(&units)
    }

    fn rect(left: i32, width: i32) -> Rect {
        Rect {
            left,
            top: 0,
            right: left + width,
            bottom: 1080,
        }
    }

    fn monitor(name: &str, left: i32, primary: bool) -> Monitor {
        Monitor {
            id: id(name),
            rect: rect(left, 1920),
            primary,
        }
    }

    fn three() -> Vec<Monitor> {
        vec![
            monitor(r"\\.\DISPLAY1", 0, true),
            monitor(r"\\.\DISPLAY2", 1920, false),
            monitor(r"\\.\DISPLAY3", -1920, false),
        ]
    }

    fn current(monitors: &[Monitor]) -> Vec<(MonitorId, Rect)> {
        monitors.iter().map(|m| (m.id.clone(), m.rect)).collect()
    }

    #[test]
    fn device_name_is_trimmed_at_nul() {
        let mut buffer = [0u16; 32];
        for (slot, unit) in buffer.iter_mut().zip("ABC".encode_utf16()) {
            *slot = unit;
        }
        assert_eq!(MonitorId::from_device(&buffer), id("ABC"));
    }

    #[test]
    fn select_all_returns_every_monitor() {
        let all = three();
        let chosen = select(&all, Monitors::All, None);
        assert_eq!(chosen, all.iter().collect::<Vec<_>>());
    }

    #[test]
    fn select_cursor_keeps_the_kept_monitor() {
        let all = three();
        let kept = id(r"\\.\DISPLAY2");
        assert_eq!(select(&all, Monitors::Cursor, Some(&kept)), [&all[1]]);
    }

    #[test]
    fn select_cursor_falls_back_to_primary_when_kept_is_gone() {
        let all = three();
        let gone = id(r"\\.\DISPLAY9");
        assert_eq!(select(&all, Monitors::Cursor, Some(&gone)), [&all[0]]);
        assert_eq!(select(&all, Monitors::Cursor, None), [&all[0]]);
    }

    #[test]
    fn reconcile_without_change_is_empty() {
        let all = three();
        let wanted: Vec<&Monitor> = all.iter().collect();
        assert_eq!(reconcile(&current(&all), &wanted), []);
    }

    #[test]
    fn reconcile_moves_a_resized_monitor() {
        let before = three();
        let mut after = three();
        after[1].rect = rect(1920, 2560);
        let wanted: Vec<&Monitor> = after.iter().collect();
        assert_eq!(
            reconcile(&current(&before), &wanted),
            [Change::Move {
                id: after[1].id.clone(),
                rect: after[1].rect
            }]
        );
    }

    #[test]
    fn reconcile_creates_an_added_monitor() {
        let all = three();
        let wanted: Vec<&Monitor> = all.iter().collect();
        assert_eq!(
            reconcile(&current(&all[..2]), &wanted),
            [Change::Create(all[2].clone())]
        );
    }

    #[test]
    fn reconcile_destroys_a_removed_monitor() {
        let all = three();
        let wanted: Vec<&Monitor> = all[..2].iter().collect();
        assert_eq!(
            reconcile(&current(&all), &wanted),
            [Change::Destroy(all[2].id.clone())]
        );
    }

    #[test]
    fn reconcile_replaces_every_monitor() {
        let before = three();
        let after = [
            monitor(r"\\.\DISPLAY7", 0, true),
            monitor(r"\\.\DISPLAY8", 1920, false),
        ];
        let wanted: Vec<&Monitor> = after.iter().collect();
        let mut expected: Vec<Change> = before
            .iter()
            .map(|m| Change::Destroy(m.id.clone()))
            .collect();
        expected.extend(after.iter().cloned().map(Change::Create));
        assert_eq!(reconcile(&current(&before), &wanted), expected);
    }

    #[test]
    fn reconcile_orders_destroys_then_moves_then_creates() {
        let before = three();
        let mut after = [before[1].clone(), monitor(r"\\.\DISPLAY4", 3840, false)];
        after[0].rect = rect(1920, 1280);
        let wanted: Vec<&Monitor> = after.iter().collect();
        assert_eq!(
            reconcile(&current(&before), &wanted),
            [
                Change::Destroy(before[0].id.clone()),
                Change::Destroy(before[2].id.clone()),
                Change::Move {
                    id: after[0].id.clone(),
                    rect: after[0].rect
                },
                Change::Create(after[1].clone()),
            ]
        );
    }

    #[test]
    fn enumerate_finds_monitors_and_one_primary() {
        let monitors = enumerate().expect("EnumDisplayMonitors succeeds");
        assert!(!monitors.is_empty(), "no monitor found");
        assert_eq!(monitors.iter().filter(|m| m.primary).count(), 1);
        assert!(
            monitors
                .iter()
                .all(|m| m.rect.width() > 0 && m.rect.height() > 0)
        );
    }
}
