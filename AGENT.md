# AGENT.md — doit-gpui 接手指南

本文件面向接手本项目的 AI / 开发者。目标是在动手前先理解这份 GPUI 代码与已发布仓库的关系、数据契约、同步语义和 GPUI 平台陷阱。

用户视角与交互说明见 [README.md](./README.md)；本文件是开发约束和高风险区域清单。

## 1. 项目定位与代码副本关系

`doit-gpui` 是 `doit` 体系的原生 GUI 客户端，使用 Rust + GPUI Kit，无 WebView / Chromium。它复刻 `doit-desktop` 的视觉与交互，并与手机 / 桌面 / 终端端共享 WebDAV 文件 `doit-backup.json`。

四端仓库：

| 端 | 仓库 |
| --- | --- |
| 手机端 | `nabaonan/doit-apk` |
| 桌面端 | `nabaonan/doit-desktop` |
| GUI 端 | `nabaonan/doit-gpui` |
| 终端版 | `nabaonan/doit-tui` |
| 官网 | `nabaonan/doit-website` |

**重要：本机存在两份 doit-gpui，不要混淆。**

| 路径 | 状态 |
| --- | --- |
| `/Users/nbn/WorkBuddy/安卓应用/doit-gpui` | 当前开发版（本目录）。Edition 2024、`gpui-kit 0.8.1`、`ureq`、`version 0.1.0`，**不是 git 仓库**，无 CI / 打包脚本 |
| `/Users/nbn/workspace/github/doit-gpui` | 已发布仓库工作副本。有 `scripts/release.sh`、`.github/workflows/release.yml`、`packaging/macos`、`LICENSE`、`CHANGELOG.md`，远端当前 tag 已到 `v0.2.0` |

当前约定：以本目录的新版源码为准，同步到已发布仓库后保留其 CI / 打包资产再发版。不要把两份代码当成同一工作树直接 `git push`。

## 2. 技术栈与入口

- Rust 2024 edition，rust-version 1.85
- `gpui-kit = 0.8.1`：单一依赖带入 GPUI / gpui-base / gpui-component / Lucide 图标；选 0.8 是为了本机缓存与 API 稳定
- `serde` / `serde_json` / `chrono` / `dirs`
- `ureq 2.12` + `base64`：WebDAV 请求（同步阻塞，必须放后台线程池）
- macOS target 显式依赖 `gpui-pre-platform 0.3.10` with `font-kit`
- `[profile.release]`：`opt-level=3`、`lto="thin"`、`strip=true`
- `lib + bin` 双入口：`src/main.rs` 只启动窗口，业务全在库 crate

## 3. 文件地图

| 文件 | 职责 |
| --- | --- |
| `src/lib.rs` | 模块出口：`app` / `domain` / `sync` / `theme` / `ui`；领域层开启 `clippy::pedantic` 并逐条豁免 |
| `src/main.rs` | `application().run` → `init` → `apply_component_theme` → `open_window` |
| `src/app.rs` | `AppState`（Entity 全局状态）、`AppView`、页面枚举、主题覆盖、同步编排、分类弹窗、右键动作、月历辅助 |
| `src/domain/types.rs` | `Task` / `Category` / `SnapshotV2` / `AppSettings` / `WebDavSettings` + 日期工具 |
| `src/domain/task_tree.rs` | `build_tree` / `today_tree` / `recalc_up` / `mark_deleted` / 分类 CRUD / `CAT_PALETTE` |
| `src/domain/snapshot.rs` | `doit-backup.json` 原子写、`.bak.1~3` 轮转、损坏回退、`normalize_snapshot`、`default_data_dir` |
| `src/domain/color.rs` | `normalize_category_color` / `parse_color_or_default` / `DEFAULT_CATEGORY_COLOR` |
| `src/domain/seed.rs` | 首次启动演示数据 |
| `src/sync.rs` | WebDAV 网络层 + `merge3` 三方合并（纯函数，时间可注入） |
| `src/theme/palette.rs` | `DoitPalette` 亮 / 暗双主题、优先级色、热力档位、尺寸令牌 |
| `src/ui/topbar.rs` | 导航、同步胶囊（点击即同步）、日期 + 今日进度胶囊 |
| `src/ui/today.rs` / `task_card.rs` / `quick_add.rs` / `stats.rs` / `calendar.rs` / `settings.rs` | 四个页面与交互组件 |
| `src/ui/cat_dialog.rs` / `ctx_menu.rs` | 分类弹窗与右键菜单 |
| `src/ui/icons.rs` | 内联 SVG 图标（cloud / cloudOk / cloudDirty / sync / progress_ring 等） |
| `src/ui/mod.rs` | `sync_badge(status)` 与 `sync_icon(status)` 状态映射 |
| `index.html` | 高保真设计原型，含组件映射标注，改 UI 前可对照 |

## 4. 数据契约与同步

### 4.1 SnapshotV2

`#[serde(rename_all = "camelCase")]`：

```text
Task { uid, parentUid, title, category, priority: u8, done,
       completedAt, createdAt, updatedAt, deletedAt, depth, sortOrder }
Category { name, color, sortOrder, updatedAt, deletedAt }
SnapshotV2 { version: 2, revision, exportedAt, deviceId, lastSyncAt,
             categories, tasks }
```

- 与桌面 / 手机 / TUI 共用同一 wire 契约，远端文件名 `doit-backup.json`
- `deletedAt` 有 `#[serde(default)]` 以兼容旧快照
- 优先级：`0 不设 / 1 低 / 2 普通 / 3 高 / 4 紧急`
- 删除写墓碑，不物理移除；`today_tree()` / `build_tree()` 自动过滤；`mark_deleted()` 后调 `recalc_up()`，最后一个未完成任务被删时父任务立即完成
- `recalc_up()` 只在完成态翻转时刷新 `completed_at`/`updated_at`；启动与同步落盘前调 `recalc_all()` 自愈历史脏数据（墓碑子任务不算有效子任务）

### 4.2 同步流程

`sync_once()`：

1. `probe_root()`：`PROPFIND Depth:0` + Basic Auth
2. `get_remote()`：`GET doit-backup.json`，404 视为远端无文件
3. 对 local / remote / base 调 `normalize_snapshot()`
4. `merge3()` 三方合并
5. `put_remote()` 上传，成功后写 `sync-base.json`

超时：请求 15s / 连接 8s。`classify()` 只返回脱敏文案，禁止输出原始 `e.to_string()`（会泄漏 URL / userinfo）。

### 4.3 合并语义（`src/sync.rs::merge3`）

移植桌面端 `domain/merge.ts`，不可随意改：

1. 按 uid / name 取并集，用 `BTreeMap` 保证确定性
2. 字段级合并；同字段冲突按 `effective_time`（updated / completed / deleted / created 取最大）取新，**平局远程优先**
3. 墓碑：删除后对方又有真实编辑才复活
4. `is_stale_remote()`：远端 `exported_at` 更早且 `revision` 不高于本地时直接本地胜
5. `revision = max(local, remote) + 1`

### 4.4 颜色归一化

`domain/color.rs`：

- `normalize_category_color()`：trim → 补 `#` → 6 位 hex → 转大写 → 非法回退 `#34C7A8`
- `normalize_snapshot()` 还处理 uid 去重、priority > 4 归 0、完成态自洽、空 `parentUid` → None、非法时间 / 排序回退、空 `deviceId` → `doit-gpui`
- `CAT_PALETTE` 位于 `task_tree.rs`：`#34C7A8 / #5AA9E6 / #F2A65A / #A78BFA / #22B8CF / #F08A8A / #AACB5E / #7C96B0`
- 四端色板必须同步改；应用侧选色前先归一化，避免历史值误判占用

### 4.5 顶栏同步状态

状态字符串：`synced / dirty / syncing / error / off / ready`。

- `ready`：WebDAV 已开启但尚未验证过连接
- `dirty`：本地有未同步更改
- `syncing`：后台同步中
- `error` / `off` / `synced`：见 `src/ui/mod.rs::sync_badge`
- 同步胶囊点击 → `AppState::sync_now_entity()` → `sync_now()`；未开启 / 未配置时只 toast 提示，不进入 syncing
- 设置页「保存并同步」与顶栏点击共用 `run_sync()`，避免行为漂移
- 本地 `save()` 在同步开启且非同步中时置 `dirty`；成功回调在 `save()` 之后再置回 `synced`

新增状态时必须同时更新 `sync_badge`、`sync_icon`、`AppState` 赋值点和顶栏渲染。

### 4.6 今日页紧急优先

- `SnapshotV2::today_tree(filter, sort_by_priority)` 在第二参数为真时**只**对顶级 `TaskNode` 做稳定降序（4→0），`subs` 顺序与引用不动；顶栏进度统计传 `false`，不受排序影响
- 顶栏开关只在 `Page::Today` 显示，位于同步胶囊左侧；常态红字红框，开启后实心红底白字，点击 `AppState::toggle_sort_by_priority()`
- 偏好存 `AppSettings.sort_by_priority`（settings.json，serde camelCase），属本地 UI 设置，**不进同步快照**；四端语义一致

## 5. GPUI 平台陷阱（改 UI 前必读）

来自 `README.md` 的实战总结：

1. 内联 SVG 不继承父级 `text_color`，颜色必须写在 `svg()` 上
2. 缩进用 `padding` 而不是 `margin`（Taffy BorderBox）
3. `#[cfg(test)]` 模块里禁止 `use super::*`（会解析到 gpui 的 `test` 宏导致 SIGBUS），用精确导入
4. 闭包参数 `cx` 显式标注 `&mut App`
5. `test-support` 特性在本机因 taffy 0.15.0 / 0.15.1 冲突无法启用
6. `items_center` / `justify_center` 必须配合 `flex()`
7. 下拉浮层用根节点绝对定位 + 透明遮罩，避免被裁剪
8. 热力图不要用 `aspect_square()`
9. `when` 只接受两个参数
10. `Rgba::alpha()` 是“设置 alpha”而不是“乘以 alpha”

## 6. 命令与测试

```bash
cargo build            # 首次约 8 分钟 / 大量依赖
cargo run
cargo test
cargo test --offline   # 缓存完备时避免网络解析
cargo build --release
```

- 所有测试都是 `src/**` 内的 `#[cfg(test)]` 单元模块；`tests/` 目前为空
- README 中测试数量描述有新旧混杂，以 `cargo test` 输出为准
- 本项目无代理配置；运行期 WebDAV 由 `ureq` 发起，系统 / 环境代理可能导致私有 NAS 连接失败。`sync.rs::classify()` 会把 `ProxyConnect` 归为「无法连接 WebDAV 服务器」
- 当前本地代码把 WebDAV 密码明文写进 `settings.json`；钥匙串仍在待办，不要误以为已加密

## 7. 发版路径

本目录不是 git 仓库，不能直接发版。当前约定：

1. 将本目录的 `src/`、`Cargo.toml`、`Cargo.lock`、`README.md`、`AGENT.md`、`index.html` 同步到 `/Users/nbn/workspace/github/doit-gpui`
2. 保留已发布仓库的 `scripts/release.sh`、`.github/workflows/release.yml`、`packaging/macos`、`LICENSE`、`CHANGELOG.md`
3. 在已发布仓库 `git fetch --tags`，确认远端最新版本（当前 `v0.2.0`）
4. 同步版本号，`./scripts/release.sh patch --dry-run` 验证后发版
5. tag 触发 CI 构建 macOS arm64/x64、Windows x64、Linux x64 产物并发布 GitHub Release

**发版纪律：只有用户明确要求「发版 / 发布」时才能执行。** 日常只改代码和测试，不自动打 tag。

## 8. 接手红线清单

1. 不要混淆本地开发版与已发布仓库工作副本
2. wire 契约、合并语义、墓碑删除、颜色四端一致
3. 同步状态相关改动要同时改状态映射、图标和顶栏交互
4. WebDAV 错误必须脱敏
5. 改 GPUI UI 前先读平台陷阱清单
6. 测试用 `cargo test --offline`，注意测试模块导入方式
7. 发版只走已发布仓库的 `scripts/release.sh` + CI
