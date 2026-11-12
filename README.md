# Mix

基于 Tauri 2 + WebView2 的 Chromium 内核浏览器

## 快速开始

1. 解压 `Mix-0.4.0-release.zip`（`dist/` 里按版本号命名）到任意目录
2. 双击 `mix.exe`
3. 首次启动会在同目录生成 `UserData/`、`crx/`、`javascript/`

程序目录不可写时（例如放在 `Program Files` 下），`UserData/` 会回落到系统用户目录，启动日志第一行会写明实际路径。

## 目录结构

```
Mix/
├── mix.exe
├── UserData/
│   ├── settings.json   设置
│   ├── history.json    历史
│   ├── bookmarks.json  书签
│   ├── favicons.json   网站图标映射
│   ├── favicons/       图标本地缓存
│   ├── session.json    上次的窗口与标签
│   ├── theme.json      主题
│   ├── custom.css      自定义样式
│   └── logs/mix.log    运行日志
├── crx/                扩展文件夹
└── javascript/         用户脚本
```

**备份**：设置 → 备份 → 导出配置，会打包除 WebView2 缓存外的全部数据。

## 功能

**浏览**
- 多标签 / 多窗口 / 无痕模式
- 标签分组（Ctrl+Shift+G）、固定标签、撤销关闭（Ctrl+Shift+T）
- 标签搜索（Ctrl+Shift+A）、悬停预览缩略图
- 后台标签挂起（省内存）与冻结，白名单域名不挂起
- 侧边栏（书签 / 下载 / 历史，可放左或右）
- 会话恢复、全屏、页内查找

**数据**
- 历史：分组、搜索建议、按站点筛选、图标持久化
- 书签：文件夹分类、双击改名、拖拽排序
- 下载：列表、打开

**扩展**
- 把解压好的扩展文件夹放进 `crx/`
- 扩展页可查看 WebView2 真实的加载状态、启用/停用

**界面**
- 内部页面：新标签页、历史、书签、下载、扩展、设置、用户脚本
- 命令面板：Ctrl+Shift+P（搜索动作与历史站点）
- 主题：9 个颜色 + 圆角/字号，支持实时预览与导入导出
- 页内翻译：点工具栏的「文」原地替换正文，再点一次还原原文（失败会在右上角提示）
- 中英双语（设置 → 语言）

**系统**
- 注册为默认浏览器
- 配置备份/恢复、自定义样式、自定义搜索引擎

## 默认快捷键

| 按键 | 动作 |
|---|---|
| Ctrl+T / Ctrl+W | 新建 / 关闭标签 |
| Ctrl+Shift+T | 恢复关闭的标签 |
| Ctrl+Tab / Ctrl+Shift+Tab | 切换标签 |
| Ctrl+L | 聚焦地址栏 |
| Ctrl+R / F5 | 刷新 |
| Ctrl+D | 收藏 |
| Ctrl+J | 下载 |
| Ctrl+= / Ctrl+- / Ctrl+0 | 放大 / 缩小 / 重置 |
| Ctrl+F | 页内查找 |
| F11 / F12 | 全屏 / 开发者工具 |
| Ctrl+Shift+P | 命令面板 |

## 鼠标手势

按住**右键**划动，松开执行。默认映射：

| 手势 | 动作 |
|---|---|
| ← / → | 后退 / 前进 |
| ↑ / ↓ | 滚动到顶 / 到底 |
| ↑↓ | 刷新 |
| ↓↑ | 恢复关闭的标签 |
| ←→ / →← | 下一个 / 上一个标签 |
| ↑→ | 新建标签 |
| ↓← | 关闭标签 |
| ↑← | 收藏 |
| ↓→ | 下载 |

**设置 → 鼠标手势** 可改颜色、线宽、阈值、每个手势的动作、以及禁用域名。改完对新打开的页面生效。

## 超级拖拽

按住**链接 / 图片 / 选中文字**拖出去，按方向执行：

| 方向 | 默认动作 |
|---|---|
| ← | 在新标签中打开 |
| → | 搜索文本 |
| ↑ | 复制到剪贴板 |
| ↓ | 下载目标 |

## 扩展与用户脚本

**扩展**：解压后的文件夹放进 `crx/`，扩展页会显示。支持 MV3。

**用户脚本**：`javascript/` 里放 `*.user.js`，支持 `@name` / `@match` / `@run-at`。用户脚本页可启用/停用。

## 自定义

**主题**：设置 → 主题文件，改颜色、圆角、字号，可导出 `theme.json` 分享。

**自定义样式**：设置 → 外观 → 编辑自定义样式，写 CSS 会应用到所有网页（新打开的页面生效，已开的标签刷新一次即可）。

```css
/* 去掉 B 站首页的推荐位 */
.bili-feed4-layout .recommended-swipe { display: none !important; }
```

**搜索引擎**：设置 → 搜索引擎，填含 `%s` 的模板即可，例如 `https://duckduckgo.com/?q=%s`。

## 已知限制

- **不支持密码管理**：没有凭据库集成，也没有自动填充
- **跨域 iframe、Shadow DOM 内的文字无法翻译**
- **只能装 MV3 扩展**，且扩展的 popup 气泡打不开（WebView2 没暴露该接口），只能看扩展信息
- **画布上的文字无法选中或翻译**
- **只在 Windows 上编译验证过**：macOS / Linux 的适配层还没写（`src-tauri/src/platform/` 里只有 `windows.rs`）

## 从源码构建

需要 Rust 1.80+ 和 Node 22。

```powershell
# 先跑能自动化的检查：内部页构建、词条、硬编码文案、命令清单一致性、单元测试
powershell -File scripts\check-all.ps1

cd src-tauri
cargo build --release
powershell -File ..\scripts\build-portable.ps1
```

新增 IPC 命令后必须跑一次 `node scripts/gen-commands.js`：它把 `main.rs` 里的命令清单同步到
`build.rs`（ACL 生成）和 `capabilities/default.json`（本地页面授权），三份清单不再手工维护。
设计文档在 `docs/`，不随仓库分发。

产物在 `dist/Mix-<version>-release.zip`。

## 许可证

[LICENSE](LICENSE)
