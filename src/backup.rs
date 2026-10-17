use gpui_kit::base::Disableable as _;
use gpui_kit::base::StyledExt as _;
use gpui_kit::component::button::{ButtonVariants as _, ButtonVariant};
use gpui_kit::component::{
    ActiveTheme, Sizable, WindowExt,
    button::Button,
    input::{Input, InputState},
};
use gpui_kit::prelude::*;
use std::sync::Arc;
use gpui_kit::gpui::{
    div, App, Context, Entity, IntoElement, ParentElement, Render, SharedString, Styled, Window,
    http_client::HttpClient, px,
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
            .default_value(
                // `default_value` takes `impl Into<SharedString>`; a plain
                // String satisfies it, so no conversion wrapper is needed.
                crate::crypto::decrypt_for_display(&settings.cloud_sync.webdav_password),
            ));

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
        // Stored password is ciphertext; decrypt for display so the user (and
        // the sync flow) sees the real value, not the `enc:` blob.
        set(
            &self.pass,
            SharedString::from(crate::crypto::decrypt_for_display(
                &settings.cloud_sync.webdav_password,
            )),
        );
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

    /// Persist `snap` to the cloud (stamped "now") and apply it locally, showing
    /// `ok_msg` on success. Shared by the automatic newest-wins merge and the
    /// user-confirmed "cover local" path.
    fn finalize(
        entity: Entity<Self>,
        client: Arc<dyn HttpClient>,
        cfg: (String, String, String),
        mut snap: SyncSnapshot,
        ok_msg: &'static str,
        window: &mut Window,
        cx: &mut App,
    ) {
        window.spawn(cx, async move |async_cx| {
            // 以"现在"为同步时点：同时推进云端 exported_at 与本机记录的
            // last_synced_at（同步基准），应用后 apply_snapshot 会把这份
            // settings 立回本地，两端从此停在同一个基准上。
            let stamp = webdav::local_stamp();
            snap.exported_at = stamp.clone();
            snap.settings.cloud_sync.last_synced_at = stamp;
            let outcome: Result<(SyncSnapshot, String), String> =
                match webdav::upload_snapshot(client.as_ref(), &cfg.0, &cfg.1, &cfg.2, &snap).await
                {
                    Ok(_) => Ok((snap, ok_msg.to_string())),
                    Err(e) => Err(e),
                };
            let _ = async_cx.update(|window, app_cx| {
                BackupPanel::finish_sync(entity.clone(), outcome, window, app_cx);
            });
        })
        .detach();
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
                    // 立即同步（以上次同步为基准：只改本地→上传本地；只改云端→拉取云端；
                    // 双端都改→询问用户；两端都没改→保持不变）
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
                                                // 1) 主线程：本地快照 + 最后修改时间
                                                let local = match async_cx.update(
                                                    |_window, app_cx| {
                                                        let app = app_cx
                                                            .try_global::<DoitAppHandle>()
                                                            .map(|h| h.0.clone());
                                                        let Some(app) = app else {
                                                            return Err(
                                                                "内部错误：本地数据不可用"
                                                                    .to_string(),
                                                            );
                                                        };
                                                        let last = app
                                                            .read(app_cx)
                                                            .last_modified
                                                            .clone();
                                                        let (todos, settings) = (
                                                            app.read(app_cx).todos.clone(),
                                                            app.read(app_cx)
                                                                .settings
                                                                .clone(),
                                                        );
                                                        // 上次成功同步的时间点（衡量两侧的基准）
                                                        let base = settings
                                                            .cloud_sync
                                                            .last_synced_at
                                                            .clone();
                                                        Ok((
                                                            SyncSnapshot {
                                                                version: 1,
                                                                exported_at: last.clone(),
                                                                todos,
                                                                settings,
                                                            },
                                                            last,
                                                            base,
                                                        ))
                                                    },
                                                ) {
                                                    Ok(r) => r,
                                                    Err(e) => {
                                                        let _ = async_cx.update(
                                                            |window, app_cx| {
                                                                BackupPanel::finish_sync(
                                                                    entity.clone(),
                                                                    Err(format!(
                                                                        "窗口更新失败：{e}"
                                                                    )),
                                                                    window,
                                                                    app_cx,
                                                                );
                                                            },
                                                        );
                                                        return;
                                                    }
                                                };
                                                let (local, last, base) = match local {
                                                    Ok(pair) => pair,
                                                    Err(e) => {
                                                        let _ = async_cx.update(
                                                            |window, app_cx| {
                                                                BackupPanel::finish_sync(
                                                                    entity.clone(),
                                                                    Err(e),
                                                                    window,
                                                                    app_cx,
                                                                );
                                                            },
                                                        );
                                                        return;
                                                    }
                                                };
                                                // 2) 取云端快照（首次无备份 → None）
                                                let remote = webdav::download_snapshot(
                                                    client.as_ref(),
                                                    &cfg.0,
                                                    &cfg.1,
                                                    &cfg.2,
                                                )
                                                .await;
                                                match remote {
                                                    Err(e) => {
                                                        let _ = async_cx.update(
                                                            |window, app_cx| {
                                                                BackupPanel::finish_sync(
                                                                    entity.clone(),
                                                                    Err(e),
                                                                    window,
                                                                    app_cx,
                                                                );
                                                            },
                                                        );
                                                    }
                                                    Ok(remote) => {
                                                        // 2) 按"上次同步基准时间点"决定：
                                                        //    双端都改 → 弹框询问；仅本地改 → 上传覆盖云端；
                                                        //    仅云端改（或都没改）→ 拉取云端覆盖本地。
                                                        let action = webdav::decide_sync(
                                                            &base,
                                                            &last,
                                                            remote.as_ref(),
                                                        );
                                                        use webdav::SyncDecision;
                                                        match action {
                                                            SyncDecision::AskUser => {
                                                                // 双端都改 → 弹确认框，由用户决定保留哪边
                                                                let Some(remote) = remote else {
                                                                    let _ = async_cx.update(
                                                                        |window, app_cx| {
                                                                            BackupPanel::finish_sync(
                                                                                entity.clone(),
                                                                                Err("内部错误：云端数据缺失".to_string()),
                                                                                window,
                                                                                app_cx,
                                                                            );
                                                                        },
                                                                    );
                                                                    return;
                                                                };
                                                                let entity_c = entity.clone();
                                                                let client_c = client.clone();
                                                                let cfg_c = cfg.clone();
                                                                let local_c = local.clone();
                                                                let remote_c = remote.clone();
                                                                let _ = async_cx.update(
                                                                    move |window, app_cx| {
                                                                        window.open_alert_dialog(
                                                                            app_cx,
                                                                            move |alert, _window, _cx| {
                                                                                // open_alert_dialog 的构建闭包和两个按钮回调都是 Fn，
                                                                                // 每次调用重新克隆一份，供各自按 move 捕获
                                                                                let (entity_ok, client_ok, cfg_ok, remote_ok) = (
                                                                                    entity_c.clone(),
                                                                                    client_c.clone(),
                                                                                    cfg_c.clone(),
                                                                                    remote_c.clone(),
                                                                                );
                                                                                let (entity_keep, client_keep, cfg_keep, local_keep) = (
                                                                                    entity_c.clone(),
                                                                                    client_c.clone(),
                                                                                    cfg_c.clone(),
                                                                                    local_c.clone(),
                                                                                );
                                                                                alert
                                                                                    .title("同步冲突")
                                                                                    .description(format!(
                                                                                        "本地与云端在上次同步后都有修改（本地 {}，云端 {}）。用云端数据覆盖本地，还是保留本地数据？",
                                                                                        local_c.exported_at,
                                                                                        remote_c.exported_at,
                                                                                    ))
                                                                                    .ok_text("用云端覆盖本地")
                                                                                    .ok_variant(ButtonVariant::Danger)
                                                                                    .cancel_text("保留本地")
                                                                                    .show_cancel(true)
                                                                                    // 只能点按钮，避免 Esc 误触触发同步
                                                                                    .keyboard(false)
                                                                                    .on_ok(
                                                                                        move |_, window, cx| {
                                                                                            BackupPanel::finalize(
                                                                                                entity_ok.clone(),
                                                                                                client_ok.clone(),
                                                                                                cfg_ok.clone(),
                                                                                                remote_ok.clone(),
                                                                                                "已用云端数据覆盖本地",
                                                                                                window,
                                                                                                cx,
                                                                                            );
                                                                                            true
                                                                                        },
                                                                                    )
                                                                                    .on_cancel(
                                                                                        move |_, window, cx| {
                                                                                            BackupPanel::finalize(
                                                                                                entity_keep.clone(),
                                                                                                client_keep.clone(),
                                                                                                cfg_keep.clone(),
                                                                                                local_keep.clone(),
                                                                                                "已保留本地数据（未覆盖）",
                                                                                                window,
                                                                                                cx,
                                                                                            );
                                                                                            true
                                                                                        },
                                                                                    )
                                                                            },
                                                                        );
                                                                    },
                                                                );
                                                            }
                                                            SyncDecision::PushLocal => {
                                                                // 3) 上传本地数据覆盖云端
                                                                let _ = async_cx.update(
                                                                    move |window, app_cx| {
                                                                        BackupPanel::finalize(
                                                                            entity.clone(),
                                                                            client.clone(),
                                                                            cfg.clone(),
                                                                            local,
                                                                            "已用本地数据覆盖云端",
                                                                            window,
                                                                            app_cx,
                                                                        );
                                                                    },
                                                                );
                                                            }
                                                            SyncDecision::PullRemote => {
                                                                // 4) 拉取云端数据覆盖本地
                                                                let remote = match remote {
                                                                    Some(r) => r,
                                                                    None => {
                                                                        let _ =
                                                                            async_cx.update(
                                                                                |window,
                                                                                 app_cx| {
                                                                                    BackupPanel::finalize(
                                                                                        entity.clone(),
                                                                                        client.clone(),
                                                                                        cfg.clone(),
                                                                                        local,
                                                                                        "已同步，本地与云端均为最新",
                                                                                        window,
                                                                                        app_cx,
                                                                                    );
                                                                                },
                                                                            );
                                                                        return;
                                                                    }
                                                                };
                                                                let _ = async_cx.update(
                                                                    move |window, app_cx| {
                                                                        BackupPanel::finalize(
                                                                            entity.clone(),
                                                                            client.clone(),
                                                                            cfg.clone(),
                                                                            remote,
                                                                            "已用云端数据覆盖本地",
                                                                            window,
                                                                            app_cx,
                                                                        );
                                                                    },
                                                                );
                                                            }
                                                        }
                                                    }
                                                }
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
/// The password is encrypted at rest (and thus in any exported cloud snapshot);
/// only the in-memory value used for auth stays plaintext.
fn push_config(cx: &mut App, cfg: &(String, String, String)) {
    let app = cx.try_global::<DoitAppHandle>().map(|h| h.0.clone());
    if let Some(app) = app {
        let cfg = (cfg.0.clone(), cfg.1.clone(), cfg.2.clone());
        let password = crate::crypto::encrypt_or_error(&cfg.2).unwrap_or_default();
        app.update(cx, |app, cx| {
            app.settings.cloud_sync.webdav_url = cfg.0.into();
            app.settings.cloud_sync.webdav_username = cfg.1.into();
            app.settings.cloud_sync.webdav_password = password.into();
            app.settings.cloud_sync.enabled = true;
            app.save_local();
            cx.notify();
        });
    }
}
