# doit-gpui

> 用 Rust + **GPUI Kit** 纯原生复刻 doit-desktop —— 无 WebView、无 Chromium，视觉与交互 1:1 对齐薄荷绿审美。

这是独立于 `doit-desktop` / `doit-desktop-prototype` 的**全新项目**，不修改任何原有项目。

开发约束、数据契约、同步语义和接手须知见 [AGENT.md](./AGENT.md)。

## 当前状态：可运行的应用骨架（研发进行中）

已落地约 3100 行 Rust 代码，编译通过、可启动、40 个单元测试全绿。

### ✅ 已完成

| 模块 | 内容 | 文件 |
|---|---|---|
| 工程与依赖 | Cargo 工程，gpui-kit 0.8.1（单一依赖带入 GPUI + base + component + Lucide 图标） | `Cargo.toml` |
| 设计系统 | doit 全部设计令牌（亮/暗双主题、优先级色、热力 5 档、圆角/尺寸）翻译为 Rust 常量 | `src/theme/palette.rs` |
| 领域层 | Task/Category/Snapshot v2 数据契约（与手机/桌面端共用）；任务树构建、`recalcUp` 父子联动、今日可见性、墓碑删除 | `src/domain/types.rs`、`task_tree.rs` |
| 日期工具 | 本地时区日界（与桌面端 `startOfDay` 一致）、月历网格、月份偏移、日期加减、**周一起始的自然周 / 自然月 / 自然年区间** | `src/domain/types.rs` |
| 持久化 | `doit-backup.json` 原子写 + `.bak.1~3` 备份轮转 + 损坏回退 | `src/domain/snapshot.rs` |
| 顶栏 | logo + 四页导航（带图标）+ 同步胶囊，铺满窗口左右贴边 | `src/ui/topbar.rs` |
| 今日页 | 分类胶囊导航（**固定在顶部、滚动任务列表时不跟随**；选中态用分类自身颜色高亮、右侧显示该分类未完成顶级任务数徽标、右键弹菜单可编辑/删除分类，**编辑弹框可同时改分类颜色**；右侧虚线「+ 添加分类」按钮弹窗可一次性添加多个分类）、**父/子任务各自独立卡片**（逐级等距缩进 52px）、勾选完成（**完成态显示白色对钩**）、含子任务只读虚线圈、优先级标签、子任务进度环、**有子任务的父任务点击卡片本体即可切换展开/收起（箭头随状态翻转：展开 ↓ / 收起 →）**（独立展开按钮 / 勾选圈 / 内联输入点击不触发）、展开/收起、分类标签 | `src/ui/today.rs`、`task_card.rs`、`cat_dialog.rs` |
| 右键菜单 | **任务卡片右键弹出上下文菜单**：采用 gpui-component 官方 `ContextMenu`/`PopupMenu` 组件（自带右键捕获、坐标锚定、窗口边距钳制、点击外部关闭），每个任务 `task-<uid>` 独立菜单状态。菜单项：编辑（内联输入改标题）、子任务（**新建一张独立子任务卡片承接输入，卡片上可点优先级胶囊循环切换、默认继承父任务；子任务分类始终跟随父任务**）、优先级 ▸（子菜单 5 档，**每档前置对应颜色的圆点图标**，当前项 checked）、改分类 ▸（子菜单，仅顶级任务，当前项 checked）、删除；**已完成任务仅显示删除** | `src/ui/ctx_menu.rs`、`src/ui/task_card.rs`、`src/app.rs` |
| 快速添加栏 | 底部固定栏：**左侧优先级胶囊可展开下拉**（紧急/高/普通/低/不设，带彩色圆点）、**右侧分类下拉可展开**（全部 + 各分类，带分类色圆点）、回车创建任务时使用所选优先级与分类、点击菜单外自动收起 | `src/ui/quick_add.rs`、`src/app.rs` |
| 统计页 | **顶部近一周 / 近一月 / 近一年区间切换**（决定全部图表口径）；**上排：完成热力图（周=一行 7 格 + 周几表头、月=按周对齐、年=GitHub 式 53 周且格子正方形，**鼠标划过色块用 gpui-kit Tooltip 组件浮出「日期 · 完成 N 项」气泡**）+ 分类占比环形图（**gpui-kit 自带 `PieChart` 组件：inner_radius 形成环形【注意 inner/outer_radius 为像素非比例】、hover 扇区外凸 + 浮动 tooltip 显示「分类 · 完成数」** + 右侧图例）**；下排：每日/每月完成趋势折线图（通栏，自带数据点 hover 气泡） | `src/ui/stats.rs` |
| 月历页 | **手绘月历面板 + 右侧当日完成列表**（翻月/选中/回到今天/完成小圆点；恢复自绘实现——gpui-kit 自带 Calendar 的 item 回调不暴露日期值、clear_children 后无法加回日期数字导致格子空白，故弃用） | `src/ui/calendar.rs` |
| 设置页 | 主题切换（同时驱动自绘色板 + gpui 组件主题）、**完成进度风格色板按设计稿改为「同色描边环 + 白色内环 + 白色对钩」**、**WebDAV 完整设置区**（同步状态 / 启用开关 / 服务器地址 / 账号 / 密码令牌 / 测试连接 / 保存并同步）、开机自启开关 | `src/ui/settings.rs` |
| 添加分类弹框 | **用 gpui-kit 自带 `Dialog` 组件 + 显式 `DialogFooter`（保存/取消 Button）** 实现添加 / 编辑 / 删除确认；添加入口弹框内每行左侧色块点击弹**颜色气泡**（一整行内置 CAT_PALETTE 8 色，选中效果与设置页「完成进度风格」一致：同色描边环+白色内环+白色对钩）；「再添一个」用 gpui-kit Button | `src/ui/cat_dialog.rs`、`src/app.rs` |
| 全局质感 | **gpui-component 组件主题的 focus ring / input 边框 / accent 统一覆盖为薄荷绿**（亮暗各适配，启动与切主题时同步）；**页面切换 180ms 淡入动画**（id 随页变化重播）；**内容区用 gpui-kit `Scrollable`（`.overflow_y_scrollbar()`）滚动，超过页高自动滚动 + 右侧滚动条** | `src/app.rs` |
| 测试 | 40 个单元测试（数据类型 / 颜色归一化 / 快照校验 / 任务树与父子联动 / 墓碑级联 / 今日口径 / 日期与月历 / WebDAV 合并语义 / 卡片摊平与折叠 / 热力图与区间口径） | `src/domain/*.rs`、`src/sync.rs`、`src/ui/task_card.rs`、`src/ui/stats.rs` |

### 🚧 待接入（后续迭代，非阻塞）

- **系统集成**：托盘图标、关闭隐藏到托盘、单实例、开机自启的实际写入、窗口状态记忆（需 `tray-icon` 及平台 crate）。
- **WebDAV 同步增强**：凭据改用系统钥匙串、代理探测与更细的错误提示（当前密码保存在 `settings.json`，钥匙串待接入）。
- **应用内更新**：GitHub Releases 检查与下载。
- **交互细节**：长按完成动效、展开折叠状态持久化、分类管理弹窗。
- **原生感**：macOS 标题栏与窗口配置（现用 GPUI 默认窗口）。

> 原型的全部设计说明、组件映射表与可行性评估仍完整保留在 `index.html` 的设计说明抽屉中（右下角按钮）。

## 构建与运行

环境要求：Rust ≥ 1.85（本地实测 1.98.1）；macOS 需 `font-kit`（gpui-kit 已通过 platform 传递启用）。

```bash
cargo build        # 首次编译依赖较多（约 8 分钟、790 个 crate），之后增量编译秒级
cargo run          # 运行
cargo test         # 40 个单元测试
cargo test --offline  # 缓存完备时避免联网解析
```

首次启动会在 `~/Library/Application Support/doit-gpui/` 写入 `doit-backup.json`；无快照时注入演示数据（与原型一致），可直接看到完整界面。

## 发版

本目录是本地开发版，不是 git 仓库；发版在已发布仓库 `nabaonan/doit-gpui` 进行：

1. 把 `src/`、`Cargo.toml`、`Cargo.lock`、`README.md`、`AGENT.md`、`index.html` 同步到已发布仓库
2. 保留该仓库的 `scripts/release.sh`、`.github/workflows/release.yml`、`packaging/` 和 `LICENSE`
3. `git fetch --tags`，确认远端最新版本
4. `./scripts/release.sh patch --dry-run` 预览版本与变更日志
5. `./scripts/release.sh patch` 提交版本变更并推送 `v*` tag，CI 构建三平台产物并发布 Release

日常改动只提交代码，不自动发版。详细约定见 [AGENT.md](./AGENT.md)。

## 产品原型（HTML）

`index.html` 是单文件高保真原型（`file://` 双击即可打开），既是视觉验收基准，也是可交互设计方案：右下角按钮可开启「组件映射标注」与「设计说明抽屉」（概述 / 组件映射 / 能做到 / 路线图）。

## 技术选型要点

- **GPUI Kit 0.8.1**：长桥开源、Zed GPUI 之上、Longbridge Pro 商用验证的组件库（Apache-2.0）。选 0.8 而非最新版 0.10，因本地已缓存且 API 稳定、可离线编译。
- **自绘色板而非组件库主题**：gpui-component 主题是 shadcn 固定字段，无法精确表达 doit 薄荷绿；因此定义 `DoitPalette` 全量 hex 令牌，render 时直接引用，保证与原型肉眼一致。
- **热力图**：gpui-kit 图表无 Heatmap，用方格网格自绘（已实现）。
- **数据契约优先**：领域层完全对齐 Snapshot v2，为后续与手机/桌面端 WebDAV 互通打基础。

### ⚠️ GPUI 踩坑备忘（改代码前先看）

1. **内联 SVG 不会继承父级 `text_color`**。GPUI 的 `Svg::paint` 只读**自身**的 `style.text.color`，父容器的文字色不会下传；图标里的 `stroke="currentColor"` 因此会渲染成透明（图标直接消失）。
   ✅ 正确写法：`svg().data(&icons::check()).w(px(15.)).h(px(15.)).text_color(rgb(0xFFFFFF))` —— 颜色必须加在 `svg()` 上。
2. **缩进用 `padding` 而不是 `margin`**。卡片要 `width:100%`，若用 `margin-left` 缩进会在 Taffy（BorderBox 盒模型）下横向溢出；应写成外层容器 `.pl(px(52.))` + 内层卡片 `.w_full()`。
3. **`#[test]` 宏冲突**。在 `use gpui_kit::*` / `use super::*` 的作用域里写 `#[test]` 会解析到 gpui 的 `test` 宏（而非内置测试宏），导致宏展开递归溢出甚至 SIGBUS。测试模块里改用精确导入（`use super::flatten;`），不要 `use super::*`。
4. **闭包参数类型要显式标注**。`on_click(|_, _, cx| ...)` 中若先调用 `entity.read(cx)`（要 `&App`），`cx` 会被推断成 `&App`，随后 `entity.update(cx, ..)`（要 `&mut App`）就编译不过。写成 `|_, _, cx: &mut App| ...` 即可。
5. **`gpui-kit` 的 `test-support` 特性在本机无法启用**：会引入 `gpui-fast` 并锁定 `taffy =0.15.0`，与 `gpui-pre` 的 `^0.15.0`（锁定 0.15.1）冲突，连锁引发 `unicode-properties` 冲突。无头 UI 断言需另寻方案。
6. **`items_center` / `justify_center` 必须配 `flex()`**。缺 `flex()` 时元素仍是 block 布局，`align-items`/`justify-content` 不生效——分类胶囊里的色点会跑到文字上方（曾出现的「颜色标识错位」即此因）。
7. **下拉浮层用「根节点绝对定位 + 透明遮罩」实现**。菜单作为根 flex 的绝对定位子元素（`absolute().left()/right()/bottom()`），才能浮在滚动区之上且不被裁剪；配套一块 `absolute().inset_0()` 的透明遮罩（放在快速添加栏**之前**，保证触发器仍可点）用于「点外部关闭」。
8. **热力图不要依赖 `aspect_square()`**：网格项在 Taffy 下高度可能塌陷。改用 flex 行 + `flex_1()` 单元格 + 固定高度（周/月 62px、年 8px），既保证可见又与原型 1fr 列一致。
9. **`when` 只有两参版本**：`when(cond, |e| ...)`；没有 `else` 分支参数，需要二选一时拆成 `when(c, ..)` + `when(!c, ..)`。
10. **`Rgba::alpha()` 是「设置」alpha 而非相乘**，透明色用 `p.heat(0).alpha(0.0)`（`transparent_black()` 返回的是 `Hsla`，与 `Rgba` 混用会类型不匹配）。


## 参考

- 视觉基准 / 设计方案：`./index.html`
- 原始实现：`../doit-desktop`（Tauri + Vue）、`../doit-desktop-prototype/prototype.html`
- GPUI Kit：<https://gpui-kit.com> · <https://github.com/longbridge/gpui-kit>
