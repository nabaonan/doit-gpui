mod app;
mod backup;
mod http;
mod label_dialogs;
mod settings;
mod types;
mod webdav;

use app::{DoitApp, DoitAppHandle};
use gpui_kit::component::Root;
use gpui_kit::gpui::{Bounds, SharedString, WindowBounds, WindowOptions, px};
use gpui_kit::*;

fn main() {
    // Desktop gpui does not install an HTTP client by default, and the default
    // wiring would honor system proxies (a common cause of private-NAS WebDAV
    // failures). Build our own: no proxy, bounded timeouts.
    let http_client = http::new_http_client();

    gpui_kit::application()
        .with_http_client(http_client)
        .with_assets(gpui_kit::assets::Assets)
        .run(move |cx| {
            gpui_kit::init(cx);

            let opts = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    gpui_kit::gpui::Size::new(px(860.), px(640.)),
                    cx,
                ))),
                titlebar: Some(gpui_kit::gpui::TitlebarOptions {
                    title: Some(SharedString::from("Doit")),
                    ..Default::default()
                }),
                ..Default::default()
            };
            cx.open_window(opts, |window, cx| {
                let app = cx.new(|cx| DoitApp::new(window, cx));
                cx.set_global(DoitAppHandle(app.clone()));
                cx.new(|cx| Root::new(app, window, cx))
            })
            .expect("failed to open window");
        });
}
