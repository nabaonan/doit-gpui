use gpui_kit::base::Disableable as _;
use gpui_kit::base::Selectable as _;
use gpui_kit::base::StyledExt as _;
use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::button::ButtonVariant;
use gpui_kit::component::dialog::DialogButtonProps;
use gpui_kit::component::{
    ActiveTheme, Sizable, Theme, ThemeMode, WindowExt,
    button::Button,
    input::{Input, InputState, InputEvent},
    kbd::Kbd,
    notification::Notification,
    radio::RadioGroup,
    slider::{Slider, SliderEvent, SliderState, SliderValue},
    switch::Switch,
};
use gpui_kit::prelude::*;
use std::sync::Arc;
use gpui_kit::gpui::{
    div, App, Context, Entity, FocusHandle,
    IntoElement, KeyDownEvent, Keystroke, Modifiers, ParentElement, PathPromptOptions, Render,
    SharedString, Styled, Subscription, Window, px, ElementId,
};
use crate::app::DoitAppHandle;
use crate::label_dialogs::{CategoriesPanel, TagsPanel};
use crate::types::*;
use crate::webdav::{self, TransferState};

// ── Section navigation ──────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq)]
enum Section {
    Appearance,
    Interaction,
    Sync,
    Labels,
    Data,
    About,
}

impl Section {
    fn all() -> [Section; 6] {
        [
            Section::Appearance,
            Section::Interaction,
            Section::Sync,
            Section::Labels,
            Section::Data,
            Section::About,
        ]
    }

    fn label(&self) -> &'static str {
        match self {
            Self::Appearance => "外观",
            Self::Interaction => "交互",
            Self::Sync => "同步",
            Self::Labels => "分类与标签",
            Self::Data => "数据",
            Self::About => "关于",
        }
    }

    fn index(&self) -> u32 {
        match self {
            Self::Appearance => 0,
            Self::Interaction => 1,
            Self::Sync => 2,
            Self::Labels => 3,
            Self::Data => 4,
            Self::About => 5,
        }
    }
}

// ── Settings Panel ─────────────────────────────────────────────────────────

pub struct SettingsPanel {
    local: AppSettings,
    active: Section,

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
                this.local.cloud_sync.webdav_password = st.read(cx).value();
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
            active: Section::Appearance,
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
            categories_panel,
            tags_panel,
            _subs: subs,
        }
    }

    /// Copy fresh settings into the panel and reset the text/slider controls.
    pub(crate) fn sync_from(&mut self, settings: AppSettings, window: &mut Window, cx: &mut Context<Self>) {
        self.local = settings.clone();
        self.active = Section::Appearance;
        self.recording = false;
        self.sync_test = TransferState::Idle;

        let mut set = |st: &Entity<InputState>, value: SharedString| {
            st.update(cx, |s, cx| s.set_value(value, window, cx));
        };
        set(&self.url, settings.cloud_sync.webdav_url.clone());
        set(&self.user, settings.cloud_sync.webdav_username.clone());
        set(&self.pass, settings.cloud_sync.webdav_password.clone());
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
    fn push_settings(&mut self, cx: &mut Context<Self>) {
        let app = cx
            .try_global::<DoitAppHandle>()
            .map(|h| h.0.clone());
        if let Some(app) = app {
            let settings = self.local.clone();
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
        Theme::change(mode, Some(window), cx);
        self.push_settings(cx);
    }
}

// ── Render ──────────────────────────────────────────────────────────────────

impl Render for SettingsPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.render_root(window, cx)
    }
}

impl SettingsPanel {
    fn render_root(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .size_full()
            .min_w(px(680.))
            .min_h(px(460.))
            .child(self.render_nav(cx))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .p_4()
                    .child(self.render_active(window, cx).into_any_element()),
            )
    }

    fn render_nav(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .v_flex()
            .gap_1()
            .w(px(116.))
            .p_2()
            .border_r_1()
            .border_color(cx.theme().border)
            .children(Section::all().iter().map(|sec| {
                let sec = *sec;
                Button::new(("section", sec.index()))
                    .when(self.active == sec, |b| b.selected(true))
                    .ghost()
                    .small()
                    .label(sec.label())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.active = sec;
                        cx.notify();
                    }))
            }))
    }

    fn render_active(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        match self.active {
            Section::Appearance => self.render_appearance(cx).into_any_element(),
            Section::Interaction => self.render_interaction(window, cx).into_any_element(),
            Section::Sync => self.render_sync(window, cx).into_any_element(),
            Section::Labels => self.render_labels(window, cx).into_any_element(),
            Section::Data => self.render_data(window, cx).into_any_element(),
            Section::About => self.render_about(cx).into_any_element(),
        }
    }

    // ── 分类与标签 ──

    fn render_labels(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div()
            .v_flex()
            .gap_4()
            .child(
                div()
                    .v_flex()
                    .gap_1()
                    .child(div().text_sm().font_bold().child("分类"))
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("点「编辑」修改名称与颜色，勾选默认分类。"),
                    ),
            )
            .child(self.categories_panel.clone())
            .child(div().border_t_1().border_color(cx.theme().border))
            .child(
                div()
                    .v_flex()
                    .gap_1()
                    .child(div().text_sm().font_bold().child("标签"))
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("点「编辑」修改名称与颜色。"),
                    ),
            )
            .child(self.tags_panel.clone())
    }

    // ── 外观 ──

    fn render_appearance(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme_ix = match self.local.theme.as_ref() {
            "light" => Some(1),
            "dark" => Some(2),
            _ => Some(0),
        };

        div()
            .v_flex()
            .gap_6()
            .child(
                section_block("主题", cx).child(
                    RadioGroup::horizontal("theme-radios")
                        .selected_index(theme_ix)
                        .child("系统")
                        .child("浅色")
                        .child("深色")
                        .on_click({
                            let entity = cx.entity();
                            move |ix, window, cx| {
                                entity.update(cx, |p, cx| {
                                    p.local.theme = match *ix {
                                        1 => "light".into(),
                                        2 => "dark".into(),
                                        _ => "system".into(),
                                    };
                                    p.apply_theme(window, cx);
                                    cx.notify();
                                });
                            }
                        }),
                ),
            )
    }

    // ── 交互 ──

    fn render_interaction(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let mode_ix = if self.local.completion_mode.as_ref() == "longpress" {
            Some(1)
        } else {
            Some(0)
        };
        let is_longpress = self.local.completion_mode.as_ref() == "longpress";
        let is_recording = self.recording;
        let shortcut = self.local.add_todo_shortcut.clone();

        let shortcut_row = div()
            .id("shortcut-recorder")
            .track_focus(&self.recording_handle)
            .on_key_down({
                let entity = cx.entity();
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
                        "control" | "shift" | "alt" | "meta" | "super" | "command"
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
                                let entity = cx.entity();
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
            );

        div()
            .v_flex()
            .gap_6()
            .child(
                section_block("完成方式", cx)
                    .child(
                        RadioGroup::horizontal("completion-radios")
                            .selected_index(mode_ix)
                            .child("勾选")
                            .child("长按")
                            .on_click({
                                let entity = cx.entity();
                                move |ix, _window, cx| {
                                    entity.update(cx, |p, cx| {
                                        p.local.completion_mode = if *ix == 1 {
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
                        el.child(
                            div()
                                .flex()
                                .items_center()
                                .gap_3()
                                .child(
                                    div()
                                        .w(px(240.))
                                        .child(Slider::new(&self.long_press)),
                                )
                                .child(
                                    div()
                                        .text_sm()
                                        .child(format!("{} 秒", self.local.long_press_duration)),
                                ),
                        )
                    }),
            )
            .child(section_block("添加快捷键", cx).child(shortcut_row))
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("该快捷键用于在输入框中确认新增（待办、分类、标签），默认直接按 Enter 即可。"),
            )
    }

    // ── 同步 ──

    fn render_sync(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let sync_enabled = self.local.cloud_sync.enabled;
        let cloud = self.local.cloud_sync.clone();
        let entity = cx.entity();

        div()
            .v_flex()
            .gap_6()
            .child(
                row_block("启用云同步", "WebDAV 同步本地数据", cx).child(
                    Switch::new("cloud-enabled")
                        .small()
                        .checked(cloud.enabled)
                        .on_click({
                            let entity = entity.clone();
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
                el.child(
                    div()
                        .v_flex()
                        .gap_3()
                        .child(
                            field_block("WebDAV 地址", cx).child(Input::new(&self.url).small()),
                        )
                        .child(field_block("用户名", cx).child(Input::new(&self.user).small()))
                        .child(field_block("密码", cx).child(Input::new(&self.pass).small()))
                        .child(
                            row_block(
                                "信任自签名证书",
                                "跳过证书校验（用于自签名 / 内网 NAS）",
                                cx,
                            )
                            .child(
                                Switch::new("trust-self-signed")
                                    .small()
                                    .checked(self.local.cloud_sync.trust_self_signed)
                                    .on_click({
                                        let entity = cx.entity();
                                        move |checked, _window, cx| {
                                            entity.update(cx, |p, cx| {
                                                p.local.cloud_sync.trust_self_signed = *checked;
                                                crate::http::set_trust_self_signed(*checked);
                                                p.push_settings(cx);
                                                cx.notify();
                                            });
                                        }
                                    }),
                            ),
                        )
                        .child(self.render_sync_test(&entity, cx)),
                )
            })
            .child(
                row_block("启动时拉取", "应用启动时从 WebDAV 拉取最新数据", cx).child(
                    Switch::new("fetch-on-startup")
                        .small()
                        .disabled(!sync_enabled)
                        .checked(cloud.fetch_on_startup)
                        .on_click({
                            let entity = entity.clone();
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
                row_block("关闭时上传", "关闭窗口时自动把本地数据上传到 WebDAV", cx).child(
                    Switch::new("upload-on-exit")
                        .small()
                        .disabled(!sync_enabled)
                        .checked(cloud.upload_on_exit)
                        .on_click({
                            let entity = entity.clone();
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
            .child(
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
                            .child(Input::new(&self.keep_recent).small().disabled(!sync_enabled)),
                    ),
            )
            .child(div().border_t_1().border_color(cx.theme().border))
            .child(self.render_schedule("定时备份", &self.local.auto_backup, cx))
            .child(self.render_schedule("定时恢复", &self.local.auto_restore, cx))
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().warning)
                    .child("提示：自动恢复会覆盖本地数据，请谨慎启用。"),
            )
    }

    /// A 「测试连接」button plus a live result line, using the typed config.
    fn render_sync_test(&self, entity: &Entity<Self>, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.sync_test.clone();
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
                            let entity = entity.clone();
                            let client = client.clone();
                            Self::run_sync_test(entity, client, window, cx);
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
                            let entity = entity.clone();
                            let client = client.clone();
                            Self::run_sync_test(entity, client, window, cx);
                        }),
                )
            })
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
                p.local.cloud_sync.webdav_password.to_string(),
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

    fn render_schedule(
        &self,
        title: &'static str,
        schedule: &ScheduleConfig,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let enabled = schedule.enabled;
        let unit_ix = match schedule.unit.as_ref() {
            "hour" => Some(1),
            "day" => Some(2),
            _ => Some(0),
        };
        let reachable = self.local.cloud_sync.enabled;
        let is_backup = title == "定时备份";
        let interval_field: Entity<InputState> =
            if is_backup { self.backup_interval.clone() } else { self.restore_interval.clone() };
        let key = if is_backup { "backup" } else { "restore" };
        let entity = cx.entity();
        let entity2 = entity.clone();

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
                                .child(Input::new(&interval_field).small().disabled(!reachable)),
                        )
                        .child(
                            RadioGroup::horizontal(ElementId::Name(format!("unit-{key}").into()))
                                .selected_index(unit_ix)
                                .child("分钟")
                                .child("小时")
                                .child("天")
                                .on_click({
                                    let entity = entity2.clone();
                                    move |ix, _window, cx| {
                                        entity.update(cx, |p, cx| {
                                            let unit = match *ix {
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
                                    }
                                }),
                        ),
                )
            })
    }

    // ── 数据 ──

    fn render_data(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .v_flex()
            .gap_6()
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
                                    let json = match serde_json::to_string_pretty(&snap) {
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
                                    let receiver =
                                        cx.prompt_for_new_path(&dir, Some("doit-backup.json"));
                                    window.spawn(&*cx, async move |async_cx| {
                                        let path = match receiver.await {
                                            Ok(Ok(Some(path))) => path,
                                            _ => return, // 用户取消
                                        };
                                        let result = std::fs::write(&path, &json);
                                        let _ = async_cx.update(|window, app_cx| {
                                            let ok = result.is_ok();
                                            window.push_notification(
                                                if ok {
                                                    Notification::success(format!(
                                                        "已导出到 {}",
                                                        path.display()
                                                    ))
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
                                let entity = cx.entity();
                                move |_, window, cx| {
                                    let receiver = cx.prompt_for_paths(PathPromptOptions {
                                        files: true,
                                        directories: false,
                                        multiple: false,
                                        prompt: Some("选择要导入的 JSON 备份文件".into()),
                                    });
                                    let entity = entity.clone();
                                    window.spawn(&*cx, async move |async_cx| {
                                        let path = match receiver.await {
                                            Ok(Ok(Some(paths))) => match paths.into_iter().next() {
                                                Some(path) => path,
                                                None => return,
                                            },
                                            _ => return, // 用户取消
                                        };
                                        let parsed = std::fs::read_to_string(&path)
                                            .ok()
                                            .and_then(|s| {
                                                serde_json::from_str::<SyncSnapshot>(&s).ok()
                                            });
                                        let _ = async_cx.update(|window, app_cx| {
                                            let Some(snap) = parsed else {
                                                window.push_notification(
                                                    Notification::error("导入失败：不是有效的备份文件"),
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
                                                let settings = snap.settings.clone();
                                                app.update(app_cx, |app, cx| {
                                                    app.apply_snapshot(snap, window, cx);
                                                });
                                                entity.update(app_cx, |p, cx| {
                                                    p.reload_after_import(settings, window, cx);
                                                });
                                            }
                                            window.push_notification(
                                                Notification::success("已导入数据"),
                                                app_cx,
                                            );
                                        });
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
                                let entity = cx.entity();
                                move |_, window, cx| {
                                    let entity = entity.clone();
                                    window.open_alert_dialog(cx, move |alert, _window, _cx| {
                                        let entity = entity.clone();
                                        alert
                                            .title("清空所有数据？")
                                            .description("将删除所有待办、分类和标签，此操作不可恢复。")
                                            .button_props(
                                                DialogButtonProps::default()
                                                    .ok_text("清空")
                                                    .ok_variant(ButtonVariant::Danger)
                                                    .on_ok(move |_, _window, cx| {
                                                        if let Some(handle) =
                                                            cx.try_global::<DoitAppHandle>()
                                                        {
                                                            let app = handle.0.clone();
                                                            app.update(cx, |app, cx| {
                                                                app.clear_all(cx);
                                                            });
                                                        }
                                                        entity.update(cx, |p, cx| {
                                                            p.local = AppSettings::default();
                                                            p.active = Section::Appearance;
                                                            p.recording = false;
                                                            cx.notify();
                                                        });
                                                        true
                                                    }),
                                            )
                                    });
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
    }

    // ── 关于 ──

    fn render_about(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .v_flex()
            .gap_4()
            .child(div().text_lg().font_bold().child("Doit"))
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
                            .child(format!("v{}", env!("CARGO_PKG_VERSION"))),
                    ),
            )
            .child(
                Button::new("check-update")
                    .small()
                    .outline()
                    .disabled(true)
                    .tooltip("暂不支持")
                    .label("检查更新"),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("用 GPUI Kit 纯 Rust 重写的 Doit 桌面待办应用。"),
            )
    }
}

// ── Small composition helpers ───────────────────────────────────────────────

fn section_block(title: &'static str, cx: &App) -> gpui_kit::Div {
    div()
        .v_flex()
        .gap_2()
        .child(
            div()
                .text_xs()
                .font_semibold()
                .text_color(cx.theme().muted_foreground)
                .child(title),
        )
}

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
fn sync_state_text(state: &TransferState, cx: &App) -> impl IntoElement {
    match state {
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

/// Build the `Keystroke` a `ShortcutConfig` describes, so it can be shown as
/// a native `Kbd` chip (or parsed back into an action binding).
fn shortcut_keystroke(sc: &ShortcutConfig) -> Keystroke {
    Keystroke {
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
