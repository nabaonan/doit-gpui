use gpui_kit::base::Disableable as _;
use gpui_kit::base::Selectable as _;
use gpui_kit::base::StyledExt as _;
use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::button::ButtonVariant;
use gpui_kit::component::dialog::DialogButtonProps;
use gpui_kit::component::{
    ActiveTheme, Sizable, ThemeMode, WindowExt,
    button::Button,
    input::{Input, InputState, InputEvent},
    kbd::Kbd,
    notification::Notification,
    progress::Progress,
    radio::RadioGroup,
    setting::{SettingGroup, SettingItem, SettingPage, Settings},
    slider::{Slider, SliderEvent, SliderState, SliderValue},
    switch::Switch,
    text::TextView,
};
use gpui_kit::prelude::*;
use futures::AsyncReadExt;
use std::sync::Arc;
use gpui_kit::gpui::{
    div, App, Context, Entity, FocusHandle,
    IntoElement, KeyDownEvent, Keystroke, Modifiers, ParentElement, PathPromptOptions, Render,
    SharedString, Styled, Subscription, Window, px, ElementId,
};
use gpui_kit::gpui::http_client::{AsyncBody, HttpClient, Method, Request, StatusCode};
use crate::app::DoitAppHandle;
use crate::label_dialogs::{CategoriesPanel, TagsPanel};
use crate::types::*;
use std::cmp::Ordering;
use crate::webdav::{self, TransferState};

// ── Settings Panel ─────────────────────────────────────────────────────────

pub struct SettingsPanel {
    local: AppSettings,

    url: Entity<InputState>,
    user: Entity<InputState>,
    pass: Entity<InputState>,
    keep_recent: Entity<InputState>,
    backup_interval: Entity<InputState>,
    restore_interval: Entity<InputState>,
    long_press: Entity<SliderState>,

    recording: bool,
    recording_handle: FocusHandle,
    sync_test: TransferState,
    update_state: TransferState,
    categories_panel: Entity<CategoriesPanel>,
    tags_panel: Entity<TagsPanel>,

    _subs: Vec<Subscription>,
}

impl SettingsPanel {
    pub fn new(settings: AppSettings, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let url = cx.new(|cx| InputState::new(window, cx).placeholder("https://example.com/webdav/"));
        let user = cx.new(|cx| InputState::new(window, cx).placeholder("用户名"));
        let pass = cx.new(|cx| InputState::new(window, cx).placeholder("密码").masked(true));
        let keep_recent = cx.new(|cx| {
            InputState::new(window, cx).default_value(settings.cloud_sync.keep_recent.to_string())
        });
        let backup_interval = cx.new(|cx| {
            InputState::new(window, cx).default_value(settings.auto_backup.interval.to_string())
        });
        let restore_interval = cx.new(|cx| {
            InputState::new(window, cx).default_value(settings.auto_restore.interval.to_string())
        });
        let long_press = cx.new(|_cx| SliderState::new().min(1.0).max(10.0).step(1.0));

        let mut subs = Vec::new();
        subs.push(cx.subscribe::<InputState, InputEvent>(&url, |this, st, ev, cx| {
            if let InputEvent::Change = ev {
                this.local.cloud_sync.webdav_url = st.read(cx).value();
                this.push_settings(cx);
            }
        }));
        subs.push(cx.subscribe::<InputState, InputEvent>(&user, |this, st, ev, cx| {
            if let InputEvent::Change = ev {
                this.local.cloud_sync.webdav_username = st.read(cx).value();
                this.push_settings(cx);
            }
        }));
        subs.push(cx.subscribe::<InputState, InputEvent>(&pass, |this, st, ev, cx| {
            if let InputEvent::Change = ev {
                // Encrypt before persisting: settings are written to disk and
                // exported in cloud snapshots; never store the plaintext.
                this.local.cloud_sync.webdav_password =
                    crate::crypto::encrypt_or_error(&st.read(cx).value())
                        .unwrap_or_default()
                        .into();
                this.push_settings(cx);
            }
        }));
        subs.push(cx.subscribe::<InputState, InputEvent>(&keep_recent, |this, st, ev, cx| {
            if let InputEvent::Change = ev {
                this.local.cloud_sync.keep_recent = st.read(cx).value().parse().unwrap_or(0);
                this.push_settings(cx);
            }
        }));
        subs.push(cx.subscribe::<InputState, InputEvent>(&backup_interval, |this, st, ev, cx| {
            if let InputEvent::Change = ev {
                this.local.auto_backup.interval = st.read(cx).value().parse().unwrap_or(30);
                this.push_settings(cx);
            }
        }));
        subs.push(cx.subscribe::<InputState, InputEvent>(&restore_interval, |this, st, ev, cx| {
            if let InputEvent::Change = ev {
                this.local.auto_restore.interval = st.read(cx).value().parse().unwrap_or(30);
                this.push_settings(cx);
            }
        }));
        subs.push(cx.subscribe::<SliderState, SliderEvent>(&long_press, |this, _st, ev, cx| {
            if let SliderEvent::Change(SliderValue::Single(v)) = ev {
                this.local.long_press_duration = *v as u32;
                this.push_settings(cx);
            }
        }));

        let categories_panel = cx.new(|cx| {
            CategoriesPanel::new(
                settings.categories.clone(),
                settings.default_category_id.clone(),
                false,
                window,
                cx,
            )
        });
        let tags_panel = cx.new(|cx| TagsPanel::new(settings.tags.clone(), false, window, cx));

        Self {
            local: settings,
            url,
            user,
            pass,
            keep_recent,
            backup_interval,
            restore_interval,
            long_press,
            recording: false,
            recording_handle: cx.focus_handle(),
            sync_test: TransferState::Idle,
            update_state: TransferState::Idle,
            categories_panel,
            tags_panel,
            _subs: subs,
        }
    }

    /// Copy fresh settings into the panel and reset the text/slider controls.
    pub(crate) fn sync_from(&mut self, settings: AppSettings, window: &mut Window, cx: &mut Context<Self>) {
        self.local = settings.clone();
        self.recording = false;
        self.sync_test = TransferState::Idle;
        // A floating update request must never leave the button stuck.
        self.update_state = TransferState::Idle;

        let mut set = |st: &Entity<InputState>, value: SharedString| {
            st.update(cx, |s, cx| s.set_value(value, window, cx));
        };
        set(&self.url, settings.cloud_sync.webdav_url.clone());
        set(&self.user, settings.cloud_sync.webdav_username.clone());
        // Stored password is ciphertext; decrypt for display in the field.
        set(
            &self.pass,
            SharedString::from(crate::crypto::decrypt_for_display(
                &settings.cloud_sync.webdav_password,
            )),
        );
        set(&self.keep_recent, settings.cloud_sync.keep_recent.to_string().into());
        set(&self.backup_interval, settings.auto_backup.interval.to_string().into());
        set(&self.restore_interval, settings.auto_restore.interval.to_string().into());
        self.long_press.update(cx, |s, cx| {
            s.set_value(settings.long_press_duration as f32, window, cx)
        });
    }

    /// Rebuild the working copy plus the category/tag panels after an import
    /// replaced the application data, so the dialog reflects the new data.
    fn reload_after_import(&mut self, settings: AppSettings, window: &mut Window, cx: &mut Context<Self>) {
        self.sync_from(settings, window, cx);
        self.categories_panel = cx.new(|cx| {
            CategoriesPanel::new(
                self.local.categories.clone(),
                self.local.default_category_id.clone(),
                false,
                window,
                cx,
            )
        });
        self.tags_panel = cx.new(|cx| TagsPanel::new(self.local.tags.clone(), false, window, cx));
        cx.notify();
    }

    /// Mirror the working copy into the application's settings entity.
    /// Tags / categories / default category are edited by the dedicated label
    /// panels (which write straight into `app.settings` via `set_tags` /
    /// `set_categories`), so they must NOT be overwritten here with this
    /// panel's possibly-stale snapshot — otherwise a later unrelated setting
    /// change (theme, shortcut…) would silently roll back any labels added
    /// since the settings page was opened, and that content would never reach
    /// the cloud sync.
    fn push_settings(&mut self, cx: &mut Context<Self>) {
        let app = cx
            .try_global::<DoitAppHandle>()
            .map(|h| h.0.clone());
        if let Some(app) = app {
            let mut settings = self.local.clone();
            let current = app.read(cx);
            settings.tags = current.settings.tags.clone();
            settings.categories = current.settings.categories.clone();
            settings.default_category_id = current.settings.default_category_id.clone();
            app.update(cx, |app, cx| {
                app.settings = settings;
                app.save_local();
                cx.notify();
            });
        }
    }

    /// Apply the current theme choice to the window and mirror settings.
    fn apply_theme(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mode = match self.local.theme.as_ref() {
            "light" => ThemeMode::Light,
            "dark" => ThemeMode::Dark,
            _ => match window.appearance() {
                gpui_kit::gpui::WindowAppearance::Dark
                | gpui_kit::gpui::WindowAppearance::VibrantDark => ThemeMode::Dark,
                _ => ThemeMode::Light,
            },
        };
        crate::app::change_theme(mode, window, cx);
        self.push_settings(cx);
    }
}

// ── Render ──────────────────────────────────────────────────────────────────

impl Render for SettingsPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity();
        Settings::new("app-settings")
            .sidebar_width(px(180.))
            .sidebar_size_range(px(140.)..px(240.))
            .pages([
                Self::page_appearance(&entity),
                Self::page_interaction(&entity),
                Self::page_sync(&entity),
                Self::page_labels(&entity),
                Self::page_data(&entity),
                Self::page_about(&entity),
            ])
    }
}

impl SettingsPanel {
    // ── 外观 ──

    fn page_appearance(entity: &Entity<Self>) -> SettingPage {
        let entity = entity.clone();
        SettingPage::new("外观")
            .description("界面主题与配色")
            .group(
                SettingGroup::new()
                    .title("主题")
                    .item(SettingItem::render(move |_, _window, cx| {
                        let theme_ix = match entity.read(cx).local.theme.as_ref() {
                            "light" => Some(1),
                            "dark" => Some(2),
                            _ => Some(0),
                        };
                        let entity = entity.clone();
                        div()
                            .px_1()
                            .child(
                                RadioGroup::horizontal("theme-radios")
                                    .selected_index(theme_ix)
                                    .child("系统")
                                    .child("浅色")
                                    .child("深色")
                                    .on_click(move |ix, window, cx| {
                                        let ix = *ix;
                                        entity.update(cx, |p, cx| {
                                            p.local.theme = match ix {
                                                1 => "light".into(),
                                                2 => "dark".into(),
                                                _ => "system".into(),
                                            };
                                            p.apply_theme(window, cx);
                                            cx.notify();
                                        });
                                    })
                            )
                    })),
            )
    }

    // ── 分类与标签 ──

    fn page_labels(entity: &Entity<Self>) -> SettingPage {
        let categories_entity = entity.clone();
        let tags_entity = entity.clone();
        SettingPage::new("分类与标签")
            .description("管理分类与标签的颜色和顺序")
            .group(
                SettingGroup::new()
                    .title("分类")
                    .item(SettingItem::render(move |_, _window, cx| {
                        let panel = categories_entity.read(cx);
                        div().child(panel.categories_panel.clone())
                    })),
            )
            .group(
                SettingGroup::new()
                    .title("标签")
                    .item(SettingItem::render(move |_, _window, cx| {
                        let panel = tags_entity.read(cx);
                        div().child(panel.tags_panel.clone())
                    })),
            )
    }

    // ── 交互 ──

    fn page_interaction(entity: &Entity<Self>) -> SettingPage {
        let entity = entity.clone();
        let entity2 = entity.clone();
        SettingPage::new("交互")
            .description("完成方式与快捷键")
            .group(
                SettingGroup::new()
                    .title("完成方式")
                    .item(SettingItem::render(move |_, _window, cx| {
                        let is_longpress =
                            entity.read(cx).local.completion_mode.as_ref() == "longpress";
                        let mode_ix = if is_longpress { Some(1) } else { Some(0) };
                        div()
                            .v_flex()
                            .gap_3()
                            .child(
                                RadioGroup::horizontal("completion-radios")
                                    .selected_index(mode_ix)
                                    .child("勾选")
                                    .child("长按")
                                    .on_click({
                                        let entity = entity.clone();
                                        move |ix, _window, cx| {
                                            let ix = *ix;
                                            entity.update(cx, |p, cx| {
                                                p.local.completion_mode = if ix == 1 {
                                                    "longpress".into()
                                                } else {
                                                    "checkbox".into()
                                                };
                                                p.push_settings(cx);
                                                cx.notify();
                                            });
                                        }
                                    }),
                            )
                            .when(is_longpress, |el| {
                                let state = entity.read(cx);
                                let long_press = state.long_press.clone();
                                el.child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap_3()
                                        .child(
                                            div()
                                                .w(px(240.))
                                                .child(Slider::new(&long_press)),
                                        )
                                        .child(
                                            div()
                                                .text_sm()
                                                .child(format!(
                                                    "{} 秒",
                                                    state.local.long_press_duration
                                                )),
                                        ),
                                )
                            })
                    })),
            )
            .group(
                SettingGroup::new()
                    .title("添加快捷键")
                    .item(SettingItem::render(move |_, _window, cx| {
                        let state = entity2.read(cx);
                        let shortcut = state.local.add_todo_shortcut.clone();
                        let is_recording = state.recording;
                        let recording_handle = state.recording_handle.clone();
                        let shortcut_row = div()
                            .id("shortcut-recorder")
                            .track_focus(&recording_handle)
                            .on_key_down({
                                let entity = entity2.clone();
                                move |event: &KeyDownEvent, _window, cx| {
                                    if !entity.read(cx).recording {
                                        return;
                                    }
                                    let key = event.keystroke.key.clone();
                                    if key == "escape" {
                                        entity.update(cx, |p, cx| {
                                            p.recording = false;
                                            cx.notify();
                                        });
                                        return;
                                    }
                                    if matches!(
                                        key.as_str(),
                                        "control" | "shift" | "alt" | "meta" | "super"
                                            | "command"
                                    ) {
                                        return;
                                    }
                                    let mods = event.keystroke.modifiers;
                                    entity.update(cx, |p, cx| {
                                        p.local.add_todo_shortcut = ShortcutConfig {
                                            key: key.into(),
                                            ctrl: mods.control,
                                            shift: mods.shift,
                                            alt: mods.alt,
                                            meta: mods.platform,
                                        };
                                        p.recording = false;
                                        p.push_settings(cx);
                                        cx.notify();
                                    });
                                }
                            })
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(Kbd::new(shortcut_keystroke(&shortcut)))
                                    .child(
                                        Button::new("record-shortcut")
                                            .when(is_recording, |b| b.selected(true))
                                            .ghost()
                                            .small()
                                            .label(if is_recording {
                                                "请按下组合键…".to_string()
                                            } else {
                                                "改键".to_string()
                                            })
                                            .on_click({
                                                let entity = entity2.clone();
                                                move |_, window, cx| {
                                                    let handle = {
                                                        let panel = entity.read(cx);
                                                        panel.recording_handle.clone()
                                                    };
                                                    entity.update(cx, |p, cx| {
                                                        p.recording = true;
                                                        cx.notify();
                                                    });
                                                    window.focus(&handle, cx);
                                                }
                                            }),
                                    ),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(
                                        "该快捷键用于在输入框中确认新增（待办、分类、标签）。只有按下这里的组合键才会添加，默认是 Enter。",
                                    ),
                            );
                        div().v_flex().gap_2().child(shortcut_row)
                    })),
            )
    }

    // ── 同步 ──

    fn page_sync(entity: &Entity<Self>) -> SettingPage {
        let connect_entity = entity.clone();
        let recent_entity = entity.clone();
        let behavior_entity = entity.clone();
        let schedule_entity = entity.clone();
        let schedule_entity2 = entity.clone();
        SettingPage::new("同步")
            .description("WebDAV 云同步与定时任务")
            .group(
                SettingGroup::new()
                    .title("连接")
                    .items([
                        SettingItem::render(move |_, _window, cx| {
                            let state = connect_entity.read(cx);
                            let cloud = state.local.cloud_sync.clone();
                            let sync_enabled = cloud.enabled;
                            let url = state.url.clone();
                            let user = state.user.clone();
                            let pass = state.pass.clone();
                            div()
                                .v_flex()
                                .gap_3()
                                .child(
                                    row_block("启用云同步", "WebDAV 同步本地数据", cx).child(
                                        Switch::new("cloud-enabled")
                                            .small()
                                            .checked(cloud.enabled)
                                            .on_click({
                                                let entity = connect_entity.clone();
                                                move |checked, _window, cx| {
                                                    entity.update(cx, |p, cx| {
                                                        p.local.cloud_sync.enabled = *checked;
                                                        p.push_settings(cx);
                                                        cx.notify();
                                                    });
                                                }
                                            }),
                                    ),
                                )
                                .when(sync_enabled, |el| {
                                    let entity = connect_entity.clone();
                                    el.child(
                                        div()
                                            .v_flex()
                                            .gap_3()
                                            .child(
                                                field_block("WebDAV 地址", cx)
                                                    .child(Input::new(&url).small()),
                                            )
                                            .child(field_block("用户名", cx).child(Input::new(&user).small()))
                                            .child(field_block("密码", cx).child(Input::new(&pass).small()))
                                            .child(
                                                row_block(
                                                    "信任自签名证书",
                                                    "跳过证书校验（用于自签名 / 内网 NAS）",
                                                    cx,
                                                )
                                                .child(
                                                    Switch::new("trust-self-signed")
                                                        .small()
                                                        .checked(
                                                            entity.read(cx)
                                                                .local
                                                                .cloud_sync
                                                                .trust_self_signed,
                                                        )
                                                        .on_click({
                                                            let entity = connect_entity.clone();
                                                            move |checked, _window, cx| {
                                                                entity.update(cx, |p, cx| {
                                                                    p.local
                                                                        .cloud_sync
                                                                        .trust_self_signed = *checked;
                                                                    crate::http::set_trust_self_signed(
                                                                        *checked,
                                                                    );
                                                                    p.push_settings(cx);
                                                                    cx.notify();
                                                                });
                                                            }
                                                        }),
                                                ),
                                            )
                                            .child(Self::sync_test_row(&entity, cx)),
                                    )
                                })
                        }),
                        SettingItem::render(move |_, _window, cx| {
                            let state = recent_entity.read(cx);
                            let sync_enabled = state.local.cloud_sync.enabled;
                            let keep_recent = state.keep_recent.clone();
                            div()
                                .flex()
                                .items_center()
                                .justify_between()
                                .child(
                                    div()
                                        .v_flex()
                                        .gap_1()
                                        .child(div().text_sm().child("云端保留近"))
                                        .child(
                                            div()
                                                .text_xs()
                                                .text_color(cx.theme().muted_foreground)
                                                .child("0 = 不限制，保留的历史备份条数"),
                                        ),
                                )
                                .child(
                                    div()
                                        .w(px(120.))
                                        .child(
                                            Input::new(&keep_recent).small().disabled(!sync_enabled),
                                        ),
                                )
                        }),
                    ]),
            )
            .group(
                SettingGroup::new()
                    .title("同步行为")
                    .items([
                        SettingItem::render(move |_, _window, cx| {
                            let state = behavior_entity.read(cx);
                            let sync_enabled = state.local.cloud_sync.enabled;
                            let cloud = state.local.cloud_sync.clone();
                            div()
                                .v_flex()
                                .gap_3()
                                .child(
                                    row_block("启动时拉取", "应用启动时从 WebDAV 拉取最新数据", cx)
                                        .child(
                                            Switch::new("fetch-on-startup")
                                                .small()
                                                .disabled(!sync_enabled)
                                                .checked(cloud.fetch_on_startup)
                                                .on_click({
                                                    let entity = behavior_entity.clone();
                                                    move |checked, _window, cx| {
                                                        entity.update(cx, |p, cx| {
                                                            p.local.cloud_sync.fetch_on_startup = *checked;
                                                            p.push_settings(cx);
                                                            cx.notify();
                                                        });
                                                    }
                                                }),
                                        ),
                                )
                                .child(
                                    row_block("关闭时上传", "关闭窗口时自动把本地数据上传到 WebDAV", cx)
                                        .child(
                                            Switch::new("upload-on-exit")
                                                .small()
                                                .disabled(!sync_enabled)
                                                .checked(cloud.upload_on_exit)
                                                .on_click({
                                                    let entity = behavior_entity.clone();
                                                    move |checked, _window, cx| {
                                                        entity.update(cx, |p, cx| {
                                                            p.local.cloud_sync.upload_on_exit = *checked;
                                                            p.push_settings(cx);
                                                            cx.notify();
                                                        });
                                                    }
                                                }),
                                        ),
                                )
                        }),
                    ]),
            )
            .group(
                SettingGroup::new()
                    .title("定时任务")
                    .items([
                        SettingItem::render(move |_, _window, cx| {
                            let state = schedule_entity.read(cx);
                            let reachable = state.local.cloud_sync.enabled;
                            let backup = state.local.auto_backup.clone();
                            let backup_interval = state.backup_interval.clone();
                            Self::schedule_rows("定时备份", &backup, &backup_interval, reachable, &schedule_entity, cx)
                        }),
                        SettingItem::render(move |_, _window, cx| {
                            let state = schedule_entity2.read(cx);
                            let reachable = state.local.cloud_sync.enabled;
                            let restore = state.local.auto_restore.clone();
                            let restore_interval = state.restore_interval.clone();
                            div()
                                .v_flex()
                                .gap_2()
                                .child(Self::schedule_rows("定时恢复", &restore, &restore_interval, reachable, &schedule_entity2, cx))
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(cx.theme().warning)
                                        .child("提示：自动恢复会覆盖本地数据，请谨慎启用。"),
                                )
                        }),
                    ]),
            )
    }

    /// Run a WebDAV connection test with the panel's current credentials,
    /// updating the live status; shared by both test buttons.
    fn run_sync_test(
        entity: Entity<Self>,
        client: Arc<dyn gpui_kit::gpui::http_client::HttpClient>,
        window: &mut Window,
        cx: &mut App,
    ) {
        let cfg = {
            let p = entity.read(cx);
            (
                p.local.cloud_sync.webdav_url.trim().to_string(),
                p.local.cloud_sync.webdav_username.to_string(),
                // Stored value is ciphertext — decrypt for the connection test.
                crate::crypto::decrypt_for_display(&p.local.cloud_sync.webdav_password),
            )
        };
        entity.update(cx, |p, cx| {
            p.sync_test = TransferState::Busy;
            cx.notify();
        });
        window
            .spawn(&*cx, async move |async_cx| {
                let result =
                    webdav::test_connection(client.as_ref(), &cfg.0, &cfg.1, &cfg.2).await;
                let _ = async_cx.update(|_window, app_cx| {
                    let msg = match result {
                        Ok(m) => TransferState::Ok(m),
                        Err(e) => TransferState::Err(e),
                    };
                    entity.update(app_cx, |p, cx| {
                        p.sync_test = msg;
                        cx.notify();
                    });
                });
            })
            .detach();
    }

    /// A 「测试连接」button plus a live result line. Built from the live panel
    /// state so it works inside a `SettingItem::render` closure (`&mut App`).
    fn sync_test_row(entity: &Entity<Self>, cx: &mut App) -> impl IntoElement {
        let state = entity.read(cx).sync_test.clone();
        let entity = entity.clone();
        let client = cx.http_client();

        div()
            .flex()
            .items_center()
            .gap_3()
            .child(
                Button::new("sync-test")
                    .small()
                    .outline()
                    .label(match state {
                        TransferState::Busy => "测试中…".to_string(),
                        _ => "测试连接".to_string(),
                    })
                    .disabled(matches!(state, TransferState::Busy))
                    .on_click({
                        let entity = entity.clone();
                        let client = client.clone();
                        move |_, window, cx| {
                            Self::run_sync_test(entity.clone(), client.clone(), window, cx);
                        }
                    }),
            )
            .child(sync_state_text(&state, cx))
            .when(is_cert_error(&state), |el| {
                let entity = entity.clone();
                let client = client.clone();
                el.child(
                    Button::new("trust-cert-and-retry")
                        .small()
                        .ghost()
                        .label("信任此证书并重试")
                        .on_click(move |_, window, cx| {
                            entity.update(cx, |p, cx| {
                                p.local.cloud_sync.trust_self_signed = true;
                                crate::http::set_trust_self_signed(true);
                                p.push_settings(cx);
                                cx.notify();
                            });
                            Self::run_sync_test(entity.clone(), client.clone(), window, cx);
                        }),
                )
            })
    }

    /// One 「定时备份/恢复」row: enable switch + interval input + unit radio.
    /// The ignore `_window` — the settings shell owns window sizing.
    fn schedule_rows(
        title: &'static str,
        schedule: &ScheduleConfig,
        interval: &Entity<InputState>,
        reachable: bool,
        entity: &Entity<Self>,
        cx: &mut App,
    ) -> impl IntoElement {
        let enabled = schedule.enabled;
        let unit_ix = match schedule.unit.as_ref() {
            "hour" => Some(1),
            "day" => Some(2),
            _ => Some(0),
        };
        let is_backup = title == "定时备份";
        let key = if is_backup { "backup" } else { "restore" };
        let entity = entity.clone();

        div()
            .v_flex()
            .gap_2()
            .child(
                row_block(title, "", cx).child(
                    Switch::new(ElementId::Name(format!("schedule-{key}").into()))
                        .small()
                        .disabled(!reachable)
                        .checked(enabled)
                        .on_click({
                            let entity = entity.clone();
                            move |checked, _window, cx| {
                                entity.update(cx, |p, cx| {
                                    if is_backup {
                                        p.local.auto_backup.enabled = *checked;
                                    } else {
                                        p.local.auto_restore.enabled = *checked;
                                    }
                                    p.push_settings(cx);
                                    cx.notify();
                                });
                            }
                        }),
                ),
            )
            .when(enabled, |el| {
                let interval = interval.clone();
                let entity = entity.clone();
                el.child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                .child("每"),
                        )
                        .child(
                            div()
                                .w(px(100.))
                                .child(Input::new(&interval).small().disabled(!reachable)),
                        )
                        .child({
                            let unit_ix = unit_ix;
                            let entity = entity.clone();
                            RadioGroup::horizontal(ElementId::Name(format!("unit-{key}").into()))
                                .selected_index(unit_ix)
                                .child("分钟")
                                .child("小时")
                                .child("天")
                                .on_click(move |ix, _window, cx| {
                                    let ix = *ix;
                                    entity.update(cx, |p, cx| {
                                        let unit = match ix {
                                            1 => "hour",
                                            2 => "day",
                                            _ => "minute",
                                        };
                                        if is_backup {
                                            p.local.auto_backup.unit = unit.into();
                                        } else {
                                            p.local.auto_restore.unit = unit.into();
                                        }
                                        p.push_settings(cx);
                                        cx.notify();
                                    });
                                })
                        }),
                )
            })
    }

    // ── 数据 ──

    fn page_data(entity: &Entity<Self>) -> SettingPage {
        let data_entity = entity.clone();
        SettingPage::new("数据")
            .description("导出、导入或清空本地数据")
            .group(
                SettingGroup::new()
                    .title("备份")
                    .item(SettingItem::render(move |_, _window, cx| {
                        let entity = data_entity.clone();
                        div()
                            .v_flex()
                            .gap_3()
                            .child(
                                div()
                                    .flex()
                                    .gap_2()
                                    .child(
                                        Button::new("export-db")
                                            .outline()
                                            .small()
                                            .label("导出")
                                            .on_click({
                                                move |_, window, cx| {
                                                    let Some(app) = cx
                                                        .try_global::<DoitAppHandle>()
                                                        .map(|h| h.0.clone())
                                                    else {
                                                        return;
                                                    };
                                                    // Snapshot the current data up-front; the
                                                    // save dialog resolves asynchronously.
                                                    let snap = {
                                                        let app = app.read(&*cx);
                                                        SyncSnapshot {
                                                            version: 1,
                                                            exported_at: app.last_modified.clone(),
                                                            todos: app.todos.clone(),
                                                            settings: app.settings.clone(),
                                                        }
                                                    };
                                                    let json = match serde_json::to_string_pretty(
                                                        &snap,
                                                    ) {
                                                        Ok(json) => json,
                                                        Err(_) => {
                                                            window.push_notification(
                                                                Notification::error("序列化数据失败"),
                                                                &mut *cx,
                                                            );
                                                            return;
                                                        }
                                                    };
                                                    let dir = std::env::temp_dir();
                                                    let receiver = cx.prompt_for_new_path(
                                                        &dir,
                                                        Some("doit-backup.json"),
                                                    );
                                                    window.spawn(&*cx, async move |async_cx| {
                                                        let path = match receiver.await {
                                                            Ok(Ok(Some(path))) => path,
                                                            _ => return, // 用户取消
                                                        };
                                                        let result =
                                                            std::fs::write(&path, &json);
                                                        let _ = async_cx.update(|window, app_cx| {
                                                            let ok = result.is_ok();
                                                            window.push_notification(
                                                                if ok {
                                                                    Notification::success(
                                                                        format!(
                                                                            "已导出到 {}",
                                                                            path.display()
                                                                        ),
                                                                    )
                                                                } else {
                                                                    Notification::error("导出失败")
                                                                },
                                                                app_cx,
                                                            );
                                                        });
                                                    })
                                                    .detach();
                                                }
                                            }),
                                    )
                                    .child(
                                        Button::new("import-db")
                                            .outline()
                                            .small()
                                            .label("导入")
                                            .on_click({
                                                let entity = entity.clone();
                                                move |_, window, cx| {
                                                    let receiver = cx.prompt_for_paths(
                                                        PathPromptOptions {
                                                            files: true,
                                                            directories: false,
                                                            multiple: false,
                                                            prompt: Some(
                                                                "选择要导入的 JSON 备份文件".into(),
                                                            ),
                                                        },
                                                    );
                                                    let entity = entity.clone();
                                                    window.spawn(&*cx, async move |async_cx| {
                                                        let path = match receiver.await {
                                                            Ok(Ok(Some(paths))) => {
                                                                match paths.into_iter().next() {
                                                                    Some(path) => path,
                                                                    None => return,
                                                                }
                                                            }
                                                            _ => return, // 用户取消
                                                        };
                                                        let parsed = std::fs::read_to_string(&path)
                                                            .ok()
                                                            .and_then(|s| {
                                                                serde_json::from_str::<SyncSnapshot>(
                                                                    &s,
                                                                )
                                                                .ok()
                                                            });
                                                        let _ = async_cx.update(
                                                            |window, app_cx| {
                                                                let Some(snap) = parsed else {
                                                                    window.push_notification(
                                                                        Notification::error(
                                                                            "导入失败：不是有效的备份文件",
                                                                        ),
                                                                        app_cx,
                                                                    );
                                                                    return;
                                                                };
                                                                // Apply to the app first (todos +
                                                                // settings + theme), then refresh
                                                                // this panel's working copy.
                                                                if let Some(handle) = app_cx
                                                                    .try_global::<DoitAppHandle>()
                                                                {
                                                                    let app = handle.0.clone();
                                                                    let settings =
                                                                        snap.settings.clone();
                                                                    app.update(
                                                                        app_cx,
                                                                        |app, cx| {
                                                                            app.apply_snapshot(
                                                                                snap, window, cx,
                                                                            );
                                                                        },
                                                                    );
                                                                    entity.update(
                                                                        app_cx,
                                                                        |p, cx| {
                                                                            p.reload_after_import(
                                                                                settings,
                                                                                window,
                                                                                cx,
                                                                            );
                                                                        },
                                                                    );
                                                                }
                                                                window.push_notification(
                                                                    Notification::success(
                                                                        "已导入数据",
                                                                    ),
                                                                    app_cx,
                                                                );
                                                            },
                                                        );
                                                    })
                                                    .detach();
                                                }
                                            }),
                                    )
                                    .child(
                                        Button::new("clear-data")
                                            .danger()
                                            .small()
                                            .outline()
                                            .label("清空")
                                            .on_click({
                                                let entity = entity.clone();
                                                move |_, window, cx| {
                                                    let entity = entity.clone();
                                                    window.open_alert_dialog(
                                                        cx,
                                                        move |alert, _window, _cx| {
                                                            let entity = entity.clone();
                                                            alert
                                                                .title("清空所有数据？")
                                                                .description("将删除所有待办、分类和标签，此操作不可恢复。")
                                                                .button_props(
                                                                    DialogButtonProps::default()
                                                                        .ok_text("清空")
                                                                        .ok_variant(ButtonVariant::Danger)
                                                                        .on_ok(
                                                                            move |_, _window, cx| {
                                                                                if let Some(handle) =
                                                                                    cx.try_global::<DoitAppHandle>()
                                                                                {
                                                                                    let app = handle.0.clone();
                                                                                    app.update(cx, |app, cx| {
                                                                                        app.clear_all(cx);
                                                                                    });
                                                                                }
                                                                                entity.update(cx, |p, cx| {
                                                                                    p.local =
                                                                                        AppSettings::default();
                                                                                    p.recording = false;
                                                                                    cx.notify();
                                                                                });
                                                                                true
                                                                            },
                                                                        ),
                                                                )
                                                        },
                                                    );
                                                }
                                            }),
                                    ),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(cx.theme().muted_foreground)
                                    .child("清空将删除所有待办、分类和标签；主题与快捷键等设置会保留。"),
                            )
                    })),
            )
    }

    // ── 关于 ──

    fn page_about(entity: &Entity<Self>) -> SettingPage {
        let about_entity = entity.clone();
        SettingPage::new("关于")
            .description("版本信息与更新检查")
            .group(
                SettingGroup::new()
                    .title("Doit")
                    .item(SettingItem::render(move |_, _window, cx| {
                        let update_state = about_entity.read(cx).update_state.clone();
                        let downloading = cx
                            .try_global::<DoitAppHandle>()
                            .map(|h| h.0.read(cx).update_download.is_some())
                            .unwrap_or(false);
                        let entity = about_entity.clone();
                        div()
                            .v_flex()
                            .gap_4()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .child(
                                        div()
                                            .text_sm()
                                            .text_color(cx.theme().muted_foreground)
                                            .child("当前版本"),
                                    )
                                    .child(
                                        div()
                                            .text_sm()
                                            .child(format!(
                                                "v{}",
                                                env!("CARGO_PKG_VERSION")
                                            )),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_3()
                                    .child(
                                        Button::new("check-update")
                                            .small()
                                            .outline()
                                            .label(match &update_state {
                                                TransferState::Busy => {
                                                    "检查中…".to_string()
                                                }
                                                _ if downloading => "下载中…".to_string(),
                                                _ => "检查更新".to_string(),
                                            })
                                            .disabled(
                                                matches!(update_state, TransferState::Busy)
                                                    || downloading,
                                            )
                                            .on_click({
                                                let entity = entity.clone();
                                                move |_, window, cx| {
                                                    // A fresh proxy-aware client: the update check must
                                                    // honour the system proxy to reach api.github.com,
                                                    // unlike the `no_proxy` WebDAV sync client.
                                                    run_update_check(
                                                        entity.clone(),
                                                        crate::http::new_public_http_client(),
                                                        window,
                                                        cx,
                                                    )
                                                }
                                            }),
                                    )
                                    .child(update_state_text(&update_state, cx)),
                            )
                            .when_some(
                                cx.try_global::<DoitAppHandle>()
                                    .and_then(|h| h.0.read(cx).update_download.clone()),
                                |el, dl| {
                                    el.child(
                                        div()
                                            .v_flex()
                                            .gap_1()
                                            .w_full()
                                            .child(
                                                div()
                                                    .flex()
                                                    .items_center()
                                                    .justify_between()
                                                    .text_xs()
                                                    .text_color(cx.theme().muted_foreground)
                                                    .child(format!("正在下载 {}", dl.file))
                                                    .child(format!("{:.0}%", dl.frac * 100.0)),
                                            )
                                            .child(
                                                Progress::new("update-download-progress")
                                                    .value(dl.frac * 100.0),
                                            ),
                                    )
                                },
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(cx.theme().muted_foreground)
                                    .child("用 GPUI Kit 纯 Rust 重写的 Doit 桌面待办应用。"),
                            )
                    })),
            )
    }
}

// ── Check for updates (GitHub releases) ────────────────────────────────────

/// GitHub latest-release JSON fields we consume.
const RELEASES_API: &str = "https://api.github.com/repos/nabaonan/doit-gpui/releases/latest";

fn run_update_check(
    entity: Entity<SettingsPanel>,
    client: Arc<dyn HttpClient>,
    window: &mut Window,
    cx: &mut App,
) {
    entity.update(cx, |p, cx| {
        p.update_state = TransferState::Busy;
        cx.notify();
    });
    let current = env!("CARGO_PKG_VERSION").to_string();
    window
        .spawn(&*cx, async move |async_cx| {
            let outcome = check_latest_release(client.as_ref()).await;
            let _ = async_cx.update(move |window, app_cx| {
                let current = current.clone();
                let state = match outcome {
                    Ok(rel) => {
                        let tag_v = rel.tag.trim_start_matches(['v', 'V']);
                        if cmp_version(tag_v, &current) == Ordering::Greater {
                            let arch = current_asset_key();
                            let tag = rel.tag.clone();
                            let title = format!("发现新版本 {tag}");
                            let meta = format!(
                                "当前版本 v{current}（{arch}）  →  新版本 {tag}"
                            );
                            let asset_name = rel.asset_name.clone();
                            let asset_line = asset_name
                                .as_ref()
                                .map(|name| format!("安装包：{name}"))
                                .unwrap_or_else(|| {
                                    "当前平台暂未提供安装包，无法在应用内下载。".to_string()
                                });
                            let notes = rel.notes.trim().to_string();
                            let notes_id = format!("release-notes-{tag}");
                            let download_url = rel.asset_url.clone();
                            let entity_for_dl = entity.clone();
                            window.open_dialog(app_cx, move |dialog, _window, cx| {
                                let notes = if notes.is_empty() {
                                    "暂无更新说明。".to_string()
                                } else {
                                    notes.clone()
                                };
                                let mut dialog = dialog
                                    .title(title.clone())
                                    .w(px(600.))
                                    .child(
                                        div()
                                            .v_flex()
                                            .gap_3()
                                            .child(
                                                div()
                                                    .text_sm()
                                                    .text_color(cx.theme().muted_foreground)
                                                    .child(meta.clone()),
                                            )
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .text_color(cx.theme().muted_foreground)
                                                    .child(asset_line.clone()),
                                            )
                                            // Release notes are Markdown on GitHub; render
                                            // them instead of showing the raw source.
                                            .child(TextView::markdown(
                                                notes_id.clone(),
                                                notes,
                                            )),
                                    );
                                let can_download =
                                    download_url.is_some() && asset_name.is_some();
                                let mut footer = div()
                                    .flex()
                                    .w_full()
                                    .items_center()
                                    .justify_end()
                                    .gap_2()
                                    .child(
                                        Button::new("update-later")
                                            .outline()
                                            .label("暂不")
                                            .on_click(|_, window, cx| {
                                                window.close_dialog(cx);
                                            }),
                                    );
                                if can_download {
                                    let url = download_url.clone().unwrap_or_default();
                                    let file = asset_name.clone().unwrap_or_default();
                                    let version = tag.clone();
                                    let entity = entity_for_dl.clone();
                                    footer = footer.child(
                                        Button::new("update-download-now")
                                            .primary()
                                            .label("立即下载")
                                            .on_click(move |_, window, cx| {
                                                window.close_dialog(cx);
                                                start_update_download(
                                                    entity.clone(),
                                                    &url,
                                                    &file,
                                                    &version,
                                                    window,
                                                    cx,
                                                );
                                            }),
                                    );
                                } else {
                                    footer = footer.child(
                                        Button::new("update-ok")
                                            .primary()
                                            .label("知道了")
                                            .on_click(|_, window, cx| {
                                                window.close_dialog(cx);
                                            }),
                                    );
                                }
                                dialog = dialog.footer(footer);
                                dialog
                            });
                            TransferState::Ok(format!("发现新版本 {}", rel.tag))
                        } else {
                            TransferState::Ok(format!("已是最新版本 v{current}"))
                        }
                    }
                    Err(e) => TransferState::Err(format!("检查更新失败：{e}")),
                };
                entity.update(app_cx, |p, cx| {
                    p.update_state = state;
                    cx.notify();
                });
            });
        })
        .detach();
}

/// A release that may offer an install package for the current platform.
#[derive(Debug)]
struct LatestRelease {
    tag: String,
    notes: String,
    asset_name: Option<String>,
    asset_url: Option<String>,
}

/// Message from the background download thread to the UI loop.
enum DownloadMsg {
    Progress(f32),
    Finished(Result<std::path::PathBuf, String>),
}

/// Download a release asset inside the app (no browser hop) and keep it running
/// in the background: progress is reflected in the title bar and the About
/// page, and the finished package is only revealed to the user — never opened —
/// so it can be installed after Doit quits.
///
/// The download itself runs on a dedicated Tokio runtime in a background thread
/// (reqwest can't run on gpui's executor); progress is forwarded over a
/// mpsc channel back to the UI loop, which updates the shared state and
/// re-renders.
fn start_update_download(
    entity: Entity<SettingsPanel>,
    url: &str,
    file_name: &str,
    version: &str,
    window: &mut Window,
    cx: &mut App,
) {
    // One download at a time; ignore duplicate clicks.
    if cx
        .try_global::<DoitAppHandle>()
        .map(|h| h.0.read(cx).update_download.is_some())
        .unwrap_or(false)
    {
        return;
    }
    set_update_download(
        cx,
        Some(UpdateDownloadState {
            frac: 0.0,
            file: file_name.to_string(),
            version: version.to_string(),
        }),
    );
    entity.update(cx, |_, cx| cx.notify());

    let url = url.to_string();
    let file_name = file_name.to_string();
    let version = version.to_string();
    let dest = download_dest(&file_name);
    let part = part_path(&dest);
    let (tx, mut rx) = futures::channel::mpsc::unbounded::<DownloadMsg>();
    std::thread::spawn(move || {
        let result = match tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
        {
            Ok(rt) => rt.block_on(download_to_file(&url, &part, |frac| {
                let _ = tx.unbounded_send(DownloadMsg::Progress(frac));
            })),
            Err(e) => Err(format!("初始化下载线程失败：{e}")),
        };
        // Only a fully written file gets the final name.
        let result = result.and_then(|_| {
            std::fs::rename(&part, &dest)
                .map(|_| dest.clone())
                .map_err(|e| format!("保存安装包失败：{e}"))
        });
        if result.is_err() {
            let _ = std::fs::remove_file(&part);
        }
        let _ = tx.unbounded_send(DownloadMsg::Finished(result));
    });

    window.spawn(&*cx, async move |async_cx| {
        while let Some(msg) = futures::StreamExt::next(&mut rx).await {
            match msg {
                DownloadMsg::Progress(frac) => {
                    let _ = async_cx.update(|_window, app_cx| {
                        update_download_progress(app_cx, frac);
                        entity.update(app_cx, |_, cx| cx.notify());
                    });
                }
                DownloadMsg::Finished(result) => {
                    let _ = async_cx.update(move |window, app_cx| {
                        set_update_download(app_cx, None);
                        match result {
                            Ok(path) => {
                                // Open the downloaded package right away so the
                                // user does not have to find it in Finder. The
                                // installer itself is still only run after Doit
                                // quits.
                                let opened = open_downloaded_package(&path);
                                entity.update(app_cx, |p, cx| {
                                    p.update_state = TransferState::Ok(if opened {
                                        format!("{version} 已下载完成，安装包已打开。")
                                    } else {
                                        format!("{version} 已下载完成。")
                                    });
                                    cx.notify();
                                });
                                show_download_complete_dialog(
                                    window, app_cx, &path, opened,
                                );
                            }
                            Err(err) => {
                                entity.update(app_cx, |p, cx| {
                                    p.update_state =
                                        TransferState::Err(format!("下载安装包失败：{err}"));
                                    cx.notify();
                                });
                                let message = format!("{err}\n\n请检查网络后重试。");
                                window.open_alert_dialog(app_cx, move |alert, _, _| {
                                    alert
                                        .title("下载安装包失败")
                                        .description(message.clone())
                                        .ok_text("知道了")
                                });
                            }
                        }
                    });
                    break;
                }
            }
        }
    })
    .detach();
}

/// Store (or clear) the shared download state shown by the title bar.
fn set_update_download(cx: &mut App, value: Option<UpdateDownloadState>) {
    if let Some(handle) = cx.try_global::<DoitAppHandle>() {
        let app = handle.0.clone();
        app.update(cx, |app, cx| {
            app.update_download = value;
            cx.notify();
        });
    }
}

fn update_download_progress(cx: &mut App, frac: f32) {
    if let Some(handle) = cx.try_global::<DoitAppHandle>() {
        let app = handle.0.clone();
        app.update(cx, |app, cx| {
            if let Some(dl) = app.update_download.as_mut() {
                dl.frac = frac;
            }
            cx.notify();
        });
    }
}

/// Where the installer is saved. Prefer `~/Downloads` (easy to find and
/// survives a reboot); fall back to the app's own data dir when the system
/// denies access (macOS TCC can block Downloads for unsigned apps), and to the
/// temp dir as a last resort.
fn download_dest(file_name: &str) -> std::path::PathBuf {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(std::path::PathBuf::from);
    let mut candidates: Vec<std::path::PathBuf> = Vec::new();
    if let Some(home) = home.as_ref() {
        candidates.push(home.join("Downloads"));
        candidates.push(home.join(".doit-gpui").join("updates"));
    }
    candidates.push(std::env::temp_dir().join("doit-updates"));

    for base in candidates {
        if !dir_writable(&base) {
            continue;
        }
        let dest = base.join(file_name);
        // A stale package with the same name must not poison this download.
        let _ = std::fs::remove_file(&dest);
        return dest;
    }
    std::env::temp_dir().join(file_name)
}

/// True when `dir` exists (or can be created) and a probe file can be written
/// there. Used to skip locations the OS refuses without failing the download.
fn dir_writable(dir: &std::path::Path) -> bool {
    if std::fs::create_dir_all(dir).is_err() {
        return false;
    }
    let probe = dir.join(".doit-download-probe");
    match std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&probe)
    {
        Ok(_) => {
            let _ = std::fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

/// `foo.dmg` downloads into `foo.dmg.part` and is renamed once complete.
fn part_path(dest: &std::path::Path) -> std::path::PathBuf {
    let mut part = dest.as_os_str().to_owned();
    part.push(".part");
    std::path::PathBuf::from(part)
}

/// Completion prompt: the package is already open; install it after quitting.
fn show_download_complete_dialog(
    window: &mut Window,
    cx: &mut App,
    path: &std::path::Path,
    opened: bool,
) {
    let file = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let display = path.display().to_string();
    let path = path.to_path_buf();
    let reveal = if cfg!(target_os = "macos") {
        "在访达中显示"
    } else {
        "打开所在文件夹"
    };
    let install_hint = if cfg!(target_os = "macos") {
        "请退出 Doit 后，把安装包里的 Doit 拖入「应用程序」完成安装。"
    } else {
        "请退出 Doit 后，解压安装包并替换旧版本。"
    };
    let open_hint = if opened {
        "安装包已自动打开。"
    } else {
        "安装包已保存，但自动打开失败，请手动打开。"
    };
    window.open_alert_dialog(cx, move |alert, _w, _c| {
        let mut alert = alert
            .title("更新下载完成")
            .description(
                div()
                    .v_flex()
                    .gap_2()
                    .child(format!("{file} 已保存到："))
                    .child(display.clone())
                    .child(open_hint)
                    .child(install_hint),
            )
            .ok_text("知道了")
            .on_ok(|_, _, _| true);
        if !opened {
            alert = alert
                .cancel_text(reveal)
                .show_cancel(true)
                .on_cancel({
                    let path = path.clone();
                    move |_, _, _| {
                        reveal_in_file_manager(&path);
                        true
                    }
                });
        }
        alert
    });
}

/// Open the downloaded package with the platform's default handler (on macOS
/// this mounts the DMG and shows the install window). The child is reaped on a
/// helper thread so the UI loop never blocks on the launcher.
fn open_downloaded_package(path: &std::path::Path) -> bool {
    #[cfg(target_os = "macos")]
    let command = {
        let mut command = std::process::Command::new("open");
        command.arg(path);
        command
    };
    #[cfg(target_os = "windows")]
    let command = {
        let mut command = std::process::Command::new("cmd");
        command.args(["/C", "start", ""]).arg(path);
        command
    };
    #[cfg(all(unix, not(target_os = "macos")))]
    let command = {
        let mut command = std::process::Command::new("xdg-open");
        command.arg(path);
        command
    };

    spawn_detached(command)
}

/// Spawn a launcher and reap it on a helper thread so the UI loop never blocks.
fn spawn_detached(mut command: std::process::Command) -> bool {
    match command.spawn() {
        Ok(mut child) => {
            std::thread::spawn(move || {
                let _ = child.wait();
            });
            true
        }
        Err(_) => false,
    }
}

/// Show the downloaded package in the platform file manager.
fn reveal_in_file_manager(path: &std::path::Path) {
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg("-R").arg(path).spawn();
    }
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("explorer")
            .arg(format!("/select,\"{}\"", path.display()))
            .spawn();
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let dir = path.parent().unwrap_or(path);
        let _ = std::process::Command::new("xdg-open").arg(dir).spawn();
    }
}

/// Stream `url` into `dest`, invoking `on_progress` with the 0..1 fraction as
/// bytes arrive (using Content-Length when the server provides it).
/// Runs on a Tokio runtime; uses the vendored reqwest client directly to get
/// an incremental byte stream (the shared [`HttpClient`] buffers full bodies).
async fn download_to_file(
    url: &str,
    dest: &std::path::Path,
    on_progress: impl Fn(f32),
) -> Result<std::path::PathBuf, String> {
    let client = gpui_pre_reqwest::Client::builder()
        .use_rustls_tls()
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .map_err(|e| format!("初始化下载客户端失败：{e}"))?;
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("发起下载失败：{e}"))?;
    if !resp.status().is_success() {
        return Err(format!("下载请求失败（HTTP {}）", resp.status().as_u16()));
    }
    let total = resp.content_length().unwrap_or(0);
    use futures::StreamExt;
    let mut stream = resp.bytes_stream();
    let mut file =
        std::fs::File::create(dest).map_err(|e| format!("创建临时文件失败：{e}"))?;
    let mut got: u64 = 0;
    let mut last_report: u64 = 0;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("读取下载内容失败：{e}"))?;
        std::io::Write::write_all(&mut file, &chunk).map_err(|e| format!("写入文件失败：{e}"))?;
        got += chunk.len() as u64;
        // Cap UI updates to ~every 256 KB to avoid flooding notify.
        if total == 0 || got - last_report >= 256 * 1024 {
            on_progress(if total > 0 {
                (got as f32 / total as f32).min(1.0)
            } else {
                0.0
            });
            last_report = got;
        }
    }
    if total > 0 {
        on_progress(1.0);
    }
    Ok(dest.to_path_buf())
}


/// Which `{os}-{arch}` asset key this build should download, e.g. `macos-arm64`.
/// Mirrors the release file naming: `doit-gpui-<ver>-<os>-<arch>.<ext>`.
fn current_asset_key() -> String {
    let os = match std::env::consts::OS {
        "macos" => "macos",
        "windows" => "windows",
        "linux" => "linux",
        other => other,
    };
    let arch = match std::env::consts::ARCH {
        "aarch64" | "arm64" => "arm64",
        "x86_64" | "x64" => "x64",
        other => other,
    };
    format!("{os}-{arch}")
}

/// Fetch the latest release from GitHub and pick the asset for this platform.
async fn check_latest_release(client: &dyn HttpClient) -> Result<LatestRelease, String> {
    let req = Request::builder()
        .method(Method::GET)
        .uri(RELEASES_API)
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", "doit-gpui")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .body(AsyncBody::empty())
        .map_err(|e| e.to_string())?;
    let resp = client
        .send(req)
        .await
        .map_err(|e| format!("网络请求失败：{}", webdav::classify_transport(e.as_ref())))?;
    let status = resp.status();
    if status == StatusCode::TOO_MANY_REQUESTS {
        return Err("GitHub API 请求过于频繁，请稍后再试。".into());
    }
    if !status.is_success() {
        return Err(format!("检查更新失败（HTTP {}）", status.as_u16()));
    }
    let mut body = resp.into_body();
    let mut bytes = Vec::new();
    body.read_to_end(&mut bytes)
        .await
        .map_err(|e| format!("读取更新信息失败：{e}"))?;
    let v: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|e| format!("解析更新信息失败：{e}"))?;
    let tag = v.get("tag_name").and_then(|x| x.as_str()).unwrap_or("").to_string();
    let notes = v.get("body").and_then(|x| x.as_str()).unwrap_or("").to_string();

    // The release ships per-arch packages (`-macos-arm64.dmg`, `-macos-x64.dmg`,
    // `-linux-x64.tar.gz`, `-windows-x64.zip`); pick the one matching this build.
    let key = format!("-{}", current_asset_key());
    let mut asset_name = None;
    let mut asset_url = None;
    if let Some(assets) = v.get("assets").and_then(|a| a.as_array()) {
        for a in assets {
            let Some(name) = a.get("name").and_then(|n| n.as_str()) else {
                continue;
            };
            if name.contains(&key) {
                asset_name = Some(name.to_string());
                asset_url =
                    a.get("browser_download_url").and_then(|u| u.as_str()).map(|u| u.to_string());
                break;
            }
        }
    }
    if tag.is_empty() {
        return Err("未能从仓库获取到有效的版本信息。".into());
    }
    Ok(LatestRelease { tag, notes, asset_name, asset_url })
}

/// Compare dotted-numeric versions ("0.1.7" vs "0.1.10"); non-numeric segments count as 0.
fn cmp_version(a: &str, b: &str) -> Ordering {
    let pa: Vec<u64> = a.split('.').filter_map(|s| s.parse().ok()).collect();
    let pb: Vec<u64> = b.split('.').filter_map(|s| s.parse().ok()).collect();
    for i in 0..pa.len().max(pb.len()) {
        match pa.get(i).copied().unwrap_or(0).cmp(&pb.get(i).copied().unwrap_or(0)) {
            Ordering::Equal => {}
            o => return o,
        }
    }
    Ordering::Equal
}

// ── Small composition helpers ───────────────────────────────────────────────

fn row_block(title: &str, hint: &str, cx: &App) -> gpui_kit::Div {
    let title = title.to_string();
    let hint = hint.to_string();
    div()
        .flex()
        .items_center()
        .justify_between()
        .child(
            div()
                .v_flex()
                .gap_1()
                .child(div().text_sm().child(title))
                .when(!hint.is_empty(), |el| {
                    el.child(div().text_xs().text_color(cx.theme().muted_foreground).child(hint))
                }),
        )
}

fn field_block(label: &str, cx: &App) -> gpui_kit::Div {
    let label = label.to_string();
    div()
        .v_flex()
        .gap_1()
        .child(
            div()
                .text_xs()
                .font_semibold()
                .text_color(cx.theme().muted_foreground)
                .child(label),
        )
}

/// Whether the latest sync error is caused by an untrusted server certificate,
/// so we can offer a one-click "trust and retry".
fn is_cert_error(state: &TransferState) -> bool {
    matches!(state, TransferState::Err(msg)
        if msg.contains("证书") || msg.contains("UnknownIssuer") || msg.contains("invalid peer"))
}

/// The live status line under the sync test button.
fn sync_state_text(state: &TransferState, cx: &App) -> impl IntoElement {    match state {
        TransferState::Idle => div()
            .text_xs()
            .text_color(cx.theme().muted_foreground)
            .child("填写后点击「测试连接」验证地址与凭据。")
            .into_any_element(),
        TransferState::Busy => div()
            .text_xs()
            .text_color(cx.theme().muted_foreground)
            .child("正在请求…")
            .into_any_element(),
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

/// The live status line next to the update-check button.
fn update_state_text(state: &TransferState, cx: &App) -> impl IntoElement {
    match state {
        TransferState::Idle => div().into_any_element(),
        TransferState::Busy => div()
            .text_xs()
            .text_color(cx.theme().muted_foreground)
            .child("正在请求 GitHub 最新版本…")
            .into_any_element(),
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

/// Build the `Keystroke` a `ShortcutConfig` describes, so it can be shown as
/// a native `Kbd` chip (or parsed back into an action binding).
fn shortcut_keystroke(sc: &ShortcutConfig) -> Keystroke {    Keystroke {
        modifiers: Modifiers {
            control: sc.ctrl,
            alt: sc.alt,
            shift: sc.shift,
            platform: sc.meta,
            function: false,
        },
        key: sc.key.to_string(),
        key_char: None,
    }
}

#[cfg(test)]
mod tests {
    use super::cmp_version;
    use std::cmp::Ordering;


    #[test]
    fn version_compare_is_numeric_not_lexical() {
        assert_eq!(cmp_version("0.1.10", "0.1.9"), Ordering::Greater);
        assert_eq!(cmp_version("0.1.9", "0.1.10"), Ordering::Less);
        assert_eq!(cmp_version("0.1.7", "0.1.7"), Ordering::Equal);
        assert_eq!(cmp_version("0.10.0", "0.9.9"), Ordering::Greater);
        // missing trailing segments count as 0
        assert_eq!(cmp_version("0.1.7", "0.1"), Ordering::Greater);
    }
}
