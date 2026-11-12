//! The sidebar: a column beside the page holding bookmarks, downloads and history.

use crate::ipc::AppState;
use crate::window::{logical_size, set_bounds};
use tauri::{
    AppHandle, LogicalPosition, LogicalSize, Manager, WebviewBuilder, WebviewUrl,
};

/// Which side the sidebar is on, and how wide it should be, for this window.
pub struct Plan {
    pub side: String,
    pub reserve: f64,
}

impl Plan {
    fn off() -> Self {
        Self {
            side: "off".to_string(),
            reserve: 0.0,
        }
    }

    pub fn on(&self) -> bool {
        self.reserve > 0.0
    }

    /// Where the pages start, and how much room they have.
    pub fn page(&self, width: f64) -> (f64, f64) {
        let x = if self.side == "left" { self.reserve } else { 0.0 };
        (x, (width - self.reserve).max(1.0))
    }

    /// Where the sidebar itself goes.
    pub fn panel_x(&self, width: f64) -> f64 {
        if self.side == "left" {
            0.0
        } else {
            width - self.reserve
        }
    }
}

/// Reads the setting and works out the geometry for a window of this width.
pub fn plan(app: &AppHandle, width: f64, fullscreen: bool) -> Plan {
    let Some(state) = app.try_state::<AppState>() else {
        return Plan::off();
    };
    let settings = state.settings.read();
    let side = if fullscreen {
        "off".to_string()
    } else {
        settings.sidebar.clone()
    };
    let wanted = settings.sidebar_width;
    drop(settings);
    if side != "left" && side != "right" {
        return Plan::off();
    }
    Plan {
        side,
        reserve: wanted.clamp(180.0, 640.0).min((width - 200.0).max(0.0)),
    }
}

/// Creates the sidebar webview for a window if it is wanted and missing.
pub fn ensure(app: &AppHandle, label: &str) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let side = state.settings.read().sidebar.clone();
    if side != "left" && side != "right" {
        return;
    }
    let label_side = format!("{label}-side");
    if app.get_webview(&label_side).is_some() {
        return;
    }
    let Some(win) = app.get_window(label) else {
        return;
    };
    let Ok((width_px, height)) = logical_size(&win) else {
        return;
    };
    let chrome_h = crate::ipc::chrome_height(app);
    let plan = plan(app, width_px, false);
    if !plan.on() {
        return;
    }
    let opts = state.opts.clone();
    let builder = WebviewBuilder::new(
        label_side.clone(),
        WebviewUrl::App("pages/sidebar.html".into()),
    )
    .devtools(true);
    let builder = crate::tabs::apply_profile(builder, &opts.ui_data_dir);
    let builder = crate::tabs::apply_extensions(builder, &opts);
    match win.add_child(
        builder,
        LogicalPosition::new(plan.panel_x(width_px), chrome_h),
        LogicalSize::new(
            plan.reserve.max(1.0),
            (height - chrome_h).max(1.0),
        ),
    ) {
        Ok(_) => crate::logging::info("sidebar created"),
        Err(err) => crate::logging::warn(&format!("sidebar refused: {err}")),
    }
}

/// Tells the page which edge it is on, so the resize grip sits on the inner edge.
pub fn tell_side(app: &AppHandle, label: &str, side: &str) {
    let Some(view) = app.get_webview(&format!("{label}-side")) else {
        return;
    };
    let value = if side == "right" { "right" } else { "left" };
    let _ = view.eval(&format!(
        "window.__mixSidebarSide && window.__mixSidebarSide(\"{value}\")"
    ));
}

/// Shows, hides and places the sidebar to match the plan.
pub fn apply(
    app: &AppHandle,
    label: &str,
    plan: &Plan,
    width: f64,
    y: f64,
    height: f64,
) -> Result<(), tauri::Error> {
    let side_label = format!("{label}-side");
    match app.get_webview(&side_label) {
        Some(webview) if plan.on() => {
            let _ = webview.show();
            set_bounds(&webview, plan.panel_x(width), y, plan.reserve, height)?;
        }
        Some(webview) => {
            let _ = webview.hide();
        }
        None if plan.on() => {
            crate::logging::warn("sidebar is on but was never created; reopening the window will");
        }
        None => {}
    }
    Ok(())
}
