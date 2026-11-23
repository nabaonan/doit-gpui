//! doit-gpui 应用入口（可执行文件）
//!
//! 业务实现全部位于库 crate `doit_gpui`（见 `src/lib.rs`），
//! 这里只负责启动 GPUI 应用并打开主窗口。

use doit_gpui::app::{apply_component_theme, AppState, AppView};
use gpui_kit::*;

fn main() {
    gpui_kit::application().run(|cx| {
        gpui_kit::init(cx);
        // 把 gpui-component 内置主题的 focus ring / input 边框统一覆盖为薄荷绿，
        // 与应用自身主题联动（设置页切换主题时会再次调用保持同步）
        apply_component_theme(false, cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            let state = cx.new(|cx| AppState::new(window, cx));
            cx.new(|cx| AppView::new(state.clone(), window, cx))
        })
        .expect("failed to open window");
    });
}
