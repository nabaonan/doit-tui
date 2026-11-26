# AGENT.md — doit-tui 接手指南

本文件面向接手本项目的 AI / 开发者。目标是让接手者在动手前先理解架构边界、数据契约和不可破坏的约定。

**先读**：[README.md](./README.md)（用户视角功能与键位）。  
**再读**：本文件（开发视角红线与实现细节）。

## 1. 项目定位

`doit-tui` 是 `doit` 待办体系的终端客户端（Rust + ratatui）。它不是独立应用，而是四端同步体系中的一端：

| 端 | 仓库 |
| --- | --- |
| 手机端 | `nabaonan/doit-apk` |
| 桌面端 | `nabaonan/doit-desktop` |
| GUI 端 | `nabaonan/doit-gpui` |
| 终端版 | `nabaonan/doit-tui`（本仓库） |
| 官网 | `nabaonan/doit-website` |

所有端通过 WebDAV 共享同一个远端文件 `doit-backup.json`，并使用统一的 `SnapshotV2` wire 契约。**任何模型字段、合并语义、颜色规范的改动都必须同时评估其它三端。**

## 2. 技术栈与入口

- Rust 2024 edition；CI 使用 stable toolchain（本机为 Rust 1.98.1）
- `ratatui 0.29` + `crossterm 0.28`：终端 UI 与事件
- `serde` / `serde_json`：持久化与 wire 序列化
- `ureq`：WebDAV HTTP 请求（同步阻塞，放在事件循环外调用）
- `chrono`：毫秒时间戳与本地日期
- `unicode-width`：中文宽度计算
- `dirs`：默认数据目录
- `anyhow`：错误传播

入口：`src/main.rs`
- `ratatui::init/restore`
- `run()` 事件循环，`poll` 间隔 200ms，只处理 `KeyEventKind::Press`
- `on_key()` 全局键位路由
- `do_sync()` 同步入口
- `draw()` 五段布局与弹层绘制

## 3. 文件地图

| 文件 | 职责 |
| --- | --- |
| `src/main.rs` | 入口、事件循环、键位路由、同步触发、渲染编排、交互测试 |
| `src/app.rs` | `App` 状态机、`Page`/`InputMode`/弹框状态、任务与分类增删改、完成联动、折叠、今日统计 |
| `src/model.rs` | 内部嵌套模型 + wire 模型 + serde 双向转换 + 树工具 + `mod legacy` 旧格式迁移 |
| `src/store.rs` | 本地持久化、`Settings`、`WebDavSettings`、`SyncBase`、原子写、设备 ID |
| `src/sync.rs` | WebDAV GET/PUT、三方合并 `merge_wire`、冲突裁决、陈旧远端短路 |
| `src/theme.rs` | 优先级色 / 分类色板 / 归一化 / 亮暗主题令牌 |
| `src/ui/mod.rs` | 顶栏、分类条、键位栏、toast、帮助框、分类弹框、优先级徽标 |
| `src/ui/today.rs` | 今日页任务树与输入条 |
| `src/ui/stats.rs` | 统计页：指标、热力图、环形图、折线图 |
| `src/ui/calendar.rs` | 月历页 |
| `src/ui/settings.rs` | 设置页与版本 / 更新状态显示 |
| `src/update.rs` | GitHub Releases 检查更新、平台产物匹配、下载并替换本地可执行文件 |
| `release.sh` | 版本号同步 + tag + push |
| `.github/workflows/ci.yml` | `cargo test --all-targets` + `cargo build --release` |
| `.github/workflows/release.yml` | `v*` tag 三平台构建并发布 GitHub Release |

## 4. 数据契约（最高优先级）

### 4.1 wire 格式

磁盘 `data.json` 和远端 `doit-backup.json` **一律使用桌面端 wire 格式**，禁止退回旧 snake_case 嵌套 `subs`：

```text
SnapshotV2 {
  version: 2,
  revision: number,
  exportedAt: i64 (ms),
  deviceId: string,
  lastSyncAt: i64 | null,
  categories: WireCategory[],
  tasks: WireTask[]
}

WireCategory { name, color, sortOrder, updatedAt, deletedAt }
WireTask { uid, parentUid, title, category, priority, done,
           completedAt, createdAt, updatedAt, deletedAt, depth, sortOrder }
```

- `parentUid: null` 表示顶级任务；子任务 `category` 置空，分类跟随父任务
- `priority`：`0 不设 / 1 低 / 2 普通 / 3 高 / 4 紧急`
- 时间戳统一 `i64` 毫秒
- 内部仍保留嵌套树 `Task { subs }`，由 `Snapshot` 上的 `#[serde(from = "WireSnapshot", into = "WireSnapshot")]` 在序列化边界双向转换
- `parse_snapshot()` 检测旧格式（`id` 无 `uid`）并调用 `legacy::to_snapshot()` 迁移，保留子任务并补齐墓碑 / 排序 / 颜色
- `sync-base.json` 同时兼容新 wire 顶层结构和旧 `SyncBase { snapshot }` 包装

### 4.2 墓碑删除

任务、子任务、分类删除一律写 `deleted_at`，禁止物理移除：

- `app.rs::del_selected()` 给顶级任务或递归子树打墓碑
- `app.rs::delete_category()` 给分类打墓碑，并把其任务迁移到「默认」
- `model.rs::active_tasks()` / `active_categories()` / `collect_subs()` 负责过滤墓碑
- 只有同步合并明确判定「删除后对方发生真实编辑」才允许复活

### 4.3 合并语义（`src/sync.rs::merge_wire`）

移植自桌面端 `domain/merge.ts`，不可随意改：

1. 任务按 `uid`、分类按 `name` 取并集，单边独有保留
2. 字段级合并：仅一边相对 base 改变则取该边
3. 同字段冲突按 `effective_time = max(updatedAt, completedAt, deletedAt, createdAt)` 取较新者；**平局远程优先**
4. 墓碑 vs 编辑：对方在删除之后又有真实编辑才复活；否则删除胜
5. `is_stale_remote()`：远端 `exportedAt` 更早且 `revision` 不高于本地时，直接用本地并强制上传
6. `revision = max(local, remote) + 1`
7. 输出任务按 `(depth, sortOrder, uid)` 排序

修改这块前必须跑 `cargo test --all-targets`，尤其关注 `sync.rs` 中的墓碑与陈旧远端测试。

### 4.4 颜色契约

`src/theme.rs`：

```text
CAT_PALETTE = #34C7A8 / #5AA9E6 / #F2A65A / #A78BFA /
              #22B8CF / #F08A8A / #AACB5E / #7C96B0
DEFAULT_CATEGORY_COLOR = #34C7A8
```

- 统一存储为**带 `#` 的大写 `#RRGGBB`**
- `normalize_category_color()` 兼容手机端历史值（可能无 `#`）：trim → 补 `#` → 校验 6 位 hex → 转大写 → 非法回退默认色
- 四端色板位置：手机 `Color.kt:PaletteColors`、桌面 `tokens.ts:CATEGORY_SWATCHES`、gpui `task_tree.rs:CAT_PALETTE`、tui `theme.rs:CAT_PALETTE`。改一处必须同步改四处。
- 优先级色：`低 #4FBF9E / 普通 #4A86D6 / 高 #E08A34 / 紧急 #DE5B55`

## 5. 交互与键位路由

- 全局键位在 `src/main.rs::on_key()`；页面专属键位在同一函数的 `match app.page` 分支
- 处理顺序敏感：输入条 → 删除确认 → 新建分类 → 帮助 → 全局 → 页面
- `Ctrl+C` 分层关闭；其它 `Ctrl` 组合键一律忽略（`Ctrl+P` 在输入条内例外），避免误触单字母快捷键
- 设置页 `v` 检查更新、`i` 下载安装：网络请求在后台线程，`App::poll_update()` 每帧收取结果，禁止在事件循环里同步请求
- 任务标题后缀 `!1`~`!4` 在 `app.rs` 解析为优先级
- 终端宽度自适应：顶栏 / 键位栏有 full / short / 极简三级；中文宽度用 `unicode-width`

改动键位时保留这些顺序，并补充 `main.rs` 中 `key_tests` 的覆盖。

## 6. 持久化与安全

默认目录 `~/.doit-tui/`：

| 文件 | 用途 |
| --- | --- |
| `data.json` | 本地 SnapshotV2 |
| `settings.json` | 主题 + WebDAV 配置 |
| `sync-base.json` | 合并基线 |
| `device-id` | 设备标识 |

- 写盘一律 `.tmp` + `rename` 原子替换
- `DOIT_TUI_DIR` 可覆盖数据目录，但 **`device_id()` 固定读写 `~/.doit-tui/device-id`**，不受该变量影响
- `settings.json` 当前明文保存 WebDAV 密码；`.gitignore` 已排除 `~/.doit-tui/` 与 `doit-tui-data/`。绝不提交真实配置，也不要在日志 / 错误信息里输出 URL、账号或密码
- `sync.rs::classify()` 只返回脱敏文案，禁止直接外抛底层网络错误字符串

### 6.1 应用内更新（`src/update.rs`）

- 检查更新：`api.github.com/repos/nabaonan/doit-tui/releases/latest`（public 仓库、匿名、无需 Token），语义版本比较用 `is_newer()`，平台产物后缀必须与 `.github/workflows/release.yml` 一致（macOS 两个 triple / `_win-x64.exe` / `_linux-x64`）。
- 安装：下载裸二进制到当前可执行文件旁 `*.doit-update.new`，Unix 备份 `*.doit-update.bak` 后 `rename` 覆盖当前程序（运行中可替换），Windows 因占用只留 `.new` 并提示退出后手动替换；`MAX_ASSET_BYTES = 200MB` 防止异常大包。
- UI 状态在 `App::{update_check_rx, update_install_rx, update_busy, update_info, update_hint}`，后台线程只发 `Result`，错误必须是安全文案；`releases_url()` 供下载地址展示。
- 改动更新逻辑要补 `update.rs` 单测，并跑 `cargo test --offline --all-targets`。

## 7. 常用命令

```bash
cargo run                  # 开发运行
cargo build --release      # LTO + strip 发布构建
cargo test                 # 单元测试
cargo test --all-targets   # CI 同款
cargo fmt
cargo clippy
```

测试约定：

- 用 `Store::with_dir()` 指向 `/tmp/doit-tui-*`，每个测试先清理目录
- 不要使用真实 `~/.doit-tui/`，不要连接真实 WebDAV
- `render_tests` 中的 `dump_*` 仅打印帧用于肉眼检查，没有断言；唯一有断言的渲染测试是 `stats_canvas_bg_follows_theme`
- `cargo test --offline` 可用于缓存完备时避免网络解析

Rust 2024 注意：

- 禁止 `r##"..."##` 多 `#` raw string
- raw string 内 JSON 颜色 `"#RRGGBB"` 的 `"#` 会提前终止 `r#"，请用 `\u0023` 表示 `#`

## 8. 发版纪律

- **只有用户明确要求「发版 / 发布」时才能执行发版。** 日常改动只 `commit + push main`。
- 用法：`./release.sh [patch|minor|major|1.2.3] [--dry-run] [--force]`
- 脚本从最近 `vX.Y.Z` tag 推导下一版本，校验工作区干净和远端无同名 tag，同步 `Cargo.toml` / `Cargo.lock`，commit bump → `git tag -a vX` → `git push origin vX`
- `v*` tag 触发 `.github/workflows/release.yml` 构建 macOS（aarch64 + x86_64）、Windows x64、Linux x64 三平台产物并发布 GitHub Release
- `workflow_dispatch` 只构建产物，不创建 Release
- 发版前先 `--dry-run` 确认版本号；发版后核对 tag 与产物

## 9. 接手红线清单

1. 写盘 / 远端一律 camelCase wire + 扁平 `tasks[] + parentUid`，不要恢复旧嵌套格式
2. 删除必须打墓碑；分类删除要把任务迁到「默认」
3. 合并规则里的平局远程优先、陈旧远端短路、删除后真编辑才复活，都是历史 bug 的修复点
4. 色板四端同步改，颜色统一带 `#` 大写
5. 键位路由顺序不要破坏，`Ctrl+C` 分层关闭要有测试
6. 测试目录必须隔离，不能污染用户真实数据
7. 凭据与网络错误必须脱敏
8. 文档与代码同步：改行为时更新 `README.md` 和本文件
