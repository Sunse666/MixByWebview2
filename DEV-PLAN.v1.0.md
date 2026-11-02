# Mix 跨平台重构 · 开发大纲

| 项目 | 内容 |
| --- | --- |
| 文档版本 | v1.0 |
| 状态 | 草案 |
| 目标 | 一套代码覆盖 Win10+ / macOS 12+ / Linux，安装包 ≤ 10 MB |
| 技术选型 | **Tauri 2**（启用 unstable 多 webview），遇到 API 阻塞时下沉 **wry** |
| 相关文档 | 无（本文件为立项文档） |

---

## 目录

1. 范围定义
2. 架构
3. 里程碑
4. 任务分解
5. IPC 契约
6. 测试策略
7. CI/CD
8. 风险登记表
9. 迁移映射
10. 每周节奏建议
11. 总验收标准
12. 附录 A：M0 验证工程说明
13. 附录 B：关键技术决策记录

---

## 1. 范围定义（必须先锁死，否则无限膨胀）

### 1.1 做

- 无边框自绘窗口（圆角 / 阴影 / 淡入 / 拖拽 / 8 向 resize）
- 多标签浏览（独立 webview、标题同步、关闭、会话恢复）
- **Windows**：加载本地 MV3 crx 扩展
- **macOS / Linux**：用户脚本（userscript）注入
- 托盘、全局停靠热键、单实例、开机自启
- 三平台安装包 + 自动更新

### 1.2 不做（明确排除）

- MV2 扩展、Chrome 商店扩展、扩展的 popup UI
- DRM 视频（Widevine）、浏览器账号同步、书签/历史管理
- 多窗口（只做单窗口多标签）
- 标签页进程隔离级崩溃恢复（P2）
- 移动端

### 1.3 验收硬指标

| 指标 | 目标 |
| --- | --- |
| 安装包体积 | Win ≤ 7 MB，mac ≤ 12 MB，Linux ≤ 15 MB |
| 冷启动到可交互 | ≤ 1.5 s（SSD，空 profile） |
| 单标签内存 | ≤ 120 MB |
| 开 5 个标签切换 | 无可见白屏（≤ 100 ms） |
| Win 扩展加载成功率 | 自测的 MV3 crx ≥ 90% 能跑 content script |

---

## 2. 架构

### 2.1 分层

```
┌──────────────────────────── Tauri 2 ────────────────────────────┐
│  Rust 主进程（tauri-plugin-*）                                    │
│  ├─ window      : 无边框窗、拖拽、最大化状态、圆角                  │
│  ├─ tabs        : TabManager（Vec<Tab>，布局计算，生命周期）        │
│  ├─ profile     : data_directory，单例持久化 session              │
│  ├─ extensions  : crx 解包 → 校验 → WebView2 加载（Win only）      │
│  ├─ userscripts : 脚本库 + loader 生成 + 分发（三平台）             │
│  ├─ shortcuts   : 全局热键 + 停靠状态机                            │
│  ├─ tray        : TrayIconBuilder                                  │
│  └─ store       : 设置/会话 JSON                                    │
│                                                                   │
│  ┌── chrome webview（窗口自身那个） ──┐   ┌─ tab webview #1 ─┐      │
│  │  只画 32px 标签栏 + 地址栏（TS）    │   │  独立 WebView2/ │      │
│  │  通过 invoke/event 与 Rust 通信     │   │  WKWebView/     │  ... │
│  └────────────────────────────────┘   │  WebKitGTK      │      │
└───────────────────────────────────────────────────────────────────┘
```

关键点：**chrome UI 自己也是一个 webview**，tab 是同一窗口下的子 webview，用 `add_child` 叠在 chrome 下方（`y = 标题栏高度`）。这复刻了现有 `win.ui.tabs` + 每 tab 一个 `web.view` 的结构。

### 2.2 目录结构

```
mix/
├─ src-tauri/
│  ├─ Cargo.toml                 # features = ["unstable", "tray-icon"]
│  ├─ tauri.conf.json
│  ├─ capabilities/default.json  # 最小权限
│  ├─ build.rs
│  ├─ src/
│  │  ├─ main.rs                 # 启动、插件注册、单实例
│  │  ├─ commands.rs             # 所有 IPC 命令（见 §5）
│  │  ├─ state.rs                # AppState { tabs, settings, profile }
│  │  ├─ platform/               # 各平台差异隔离层
│  │  │  ├─ mod.rs               # trait PlatformHooks
│  │  │  ├─ windows.rs
│  │  │  ├─ macos.rs
│  │  │  └─ linux.rs
│  │  ├─ window.rs               # 无边框、拖拽、resize 热区、圆角
│  │  ├─ tabs.rs                 # TabManager + 布局
│  │  ├─ extensions/
│  │  │  ├─ mod.rs               # Windows: 加载；其他: Unsupported
│  │  │  └─ crx.rs               # CRX2/3 解包
│  │  ├─ userscripts.rs          # 扫描、@match 解析、loader 拼装
│  │  ├─ shortcuts.rs            # 热键注册 + 失败降级
│  │  ├─ snap.rs                 # 停靠状态机
│  │  ├─ tray.rs
│  │  └─ logging.rs              # panic hook + 文件日志
│  └─ icons/
├─ src/                          # chrome UI（Vite + TS，无框架或 Svelte）
│  ├─ index.html
│  ├─ styles/theme.css           # #1F2020 / #00FF00 / radius 15
│  ├─ main.ts
│  ├─ tabbar.ts
│  ├─ addressbar.ts
│  └─ bridge.ts                  # 封装 invoke/event，类型安全
├─ assets/
│  ├─ fonts/fa-solid-subset.woff2
│  └─ userscripts/               # *.user.js
├─ docs/
│  ├─ DEV-PLAN.md                # 本文件
│  └─ ADR/                       # 架构决策记录
├─ .github/workflows/release.yml
└─ package.json
```

### 2.3 核心状态模型

```rust
pub struct Tab {
    pub id: TabId,              // "tab1"、"tab2"…，同时是 webview label
    pub url: String,
    pub title: String,
    pub favicon: Option<String>,
    pub loading: bool,
    pub created_at: u64,
}

pub struct AppState {
    pub tabs: Vec<Tab>,
    pub selected: Option<TabId>,
    pub settings: Settings,
    pub chrome_height: f64,     // 由前端 layout_update 上报
}

pub struct Settings {
    pub restore_session: bool,
    pub start_url: String,
    pub hotkeys_enabled: bool,
    pub autostart: bool,
    pub theme: Theme,
}
```

---

## 3. 里程碑

| # | 里程碑 | 周期 | DoD（完成判定） |
| --- | --- | --- | --- |
| **M0** | 选型验证 | 3 天 | 4 项实验全绿（见附录 A），否则切 wry |
| **M1** | 骨架跑起来 | 1 周 | 无边框窗 + 荧光绿标题栏 + 1 个 tab 能上网 + 拖拽/最大化正常 |
| **M2** | 多标签 | 1.5 周 | 开/关/切 10 个标签不崩，布局跟随 resize，标题同步 |
| **M3** | Windows 扩展 | 1 周 | 3 个真实 MV3 crx 加载成功，失败有可见提示 |
| **M4** | 脚本引擎 | 1 周 | macOS/Linux 上 @match 分发正确，iframe 内生效 |
| **M5** | 系统集成 | 1 周 | 托盘/热键/停靠/单实例/开机自启，Wayland 降级不崩 |
| **M6** | 打包发布 | 1.5 周 | 三平台安装包产出，Win 签名过 SmartScreen，mac 公证成功 |
| **M7** | 兼容性回归 | 1 周 | 三平台 smoke 全绿 + 体积/启动基线达标 |
| — | 缓冲 | 1 周 | — |

**单人总计约 9 周**；两人并行（Rust / 前端 + CI）可压到 5–6 周。

---

## 4. 任务分解

### 4.1 M0 选型验证（3 天，先做，失败就换路线）

| # | 实验 | 通过标准 |
| --- | --- | --- |
| 1 | 解包一个真实 crx，`browser_extensions_enabled(true).extensions_path(dir)` 加载 | content script 在目标页生效，能读到 `chrome.runtime.id` |
| 2 | 同窗口 `add_child` 2 个 webview，各自导航 | 互不干扰，滚动/输入/焦点正常，resize 跟随 |
| 3 | `initialization_script` 在 iframe 内也执行（用 `initialization_script_for_all_frames`） | 执行早于页面脚本 |
| 4 | Linux（Wayland + X11 各一台）跑通 1–3 | 不崩；热键失败有降级路径 |

> 判据：1、2 任一失败 → 直接改 wry（API 平移，成本约 1 天），别再调。

### 4.2 窗口与自绘 chrome（M1）

- [ ] `decorations: false` + `transparent: false`（Windows 上 `transparent:true` 会拖慢拖动和 resize）
- [ ] 标题栏 32px：左上图标、min/max/close/Cute 四个按钮（FontAwesome 子集）
- [ ] 拖拽：CSS `-webkit-app-region: drag`（三平台都已支持），按钮区设 `no-drag`
- [ ] 双击标题栏最大化 / 还原；最大化时隐藏圆角
- [ ] 圆角：Windows 11 用系统原生；其他平台 CSS `border-radius` + `overflow:hidden`（Linux 合成器可能吃掉阴影，要有降级）
- [ ] resize：`appWindow.startResizeDragging(方向)` 挂 8 个 4px 热区
- [ ] `minWidth/minHeight` = 500×600（对应原 `win.ui.minmax`）
- [ ] 淡入：CSS `opacity` 过渡 120 ms（对应原 `win.ui.fade`）
- [ ] 无边框窗的 DPI 变化处理（`onScaleChanged` 重算布局）

### 4.3 标签管理器（M2）

- [ ] `Tab` 结构体 + `TabManager`（增删查、布局缓存）
- [ ] `add_child` 创建/销毁，`y = chrome_height`，宽高随窗口 `onResized` 重算
- [ ] 标题同步：`on_document_title_changed` → emit `tab:title`
- [ ] 关闭：先 `close` 子 webview 再删状态，防止句柄泄漏
- [ ] 会话恢复：退出时存 `[{url, index}]`，启动恢复（`tauri-plugin-store`）
- [ ] 上限保护：> 30 个标签时提示或暂停后台标签（`background_throttling`）
- [ ] 新窗口拦截：`on_new_window_req_handler` → 转为新标签（原代码缺这一环）
- [ ] P2：拖拽排序、标签休眠

### 4.4 Windows 扩展（M3）

- [ ] `crx.rs`：CRX2/CRX3 解包（`Cr24` 魔数 + 头长度 + zip payload）
- [ ] 校验：`manifest.json` 存在、`manifest_version == 3`（v2 直接拒绝并提示）
- [ ] 加载：`browser_extensions_enabled(true)` + `extensions_path(<userData>/extensions/<id>)`
- [ ] 多扩展：逐个解包到子目录；或用 `additional_browser_args("--disable-extensions-except=a;b --load-extension=a;b")` 一次性验证
- [ ] 结果反馈：加载后列扩展 + 失败原因（WebView2 无 chrome://extensions 页面，靠 `chrome.runtime.id` 探测）
- [ ] 非 Windows：`#[cfg(not(windows))]` 分支返回 `Unsupported`，UI 显示"当前平台不支持扩展，建议转换为用户脚本"
- [ ] 安全：只从 `<userData>/extensions` 白名单目录加载，路径规范化，拒绝 `..`

### 4.5 用户脚本引擎（M4）

- [ ] 解析 `// ==UserScript==` 头：`@match`、`@include`、`@exclude`、`@run-at`
- [ ] `@match` 通配符 → 正则（`*`、`*.`、`<all_urls>`）
- [ ] 生成 loader（全局注入一次）：按 URL 分发、try/catch、按 `@run-at` 排序
- [ ] `@run-at document-start` 走 `initialization_script`；`document-end` 走 loader 内 `DOMContentLoaded`
- [ ] 隔离：脚本运行在页面上下文（沙箱模式 P2）
- [ ] `GM_*` 子集：`GM_setValue/getValue`（用 `localStorage` 前缀或 Rust 端存储）
- [ ] 热重载：开发期监听目录变化，重建 loader（生产环境重启生效）

### 4.6 系统集成（M5）

- [ ] 托盘：`TrayIconBuilder` + 菜单（打开/新建标签/退出）；Linux 依赖 AppIndicator，缺失时降级为"仅标题栏最小化"
- [ ] 最小化到托盘：`on_window_event` 拦 `CloseRequested` / `Minimized` → `hide()` + 托盘提示
- [ ] 热键：`tauri-plugin-global-shortcut`，注册失败（Wayland/被占用）→ 设置页提示"当前环境不支持全局热键，已降级为窗口内快捷键"
- [ ] 停靠：移植原 `main.aardio:301-377` 状态机为表格驱动
- [ ] 用 `current_monitor()` + `work_area` 计算，**用宽高不用 right/bottom**（修掉原代码的坑）
- [ ] 防抖 200 ms（替代原代码的 `win.delay(100)`）
- [ ] 单实例：`tauri-plugin-single-instance`，第二次启动唤起已有窗口
- [ ] 开机自启：`tauri-plugin-autostart`（默认关）
- [ ] 窗口位置记忆：`tauri-plugin-window-state`

```rust
// (winx, winy) -> (x, y, w, h) 比例；替代原代码的 if/elseif 嵌套
const CELLS: &[(i8, i8, f32, f32, f32, f32)] = &[
    ( 0,  0, 0.0, 0.0, 1.0, 1.0),   // 满屏
    ( 0,  1, 0.0, 0.0, 1.0, 0.5),   // 上半
    ( 0, -1, 0.0, 0.5, 1.0, 0.5),   // 下半
    ( 1,  0, 0.5, 0.0, 0.5, 1.0),   // 右半
    (-1,  0, 0.0, 0.0, 0.5, 1.0),   // 左半
    ( 1,  1, 0.5, 0.0, 0.5, 0.5),   // 右上
    (-1,  1, 0.0, 0.0, 0.5, 0.5),   // 左上
    ( 1, -1, 0.5, 0.5, 0.5, 0.5),   // 右下
    (-1, -1, 0.0, 0.5, 0.5, 0.5),   // 左下
];
```

### 4.7 安全与健壮性（贯穿）

- [ ] `capabilities/default.json` 只开必要命令（不要给通用 `shell` / `fs` 全权限）
- [ ] chrome webview 开 CSP，禁 `unsafe-eval`（loader 的 eval 放在 tab webview 侧）
- [ ] 所有 `unwrap` 换 `Result` + 用户可见错误（原代码 `fsys.enum` 无 try/catch 的教训）
- [ ] 崩溃处理：注册 panic hook 写日志 + 弹窗（`panic = "abort"` 会闪退无提示）
- [ ] 日志目录：Win `%LOCALAPPDATA%/Mix/logs`，mac `~/Library/Logs/Mix`，Linux `~/.local/state/mix`
- [ ] 崩溃后重启恢复：记录最后会话快照

### 4.8 主题 / 国际化（穿插）

- [ ] `theme.css` 变量化：`--bg: #1F2020; --accent: #00FF00; --radius: 15px`
- [ ] FontAwesome 子集化（只打包用到的 6 个图标，woff2 约 3 KB）
- [ ] 明暗主题（P2）
- [ ] i18n：zh_CN / en_US，Rust 侧 `rust-i18n`，前端共用 JSON

---

## 5. IPC 契约（前后端先定，再并行开发）

### 5.1 Commands（前端 → Rust）

| 命令 | 参数 | 返回 | 说明 |
| --- | --- | --- | --- |
| `tab_list` | — | `TabMeta[]` | 启动/恢复用 |
| `tab_create` | `{url?, index?}` | `TabId` | |
| `tab_close` | `{id}` | `()` | |
| `tab_select` | `{id}` | `()` | |
| `tab_navigate` | `{id, url}` | `()` | |
| `tab_nav` | `{id, action}` | `()` | action: back / forward / reload / stop |
| `layout_update` | `{top, width, height, scale}` | `()` | 窗口 resize 时由前端算好传入 |
| `window_control` | `{action}` | `()` | action: min / max / close |
| `ext_list` | — | `ExtInfo[]` | |
| `ext_reload` | `{id?}` | `ExtInfo[]` | |
| `script_list` | — | `ScriptInfo[]` | |
| `script_reload` | — | `ScriptInfo[]` | |
| `settings_get` / `settings_set` | — / `Settings` | `Settings` | |

### 5.2 Events（Rust → 前端）

`tab:created`、`tab:closed`、`tab:selected`、`tab:title`、`tab:favicon`、`tab:loading`、`tab:nav`、`window:maximized`、`ext:loaded`、`ext:error`、`app:ready`

> 前后端按这张表并行，2 天就能各自 mock 跑通。

---

## 6. 测试策略

| 层级 | 内容 | 工具 |
| --- | --- | --- |
| 单元 | crx 解包、@match 匹配、停靠坐标计算、会话序列化 | `cargo test` |
| 集成 | 启动 → 开 3 tab → 切 → 关 → 退出恢复 | `tauri-driver` + WebDriver |
| 视觉 | 三平台标题栏截图对比（防 CSS 回归） | Playwright + 截图 diff |
| 冒烟 | 每平台 10 项手测清单 | 人工 |
| 性能 | 冷启动、切标签、内存 | 自建脚本 + 基线告警 |

### 6.1 冒烟清单（每平台必过）

1. 冷启动 ≤ 1.5 s
2. 新开标签能正常访问网页
3. 开 10 标签切换不白屏
4. 拖拽 / 双击 / 最大化 / 还原
5. 拖边 resize 到最小尺寸不崩
6. 最小化到托盘 + 双击恢复
7. 热键停靠 4 个方向
8. 关闭最后一个标签不退出
9. 重启恢复会话
10. 更新提示可下载安装

---

## 7. CI/CD

```yaml
strategy:
  matrix:
    include:
      - { os: windows-latest, target: x86_64-pc-windows-msvc,  pack: "nsis,msi" }
      - { os: macos-14,       target: aarch64-apple-darwin,     pack: "dmg" }
      - { os: macos-13,       target: x86_64-apple-darwin,      pack: "dmg" }
      - { os: ubuntu-22.04,   target: x86_64-unknown-linux-gnu, pack: "appimage,deb" }
```

- Windows：`signtool` + EV 证书（证书 base64 存 secret）
- macOS：`codesign` + `notarytool`（Apple ID + Team ID，**提前 2 周申请**）
- Linux：AppImage + deb；构建依赖 `libwebkit2gtk-4.1-dev`
- 产物：`latest.json`（updater 清单）+ sha256 + 变更日志

---

## 8. 风险登记表

| # | 风险 | 影响 | 缓解 | 触发点 |
| --- | --- | --- | --- | --- |
| 1 | `unstable` 多 webview API 变更 | 中 | M0 验证 + 抽 `TabHost` trait，必要时 1 天切 wry | 升级 Tauri 大版本 |
| 2 | WebView2 只支持 MV3、无 popup | **高** | M0 就用真实 crx 验证；准备"扩展 → 用户脚本"转换器 | 用户反馈扩展不生效 |
| 3 | WebKitGTK 无 H.264 / 字体渲染差 | 中 | 启动探测 `canPlayType('video/mp4')`，提示装 gstreamer 插件 | M7 回归 |
| 4 | Wayland 无法全局热键 / 无法定位窗口 | 中 | 降级为窗口内快捷键 + 文档说明 | M5 |
| 5 | macOS 公证失败、Win SmartScreen 拦截 | **高** | W1 就提交一次测试签名，别拖到最后 | M6 |
| 6 | 体积超 10 MB | 低 | `strip = true`、`lto = true`、`opt-level = "s"`、图标子集化 | 每次发版 |
| 7 | 三平台视觉不一致 | 中 | CSS 变量统一 + 截图 diff | 每次合并 |

---

## 9. 迁移映射（现有代码 → 新模块）

| 现有 main.aardio | 新位置 | 说明 |
| --- | --- | --- |
| 6–18 窗体定义、20–84 skin | `src/index.html` + `theme.css` | 视觉参数照抄 |
| 86–94 圆角/阴影/淡入 | `window.rs` + CSS | Win11 走原生 |
| 96–101 minmax | `window.rs` | 用宽高，不用 right/bottom |
| 106–129 共享 env + crx | `profile.rs` + `extensions/` | **架构保留**，实现换掉 |
| 131–193 多标签 | `tabs.rs` + `tabbar.ts` | |
| 195–224 NewTabUrl / updateTabTitle | `commands.rs` + `bridge.ts` | 全局变量 → IPC |
| 245–284 托盘 | `tray.rs` | 顺手修掉重复创建的 bug |
| 301–377 停靠状态机 | `snap.rs` | 表格化，修掉 -2 死状态 |
| 379–441 热键 | `shortcuts.rs` | 加防抖 + 失败降级 |
| 444–453 新建标签 | `tab_create` | 与 openInNewTab 合并 |

### 9.1 原代码已知缺陷（重构时一并修掉）

| 缺陷 | 原位置 | 处理 |
| --- | --- | --- |
| 类 C 风格 for 循环写法不合规 | 203 | 改为遍历 tabs |
| `tbs.getForm` 不是 `win.ui.tabs` 的 API | 204 | 用 `tabs.forms[i]` |
| `_newTabUrl` 与 `NewTabUrl` 变量名不一致 | 446 | 统一为 IPC 参数 |
| 托盘重复创建、句柄泄漏 | 248 / 254 | 单例 + 关闭时销毁 |
| 裸键全局热键抢占系统按键 | 384–413 | 改组合键或窗口内热键 |
| 热键回调里 `win.delay` 造成重入 | 387 等 | 改 200 ms 防抖 |
| 相对 CWD 扫描 crx + 无异常保护 | 123 | 白名单目录 + try/catch |
| `io.fullpath("~")` 指向 exe 目录 | 110 | 用平台规范的数据目录 |
| `Winx=-2` 落到 else 分支跳错半屏 | 328 等 | 表格化状态机 |
| `Scright/Scbottom` 当宽高用 | 98–101 | 用 `work_area` 的宽高 |

---

## 10. 每周节奏建议

| 周 | 内容 | 出口 |
| --- | --- | --- |
| W1 | M0 全部 4 项实验 + M1 骨架 | 能跑的窗口 + 1 tab |
| W2 | M2 多标签核心 | 10 tab 稳定 |
| W3 | M2 收尾（会话恢复/上限）+ M3 扩展解包与加载 | Win 上 crx 生效 |
| W4 | M4 脚本引擎 | mac/Linux 脚本注入 |
| W5 | M5 托盘/热键/停靠/单实例 | 功能对齐原 aardio 版 |
| W6 | M6 Windows 签名 + Linux 打包 | 两个平台可发布 |
| W7 | M6 macOS 签名公证 + updater | 三平台可发布 |
| W8 | M7 回归 + 性能基线 + 文档 | 候选版 |
| W9 | 缓冲 | 正式版 |

---

## 11. 总验收（Definition of Done）

- [ ] 三平台安装包均 ≤ 体积目标，签名/公证通过
- [ ] 冷启动 ≤ 1.5 s，5 标签内存 ≤ 600 MB
- [ ] Windows：3 个真实 MV3 crx 生效；mac/Linux：10 个用户脚本按 `@match` 正确分发
- [ ] 冒烟清单 10 项 × 3 平台全绿
- [ ] 无一次 `unwrap` 导致的闪退（panic hook 验证）
- [ ] 自动更新可从 1.0.0 → 1.0.1 成功
- [ ] 崩溃日志、用户设置、会话文件路径符合平台规范

---

## 附录 A：M0 验证工程说明

**目的**：用最小代价证伪"Tauri 2 能满足本项目"，避免投入周级工作量后才发现走不通。

**产出**：一个 `m0-probe` 工程，可 `cargo tauri dev` 直接跑，包含 4 个可独立开关的实验：

1. `probe_extensions`：解包 → 加载 → 页面内探测 `chrome.runtime.id`
2. `probe_tabs`：同窗口 `add_child` 2 个 webview，各自导航与输入
3. `probe_inject`：`initialization_script` 在顶层与 iframe 内的执行时机
4. `probe_platform`：托盘/热键/窗口定位在各平台的实际可用性

**判据表**

| 实验 | 通过 | 不通过时的动作 |
| --- | --- | --- |
| extensions | content script 生效 | 仅 Windows 保留扩展，其他平台全走脚本；若 Win 也不行 → 退回"扩展转脚本"方案 |
| tabs | 两标签互不干扰 | 切 wry（1 天） |
| inject | iframe 内也执行 | 用 `eval` + 导航钩子补丁，标记为风险 |
| platform | 三平台都不崩 | Wayland 降级，写入文档 |

---

## 附录 B：关键技术决策记录

### ADR-001：为什么选 Tauri 2 而不是 Electron

- 约束：编译产物不自带浏览器内核，使用系统 webview。
- Electron 自带 Chromium，安装包 100 MB+，**违反约束**。
- Tauri 2 = tao（窗口）+ wry（webview），三平台分别使用 WebView2 / WKWebView / WebKitGTK。

### ADR-002：为什么 Windows 限制为 MV3

- Edge/Chromium 已弃用 Manifest V2；WebView2 跟随 Edge 的扩展能力。
- 且 WebView2 无浏览器 UI，扩展的 popup 无宿主窗口 → 交互式扩展天然不可用。
- 结论：只承诺"去广告/脚本注入/翻译"类扩展，其余引导用户转成用户脚本。

### ADR-003：为什么 crx 必须先解包

- CRX 是 zip + 头部封装（`Cr24` 魔数 + 版本 + 可选 pubkey/sig + header 长度），扩展加载接口只接受**目录**。
- 解包后还能做 MV3 校验与路径白名单，是安全边界的一部分。

### ADR-004：为什么 chrome UI 也用 webview

- 三平台统一渲染，主题/布局只写一次；代价是需要自己处理 `-webkit-app-region` 与 DPI。
- 备选（原生控件自绘标题栏）会让每平台写三份，与"缩减平台差异"的目标相反。

### ADR-005：为什么先用 Tauri 再考虑 wry

- Tauri 提供打包/签名/更新/托盘/热键等配套，能省 1–2 周。
- 唯一依赖的 `unstable` 能力（同窗口多 webview）已抽成 `TabHost` trait，切换成本 ≈ 1 天。
