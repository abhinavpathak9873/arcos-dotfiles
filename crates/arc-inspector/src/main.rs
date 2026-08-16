use arc_protocol::{socket_path, Request, Response};
use gtk::{gdk, gio, prelude::*};
use serde_json::{json, Value};
use std::{
    cell::RefCell,
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixStream,
    rc::Rc,
    time::Duration,
};

type PageRequest = (&'static str, &'static str, Value);

fn main() {
    // GTK 3 derives its Wayland app_id from the GLib program name, not solely
    // from GtkApplication's D-Bus ID. Set it before GDK opens the display so
    // Sway can apply a precise native-window rule.
    gtk::glib::set_prgname(Some("ai.arcos.inspector"));
    let application = gtk::Application::new(
        Some("ai.arcos.inspector"),
        gio::ApplicationFlags::NON_UNIQUE,
    );
    application.connect_startup(|_| install_style());
    application.connect_activate(build);
    application.run();
}

fn build(application: &gtk::Application) {
    let window = gtk::ApplicationWindow::new(application);
    window.set_title("Arc System Overview");
    window.set_default_size(1000, 680);
    window.set_icon_name(Some("preferences-system"));

    let header = gtk::HeaderBar::new();
    header.set_title(Some("Arc"));
    header.set_subtitle(Some("System overview"));
    header.set_show_close_button(true);
    let refresh = gtk::Button::from_icon_name(Some("view-refresh-symbolic"), gtk::IconSize::Button);
    refresh.set_tooltip_text(Some("Reload state from arc-core"));
    header.pack_end(&refresh);
    window.set_titlebar(Some(&header));

    let notebook = gtk::Notebook::new();
    notebook.set_scrollable(true);
    notebook.set_tab_pos(gtk::PositionType::Left);
    notebook.set_show_border(false);
    let requests: Vec<PageRequest> = vec![
        ("Timeline", "activity/list", json!({"limit": 100})),
        ("Rooms", "rooms/list", json!({})),
        ("Tasks", "tasks/list", json!({})),
        ("Permissions & audit", "audit/list", json!({"limit": 200})),
        ("Memory", "system/hermesIdentity", json!({})),
        (
            "Skills",
            "hermes/http",
            json!({"path": "/api/skills", "method": "GET"}),
        ),
        (
            "Schedules",
            "hermes/http",
            json!({"path": "/api/schedules", "method": "GET"}),
        ),
        (
            "Providers & settings",
            "hermes/http",
            json!({"path": "/api/config", "method": "GET"}),
        ),
        ("Services", "health", json!({})),
    ];
    let pages: Rc<RefCell<Vec<gtk::TextBuffer>>> = Rc::new(RefCell::new(Vec::new()));
    for (title, _, _) in &requests {
        let (page, buffer) = text_page();
        buffer.set_text("Loading from Arc…");
        notebook.append_page(&page, Some(&gtk::Label::new(Some(title))));
        pages.borrow_mut().push(buffer);
    }

    // GTK 3's GLib channel is the stable bridge from worker threads back to
    // main-thread-only widgets. The inspector intentionally remains GTK 3 so
    // it works on the ArcOS image without adding another UI runtime.
    #[allow(deprecated)]
    let (sender, receiver) =
        gtk::glib::MainContext::channel::<(usize, String)>(gtk::glib::Priority::DEFAULT);
    let pages_for_results = pages.clone();
    receiver.attach(None, move |(index, content)| {
        if let Some(buffer) = pages_for_results.borrow().get(index) {
            buffer.set_text(&content);
        }
        gtk::glib::ControlFlow::Continue
    });

    let load = move || {
        let sender = sender.clone();
        let requests = requests.clone();
        std::thread::spawn(move || {
            for (index, (_, method, params)) in requests.into_iter().enumerate() {
                let _ = sender.send((index, render(method, params)));
            }
        });
    };
    load();

    let load_for_refresh = load.clone();
    let pages_for_refresh = pages.clone();
    refresh.connect_clicked(move |_| {
        for buffer in pages_for_refresh.borrow().iter() {
            buffer.set_text("Refreshing from Arc…");
        }
        load_for_refresh();
    });

    window.add(&notebook);
    window.show_all();
}

fn text_page() -> (gtk::ScrolledWindow, gtk::TextBuffer) {
    let scrolled = gtk::ScrolledWindow::new(None::<&gtk::Adjustment>, None::<&gtk::Adjustment>);
    scrolled.set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Automatic);
    let view = gtk::TextView::new();
    view.set_editable(false);
    view.set_cursor_visible(false);
    view.set_wrap_mode(gtk::WrapMode::WordChar);
    view.set_left_margin(36);
    view.set_right_margin(36);
    view.set_top_margin(30);
    view.set_bottom_margin(30);
    view.set_monospace(false);
    let buffer = view.buffer().expect("GTK text view has a buffer");
    scrolled.add(&view);
    (scrolled, buffer)
}

fn render(method: &str, params: Value) -> String {
    match rpc(method, params) {
        Ok(value) if method == "activity/list" => render_activity(&value),
        Ok(value) => serde_json::to_string_pretty(&value).unwrap_or_else(|_| "No data".into()),
        Err(error) => format!("Arc core is unavailable\n\n{error}\n\nThe inspector does not own Arc. You can close it safely while services continue."),
    }
}

fn render_activity(value: &Value) -> String {
    let Some(items) = value.get("items").and_then(Value::as_array) else {
        return "No recent Arc activity.".into();
    };
    items
        .iter()
        .map(|item| {
            format!(
                "{}                                      {}\n{}\n",
                item.get("title").and_then(Value::as_str).unwrap_or("Arc"),
                short_time(item.get("at").and_then(Value::as_str).unwrap_or_default()),
                item.get("body").and_then(Value::as_str).unwrap_or_default()
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn short_time(timestamp: &str) -> String {
    timestamp
        .split_once('T')
        .and_then(|(_, time)| time.get(..5))
        .unwrap_or(timestamp)
        .to_owned()
}

fn rpc(method: &str, params: Value) -> anyhow::Result<Value> {
    let path = socket_path("arc-core");
    let mut stream = UnixStream::connect(&path)
        .map_err(|error| anyhow::anyhow!("cannot connect to {}: {error}", path.display()))?;
    stream.set_read_timeout(Some(Duration::from_secs(3)))?;
    stream.set_write_timeout(Some(Duration::from_secs(3)))?;
    let request = Request {
        jsonrpc: "2.0".into(),
        id: 1,
        method: method.into(),
        params,
    };
    writeln!(stream, "{}", serde_json::to_string(&request)?)?;
    let mut line = String::new();
    BufReader::new(stream).read_line(&mut line)?;
    let response: Response = serde_json::from_str(&line)?;
    if let Some(error) = response.error {
        anyhow::bail!("{} ({})", error.message, error.code);
    }
    Ok(response.result.unwrap_or(Value::Null))
}

fn install_style() {
    let provider = gtk::CssProvider::new();
    provider
        .load_from_data(
            br#"
            * {
              font-family: Inter, sans-serif;
              outline-color: transparent;
            }
            window {
              background: #12141d;
              color: #d9dcec;
            }
            headerbar {
              min-height: 48px;
              padding: 0 10px;
              background: #171923;
              color: #e8e9f1;
              border: none;
              border-bottom: 1px solid rgba(148, 156, 187, 0.16);
              box-shadow: none;
            }
            headerbar .title { font-size: 14px; font-weight: 600; }
            headerbar .subtitle { color: #949cbb; font-size: 11px; }
            notebook { background: #1b1e2a; border: none; }
            notebook > header {
              min-width: 194px;
              padding: 18px 10px;
              background: #151722;
              border: none;
              border-right: 1px solid rgba(148, 156, 187, 0.14);
            }
            notebook > header > tabs > tab {
              min-height: 24px;
              margin: 2px 0;
              padding: 10px 14px;
              color: #949cbb;
              background: transparent;
              border: none;
              border-radius: 8px;
              font-size: 12px;
              font-weight: 500;
            }
            notebook > header > tabs > tab:hover {
              color: #c6d0f5;
              background: rgba(65, 69, 89, 0.34);
            }
            notebook > header > tabs > tab:checked {
              color: #eadcf2;
              background: rgba(202, 158, 230, 0.13);
              box-shadow: inset 3px 0 #ca9ee6;
              font-weight: 600;
            }
            scrolledwindow { border: none; background: #1b1e2a; }
            textview, textview text {
              background: #1b1e2a;
              color: #cdd1e4;
              font: 13px Inter;
            }
            textview text selection { background: #51576d; color: #ffffff; }
            button {
              min-width: 28px;
              min-height: 28px;
              padding: 4px;
              color: #b5bfe2;
              background: rgba(65, 69, 89, 0.48);
              border: 1px solid rgba(148, 156, 187, 0.16);
              border-radius: 8px;
              box-shadow: none;
            }
            button:hover { color: #eadcf2; background: rgba(202, 158, 230, 0.14); }
            scrollbar slider {
              min-width: 6px;
              min-height: 36px;
              background: rgba(148, 156, 187, 0.32);
              border-radius: 6px;
            }
            "#,
        )
        .expect("valid Arc inspector CSS");
    if let Some(screen) = gdk::Screen::default() {
        gtk::StyleContext::add_provider_for_screen(
            &screen,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}
