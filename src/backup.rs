use gpui_kit::base::Disableable as _;
use gpui_kit::base::StyledExt as _;
use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::notification::Notification;
use gpui_kit::component::{
    ActiveTheme, Sizable, WindowExt,
    button::Button,
    input::{Input, InputState},
};
use gpui_kit::prelude::*;
use gpui_kit::gpui::{
    div, App, Context, Entity, IntoElement, ParentElement, Render, SharedString, Styled, Window,
    px,
};
use crate::app::DoitAppHandle;
use crate::types::*;
use crate::webdav::{self, TransferState};

// ── Backup / sync panel ─────────────────────────────────────────────────────

/// Each operation (test / upload / download) tracks its own state, so running
/// one does not disable the others.
pub struct BackupPanel {
    url: Entity<InputState>,
    user: Entity<InputState>,
    pass: Entity<InputState>,
    test_state: TransferState,
    upload_state: TransferState,
    download_state: TransferState,
}

impl BackupPanel {
    pub fn new(settings: &AppSettings, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let url = cx.new(|cx| InputState::new(window, cx)
            .placeholder("https://example.com/webdav/")
            .default_value(settings.cloud_sync.webdav_url.clone()));
        let user = cx.new(|cx| InputState::new(window, cx)
            .placeholder("用户名")
            .default_value(settings.cloud_sync.webdav_username.clone()));
        let pass = cx.new(|cx| InputState::new(window, cx)
            .placeholder("密码")
            .masked(true)
            .default_value(settings.cloud_sync.webdav_password.clone()));

        Self {
            url,
            user,
            pass,
            test_state: TransferState::Idle,
            upload_state: TransferState::Idle,
            download_state: TransferState::Idle,
        }
    }

    /// Prefill the config inputs from the current settings and reset all states.
    pub(crate) fn sync_from(
        &mut self,
        settings: &AppSettings,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.test_state = TransferState::Idle;
        self.upload_state = TransferState::Idle;
        self.download_state = TransferState::Idle;
        let mut set = |st: &Entity<InputState>, value: SharedString| {
            st.update(cx, |s, cx| s.set_value(value, window, cx));
        };
        set(&self.url, settings.cloud_sync.webdav_url.clone());
        set(&self.user, settings.cloud_sync.webdav_username.clone());
        set(&self.pass, settings.cloud_sync.webdav_password.clone());
    }

    /// The current URL / user / password typed into the panel.
    fn config(&self, cx: &App) -> (String, String, String) {
        (
            self.url.read(cx).value().to_string(),
            self.user.read(cx).value().to_string(),
            self.pass.read(cx).value().to_string(),
        )
    }

    // ── Async flow helpers ──

    fn finish_test(entity: Entity<Self>, result: Result<String, String>, cx: &mut App) {
        let msg = match result {
            Ok(m) => TransferState::Ok(m),
            Err(e) => TransferState::Err(e),
        };
        entity.update(cx, |p, cx| {
            p.test_state = msg;
            cx.notify();
        });
    }

    fn finish_upload(
        entity: Entity<Self>,
        result: Result<String, String>,
        window: &mut Window,
        cx: &mut App,
    ) {
        let ok = result.is_ok();
        let msg = match result {
            Ok(m) => m,
            Err(e) => e,
        };
        entity.update(cx, |p, cx| {
            p.upload_state = if ok {
                TransferState::Ok(msg.clone())
            } else {
                TransferState::Err(msg.clone())
            };
            cx.notify();
        });
        let note = if ok {
            Notification::success(msg).title("上传完成")
        } else {
            Notification::error(msg).title("上传失败")
        };
        window.push_notification(note, cx);
    }

    fn finish_download(
        entity: Entity<Self>,
        result: Result<SyncSnapshot, String>,
        window: &mut Window,
        cx: &mut App,
    ) {
        match result {
            Ok(snap) => {
                let app = cx.try_global::<DoitAppHandle>().map(|h| h.0.clone());
                if let Some(app) = app {
                    let snap = snap.clone();
                    app.update(cx, |app, cx| app.apply_snapshot(snap, window, cx));
                }
                entity.update(cx, |p, cx| {
                    p.download_state =
                        TransferState::Ok("下载成功，本地数据已恢复".to_string());
                    cx.notify();
                });
                window.push_notification(
                    Notification::success("下载成功，本地数据已恢复").title("下载恢复"),
                    cx,
                );
            }
            Err(e) => {
                entity.update(cx, |p, cx| {
                    p.download_state = TransferState::Err(e.clone());
                    cx.notify();
                });
                window.push_notification(Notification::error(e).title("下载恢复"), cx);
            }
        }
    }
}

impl Render for BackupPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity();
        let url_value = self.url.read(cx).value().to_string();
        let test_busy = matches!(self.test_state, TransferState::Busy);
        let upload_busy = matches!(self.upload_state, TransferState::Busy);
        let download_busy = matches!(self.download_state, TransferState::Busy);

        div()
            .v_flex()
            .size_full()
            .min_h(px(300.))
            .p_4()
            .gap_4()
            .child(
                div()
                    .v_flex()
                    .gap_3()
                    .child(
                        div()
                            .v_flex()
                            .gap_1()
                            .child(
                                div()
                                    .text_xs()
                                    .font_semibold()
                                    .text_color(cx.theme().muted_foreground)
                                    .child("WebDAV 地址"),
                            )
                            .child(Input::new(&self.url).small()),
                    )
                    .child(
                        div()
                            .v_flex()
                            .gap_1()
                            .child(
                                div()
                                    .text_xs()
                                    .font_semibold()
                                    .text_color(cx.theme().muted_foreground)
                                    .child("用户名"),
                            )
                            .child(Input::new(&self.user).small()),
                    )
                    .child(
                        div()
                            .v_flex()
                            .gap_1()
                            .child(
                                div()
                                    .text_xs()
                                    .font_semibold()
                                    .text_color(cx.theme().muted_foreground)
                                    .child("密码"),
                            )
                            .child(Input::new(&self.pass).small()),
                    ),
            )
            .child(div().flex_1().min_h_0())
            .child(div().border_t_1().border_color(cx.theme().border))
            .child(
                div()
                    .flex()
                    .items_start()
                    .gap_4()
                    // 测试连接
                    .child(
                        div()
                            .v_flex()
                            .gap_1()
                            .items_start()
                            .child(
                                Button::new("backup-test")
                                    .small()
                                    .ghost()
                                    .label(if test_busy { "测试中…".to_string() } else { "测试连接".to_string() })
                                    .disabled(test_busy)
                                    .on_click({
                                        let entity = entity.clone();
                                        let client = cx.http_client();
                                        move |_, window, cx| {
                                            let entity = entity.clone();
                                            let client = client.clone();
                                            let cfg = entity.read(&*cx).config(&*cx);
                                            entity.update(&mut *cx, |p, cx| {
                                                p.test_state = TransferState::Busy;
                                                cx.notify();
                                            });
                                            push_config(&mut *cx, &cfg);
                                            window.spawn(&*cx, async move |async_cx| {
                                                let result = webdav::test_connection(
                                                    client.as_ref(),
                                                    &cfg.0,
                                                    &cfg.1,
                                                    &cfg.2,
                                                )
                                                .await;
                                                let _ = async_cx.update(|_window, app_cx| {
                                                    BackupPanel::finish_test(
                                                        entity.clone(),
                                                        result,
                                                        app_cx,
                                                    );
                                                });
                                            })
                                            .detach();
                                        }
                                    }),
                            )
                            .child(action_status(&self.test_state, cx)),
                    )
                    // 上传快照
                    .child(
                        div()
                            .v_flex()
                            .gap_1()
                            .items_start()
                            .child(
                                Button::new("backup-upload")
                                    .small()
                                    .ghost()
                                    .label(if upload_busy { "上传中…".to_string() } else { "上传".to_string() })
                                    .disabled(upload_busy || url_value.is_empty())
                                    .on_click({
                                        let entity = entity.clone();
                                        let client = cx.http_client();
                                        move |_, window, cx| {
                                            let entity = entity.clone();
                                            let client = client.clone();
                                            let cfg = entity.read(&*cx).config(&*cx);
                                            let snapshot = {
                                                let app = cx
                                                    .try_global::<DoitAppHandle>()
                                                    .map(|h| h.0.clone());
                                                match app {
                                                    Some(app) => {
                                                        let (todos, settings) =
                                                            app.read(cx).snapshot_data();
                                                        SyncSnapshot {
                                                            version: 1,
                                                            exported_at: webdav::local_stamp(),
                                                            todos,
                                                            settings,
                                                        }
                                                    }
                                                    None => return,
                                                }
                                            };
                                            entity.update(&mut *cx, |p, cx| {
                                                p.upload_state = TransferState::Busy;
                                                cx.notify();
                                            });
                                            push_config(&mut *cx, &cfg);
                                            window.spawn(&*cx, async move |async_cx| {
                                                let result = webdav::upload_snapshot(
                                                    client.as_ref(),
                                                    &cfg.0,
                                                    &cfg.1,
                                                    &cfg.2,
                                                    &snapshot,
                                                )
                                                .await;
                                                let _ = async_cx.update(|window, app_cx| {
                                                    BackupPanel::finish_upload(
                                                        entity.clone(),
                                                        result,
                                                        window,
                                                        app_cx,
                                                    );
                                                });
                                            })
                                            .detach();
                                        }
                                    }),
                            )
                            .child(action_status(&self.upload_state, cx)),
                    )
                    // 下载恢复
                    .child(
                        div()
                            .v_flex()
                            .gap_1()
                            .items_start()
                            .child(
                                Button::new("backup-download")
                                    .small()
                                    .ghost()
                                    .label(if download_busy { "下载中…".to_string() } else { "下载".to_string() })
                                    .disabled(download_busy || url_value.is_empty())
                                    .on_click({
                                        let entity = entity.clone();
                                        let client = cx.http_client();
                                        move |_, window, cx| {
                                            let entity = entity.clone();
                                            let client = client.clone();
                                            let cfg = entity.read(&*cx).config(&*cx);
                                            entity.update(&mut *cx, |p, cx| {
                                                p.download_state = TransferState::Busy;
                                                cx.notify();
                                            });
                                            push_config(&mut *cx, &cfg);
                                            window.spawn(&*cx, async move |async_cx| {
                                                let result = webdav::download_snapshot(
                                                    client.as_ref(),
                                                    &cfg.0,
                                                    &cfg.1,
                                                    &cfg.2,
                                                )
                                                .await;
                                                let _ = async_cx.update(|window, app_cx| {
                                                    BackupPanel::finish_download(
                                                        entity.clone(),
                                                        result,
                                                        window,
                                                        app_cx,
                                                    );
                                                });
                                            })
                                            .detach();
                                        }
                                    }),
                            )
                            .child(action_status(&self.download_state, cx)),
                    ),
            )
    }
}

// ── Small composition helpers ───────────────────────────────────────────────

/// The inline result text for one operation (hidden while idle/busy — the
/// button label already carries the busy state).
fn action_status(state: &TransferState, cx: &App) -> impl IntoElement {
    match state {
        TransferState::Idle | TransferState::Busy => div().into_any_element(),
        TransferState::Ok(msg) => div()
            .text_xs()
            .text_color(cx.theme().success)
            .child(msg.clone())
            .into_any_element(),
        TransferState::Err(msg) => div()
            .text_xs()
            .text_color(cx.theme().danger)
            .child(msg.clone())
            .into_any_element(),
    }
}

/// Mirror the typed config back into the app settings so settings stay in sync.
fn push_config(cx: &mut App, cfg: &(String, String, String)) {
    let app = cx.try_global::<DoitAppHandle>().map(|h| h.0.clone());
    if let Some(app) = app {
        let cfg = (cfg.0.clone(), cfg.1.clone(), cfg.2.clone());
        app.update(cx, |app, cx| {
            app.settings.cloud_sync.webdav_url = cfg.0.into();
            app.settings.cloud_sync.webdav_username = cfg.1.into();
            app.settings.cloud_sync.webdav_password = cfg.2.into();
            app.settings.cloud_sync.enabled = true;
            cx.notify();
        });
    }
}
