use std::time::{Duration, Instant};

use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize};

/// Cells of the screen layout. Every dock action cycles through the cells that
/// belong to its direction, so one key never needs a second binding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cell {
    Full,
    LeftHalf,
    RightHalf,
    TopHalf,
    BottomHalf,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Left,
    Right,
    Up,
    Down,
}

#[derive(Default)]
pub struct SnapState {
    last: Option<Cell>,
    restore: Option<(i32, i32, u32, u32)>,
    last_at: Option<Instant>,
}

/// Keyboard auto repeat would otherwise run through the whole cycle in one press.
const DEBOUNCE: Duration = Duration::from_millis(160);

pub fn cycle(app: &AppHandle, direction: Direction) -> Result<(), String> {
    let Some(win) = crate::ipc::focused_window(app) else {
        return Err("no main window".into());
    };

    let was_maximized = win.is_maximized().unwrap_or(false);

    let (next, restore) = {
        let state = app.state::<crate::ipc::AppState>();
        let mut snap = state.snap.lock().unwrap_or_else(|e| e.into_inner());

        if let Some(last) = snap.last_at {
            if last.elapsed() < DEBOUNCE {
                return Ok(());
            }
        }
        snap.last_at = Some(Instant::now());

        if snap.last.is_none() || was_maximized {
            // First dock: remember where the window was so Down can put it back.
            snap.restore = bounds(&win);
        }

        let next = next_cell(snap.last, direction);
        if next.is_none() {
            let restore = snap.restore.take();
            snap.last = None;
            (None, restore)
        } else {
            snap.last = next;
            (next, None)
        }
    };

    if let Some((x, y, w, h)) = restore {
        let _ = win.unmaximize();
        let _ = win.set_position(PhysicalPosition::new(x, y));
        let _ = win.set_size(PhysicalSize::new(w.max(200), h.max(200)));
        return Ok(());
    }

    let Some(cell) = next else {
        return Ok(());
    };

    if cell == Cell::Full {
        let _ = win.maximize();
        return Ok(());
    }

    let Some(monitor) = win.current_monitor().map_err(|e| e.to_string())? else {
        return Err("no monitor under the window".into());
    };
    let area = monitor.work_area();
    let (ax, ay) = (area.position.x, area.position.y);
    let (aw, ah) = (area.size.width as i32, area.size.height as i32);
    let (half_w, half_h) = (aw / 2, ah / 2);

    let (x, y, w, h) = match cell {
        Cell::Full => unreachable!(),
        Cell::LeftHalf => (ax, ay, half_w, ah),
        Cell::RightHalf => (ax + half_w, ay, half_w, ah),
        Cell::TopHalf => (ax, ay, aw, half_h),
        Cell::BottomHalf => (ax, ay + half_h, aw, half_h),
        Cell::TopLeft => (ax, ay, half_w, half_h),
        Cell::TopRight => (ax + half_w, ay, half_w, half_h),
        Cell::BottomLeft => (ax, ay + half_h, half_w, half_h),
        Cell::BottomRight => (ax + half_w, ay + half_h, half_w, half_h),
    };

    let _ = win.unmaximize();
    win.set_position(PhysicalPosition::new(x, y))
        .map_err(|e| e.to_string())?;
    win.set_size(PhysicalSize::new(w as u32, h as u32))
        .map_err(|e| e.to_string())?;
    crate::logging::info(&format!("snapped to {cell:?}"));
    Ok(())
}

/// Position is the outer corner but size is the inner one: set_size changes the
/// client area, so storing the outer size would grow the window on every restore.
fn bounds(win: &tauri::Window) -> Option<(i32, i32, u32, u32)> {
    let position = win.outer_position().ok()?;
    let size = win.inner_size().ok()?;
    Some((position.x, position.y, size.width, size.height))
}

fn next_cell(current: Option<Cell>, direction: Direction) -> Option<Cell> {
    use Cell::*;
    match direction {
        Direction::Left => match current {
            Some(LeftHalf) => Some(TopLeft),
            Some(TopLeft) => Some(BottomLeft),
            Some(BottomLeft) | None => Some(LeftHalf),
            _ => Some(LeftHalf),
        },
        Direction::Right => match current {
            Some(RightHalf) => Some(TopRight),
            Some(TopRight) => Some(BottomRight),
            Some(BottomRight) | None => Some(RightHalf),
            _ => Some(RightHalf),
        },
        Direction::Up => match current {
            Some(Full) => Some(TopHalf),
            Some(TopHalf) | None => Some(Full),
            _ => Some(Full),
        },
        Direction::Down => match current {
            Some(BottomHalf) => None,
            _ => Some(BottomHalf),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::{next_cell, Cell, Direction};

    #[test]
    fn left_cycles_half_then_corners() {
        assert_eq!(next_cell(None, Direction::Left), Some(Cell::LeftHalf));
        assert_eq!(next_cell(Some(Cell::LeftHalf), Direction::Left), Some(Cell::TopLeft));
        assert_eq!(next_cell(Some(Cell::TopLeft), Direction::Left), Some(Cell::BottomLeft));
        assert_eq!(next_cell(Some(Cell::BottomLeft), Direction::Left), Some(Cell::LeftHalf));
    }

    #[test]
    fn right_cycles_half_then_corners() {
        assert_eq!(next_cell(None, Direction::Right), Some(Cell::RightHalf));
        assert_eq!(next_cell(Some(Cell::RightHalf), Direction::Right), Some(Cell::TopRight));
        assert_eq!(next_cell(Some(Cell::BottomRight), Direction::Right), Some(Cell::RightHalf));
    }

    #[test]
    fn up_toggles_full_and_top_half() {
        assert_eq!(next_cell(None, Direction::Up), Some(Cell::Full));
        assert_eq!(next_cell(Some(Cell::Full), Direction::Up), Some(Cell::TopHalf));
    }

    #[test]
    fn down_restores_after_the_bottom_half() {
        assert_eq!(next_cell(None, Direction::Down), Some(Cell::BottomHalf));
        assert_eq!(next_cell(Some(Cell::BottomHalf), Direction::Down), None);
    }
}
/// Global hotkeys are the primary trigger. If the platform refuses them (Wayland,
/// or another app already owns the combo) the window keys still work.
pub fn install_hotkeys(app: &AppHandle) -> bool {
    use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

    let bindings = [
        (Code::ArrowLeft, Direction::Left),
        (Code::ArrowRight, Direction::Right),
        (Code::ArrowUp, Direction::Up),
        (Code::ArrowDown, Direction::Down),
    ];

    let mut ok = true;
    for (code, direction) in bindings {
        let shortcut = Shortcut::new(Some(Modifiers::ALT | Modifiers::SHIFT), code);
        let label = format!("{shortcut:?}");
        let result = app.global_shortcut().on_shortcut(shortcut, move |app, _shortcut, event| {
            if event.state() == ShortcutState::Pressed {
                if let Err(err) = cycle(app, direction) {
                    crate::logging::warn(&format!("snap failed: {err}"));
                }
            }
        });
        match result {
            Ok(()) => crate::logging::info(&format!("hotkey registered: {label}")),
            Err(err) => {
                crate::logging::warn(&format!("hotkey {label} unavailable: {err}"));
                ok = false;
            }
        }
    }

    if !ok {
        crate::logging::warn("docking falls back to the in-window shortcuts");
    }
    ok
}