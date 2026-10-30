# Doit GPUI

Doit - 一个用纯 Rust + [GPUI Kit](https://gpui-kit.com) 写的最小化待办应用。

## 功能

### 四种视图

| 视图 | 说明 |
| --- | --- |
| 今日待办 | 待办主界面：未完成 + 今天完成的条目；子任务可无限层级、折叠展开；按分类着色并显示标签徽章 |
| 日历浏览 | 月历网格，有完成记录的日子带圆点；点选日期查看当天的完成清单，可翻上/下月 |
| 时间轴 | 按天回顾完成记录：左侧日期列 + 一条贯穿圆点的竖线，从今天往前连续排列 |
| 统计 | 四块图表：近 52 周每日完成热力图、近 12 个月完成量柱状图、已完成/未完成环图、当月每日完成趋势 |

### 待办管理

- **添加**：顶部输入框按回车即添加；快捷键可在「设置 → 交互 → 添加快捷键」改成任意组合键（如 `Ctrl+Enter`），改完后只有该组合键生效。
- **编辑**：双击标题就地编辑，回车保存、Esc 取消；也可右键菜单「编辑」。
- **子任务**：右键「添加子任务」，支持任意层级；勾选父任务会联动子任务，子任务全完成时父任务自动完成。
- **拖拽排序**：拖到某行的上/下边缘 = 插入到该层该位置；拖到标题上 = 收为它的子任务；拖动父级待办会带走整棵子树，可直接跨层级调整。
- **完成方式**：设置里可选「勾选」或「长按」（长按有进度反馈），完成后自动排到列表底部。
- **提醒**：右键「设置提醒」按时间弹出系统通知，可随时清除。
- **右键菜单**：编辑、添加子任务、设置/清除提醒、设置/清除标签、移动到分类、删除。

### 分类与标签

- 分类与标签都支持自定义颜色；仍在「设置 → 分类与标签」中增删改、拖拽排序。
- 分类可指定默认分类（新待办自动归入），顶栏下拉可按分类筛选；分类只挂在顶层待办上，会随子树整体移动。
- 标签一条待办可挂多个，按颜色区分。

### 外观与交互

- 主题：跟随系统 / 浅色 / 深色，切换即时生效。
- 设置弹框右下角可拖动调整大小，长内容滚动查看。
- 状态与图表的配色走主题 token，深浅色下保持一致的可读性。

### 数据与备份

- 数据保存在本地 `~/.doit-gpui/snapshot.json`（纯 JSON），无需账号、不联网也能用。
- 「设置 → 同步」配置 WebDAV 云备份 / 恢复，密码在本机加密保存；支持定时备份与定时恢复。
- 「设置 → 数据」导出 / 导入 JSON 备份，或清空全部数据。

## 开发与运行

要求：Rust 稳定版（`rustup` 安装）。

```bash
cargo run          # debug 运行
cargo run --release  # 发布构建
```

> 注：GPUI 在 debug 构建下对 accessibility 树重复节点会触发断言（`Duplicate a11y node id`），
> release 构建无此问题；日常开发建议 `cargo run --release` 或修复该断言后使用 debug。

## 发版

发版指令 = **打 tag 推远端，由 GitHub Actions 构建并发布**，本地不做任何构建/运行。
`release.sh` 只做四件事：自动递增版本号 → 生成变更日志 → `cargo check` 自检 → commit + tag + push；
tag 推送即触发 `.github/workflows/release.yml`，自动构建四个平台产物并发布 Release（含变更日志）。

**精简指令（二选一）：**

```bash
cargo release                 # 已配好 PATH 链接，直接在仓库根执行
./scripts/release.sh          # 或直接用仓库内脚本
```

可选参数：`minor` / `major` / `x.y.z` / `--dry-run`（只预览不修改）。
首次使用 `cargo release` 前，先建立链接（一次性）：

```bash
ln -sfn "$(pwd)/scripts/release.sh" ~/.cargo/bin/cargo-release
```

> 这个链接不在仓库里，重装/升级 Rust（rustup）或迁移 HOME 后可能丢失，表现为
> `error: no such command: release`。重新执行上面的 `ln -sfn` 即可恢复；也可以直接用
> `./scripts/release.sh`，它不依赖这个链接。

示例：

```bash
cargo release                 # 0.1.0 -> 0.1.1，打 v0.1.1 并推送
cargo release minor           # 0.1.0 -> 0.2.0
cargo release --dry-run       # 只预览版本号与变更日志，不做任何修改
```

流程：

1. 读取 Cargo.toml 当前版本，计算下一个版本（`major|minor|patch` 默认 `patch`）；
2. 依据自上个 tag 以来的提交自动生成 `CHANGELOG.md` 新章节（新增 / 修复 / 优化 / 文档 / 其他）；
3. `cargo check` 快速自检；
4. 提交 `Cargo.toml` + `Cargo.lock` + `CHANGELOG.md`，打 `vX.Y.Z` 注解 tag，推送 main+tag；
5. 推送 tag 触发 GitHub Actions，构建各平台产物并以该章节作为 Release 说明。

## 平台产物

| Release 附件（含版本号，示例 v0.1.1） | 覆盖平台 |
| --- | --- |
| `doit-gpui-v0.1.1-macos-arm64.dmg` | macOS Apple Silicon（arm64） |
| `doit-gpui-v0.1.1-macos-x64.dmg` | macOS Intel（x86_64） |
| `doit-gpui-v0.1.1-windows-x64.zip` | Windows x64 |
| `doit-gpui-v0.1.1-linux-x64.tar.gz` | Linux x64 |

### macOS 使用提示

- 应用未做 Apple 公证，首次打开会在 Gatekeeper 拦截：**右键 `Doit.app` → 打开**（或系统设置 → 隐私与安全性 → 仍要打开）。
- 也可在终端移除隔离属性后正常双击：`xattr -dr com.apple.quarantine /Applications/Doit.app`
- 安装包已做 **ad-hoc 签名**（解决 x86_64 版在 Apple Silicon 的 Rosetta 下报「应用已损坏」）；正式对外分发可再用 Apple 开发者证书签名并公证。

## 许可证

MIT
