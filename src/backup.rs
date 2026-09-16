use gpui_kit::base::Disableable as _;
use gpui_kit::base::StyledExt as _;
use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::{
    ActiveTheme, Sizable,
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

/// Each operation (test / sync) tracks its own state, so running one does not
/// disable the other.
pub struct BackupPanel {
    url: Entity<InputState>,
    user: Entity<InputState>,
    pass: Entity<InputState>,
    test_state: TransferState,
    sync_state: TransferState,
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
            sync_state: TransferState::Idle,
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
        self.sync_state = TransferState::Idle;
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

    /// Finish "立即同步": apply the merged snapshot (already persisted to the
    /// cloud) onto local data and surface the result inline in the dialog.
    /// Since the dialog is the top layer, no toast is needed — one visible
    /// prompt, nothing occluded.
    fn finish_sync(
        entity: Entity<Self>,
        result: Result<(SyncSnapshot, String), String>,
        window: &mut Window,
        cx: &mut App,
    ) {
        match result {
            Ok((snap, msg)) => {
                if let Some(app) = cx.try_global::<DoitAppHandle>().map(|h| h.0.clone()) {
                    app.update(cx, |app, cx| app.apply_snapshot(snap, window, cx));
                }
                entity.update(cx, |p, cx| {
                    p.sync_state = TransferState::Ok(msg);
                    cx.notify();
                });
            }
            Err(e) => {
                entity.update(cx, |p, cx| {
                    p.sync_state = TransferState::Err(e.clone());
                    cx.notify();
                });
            }
        }
    }
}

impl Render for BackupPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity();
        let url_value = self.url.read(cx).value().to_string();
        let test_busy = matches!(self.test_state, TransferState::Busy);
        let sync_busy = matches!(self.sync_state, TransferState::Busy);

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
                    // 立即同步（双向无丢失合并：并集 + 较新者胜，本地与云端均停在最新版）
                    .child(
                        div()
                            .v_flex()
                            .gap_1()
                            .items_start()
                            .child(
                                Button::new("backup-sync")
                                    .small()
                                    .ghost()
                                    .label(if sync_busy { "同步中…".to_string() } else { "立即同步".to_string() })
                                    .disabled(sync_busy || url_value.is_empty())
                                    .on_click({
                                        let entity = entity.clone();
                                        let client = cx.http_client();
                                        move |_, window, cx| {
                                            let entity = entity.clone();
                                            let client = client.clone();
                                            let cfg = entity.read(&*cx).config(&*cx);
                                            entity.update(&mut *cx, |p, cx| {
                                                p.sync_state = TransferState::Busy;
                                                cx.notify();
                                            });
                                            push_config(&mut *cx, &cfg);
                                            window.spawn(&*cx, async move |async_cx| {
                                                // 1) 取云端快照（首次无备份 → None）
                                                let remote = webdav::download_snapshot(
                                                    client.as_ref(),
                                                    &cfg.0,
                                                    &cfg.1,
                                                    &cfg.2,
                                                )
                                                .await;
                                                // 2) 主线程构造本地快照；最新版本为准
                                                //    （本地数据最后修改时间 vs 云端 exported_at），
                                                //    本地更晚则删除/改动生效，云端更晚则拉取云端。
                                                let merged: Result<SyncSnapshot, String> =
                                                    match async_cx.update(|_window, app_cx| {
                                                        let app = app_cx
                                                            .try_global::<DoitAppHandle>()
                                                            .map(|h| h.0.clone());
                                                        let Some(app) = app else {
                                                            return Err("内部错误：本地数据不可用"
                                                                .to_string());
                                                        };
                                                        let last = app.read(app_cx).last_modified.clone();
                                                        let (todos, settings) = (
                                                            app.read(app_cx).todos.clone(),
                                                            app.read(app_cx).settings.clone(),
                                                        );
                                                        let local = SyncSnapshot {
                                                            version: 1,
                                                            exported_at: last.clone(),
                                                            todos,
                                                            settings,
                                                        };
                                                        match remote {
                                                            Err(e) => Err(e),
                                                            Ok(remote) => Ok(
                                                                webdav::choose_authoritative(
                                                                    &last,
                                                                    local,
                                                                    remote,
                                                                ),
                                                            ),
                                                        }
                                                    }) {
                                                        Ok(r) => r,
                                                        Err(e) => {
                                                            Err(format!("窗口更新失败：{e}"))
                                                        }
                                                    };
                                                // 3) 把选定的版本以"现在"为同步时点写回云端；
                                                //    成功后才应用到本地，保证两端停在同一版本。
                                                let outcome: Result<(SyncSnapshot, String), String> =
                                                    match merged {
                                                        Ok(mut snap) => {
                                                            snap.exported_at =
                                                                webdav::local_stamp();
                                                            match webdav::upload_snapshot(
                                                                client.as_ref(),
                                                                &cfg.0,
                                                                &cfg.1,
                                                                &cfg.2,
                                                                &snap,
                                                            )
                                                            .await
                                                            {
                                                                Ok(_) => Ok((
                                                                    snap,
                                                                    "已同步，本地与云端均为最新"
                                                                        .to_string(),
                                                                )),
                                                                Err(e) => Err(e),
                                                            }
                                                        }
                                                        Err(e) => Err(e),
                                                    };
                                                // 4) 应用到本地并更新状态（结果在对话框内展示）
                                                let _ = async_cx.update(|window, app_cx| {
                                                    BackupPanel::finish_sync(
                                                        entity.clone(),
                                                        outcome,
                                                        window,
                                                        app_cx,
                                                    );
                                                });
                                            })
                                            .detach();
                                        }
                                    }),
                            )
                            .child(action_status(&self.sync_state, cx)),
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
            app.save_local();
            cx.notify();
        });
    }
}
