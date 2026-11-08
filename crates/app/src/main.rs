#![cfg_attr(windows, windows_subsystem = "windows")]

mod export;
mod format;
mod live;
mod run;
mod selection;
mod source;
mod ui;

use gpui_kit::*;

fn main() {
    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(|cx| {
            gpui_kit::init(cx);
            let bounds = Bounds::centered(None, size(px(900.), px(820.)), cx);
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some(export::APP_NAME.into()),
                    ..Default::default()
                }),
                ..Default::default()
            };
            gpui_kit::open_window(options, cx, |_, cx| cx.new(ui::AppView::new))
                .expect("open the main window");
        });
}
