# Doit GPUI

Doit - 一个用纯 Rust + [GPUI Kit](https://gpui-kit.com) 写的最小化待办应用。

## 功能

- 今日待办 / 日历浏览 / 时间轴 / 统计 四种视图
- 分类与标签管理（含颜色选择器、快捷键）
- 右键菜单操作（编辑、设置标签、移动到分类、删除）
- 主题（浅色 / 深色 / 跟随系统）与完成方式（勾选 / 长按）
- 添加快捷键自定义（设置 → 交互 → 添加快捷键，默认回车）
- WebDAV 云备份 / 恢复

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

## 许可证

MIT
