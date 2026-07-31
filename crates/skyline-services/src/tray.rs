use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, OnceLock};

use skyline_core::{
    ServiceEvent, TrayItemSnapshot, TrayMenuEntry, TrayMenuSnapshot, TrayPixmap,
};
use system_tray::client::{ActivateRequest, Client};
use system_tray::item::StatusNotifierItem;
use system_tray::menu::TrayMenu;
use tracing::{info, warn};

use crate::spawn_tokio;

static CLIENT: OnceLock<Mutex<Option<Arc<Client>>>> = OnceLock::new();

fn client_slot() -> &'static Mutex<Option<Arc<Client>>> {
    CLIENT.get_or_init(|| Mutex::new(None))
}

pub fn spawn(tx: Sender<ServiceEvent>) {
    spawn_tokio("skyline-tray", async move {
        match Client::new().await {
            Ok(client) => {
                let client = Arc::new(client);
                {
                    if let Ok(mut slot) = client_slot().lock() {
                        *slot = Some(client.clone());
                    }
                }
                info!("system tray host ready");
                publish_items(&client, &tx);

                let mut rx = client.subscribe();
                loop {
                    match rx.recv().await {
                        Ok(_ev) => publish_items(&client, &tx),
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(_) => break,
                    }
                }
            }
            Err(err) => {
                warn!("tray host failed: {err}");
                let _ = tx.send(ServiceEvent::Error(format!("tray: {err}")));
            }
        }
    });
}

fn publish_items(client: &Client, tx: &Sender<ServiceEvent>) {
    let items = client.items();
    let Ok(guard) = items.lock() else {
        return;
    };
    let mut out = Vec::new();
    for (id, (item, _menu)) in guard.iter() {
        out.push(item_to_snapshot(id, item));
    }
    out.sort_by(|a, b| a.title.cmp(&b.title));
    let _ = tx.send(ServiceEvent::TrayItems(out));
}

fn item_to_snapshot(id: &str, item: &StatusNotifierItem) -> TrayItemSnapshot {
    let needs_attention = matches!(item.status, system_tray::item::Status::NeedsAttention);
    let icon_pixmap = best_pixmap(if needs_attention {
        item.attention_icon_pixmap
            .as_ref()
            .or(item.icon_pixmap.as_ref())
    } else {
        item.icon_pixmap.as_ref()
    });
    let title = item
        .title
        .clone()
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| {
            if item.id.is_empty() {
                id.to_string()
            } else {
                item.id.clone()
            }
        });
    TrayItemSnapshot {
        id: id.to_string(),
        app_id: item.id.clone(),
        title,
        icon_name: item.icon_name.clone().filter(|s| !s.is_empty()),
        icon_theme_path: item.icon_theme_path.clone().filter(|s| !s.is_empty()),
        attention_icon_name: item.attention_icon_name.clone().filter(|s| !s.is_empty()),
        needs_attention,
        icon_pixmap,
        status: format!("{:?}", item.status),
    }
}

fn best_pixmap(pixmaps: Option<&Vec<system_tray::item::IconPixmap>>) -> Option<TrayPixmap> {
    let list = pixmaps?;
    list.iter()
        .filter(|p| {
            p.width > 0
                && p.height > 0
                && p.pixels.len() >= (p.width as usize) * (p.height as usize) * 4
        })
        .max_by_key(|p| (p.width as i64) * (p.height as i64))
        .map(|p| TrayPixmap {
            width: p.width,
            height: p.height,
            bytes: p.pixels.clone(),
        })
}

pub fn activate_item(id: &str) {
    let client = {
        let slot = client_slot().lock().ok();
        slot.and_then(|g| g.clone())
    };
    let Some(client) = client else {
        return;
    };
    let address = id.to_string();
    spawn_tokio("skyline-tray-activate", async move {
        let req = ActivateRequest::Default {
            address: address.clone(),
            x: 0,
            y: 0,
        };
        if let Err(err) = client.activate(req).await {
            warn!("tray activate {address}: {err}");
        }
    });
}

pub fn request_menu(id: &str, tx: Sender<ServiceEvent>) {
    let client = {
        let slot = client_slot().lock().ok();
        slot.and_then(|g| g.clone())
    };
    let Some(client) = client else {
        return;
    };
    let id = id.to_string();
    spawn_tokio("skyline-tray-menu", async move {
        let items = client.items();
        let menu = {
            let Ok(guard) = items.lock() else {
                return;
            };
            guard.get(&id).and_then(|(_, menu)| menu.clone())
        };
        let Some(menu) = menu else {
            return;
        };
        let entries = menu_to_entries(&menu);
        let _ = tx.send(ServiceEvent::TrayMenu(TrayMenuSnapshot {
            item_id: id,
            entries,
        }));
    });
}

fn menu_to_entries(menu: &TrayMenu) -> Vec<TrayMenuEntry> {
    fn walk(items: &[system_tray::menu::MenuItem]) -> Vec<TrayMenuEntry> {
        items
            .iter()
            .map(|item| TrayMenuEntry {
                id: item.id,
                label: item.label.clone().unwrap_or_default(),
                enabled: item.enabled,
                visible: item.visible,
                separator: matches!(item.menu_type, system_tray::menu::MenuType::Separator),
                children: walk(&item.submenu),
            })
            .collect()
    }
    walk(&menu.submenus)
}

pub fn activate_menu(item_id: &str, menu_id: i32) {
    let client = {
        let slot = client_slot().lock().ok();
        slot.and_then(|g| g.clone())
    };
    let Some(client) = client else {
        return;
    };
    let address = item_id.to_string();
    spawn_tokio("skyline-tray-menuitem", async move {
        let menu_path = {
            let items = client.items();
            let Ok(guard) = items.lock() else {
                return;
            };
            guard
                .get(&address)
                .and_then(|(item, _)| item.menu.clone())
                .unwrap_or_default()
        };
        let req = ActivateRequest::MenuItem {
            address,
            menu_path,
            submenu_id: menu_id,
        };
        if let Err(err) = client.activate(req).await {
            warn!("tray menu activate: {err}");
        }
    });
}
