// Degenie desktop companion (local spike). A transparent, frameless, always-on-top window sized to the genie
// (taller while the log is open), draggable anywhere. It ignores the mouse everywhere except over the regions the page reports (genie, bubble,
// buttons, log panel), so clicks fall through to the windows underneath.

use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{LogicalPosition, LogicalSize, Manager};

const WIDTH: f64 = 400.0; // keep in sync with web/Companion.tsx
const CLOSED_H: f64 = 400.0;

#[derive(serde::Deserialize, Clone, Copy)]
struct Rect {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

struct Hits(Arc<Mutex<Vec<Rect>>>);

#[tauri::command]
fn set_hit_regions(rects: Vec<Rect>, hits: tauri::State<Hits>) {
    *hits.0.lock().unwrap() = rects;
}

fn main() {
    let hits = Arc::new(Mutex::new(Vec::<Rect>::new()));
    tauri::Builder::default()
        .plugin(tauri_plugin_http::init())
        .manage(Hits(hits.clone()))
        .invoke_handler(tauri::generate_handler![set_hit_regions])
        .setup(move |app| {
            let win = app.get_webview_window("main").expect("main window");
            // Starts in the bottom-right corner; the page resizes the window when the log opens
            if let Some(m) = win.primary_monitor()? {
                let s = m.scale_factor();
                let area = m.work_area();
                let (pos, size) = (area.position.to_logical::<f64>(s), area.size.to_logical::<f64>(s));
                win.set_size(LogicalSize::new(WIDTH, CLOSED_H))?;
                win.set_position(LogicalPosition::new(pos.x + size.width - WIDTH, pos.y + size.height - CLOSED_H))?;
            }
            // ponytail: 40 ms cursor poll; an OS hit-test hook per platform if this ever shows in CPU profiles
            let w = win.clone();
            std::thread::spawn(move || {
                let mut ignoring: Option<bool> = None;
                loop {
                    std::thread::sleep(Duration::from_millis(40));
                    let (Ok(c), Ok(p), Ok(s)) = (w.cursor_position(), w.outer_position(), w.scale_factor()) else {
                        continue;
                    };
                    let (x, y) = ((c.x - p.x as f64) / s, (c.y - p.y as f64) / s);
                    let over = hits.lock().unwrap().iter().any(|r| x >= r.x && x <= r.x + r.w && y >= r.y && y <= r.y + r.h);
                    if ignoring != Some(!over) {
                        let _ = w.set_ignore_cursor_events(!over);
                        ignoring = Some(!over);
                    }
                }
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Degenie");
}
