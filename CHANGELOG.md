# Changelog

## [v0.1.11] - 2026-10-17

### 新增

- feat: 实现应用内更新下载功能及进度显示



## [v0.1.10] - 2026-10-17

### 新增

- feat: 新增待办提醒功能，支持设置、清除提醒及系统通知
- feat: 为热力图 hover 提示添加入场动画



## [v0.1.9] - 2026-10-15

### 新增

- feat: 新增长按完成待办事项模式
- feat: 为日历热力图单元格添加悬停提示，优化更新检查功能
- feat: 添加待办完成热点图和应用更新检查功能



## [v0.1.8] - 2026-10-13

### 修复

- fix: 修复 macOS 右键编辑时输入框焦点冲突导致的崩溃



## [v0.1.7] - 2026-10-12



## [v0.1.6] - 2026-10-12

### 新增

- feat: 实现基于上次同步时间的WebDAV同步决策逻辑



## [v0.1.5] - 2026-10-12

### 新增

- feat: 升级 gpui-kit 到 0.7.1 并重构颜色选择器和日历组件 ```

### 修复

- fix(color-picker): 修复颜色选择器崩溃问题

### 优化

- refactor: 移除快乐模式和字体设置，简化长按完成逻辑



## [v0.1.4] - 2026-09-16

### 新增

- feat: 待办子任务（父级自动完成、可折叠收起、任意层级）与缩进修复
- feat: 云同步双向化，以最新版本为准（不再复活已删除数据）+ 本地持久化 + 顶部云图标

### 文档

- docs: 初始化 agent 技能配置（issue tracker / triage 标签 / 领域文档）



## [v0.1.3] - 2026-09-16

### 修复

- fix(macos): bundle a real app icon and ad-hoc sign the .app so DMGs install/launch (fix generic icon + 'damaged' x64 builds)



## [v0.1.2] - 2026-09-16

### 修复

- fix(release): version release title and name artifacts with the version (fix 'vv0.1.1')



## [v0.1.1] - 2026-09-16

### 新增

- feat: Doit GPUI desktop todo app

### 其他

- Initial commit



所有显著变更都会记录在此文件中。

本文件的每个 `## [vX.Y.Z] - 日期` 章节由 `scripts/release.sh` 自动生成（依据自上个 tag 以来的提交），并被 GitHub Actions 用作对应 Release 的说明。
