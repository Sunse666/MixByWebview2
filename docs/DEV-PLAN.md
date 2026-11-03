# Mix · 开发大纲

| 项目 | 内容 |
| --- | --- |
| 文档版本 | **v1.5**（2026-11-02；便携目录 · 主题实时预览 · 开发约定已固化） |
| 定位 | 自己和身边几个人用的多标签浏览器壳；**不对外发布、不上架、不商业分发** |
| 许可 | **MIT**（含 `LICENSE` 与第三方署名，见附录 A） |
| 平台 | **三平台都做**（Win / macOS / Linux）；Windows 为一等公民，其余"尽力而为"，已知缺陷见 §1.1 |
| 技术选型 | Tauri 2（`unstable` 多 webview）；某平台失败时只对该平台局部下沉 wry，不全局重写 |
| 周期 | **三平台 8–9 周（单人）**；不为平台一致性拖周期，小问题按 §1.1 放行 |
| 相关文档 | `DEV-PLAN.v1.0.md`（最初的立项草案，仅存档）· `M0-REPORT.md`（M0 结论） |
| 进度 | **M0–M4 完成 + 用户脚本引擎**（两处待人工确认：上传文件选择框、开机自启开关）· M5 打包管线已跑通（Windows zip 2.56 MB） |

## 变更要点

| # | 变更 | 原因 |
| --- | --- | --- |
| 1 | **删除签名 / 公证 / SmartScreen / Apple 开发者账号** | 不发布，就没有"通过 Gatekeeper 与 SmartScreen"这回事 |
| 2 | **删除自动更新**（updater 服务、`latest.json`、密钥保管） | 自用只需手动替换 exe；省掉一整块永久运维负担 |
| 3 | **删除 CI 四目标矩阵与体积守门** | 只需本地一条构建命令 + 可选 GitHub Actions 跑测试 |
| 4 | **分发方式改为便携 zip** | 解压即用、不写注册表、删了就干净，朋友最省事（ADR-012） |
| 5 | 平台从"三平台齐发（含签名公证）"改为**不发布的自用分发** | 省掉签名/公证/商店后，跨平台变得可行 |
| 6 | 体积从硬指标降为**观察项**；扩展兼容从"10 项对照表"降为"实际在用的 3–5 项" | 自用不做发布门槛 |
| 7 | 新增**诊断包**（日志 + 版本 + 设置打包导出） | 没有遥测，朋友出问题只能靠你隔空取日志 |
| 8 | 合规附录缩为 MIT + FontAwesome 署名 | MIT 自用不承担商业合规义务，但署名义务仍然存在 |
| 9 | i18n、多窗口、标签拖出成窗全部移出范围 | 自用不做 |
| 10 | 平台从"按需裁剪"→ **三平台都做**，质量线为"尽力而为" | 三个平台都有人用；小问题可接受，不为一致性拖周期 |
| 11 | 新增 §1.1 已知可接受缺陷清单 | 显式列出不修的平台差异，避免每次都想"顺手补一下" |
| 12 | 视觉从"沿用"→ **可自定义**（配色/圆角/字号/背景图，JSON 导入导出） | 自用要顺眼，且不想每次改样式都改代码 |
| 13 | 删除 "Cute 按钮" | 暂不需要 |
| 14 | 周期 6–7 周 → **8–9 周** | 摊进 M0 的三环境验证与三套打包 |
| 15 | 数据布局定死：`UserData/` + `crx/` + `javascript/`，默认便携（去掉 `--portable` 开关） | 你要的就是"拷走目录即带走一切" |
| 16 | 用户脚本改为**三平台都支持**（不再只 mac/Linux） | 既然 `javascript/` 一直存在，Windows 上也没理由不加载 |
| 17 | 主题编辑器支持**实时预览**（改即套用，取消还原） | 你的明确要求 |
| 18 | 新增 §13 开发约定（commit 粒度与格式、英文注释、push 归属） | 避免以后反复对齐工作方式 |
| 19 | identifier 定为 `com.mix.browser` | 便携目录已与 identifier 解耦，改它不丢配置 |
| 20 | 落盘 `m0-probe/` 实验工程与 `docs/M0-REPORT.md` 模板 | W1 唯一必须产出的东西 |

---

## 1. 平台与质量线（三平台都做，但不追求一致）

| 平台 | 定位 | 质量线（承诺上限） | 备注 |
| --- | --- | --- | --- |
| **Windows 10/11** | P0 · 一等公民 | 全功能可用 | 多 webview 与 MV3 扩展最成熟的一侧 |
| **macOS 12+** | P1 | 除扩展外全功能可用 | 扩展由用户脚本替代；需 ad-hoc 签名；首次"右键打开" |
| **Linux X11** | P2 | 除扩展外全功能可用 | 停靠与全局热键在 X11 正常 |
| **Linux Wayland** | P2 | 浏览器功能可用，**停靠与全局热键不可用** | 协议限制，不做绕路 |

> 三平台都做，但**不追求一致性**：上表"质量线"就是承诺上限，表外的差异按 §1.1 处理——不修、写清楚、能接受就发。这就是"尽量跨平台 + 小问题不要紧"的落地口径。

### 1.1 已知可接受缺陷（不打算修，写清楚就行）

| 缺陷 | 平台 | 影响 | 为什么不修 |
| --- | --- | --- | --- |
| 停靠 / 全局热键不可用 | Linux Wayland | 少两个系统集成功能 | 协议禁止客户端自定位窗口；全局热键无 portal 支持 |
| 无扩展，只能用户脚本 | macOS / Linux | 装不了 crx | 平台 webview 不提供扩展机制 |
| 未公证：首次要"右键打开"或去掉隔离属性 | macOS | 一次手动操作 | 不做公证（§2.2） |
| 未签名：SmartScreen 提示 + 偶发杀软误报 | Windows | 一次手动放行 | 不做签名（§2.2） |
| 部分站点视频黑屏 | Linux | 少数站点不可看 | 缺 H.264，需系统装 gstreamer 插件；文档提示 |
| PDF 依赖后端 | Linux | PDF 打不开 | WebKitGTK 需额外组件，降级为"用系统浏览器打开" |
| 托盘需 AppIndicator | Linux | GNOME 默认不显示 | 降级为"仅标题栏最小化" |

---

## 2. 范围

### 2.1 做

- 无边框自绘窗口（圆角 / 阴影 / 淡入 / 拖拽 / 8 向 resize）
- **界面可自定义**：配色 / 圆角 / 字号 / chrome 高度 / 背景图，主题 JSON 导入导出，内置 3 套预设（§3.7）
- 多标签浏览（独立 webview、标题/URL 同步、关闭、会话恢复、上限保护、挂起）
- **Windows**：加载本地 MV3 crx 扩展 ／ **macOS·Linux**：用户脚本注入
- 浏览器基线：下载、文件上传、右键菜单、键盘快捷键、查找、缩放、权限提示、全屏视频、错误页
- 托盘、全局停靠热键、单实例、开机自启（可选）、窗口位置记忆
- 便携 zip 分发 + 诊断包导出

### 2.2 不做（明确排除，避免中途加戏）

- 应用签名、公证、SmartScreen、Apple 开发者账号、应用商店
- 自动更新（含 updater 服务、版本清单、密钥）
- 遥测、崩溃上报服务、EULA、隐私政策
- MV2 扩展、扩展 popup UI、DRM（Widevine）、账号同步、书签/历史管理
- 多窗口、标签拖出成窗、i18n（只 zh_CN）
- 主题只做"样式自定义"，不做布局结构自定义（标签栏在顶、地址栏在下的结构固定，见 §3.7）
- "Cute 按钮"（暂不需要，已从 chrome 移除）
- 旧实现复用与旧数据迁移：不复用旧代码、不写旧配置导入器、不继承旧交互（见 §11）

### 2.3 验收口径（自用标准，不是发布标准）

| 指标 | 目标 | 口径 |
| --- | --- | --- |
| 冷启动到可交互 | ≤ 1.5 s | 你的主力机，空 profile，n≥5 取中位数 |
| 切标签 | 无可见白屏 | 10 标签全 attached；P95 ≤ 100 ms |
| 内存 | 5 标签 ≤ 700 MB | 全进程 RSS 之和 |
| 常用站可用 | 你日常 5 个站点正常（含视频、登录、上传） | 手工清单 |
| 朋友装机 | 3 人拿到 zip 后 5 分钟内跑起来 | 一人一份说明 |
| 体积 | 观察项：Win zip ≤ 10 MB 最好 | 只为下载/传输方便，不是门槛 |
| 可迁移 | 整个目录拷到另一台机器，设置/扩展/脚本/会话全部跟随 | 手工：两台机各启动一次 |

---

## 3. 架构

### 3.1 分层与 z-order（核心约束，别改）

```
┌──────────────────────── Tauri 2（unstable 多 webview） ────────────────────────┐
│  Rust 主进程                                                                   │
│  ├─ window      : 无边框窗 / 拖拽 / 最大化 / 圆角 / 8 向 resize                 │
│  ├─ tabs        : TabManager（Vec<Tab>、布局、生命周期、挂起）                   │
│  ├─ overlay     : 弹层 webview（地址栏补全 / 右键菜单 / 设置 / 权限询问）        │
│  ├─ downloads   : on_download → 任务 + 进度 + 打开所在文件夹                    │
│  ├─ permissions : on_permission_request → 询问 → 会话内记忆                     │
│  ├─ profile     : data_directory / 便携模式 / 缓存清理 / 诊断包                  │
│  ├─ extensions  : crx 解包 → 校验 → WebView2 加载（Win only）                   │
│  ├─ userscripts : 脚本库 + loader 生成 + 按 URL/帧分发（mac·Linux 用）           │
│  ├─ shortcuts / snap / tray / store / logging                                   │
│                                                                                │
│  z-order（自下而上，后者覆盖前者）                                                │
│   ① chrome webview（标签栏 32px + 地址栏 36px = 68px）                           │
│   ② tab webview × N（y = chrome_height，仅当前 tab 可见）                         │
│   ③ overlay webview（默认 hide，按需定位 show）                                  │
└────────────────────────────────────────────────────────────────────────────────┘
```

**为什么必须有 ③**：子 webview 是原生视图，HTML 内容压不住它。地址栏补全、右键菜单、设置面板如果不走 overlay 层，就会在 68px 的 chrome 区域里被裁掉。这是本项目最容易在第 3 周才发现的坑，**M1 就要跑通原型**（ADR-006）。

### 3.2 并发硬约束（违反即死锁）

1. 所有 IPC 命令 `async`。**禁止**在同步命令或 `on_window_event` 里创建 webview（Windows 上 `add_child` 死锁，官方 Known issues）。
2. `AppState` 用 `RwLock`；**禁止持锁发起 webview 操作或 await**：读快照 → 放锁 → 动作 → 重新加锁提交。
3. webview 变更全部投递到主线程队列（`mpsc` + `run_on_main_thread`）串行执行，避免抢 z-order。
4. 坐标一律 `PhysicalPosition`/`PhysicalSize`，入口由 `scale_factor` 换算。
5. 布局在 Rust 侧 `on_resized`/`on_scale_changed` 重算；前端只上报一次 `chrome_height`。（`auto_resize` 只按比例跟随，无法保持固定顶部偏移，不能替代重算。）

### 3.3 状态模型

```rust
pub struct Tab {
    pub id: TabId,            // "tab-{n}"，同时是 webview label
    pub url: String, pub title: String,
    pub favicon: Option<String>, pub loading: bool,
    pub alive: TabAlive,      // Attached / Suspended { scroll_y }
    pub zoom: f64,
    pub last_active: Instant,
}

pub struct AppState {
    pub tabs: Vec<Tab>, pub selected: Option<TabId>,
    pub settings: Settings, pub chrome_height: f64,
    pub overlay_open: Option<OverlayKind>,
}

pub struct Settings {
    pub restore_session: bool, pub start_url: String,
    pub hotkeys_enabled: bool, pub autostart: bool,
    pub max_live_tabs: usize,     // 默认 12，超出挂起最久未用
    pub download_dir: PathBuf,    // 默认 UserData/downloads
}
```

### 3.4 目录结构（精简版）

```
mix/
├─ src-tauri/
│  ├─ Cargo.toml            # features = ["unstable", "tray-icon"]
│  ├─ tauri.conf.json
│  ├─ capabilities/default.json
│  └─ src/
│     ├─ main.rs  lib.rs
│     ├─ ipc/{commands.rs, events.rs, error.rs}
│     ├─ state.rs  window.rs  tabs.rs  overlay.rs
│     ├─ downloads.rs  permissions.rs  profile.rs
│     ├─ extensions/{mod.rs, crx.rs}
│     ├─ userscripts.rs  shortcuts.rs  snap.rs  tray.rs
│     ├─ platform/{mod.rs, windows.rs, macos.rs, linux.rs}
│     └─ logging.rs         # panic hook + 文件日志 + 诊断包
├─ src/                     # chrome UI（Vite + TS）
│  ├─ index.html  main.ts  tabbar.ts  addressbar.ts  shortcuts.ts  bridge.ts
│  ├─ overlay/{index.html, menus.ts}
│  └─ styles/theme.css      # 默认主题变量（#1F2020 / #00FF00 / r15），可被 theme.json 覆盖
├─ assets/fonts/            # 内置字体子集（用户脚本不再内置，统一放根目录 javascript/）
├─ scripts/build-portable.ps1  # 打 zip，排除 UserData/ crx/ javascript/
├─ m0-probe/                # M0 实验工程（一次性，验证完可删）
├─ docs/{M0-REPORT.md, ADR/}
├─ LICENSE                  # MIT
├─ THIRD-PARTY.md
├─ README-friend.md         # 发给朋友的 5 行说明
└─ .gitignore               # 忽略 target/、node_modules/、UserData/、crx/、javascript/
```

### 3.5 功能清单与优先级

| 能力 | 优先 | 要点 | 平台注意 |
| --- | --- | --- | --- |
| 多标签增删切 + 标题同步 | P0 | `add_child`/`close`、`on_document_title_changed` | — |
| URL 同步 | P0 | 无原生事件：注入 `pushState/replaceState/popstate` + 300 ms 轮询兜底 | — |
| 会话恢复 | P0 | 存 `{url,index,title}`；**历史栈不恢复**，back 置灰并明示 | — |
| 挂起策略 | P0 | `max_live_tabs=12` 超过挂起最久未用；RSS > 1.2 GB 主动挂起 | — |
| 右键菜单 / 地址栏补全 / 设置面板 | P0 | 全部走 overlay 层 | — |
| 键盘快捷键 | P0 | Ctrl+T/W/L/R、F5、Ctrl+Tab、Ctrl+±、F12；页面内由 `initialization_script` 转发到 `shortcut` 命令，chrome 侧同一张映射表本地处理 | 见 §3.9 的 capability 拆分 |
| 下载 | P0 | `on_download`(`DownloadEvent`) → 任务表 + 进度 + 打开目录 | mac/Linux 需自实现 |
| 文件上传 | P0 | `<input type=file>` | WKWebView 需 `runOpenPanel` |
| 权限提示 | P0 | `on_permission_request` → overlay 询问 → 会话内记忆 | 三平台统一 |
| 扩展加载（Win） | P0 | 扫描 `crx/*.crx` → 校验解包 → 加载；增删改提示重启 | 仅 Win（§3.8） |
| 用户脚本（三平台） | P1 | 扫描 `javascript/**/*.user.js`（两层）→ 解析头 → `@match`/`@include`/`@exclude` 编译成正则 → 单个初始化脚本在页面里分发；`@run-at document-start` 立即跑，其余等 DOMContentLoaded | `GM_setValue/getValue` 走带脚本名前缀的 localStorage；脚本在页面上下文执行，无沙箱（P2） |
| 错误页 / 缩放 / 查找 / 全屏视频 | P1 | 见 §3.6 | — |
| 页面崩溃提示 | P1 | 检测 → "重新加载 / 关闭标签" | — |
| 诊断包 | P1 | 日志 + 版本 + 脱敏设置 → zip，一键导出 | 见 §7.3 |
| 主题自定义 | P1 | CSS 变量 + `theme.json`，预设 / 导入 / 导出 | 见 §3.7 |
| 启动顺序 | P0 | **先填 `core.tabs` 再建任何 webview**；chrome ui 也是先挂事件监听、最后再取一次 `tab_list` | 两处都反了会出现"标签栏空着、页面正常"；`ui_log` 命令把 UI 侧的问题写进 `mix.log`，否则前端报错完全不可见 |
| 新窗口 | P0 | `on_new_window` 拦下 `target=_blank` / `window.open` → 发 `ui:open-url` 事件 → chrome 调 `tab_new` | 不接这个处理器时 Tauri **默认拒绝**，表现就是"点了没反应"；不能在这条线程直接建 webview（会死锁） |
| 用户脚本 | P1 | 头解析 → 正则编译 → 一次注入所有脚本，页面内按 URL 选 | **`@match` 的路径片段不能自带首尾锚点**：早期版本把 `^...# Mix · 开发大纲

| 项目 | 内容 |
| --- | --- |
| 文档版本 | **v1.5**（2026-11-02；便携目录 · 主题实时预览 · 开发约定已固化） |
| 定位 | 自己和身边几个人用的多标签浏览器壳；**不对外发布、不上架、不商业分发** |
| 许可 | **MIT**（含 `LICENSE` 与第三方署名，见附录 A） |
| 平台 | **三平台都做**（Win / macOS / Linux）；Windows 为一等公民，其余"尽力而为"，已知缺陷见 §1.1 |
| 技术选型 | Tauri 2（`unstable` 多 webview）；某平台失败时只对该平台局部下沉 wry，不全局重写 |
| 周期 | **三平台 8–9 周（单人）**；不为平台一致性拖周期，小问题按 §1.1 放行 |
| 相关文档 | `DEV-PLAN.v1.0.md`（最初的立项草案，仅存档）· `M0-REPORT.md`（M0 结论） |
| 进度 | **M0–M4 完成 + 用户脚本引擎**（两处待人工确认：上传文件选择框、开机自启开关）· M5 打包管线已跑通（Windows zip 2.56 MB） |

## 变更要点

| # | 变更 | 原因 |
| --- | --- | --- |
| 1 | **删除签名 / 公证 / SmartScreen / Apple 开发者账号** | 不发布，就没有"通过 Gatekeeper 与 SmartScreen"这回事 |
| 2 | **删除自动更新**（updater 服务、`latest.json`、密钥保管） | 自用只需手动替换 exe；省掉一整块永久运维负担 |
| 3 | **删除 CI 四目标矩阵与体积守门** | 只需本地一条构建命令 + 可选 GitHub Actions 跑测试 |
| 4 | **分发方式改为便携 zip** | 解压即用、不写注册表、删了就干净，朋友最省事（ADR-012） |
| 5 | 平台从"三平台齐发（含签名公证）"改为**不发布的自用分发** | 省掉签名/公证/商店后，跨平台变得可行 |
| 6 | 体积从硬指标降为**观察项**；扩展兼容从"10 项对照表"降为"实际在用的 3–5 项" | 自用不做发布门槛 |
| 7 | 新增**诊断包**（日志 + 版本 + 设置打包导出） | 没有遥测，朋友出问题只能靠你隔空取日志 |
| 8 | 合规附录缩为 MIT + FontAwesome 署名 | MIT 自用不承担商业合规义务，但署名义务仍然存在 |
| 9 | i18n、多窗口、标签拖出成窗全部移出范围 | 自用不做 |
| 10 | 平台从"按需裁剪"→ **三平台都做**，质量线为"尽力而为" | 三个平台都有人用；小问题可接受，不为一致性拖周期 |
| 11 | 新增 §1.1 已知可接受缺陷清单 | 显式列出不修的平台差异，避免每次都想"顺手补一下" |
| 12 | 视觉从"沿用"→ **可自定义**（配色/圆角/字号/背景图，JSON 导入导出） | 自用要顺眼，且不想每次改样式都改代码 |
| 13 | 删除 "Cute 按钮" | 暂不需要 |
| 14 | 周期 6–7 周 → **8–9 周** | 摊进 M0 的三环境验证与三套打包 |
| 15 | 数据布局定死：`UserData/` + `crx/` + `javascript/`，默认便携（去掉 `--portable` 开关） | 你要的就是"拷走目录即带走一切" |
| 16 | 用户脚本改为**三平台都支持**（不再只 mac/Linux） | 既然 `javascript/` 一直存在，Windows 上也没理由不加载 |
| 17 | 主题编辑器支持**实时预览**（改即套用，取消还原） | 你的明确要求 |
| 18 | 新增 §13 开发约定（commit 粒度与格式、英文注释、push 归属） | 避免以后反复对齐工作方式 |
| 19 | identifier 定为 `com.mix.browser` | 便携目录已与 identifier 解耦，改它不丢配置 |
| 20 | 落盘 `m0-probe/` 实验工程与 `docs/M0-REPORT.md` 模板 | W1 唯一必须产出的东西 |

---

## 1. 平台与质量线（三平台都做，但不追求一致）

| 平台 | 定位 | 质量线（承诺上限） | 备注 |
| --- | --- | --- | --- |
| **Windows 10/11** | P0 · 一等公民 | 全功能可用 | 多 webview 与 MV3 扩展最成熟的一侧 |
| **macOS 12+** | P1 | 除扩展外全功能可用 | 扩展由用户脚本替代；需 ad-hoc 签名；首次"右键打开" |
| **Linux X11** | P2 | 除扩展外全功能可用 | 停靠与全局热键在 X11 正常 |
| **Linux Wayland** | P2 | 浏览器功能可用，**停靠与全局热键不可用** | 协议限制，不做绕路 |

> 三平台都做，但**不追求一致性**：上表"质量线"就是承诺上限，表外的差异按 §1.1 处理——不修、写清楚、能接受就发。这就是"尽量跨平台 + 小问题不要紧"的落地口径。

### 1.1 已知可接受缺陷（不打算修，写清楚就行）

| 缺陷 | 平台 | 影响 | 为什么不修 |
| --- | --- | --- | --- |
| 停靠 / 全局热键不可用 | Linux Wayland | 少两个系统集成功能 | 协议禁止客户端自定位窗口；全局热键无 portal 支持 |
| 无扩展，只能用户脚本 | macOS / Linux | 装不了 crx | 平台 webview 不提供扩展机制 |
| 未公证：首次要"右键打开"或去掉隔离属性 | macOS | 一次手动操作 | 不做公证（§2.2） |
| 未签名：SmartScreen 提示 + 偶发杀软误报 | Windows | 一次手动放行 | 不做签名（§2.2） |
| 部分站点视频黑屏 | Linux | 少数站点不可看 | 缺 H.264，需系统装 gstreamer 插件；文档提示 |
| PDF 依赖后端 | Linux | PDF 打不开 | WebKitGTK 需额外组件，降级为"用系统浏览器打开" |
| 托盘需 AppIndicator | Linux | GNOME 默认不显示 | 降级为"仅标题栏最小化" |

---

## 2. 范围

### 2.1 做

- 无边框自绘窗口（圆角 / 阴影 / 淡入 / 拖拽 / 8 向 resize）
- **界面可自定义**：配色 / 圆角 / 字号 / chrome 高度 / 背景图，主题 JSON 导入导出，内置 3 套预设（§3.7）
- 多标签浏览（独立 webview、标题/URL 同步、关闭、会话恢复、上限保护、挂起）
- **Windows**：加载本地 MV3 crx 扩展 ／ **macOS·Linux**：用户脚本注入
- 浏览器基线：下载、文件上传、右键菜单、键盘快捷键、查找、缩放、权限提示、全屏视频、错误页
- 托盘、全局停靠热键、单实例、开机自启（可选）、窗口位置记忆
- 便携 zip 分发 + 诊断包导出

### 2.2 不做（明确排除，避免中途加戏）

- 应用签名、公证、SmartScreen、Apple 开发者账号、应用商店
- 自动更新（含 updater 服务、版本清单、密钥）
- 遥测、崩溃上报服务、EULA、隐私政策
- MV2 扩展、扩展 popup UI、DRM（Widevine）、账号同步、书签/历史管理
- 多窗口、标签拖出成窗、i18n（只 zh_CN）
- 主题只做"样式自定义"，不做布局结构自定义（标签栏在顶、地址栏在下的结构固定，见 §3.7）
- "Cute 按钮"（暂不需要，已从 chrome 移除）
- 旧实现复用与旧数据迁移：不复用旧代码、不写旧配置导入器、不继承旧交互（见 §11）

### 2.3 验收口径（自用标准，不是发布标准）

| 指标 | 目标 | 口径 |
| --- | --- | --- |
| 冷启动到可交互 | ≤ 1.5 s | 你的主力机，空 profile，n≥5 取中位数 |
| 切标签 | 无可见白屏 | 10 标签全 attached；P95 ≤ 100 ms |
| 内存 | 5 标签 ≤ 700 MB | 全进程 RSS 之和 |
| 常用站可用 | 你日常 5 个站点正常（含视频、登录、上传） | 手工清单 |
| 朋友装机 | 3 人拿到 zip 后 5 分钟内跑起来 | 一人一份说明 |
| 体积 | 观察项：Win zip ≤ 10 MB 最好 | 只为下载/传输方便，不是门槛 |
| 可迁移 | 整个目录拷到另一台机器，设置/扩展/脚本/会话全部跟随 | 手工：两台机各启动一次 |

---

## 3. 架构

### 3.1 分层与 z-order（核心约束，别改）

```
┌──────────────────────── Tauri 2（unstable 多 webview） ────────────────────────┐
│  Rust 主进程                                                                   │
│  ├─ window      : 无边框窗 / 拖拽 / 最大化 / 圆角 / 8 向 resize                 │
│  ├─ tabs        : TabManager（Vec<Tab>、布局、生命周期、挂起）                   │
│  ├─ overlay     : 弹层 webview（地址栏补全 / 右键菜单 / 设置 / 权限询问）        │
│  ├─ downloads   : on_download → 任务 + 进度 + 打开所在文件夹                    │
│  ├─ permissions : on_permission_request → 询问 → 会话内记忆                     │
│  ├─ profile     : data_directory / 便携模式 / 缓存清理 / 诊断包                  │
│  ├─ extensions  : crx 解包 → 校验 → WebView2 加载（Win only）                   │
│  ├─ userscripts : 脚本库 + loader 生成 + 按 URL/帧分发（mac·Linux 用）           │
│  ├─ shortcuts / snap / tray / store / logging                                   │
│                                                                                │
│  z-order（自下而上，后者覆盖前者）                                                │
│   ① chrome webview（标签栏 32px + 地址栏 36px = 68px）                           │
│   ② tab webview × N（y = chrome_height，仅当前 tab 可见）                         │
│   ③ overlay webview（默认 hide，按需定位 show）                                  │
└────────────────────────────────────────────────────────────────────────────────┘
```

**为什么必须有 ③**：子 webview 是原生视图，HTML 内容压不住它。地址栏补全、右键菜单、设置面板如果不走 overlay 层，就会在 68px 的 chrome 区域里被裁掉。这是本项目最容易在第 3 周才发现的坑，**M1 就要跑通原型**（ADR-006）。

### 3.2 并发硬约束（违反即死锁）

1. 所有 IPC 命令 `async`。**禁止**在同步命令或 `on_window_event` 里创建 webview（Windows 上 `add_child` 死锁，官方 Known issues）。
2. `AppState` 用 `RwLock`；**禁止持锁发起 webview 操作或 await**：读快照 → 放锁 → 动作 → 重新加锁提交。
3. webview 变更全部投递到主线程队列（`mpsc` + `run_on_main_thread`）串行执行，避免抢 z-order。
4. 坐标一律 `PhysicalPosition`/`PhysicalSize`，入口由 `scale_factor` 换算。
5. 布局在 Rust 侧 `on_resized`/`on_scale_changed` 重算；前端只上报一次 `chrome_height`。（`auto_resize` 只按比例跟随，无法保持固定顶部偏移，不能替代重算。）

### 3.3 状态模型

```rust
pub struct Tab {
    pub id: TabId,            // "tab-{n}"，同时是 webview label
    pub url: String, pub title: String,
    pub favicon: Option<String>, pub loading: bool,
    pub alive: TabAlive,      // Attached / Suspended { scroll_y }
    pub zoom: f64,
    pub last_active: Instant,
}

pub struct AppState {
    pub tabs: Vec<Tab>, pub selected: Option<TabId>,
    pub settings: Settings, pub chrome_height: f64,
    pub overlay_open: Option<OverlayKind>,
}

pub struct Settings {
    pub restore_session: bool, pub start_url: String,
    pub hotkeys_enabled: bool, pub autostart: bool,
    pub max_live_tabs: usize,     // 默认 12，超出挂起最久未用
    pub download_dir: PathBuf,    // 默认 UserData/downloads
}
```

### 3.4 目录结构（精简版）

```
mix/
├─ src-tauri/
│  ├─ Cargo.toml            # features = ["unstable", "tray-icon"]
│  ├─ tauri.conf.json
│  ├─ capabilities/default.json
│  └─ src/
│     ├─ main.rs  lib.rs
│     ├─ ipc/{commands.rs, events.rs, error.rs}
│     ├─ state.rs  window.rs  tabs.rs  overlay.rs
│     ├─ downloads.rs  permissions.rs  profile.rs
│     ├─ extensions/{mod.rs, crx.rs}
│     ├─ userscripts.rs  shortcuts.rs  snap.rs  tray.rs
│     ├─ platform/{mod.rs, windows.rs, macos.rs, linux.rs}
│     └─ logging.rs         # panic hook + 文件日志 + 诊断包
├─ src/                     # chrome UI（Vite + TS）
│  ├─ index.html  main.ts  tabbar.ts  addressbar.ts  shortcuts.ts  bridge.ts
│  ├─ overlay/{index.html, menus.ts}
│  └─ styles/theme.css      # 默认主题变量（#1F2020 / #00FF00 / r15），可被 theme.json 覆盖
├─ assets/fonts/            # 内置字体子集（用户脚本不再内置，统一放根目录 javascript/）
├─ scripts/build-portable.ps1  # 打 zip，排除 UserData/ crx/ javascript/
├─ m0-probe/                # M0 实验工程（一次性，验证完可删）
├─ docs/{M0-REPORT.md, ADR/}
├─ LICENSE                  # MIT
├─ THIRD-PARTY.md
├─ README-friend.md         # 发给朋友的 5 行说明
└─ .gitignore               # 忽略 target/、node_modules/、UserData/、crx/、javascript/
```

### 3.5 功能清单与优先级

| 能力 | 优先 | 要点 | 平台注意 |
| --- | --- | --- | --- |
| 多标签增删切 + 标题同步 | P0 | `add_child`/`close`、`on_document_title_changed` | — |
| URL 同步 | P0 | 无原生事件：注入 `pushState/replaceState/popstate` + 300 ms 轮询兜底 | — |
| 会话恢复 | P0 | 存 `{url,index,title}`；**历史栈不恢复**，back 置灰并明示 | — |
| 挂起策略 | P0 | `max_live_tabs=12` 超过挂起最久未用；RSS > 1.2 GB 主动挂起 | — |
| 右键菜单 / 地址栏补全 / 设置面板 | P0 | 全部走 overlay 层 | — |
| 键盘快捷键 | P0 | Ctrl+T/W/L/R、F5、Ctrl+Tab、Ctrl+±、F12；页面内由 `initialization_script` 转发到 `shortcut` 命令，chrome 侧同一张映射表本地处理 | 见 §3.9 的 capability 拆分 |
| 下载 | P0 | `on_download`(`DownloadEvent`) → 任务表 + 进度 + 打开目录 | mac/Linux 需自实现 |
| 文件上传 | P0 | `<input type=file>` | WKWebView 需 `runOpenPanel` |
| 权限提示 | P0 | `on_permission_request` → overlay 询问 → 会话内记忆 | 三平台统一 |
| 扩展加载（Win） | P0 | 扫描 `crx/*.crx` → 校验解包 → 加载；增删改提示重启 | 仅 Win（§3.8） |
| 用户脚本（三平台） | P1 | 扫描 `javascript/**/*.user.js`（两层）→ 解析头 → `@match`/`@include`/`@exclude` 编译成正则 → 单个初始化脚本在页面里分发；`@run-at document-start` 立即跑，其余等 DOMContentLoaded | `GM_setValue/getValue` 走带脚本名前缀的 localStorage；脚本在页面上下文执行，无沙箱（P2） |
| 错误页 / 缩放 / 查找 / 全屏视频 | P1 | 见 §3.6 | — |
| 页面崩溃提示 | P1 | 检测 → "重新加载 / 关闭标签" | — |
| 诊断包 | P1 | 日志 + 版本 + 脱敏设置 → zip，一键导出 | 见 §7.3 |
| 主题自定义 | P1 | CSS 变量 + `theme.json`，预设 / 导入 / 导出 | 见 §3.7 |
 又套了一层，拼出 `[^/]+^/.*`，导致永不匹配（已加回归测试） |
| 位置记忆 | P0 | 窗口 x/y/宽/高/最大化写进 `session.json.window`，关闭到托盘时保存 | **不用 `tauri-plugin-window-state`**：它固定写 `%APPDATA%/<identifier>`，破坏便携承诺；核对尺寸时注意非 DPI-aware 进程（如 PowerShell）读到的 `GetWindowRect` 会被系统缩放 |
| 开机自启 | P1 | `tauri-plugin-autostart`，设置里勾选；默认关 | 勾选后写 HKCU Run 键，是本项目唯一的系统全局写入 |
| 停靠 | P0 | 9 格状态机（半分→四角循环，下行回原状），`Alt+Shift+方向键` 全局热键；注册失败降级到窗口内快捷键 | 恢复位置存 `outer_position` + **`inner_size`**：`set_size` 改的是客户区，存外框尺寸会让窗口每次还原都变大（实测 +15×+8） |
| 托盘 | P0 | 菜单 Show / New tab / Quit；左键单击或双击还原窗口 | "New tab" 走事件让 chrome 调 `tab_new`，托盘线程里直接建 webview 会死锁 |
| 单实例 | P0 | `tauri-plugin-single-instance` 最先注册，第二次启动唤起已有窗口 | 验证：第二个进程 exit 0，日志追加 forwarded |
| 关闭到托盘 | P0 | `CloseRequested` → `api.prevent_close()` + `hide()`；退出走托盘 Quit | 关窗不等于退出，日志会记 "window hidden to tray" |

> 前端构建：**M1 阶段是静态 ESM**（`src/*.js` 由 Tauri 直接托管，零构建）；等组件变多、需要类型与打包再引 Vite + TS（§3.4 的目标形态），不提前上工具链。

> E2 实测踩到的两个坑，写在这里省一次调试：① `WebviewUrl::App("x.html?q=1")` 会丢掉 query，传参用 fragment 或初始化脚本；② 子 webview 不会跟随窗口 resize（`auto_resize` 只按比例缩放、保持不了固定顶部偏移），必须自己在 `on_resized` 重算。

### 3.6 其它小项

- 错误页：本地 HTML，含 URL 与"重试"。
- 缩放：`Webview::set_zoom` + 按 tab 持久化。**页面内 Ctrl+滚轮默认是关的**，建 webview 时必须 `zoom_hotkeys_enabled(true)`（E2 实测）；chrome 侧快捷键走 `set_zoom`，两条路径读写同一个 zoom 值，避免来回跳变。
- 查找：注入 `window.find()` + 高亮（各平台表现不一，失败就给提示）。
- 全屏视频：进入时 hide chrome、tab 铺满；退出恢复。与停靠逻辑互斥。
- 最后标签关闭 → 打开新标签页，不退出。
- 无边框窗口的 resize：M1 只有"顶部 + 上半侧"热区（`start_resize_dragging`）。**底部与左右下半段需要原生命中测试**（Win `WM_NCHITTEST`、macOS `NSWindow` 边距），列入 M2，见 §3.9。

### 3.7 主题与自定义

目标：改样式不改代码、重启后保持、能导出发给别人。

| 项 | 方式 |
| --- | --- |
| 存储 | `<data>/theme.json`（便携模式在 `<exe>/data`），带 `schema_version` |
| schema | `{ colors: { bg, chrome, accent, text, border }, radius, font_size, chrome_height, background_image?, opacity }` |
| 渲染 | Rust 读出 → 前端写入 `:root` 的 CSS 变量（`--bg` / `--accent` / `--radius` …） |
| 实时预览 | 编辑器里每改一个值 → `theme_apply {persist:false}` → chrome 与 overlay 立刻套用；"取消"调 `theme_revert` 回到进入编辑器前的快照；"保存"才落盘 |
| 预设 | 荧光绿（默认，`#1F2020 / #00FF00 / r15`）、暗灰、亮白 |
| 导入导出 | `theme_export` 出 JSON；`theme_import` 读入。非法值一律回落默认并提示，不让一个手滑的 `#GGG` 把窗口变透明 |
| 背景图 | 仅 chrome 区域；存 `<data>/theme/bg.<ext>`，限制 ≤ 2 MB、宽 ≤ 4096；缺失自动回落纯色 |
| 不做 | 布局结构（标签栏位置、按钮顺序）不开放自定义——那是把 UI 改崩的最短路径（ADR-014） |

### 3.8 目录布局与路径解析（一等约束）

一切可写数据都在应用根目录的 `UserData/`；扩展与脚本是"用户投放"目录，独立在根目录，方便你自己用 git / 网盘同步和排查。

```
<应用根目录 AppRoot>/
├─ Mix.exe / Mix.app / Mix.AppImage
├─ UserData/               # 程序数据；删了等于重置（不会丢 crx/ 与 javascript/）
│  ├─ profile/             # webview user data：缓存 / Cookie / localStorage
│  ├─ settings.json
│  ├─ session.json
│  ├─ theme.json
│  ├─ theme/bg.<ext>
│  ├─ extensions/<id>/     # crx 解包产物（缓存，可随时重建）
│  ├─ downloads/           # 默认下载目录（设置可改）
│  ├─ logs/                # 滚动 10 MB
│  └─ diag/                # 诊断包输出
├─ crx/                    # 放 *.crx，启动时扫描
└─ javascript/             # 放 *.user.js，启动时扫描（递归两层）
```

**AppRoot 怎么定**

| 平台 | AppRoot |
| --- | --- |
| Windows | exe 所在目录 |
| macOS | `.app` 的父目录（**不是** bundle 内部，内部不可写且会破签名） |
| Linux | `$APPIMAGE` 所在目录；非 AppImage 时为可执行文件所在目录 |

**规则**

- 首次启动创建缺失目录，并在 `crx/`、`javascript/` 各写一份 `README.txt`（格式 + 生效方式 + 例子），别让人猜。
- `settings.json` / `theme.json` / `session.json` 都带 `schema_version`：读到旧版本先跑迁移函数再写回；迁移失败把原文件备份成 `.bak` 并回落默认（自用也一定会遇到旧配置，别省这一步）。
- `UserData/` 不可写（装在 Program Files、只读介质）→ 回落到平台数据目录（`%LOCALAPPDATA%/Mix`、`~/Library/Application Support/Mix`、`~/.local/share/mix`），窗口内提示一次；`crx/`、`javascript/` 同步回落。
- 扩展：扫描 `crx/*.crx` → 校验解包到 `UserData/extensions/<id>/`；按 mtime 比对增删改 → 提示"扩展已变更，重启生效"。删除 crx 文件 = 下次启动卸载。
- 脚本：扫描 `javascript/**/*.user.js` → 解析头信息建索引；解析失败在设置页列红字，**不静默忽略**。三平台都支持（Windows 上与扩展并存）。
- 程序自身不写任何系统全局位置。唯一例外：**勾选开机自启**时才写 HKCU Run 键（Win）/ LaunchAgent（mac）。
- 升级方式：**只替换可执行文件**，不要整包覆盖；打包脚本排除 `UserData/`、`crx/`、`javascript/`。
- 别把 `UserData/profile/` 放进 OneDrive / 坚果云同步目录：WebView2 与 WKWebView 的 profile 有锁文件，同步工具会把它改坏（§9-R10）。

---

### 3.9 已知实现取舍（M1）

| 项 | 现状 | 备注 |
| --- | --- | --- |
| 扩展 | 扫描 `crx/` → 解包到 `UserData/extensions/` → MV3 校验 → WebView2 加载 | 仅 Windows；V2 标记 `skipped` 并提示转用户脚本；状态由 `ext_list` 暴露，UI 列表放 M4 |
| 便携 profile | 所有 webview 的 `data_directory` 指向 `UserData/profile`（chrome/overlay 用 `profile-ui`） | 扩展选项必须整环境一致，故 UI 层单独一个 profile |
| 下载 | `on_download` → 落 `UserData/downloads`，优先用服务端建议文件名，重名自动加序号，chrome 显示药丸并可打开所在目录 | 无进度百分比（wry 只给 Requested/Finished） |
| 快捷键 | 页面内脚本转发到 `shortcut`；命令被拆成两张 capability，远程页面只允许这一条命令 | 远程页面可触发标签/缩放类操作（不涉及文件）；URLPattern **必须写端口通配** `https://*:*/*`，否则非默认端口全部拒绝 |
| 权限 | 页面脚本包装 `getUserMedia` / `geolocation` / `Notification`，先问 overlay 再调原生 API；决定按 `origin\|kind` 记在内存 | 不阻塞 WebView2 事件线程（在 JS 层拦截，而不是 Rust 回调里同步等）；120 秒无人应答按拒绝处理；只覆盖这三类 API，其他权限仍走平台默认 |
| 右键菜单 | 页面 `contextmenu` 上报（链接/图片/选中文本/可编辑）→ overlay 渲染自绘菜单；页面 `mousedown` 在菜单显示后 8 秒内上报 `dismiss` 关闭 | 原生菜单被 `preventDefault` 屏蔽；overlay 用**唯一 label**（`overlay-N`），否则"关旧开新"会因 label 冲突失败 |
| 错误页 | 不自绘：WebView2 自带错误页 | mac/Linux 需自绘，列 v1.1 |
| 多标签 | 支持新建/切换/关闭，标题实时同步 | 只隐藏不卸载，选中才可见 |
| URL 同步 | 400 ms 轮询 `Webview::url()`，只轮询已挂载的标签 | 延迟可感知但不打断输入（地址栏聚焦时不覆盖） |
| 会话恢复 | 退出（`CloseRequested`）与结构变更时写 `session.json`，启动按序重建 | **历史栈不恢复**，back 置灰（未实现 UI，仅数据层） |
| 挂起 | `max_live_tabs`（默认 12）超出时挂起最久未用；选中挂起标签时按 URL 重建 | 挂起标签在标签栏置灰斜体，失去页面状态 |
| resize | Windows 全窗口 8 向（`WM_NCHITTEST` 子类化）；mac/Linux 仍只有 chrome 条热区 | mac 需 `NSView` 命中覆写、Linux 需 GTK 边界，列在 v1.1 |
| 前端构建 | 静态 ESM，无打包 | 组件变多再上 Vite |
| overlay | 每次打开重建，label 递增（`overlay-N`），当前 label 记在 `AppState.overlay` | 主题广播也走这个 label，别硬编码 "overlay" |

---

## 4. M0 技术验证（1.5 周，仍是第一优先级）

> 验证的是"这套架构在三平台能不能跑"。E2/E4/E6/E7 必须在 Win / mac / X11 / Wayland 四环境各跑一次——平台差异必须在第 1 周暴露，而不是最后一周。

| # | 实验 | 通过标准 | 不通过时 |
| --- | --- | --- | --- |
| **E1** | 解包真实 crx 并加载，**验证 DNR 真拦截** | uBlock Origin Lite 在测试页拦截计数 > 0；content script 能读 `chrome.runtime.id` | 扩展降级为"辅助"；去广告改用用户脚本 cosmetic 过滤（自用可接受） |
| **E2** | 同窗口 `add_child` 2 个 webview | 互不干扰，滚动/输入/焦点正常，resize 跟随 | 该平台局部切 wry 或 N 窗口方案；不动全局 |
| **E4** | overlay webview 浮于 tab 之上 | 能覆盖页面、焦点正确、无闪烁 | 降级原生 Menu 或遮挡期 hide tab |
| **E5** | 下载 + 文件上传 | 下载事件落盘；`<input type=file>` 可选文件 | 对应平台补自实现 |
| **E6** | IME：两 webview 切换后中文输入 | 不丢字、候选框位置正确 | 严重阻碍，需立刻评估替代方案 |
| **E7** | 混合 DPI：100% + 150% 双屏拖动 | 子 webview 无错位 | 统一物理像素 + 重算；仍失败则评估降级 |
| E3 | `initialization_script_for_all_frames` 在 iframe 内执行 | 早于页面脚本 | `eval` + 导航钩子补丁（仅做脚本注入时才需要） |
| E9 | 托盘 / 全局热键 / 窗口自定位 | Windows 必通；其他平台只记录结论 | 写入平台矩阵并禁用该功能组 |

**M0 产出**：`m0-probe` 工程 + `docs/M0-REPORT.md`（环境 / 步骤 / 结论 / 证据 / 触发动作）。

用法：`cd m0-probe && cargo run -- --exp e2`（可选 e1 e2 e3 e4 e5 e6 e7 e9）；每个实验都会带上同一块 68px chrome 条，保证与真实布局一致。E1 需要先把解包好的 MV3 扩展放到 `m0-probe/extensions/`。

---

## 5. 里程碑与周排期（三平台 8–9 周）

| # | 里程碑 | 周期 | DoD |
| --- | --- | --- | --- |
| **M0** | 技术验证（三环境） | 1.5 周 | §4 全绿或已定降级方案，架构冻结；三平台各出一个能跑的空壳 |
| **M1** | 骨架 + overlay + 主题 | 1.5 周 | 无边框窗 + chrome + 1 tab 能上网 + 拖拽/最大化正常 + 弹层可浮于页面 + 主题切换即时生效 |
| **M2** | 多标签 | 2 周 | 10 标签稳定切换；布局跟随 resize 与混合 DPI；标题/URL 同步；会话恢复；挂起可用 |
| **M3** | 扩展 + 脚本 + 基线能力 | 1.5 周 | Win 上 uBO Lite 真拦截；三平台用户脚本按 `@match` 分发；右键菜单/快捷键/下载/上传/权限/错误页可用 |
| **M4** | 系统集成 | 1 周 | 托盘/热键/停靠/单实例/自启/位置记忆；Wayland 按 §1.1 降级且提示正确 |
| **M5** | 三平台打包 + 朋友试用 | 1.5 周 | 三平台各一份便携包；3 人装机成功；诊断包能定位一次真实问题 |

| 周 | 内容 |
| --- | --- |
| W1–W2 上 | M0：四环境跑 E1/E2/E4/E5/E6/E7，定下各平台降级清单 |
| W2 下 | M1：骨架 + overlay 原型 + 主题 JSON 打通 |
| W3–W4 | M2 多标签全量（会话恢复、挂起、URL 同步） |
| W5 | M3 扩展（Win）+ 用户脚本（三平台）+ 基线能力 |
| W6 | M4 系统集成 + Wayland 降级验证 |
| W7 | M5 三平台打包（便携 zip / .app / AppImage）+ 发给朋友 |
| W8 | 按朋友反馈修 + 补齐 §1.1 的说明文档（缓冲周） |

> 开发主线始终是 Windows，mac/Linux 在 W2、W5、W7 各插一次"能不能跑起来"的验证（每次半天）。平台差异在最便宜的时刻暴露，而不是最后一周一次性爆。

---

## 6. IPC 契约

统一：命令全 `async`；返回 `Result<T, AppError { code, message }>`，前端 `bridge.ts` 统一 toast；事件名集中在 `ipc/events.rs`；`MIX_MOCK=1` 时走 mock，供前端 Playwright 复用。

| 命令 | 参数 | 说明 |
| --- | --- | --- |
| `tab_list` | — | 启动/恢复 |
| `tab_create` | `{url?, index?, activate?}` | **必须 async** |
| `tab_close` / `tab_select` | `{id}` | 先 `close()` 再删状态；挂起标签先重建 |
| `tab_navigate` | `{id, url}` | URL 先归一化（补 scheme / 搜索词判定） |
| `tab_nav` | `{id, action}` | `back`/`forward` 只能 `eval("history.back()")`；`reload` 用 `Webview::reload`；`stop` 用 `eval` |
| `tab_suspend` / `tab_zoom` / `tab_devtools` | `{id, …}` | devtools 仅 debug 构建 |
| `layout_report` | `{chrome_height}` | 只在 chrome 高度变化时上报 |
| `overlay_open` / `overlay_close` | `{kind, anchorRect}` | `autocomplete`/`context_menu`/`settings`/`permission` |
| `window_control` | `{action}` | min / max / close |
| `download_list` / `download_open_dir` | — / `{id}` | |
| `permission_reply` | `{id, allow, persist}` | |
| `ext_list` / `ext_reload` | — / `{id?}` | 非 Windows 返回 `Unsupported` |
| `script_list` / `script_reload` | — | |
| `theme_get` | — | 读当前生效主题 |
| `theme_apply` | `{theme, persist}` | `persist:false` 即实时预览；广播 `theme:changed` |
| `theme_revert` | — | 回到进入编辑器前的快照 |
| `theme_import` / `theme_export` | `{path?}` | JSON 导入导出；非法值回落默认 |
| `ext_scan` / `script_scan` | — | 重扫 `crx/` 与 `javascript/`，返回变更摘要 |
| `paths_get` / `open_dir` | — / `{which}` | 返回 AppRoot/UserData/crx/javascript；`which` 打开对应目录 |
| `settings_get` / `settings_set` | — / `Settings` | |
| `profile_clear_cache` / `profile_diag_export` | — | 诊断包导出到桌面 |

事件：`tab:created|closed|selected|title|url|favicon|loading|failed|suspended|resumed` · `overlay:closed` · `window:maximized` · `download:started|progress|finished` · `permission:request` · `ext:loaded|error` · `theme:changed` · `app:ready`

---

## 7. 构建与分发（自用版，替代原 CI/CD）

### 7.1 构建

```bash
cargo tauri build                      # 本地出 exe
pwsh scripts/build-portable.ps1        # 打便携 zip：exe + 图标 + README + 版本信息
```

- 三平台产物：Win 便携 zip ／ macOS `.app`（**ad-hoc 签名** `codesign -s - --deep`，否则 Apple Silicon 可能直接崩）+ 可选 dmg ／ Linux AppImage（或 tar.gz）。
- 不打安装包、不做 Windows 签名、不做公证、不配 updater。
- zip 内容 = 可执行文件 + 图标 + `README-朋友版.md` + 空的 `crx/`、`javascript/`；**排除** `UserData/`、`crx/`、`javascript/` 里已有的用户内容。
- 可选：GitHub Actions 只跑 `cargo test` + `clippy`（不需要 matrix、不需要产物）。

### 7.2 便携目录（布局见 §3.8）

- 默认就落在 AppRoot 下，**没有 `--portable` 开关**：解压即用、删目录即卸载、不写注册表（自启除外）。
- 朋友用法：解压 → 双击 → 完。要备份/迁移：整个目录拷走即可，"带走设置、扩展、脚本"三件套一次完成。
- 只读位置：检测 `UserData/` 不可写 → 回落平台数据目录并提示一次（不要静默换地方，否则用户找不到自己的配置）。
- 注意：Windows 需检测 WebView2 Runtime（Win10 1803+ / Win11 通常已内置），缺失时弹提示并给官方下载链接，**不捆绑**。

### 7.3 没有遥测，就要有诊断包

`设置 → 导出诊断包` → `UserData/diag/Mix-diag-<版本>-<时间>.zip`，内含：日志（最近 10 MB）、版本号 + git commit、脱敏后的 `settings.json` / `theme.json` / `session.json`、`paths_get` 的目录清单、扩展与脚本的扫描结果（含失败原因）、OS / WebView2 / WebKitGTK 版本、当前标签 URL 列表。

**不含**：`profile/`（Cookie、缓存）与任何页面内容——既不隐私安全，也会把 zip 撑到几百 MB。

### 7.4 分发给朋友（未签名，会遇到的两件事）

| 平台 | 现象 | 说明文档里怎么写 |
| --- | --- | --- |
| Windows | SmartScreen 提示"未知发布者"；个别杀软（360/火绒）可能误报 | 点"更多信息 → 仍要运行"；被杀软拦就加信任。**说明里写清楚这是自编译版本** |
| macOS | 未公证：首次"右键 → 打开"或 `xattr -d com.apple.quarantine Mix.app`；Apple Silicon 至少要 ad-hoc 签名（`codesign -s - --deep Mix.app`，否则可能直接崩） | 给出这两条命令，别让人猜 |
| 两平台共通 | 首次启动在程序旁生成 `UserData/`、`crx/`、`javascript/`；若放在只读位置会回落到系统数据目录 | 说明里写一句"配置在程序目录的 UserData 里"，避免"我的设置去哪了" |

> 这一节是自用项目的真实成本所在：省下的签名费用，会以"每人一次手动放行"的形式还给朋友。写一份 `README-朋友版.md`（5 行）比任何自动化都值。

### 7.5 版本一致性

自用没有更新通道，"我这是旧版还是新版"必然会成为排查噪音：`关于` 页显示 `版本 v0.3.1 (git a1b2c3d, 2026-11-02)`，让朋友报 bug 时截图即可。

---

## 8. 测试

| 层级 | 内容 | 工具 |
| --- | --- | --- |
| 单元 | crx 解包（zip-slip / 炸弹）、`@match` 匹配（proptest）、停靠坐标、会话序列化、URL 归一化 | `cargo test` + `proptest` |
| 前端 | chrome UI 组件、快捷键表、主题快照（Playwright 打开 dev 页面 + mock bridge，不用真机） | Playwright |
| 手工冒烟 | 下面 10 条，每改一次跑一遍 | 自己 |
| 真机试用 | 3 个朋友各用一周 | 人工 |

冒烟 10 条：① 冷启动 ≤1.5 s ② 10 标签切换不白屏 ③ 拖拽/双击/最大化/还原 + 跨 DPI 屏拖动不错位 ④ 缩到最小不崩 ⑤ 托盘图标与恢复 ⑥ 停靠四方向 ⑦ 下载 + 上传 + 权限弹层 + 全屏视频 ⑧ 退出重启恢复会话（back 置灰） ⑨ 改主题（配色/圆角/背景图）→ 重启保持，非法 JSON 自动回落；实时预览与"取消还原"正常 ⑩ 首次启动生成 `UserData/ crx/ javascript/` 与 README；整个目录拷到另一台机器后设置/扩展/脚本全部跟随

---

## 9. 风险（自用视角）

| # | 风险 | 影响 | 缓解 |
| --- | --- | --- | --- |
| R1 | `unstable` 多 webview 不可用或 API 变更 | 高 | M0 E2 先验；抽 `TabHost` trait；只对失败平台局部下沉 wry |
| R2 | overlay 层调不出"压住页面且焦点正常" | 高 | M1 出原型；降级原生 Menu |
| R3 | MV3 扩展拦截效果差（uBO Lite 规则受限） | 中 | 自用接受；补充用户脚本做 cosmetic 过滤 |
| R4 | 未签名导致朋友装不上/被杀软拦 | 中 | §7.4 说明文档 + 首次远程协助一次 |
| R5 | 混合 DPI / IME 在多 webview 下异常 | 中 | M0 E6/E7；坐标全物理像素 |
| R6 | 内存随标签数膨胀 | 中 | §3.5 挂起策略 + RSS 采样 |
| R7 | 出 bug 时拿不到环境信息 | 中 | §7.3 诊断包（这一条是自用项目最容易忽略的） |
| R8 | 单人 8 周被"顺手再加个功能"拖长 | 中 | §2.2 不做清单当红线；新想法一律进"v2 想法池"不进当期 |
| R9 | 自定义主题把自己改瞎（对比度低、字看不清） | 低 | 3 套预设兜底 + 非法值回落 + "恢复默认"按钮 |
| R10 | `UserData/` 被 OneDrive/坚果云同步 → webview profile 锁文件冲突、配置被改坏 | 中 | 文档明示"别把 profile 放同步盘"；启动时检测路径含常见同步盘关键字并提示；可把 `profile/` 挪到系统目录（设置项） |

---

## 10. 总验收（自用口径）

- [ ] zip 解压即用：数据落在程序旁的 `UserData/`；删目录无残留（不写注册表，自启除外）
- [ ] 冷启动 ≤ 1.5 s；5 标签内存 ≤ 700 MB；10 标签切换无白屏
- [ ] 扩展：uBO Lite 在测试页拦截计数 > 0，日常站点可用
- [ ] 右键菜单/地址补全/设置面板/权限询问都在页面之上正常显示（overlay 生效）
- [ ] 快捷键在"焦点在页面内"时也生效（Ctrl+T/W/R/F/±、F5、F12）