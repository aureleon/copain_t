use tray_icon::menu::{Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayAction {
    TogglePrompt,
    ToggleMode,
    Clear,
    Quit,
}

pub struct TrayController {
    _tray: TrayIcon,
    id_prompt: MenuId,
    id_mode: MenuId,
    id_clear: MenuId,
    id_quit: MenuId,
}

impl TrayController {
    pub fn new() -> Option<Self> {
        let item_prompt = MenuItem::new("Toggle Prompt Bar", true, None);
        let item_mode = MenuItem::new("Switch Overlay / Canvas", true, None);
        let item_clear = MenuItem::new("Clear Overlays", true, None);
        let item_quit = MenuItem::new("Quit copain_t", true, None);

        let id_prompt = item_prompt.id().clone();
        let id_mode = item_mode.id().clone();
        let id_clear = item_clear.id().clone();
        let id_quit = item_quit.id().clone();

        let menu = Menu::new();
        let _ = menu.append(&item_prompt);
        let _ = menu.append(&item_mode);
        let _ = menu.append(&item_clear);
        let _ = menu.append(&PredefinedMenuItem::separator());
        let _ = menu.append(&item_quit);

        let icon = build_tray_icon()?;

        let tray = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("copain_t — AI 2D/3D Visual Companion")
            .with_title("copain_t")
            .with_icon(icon)
            .build()
            .ok()?;

        Some(Self {
            _tray: tray,
            id_prompt,
            id_mode,
            id_clear,
            id_quit,
        })
    }

    pub fn poll(&self) -> Option<TrayAction> {
        let ev = MenuEvent::receiver().try_recv().ok()?;
        if ev.id == self.id_prompt {
            Some(TrayAction::TogglePrompt)
        } else if ev.id == self.id_mode {
            Some(TrayAction::ToggleMode)
        } else if ev.id == self.id_clear {
            Some(TrayAction::Clear)
        } else if ev.id == self.id_quit {
            Some(TrayAction::Quit)
        } else {
            None
        }
    }
}

/// Generate a crisp 22x22 RGBA orange-diamond icon for the macOS menu bar.
fn build_tray_icon() -> Option<Icon> {
    let size = 22usize;
    let mut rgba = vec![0u8; size * size * 4];
    let c = (size as f32 - 1.0) * 0.5;
    let r = 7.5f32;
    for y in 0..size {
        for x in 0..size {
            let dx = (x as f32 - c).abs();
            let dy = (y as f32 - c).abs();
            let dist = dx + dy;
            let idx = (y * size + x) * 4;
            if dist <= r {
                let alpha = ((r - dist).clamp(0.0, 1.0) * 255.0) as u8;
                rgba[idx] = 255;
                rgba[idx + 1] = 85;
                rgba[idx + 2] = 0;
                rgba[idx + 3] = alpha;
            }
        }
    }
    Icon::from_rgba(rgba, size as u32, size as u32).ok()
}
