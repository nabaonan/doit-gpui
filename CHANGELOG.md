# Changelog

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
