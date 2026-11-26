# doit-tui

`doit` 待办应用的终端版。基于 Rust + ratatui，零 WebView、无图形依赖，适合在 SSH、服务器、tmux 或纯终端环境中使用。

它不是一个独立的待办工具，而是 `doit` 多端体系中的一端：通过 WebDAV 与手机端 `doit-apk`、桌面端 `doit-desktop`、GUI 端 `doit-gpui` 共享同一份 `doit-backup.json`，并严格遵循统一的 `SnapshotV2` 数据契约、三方合并语义、墓碑删除和分类色板。

> 架构、数据契约、合并规则与 AI 接手须知见 [AGENT.md](./AGENT.md)。

## 功能特性

- 今日待办、统计、月历、设置四个页面，纯键盘操作
- 顶级任务 + 任意深度子任务，`space` 切换完成态时父子联动
- 优先级 `不设 / 低 / 普通 / 高 / 紧急`，支持 `任务标题!3` 后缀快速指定；今日页 `o` 开「紧急优先」，只重排顶级任务，子任务顺序不变（偏好存本地 settings.json）
- 无限嵌套子任务、`h/l` 折叠展开、`[` / `]` 切换分类筛选
- 统计页提供周 / 月 / 年热力图、分类占比环形图、完成趋势折线图
- 月历页显示每日完成圆点，支持跨月导航与当日完成列表
- 薄荷绿 / 深夜双主题，终端宽度不足时顶栏与键位栏自动精简
- WebDAV 双向同步：字段级三方合并、墓碑传播、陈旧远端短路、旧数据自动迁移
- 应用内检查更新：设置页按 `v` 查 GitHub Releases，发现新版按 `i` 直接下载并替换本地程序，重启生效
- 删除一律打 `deletedAt` 墓碑，不物理移除，保证跨端删除能正确传播

## 安装与运行

### 从 GitHub Release 安装

到 [nabaonan/doit-tui/releases](https://github.com/nabaonan/doit-tui/releases) 下载对应平台二进制：

- macOS：`aarch64-apple-darwin` / `x86_64-apple-darwin`
- Windows：`x86_64-pc-windows-msvc`
- Linux：`x86_64-unknown-linux-gnu`

下载后赋予可执行权限并运行：

```bash
chmod +x doit-tui
./doit-tui
```

### 从源码构建

需要较新的 Rust 工具链（项目使用 Rust 2024 edition，CI 使用 stable）：

```bash
git clone git@github.com:nabaonan/doit-tui.git
cd doit-tui
cargo build --release
./target/release/doit-tui
```

也可以直接安装到本机：

```bash
cargo install --path .
```

开发调试：

```bash
cargo run
```

## 快速开始

首次启动会在数据目录写入演示数据：分类「工作 / 生活 / 学习」和 5 条示例任务。直接用键盘修改即可，所有变更会即时落盘。

默认数据目录为 `~/.doit-tui/`，可用环境变量覆盖：

```bash
DOIT_TUI_DIR=/tmp/doit-tui-demo cargo run
```

> 注意：`device-id` 固定写在 `~/.doit-tui/device-id`，不随 `DOIT_TUI_DIR` 改变；测试请使用 `Store::with_dir()`，不要污染真实目录。

数据目录包含：

| 文件 | 用途 |
| --- | --- |
| `data.json` | 本地快照（SnapshotV2 wire 格式） |
| `settings.json` | 主题与 WebDAV 配置 |
| `sync-base.json` | 三方合并基线 |
| `device-id` | 设备标识 |

所有写盘均先写 `.tmp` 再原子重命名。WebDAV 凭据以明文保存在 `settings.json`，请勿提交或分享该文件。

## 键位

程序内按 `?` 可随时打开帮助。键位按弹层优先顺序处理，输入态、帮助框、删除确认框会先于全局快捷键消费按键。

> **鼠标**：
> - 顶栏点击页面标签切换页面；分类导航条点击切换分类或「＋ 添加分类」
> - 今日页点击任务卡片选中，点击**父级任务**切换子任务展开/收起
> - 月历页点击日期查看当天完成记录；统计页点击热力图格子弹出该日完成数
> - 有弹层打开时点击不响应，避免误操作。

### 全局

| 键 | 作用 |
| --- | --- |
| `1` / `2` / `3` / `4` | 今日 / 统计 / 月历 / 设置 |
| `Tab` / `Shift+Tab` | 顺序 / 逆序切换页面 |
| `t` | 切换薄荷绿 / 深夜主题 |
| `s` | 立即同步 |
| `[` / `]` | 上一个 / 下一个分类筛选 |
| `c` / `x` | 新建分类 / 删除当前分类 |
| `?` | 打开帮助 |
| `q` | 退出 |
| `Ctrl+C` | 分层关闭：输入条 → 删除框 → 新建框 → 帮助，无弹层才退出 |

### 今日页

| 键 | 作用 |
| --- | --- |
| `j/k` 或 `↓/↑` | 上下移动选中项 |
| `space` | 切换完成 / 取消（父子联动） |
| `a` | 新增顶级任务 |
| `A` | 为选中项新增子任务（任意深度） |
| `e` | 编辑标题（已完成任务不可编辑） |
| `d` | 删除（打墓碑） |
| `p` | 循环优先级 |
| `o` | 紧急优先开关：顶级任务按 4→0 排序（子任务不变） |
| `h` / `l` | 折叠 / 展开子任务 |

新增任务支持后缀：`买牛奶!4` 会创建「紧急」任务，`!1`~`!4` 分别对应低 / 普通 / 高 / 紧急。

### 统计页 / 月历页 / 设置页

| 页面 | 键 | 作用 |
| --- | --- | --- |
| 统计 | `←/→` 或 `h/l` | 切换周 / 月 / 年热力图 |
| 月历 | `h/l` | 前后一天 |
| 月历 | `j/k` | 前后一周 |
| 月历 | `←/→` | 前后翻月 |
| 月历 | `g` | 回到今天 |
| 设置 | `u` / `n` / `p` | 修改服务器地址 / 账号 / 密码令牌 |
| 设置 | `e` | 开启 / 关闭同步 |
| 设置 | `v` | 检查 GitHub Releases 上的新版本 |
| 设置 | `i` | 下载新版并替换当前程序（备份原文件，重启生效；Windows 需退出后手动替换） |

输入条中：`Enter` 提交并继续输入、`Esc` 关闭、`Backspace` 删除、`Tab` 切分类、`Ctrl+P` 切优先级。

新建分类弹框中：`Enter` 创建、`Esc` 取消、`←/→`/`Tab`/`h/l` 循环色卡、数字 `1-8` 直接选色。删除确认只接受 `Enter`/`y`/`Y` 确认，`Esc`/`n`/`N` 取消。

## WebDAV 同步

1. 进入设置页 `4`
2. 按 `u` 输入服务器地址，例如 `https://dav.example.com/doit/`
3. 按 `n` 输入账号，按 `p` 输入密码或应用专用令牌
4. 按 `e` 开启同步，按 `s` 立即同步

远端文件名默认为 `doit-backup.json`，与手机端、桌面端共用。同步流程为：探测远端文件 → 读取本地基线 → 三方合并 → 上传 → 保存新基线。远端 404 会被视为「尚无远端文件」并直接上传本地数据。

同步状态显示在顶栏；失败时会在底部 toast 中给出脱敏后的原因，不会输出服务器地址或凭据。

## 数据契约

磁盘与 WebDAV 使用统一的 wire 格式：

```json
{
  "version": 2,
  "revision": 12,
  "exportedAt": 1764000000000,
  "deviceId": "tui-...",
  "lastSyncAt": 1764000000000,
  "categories": [
    { "name": "工作", "color": "#5AA9E6", "sortOrder": 0, "updatedAt": 1764000000000, "deletedAt": null }
  ],
  "tasks": [
    {
      "uid": "u-...",
      "parentUid": null,
      "title": "示例任务",
      "category": "工作",
      "priority": 2,
      "done": false,
      "completedAt": null,
      "createdAt": 1764000000000,
      "updatedAt": 1764000000000,
      "deletedAt": null,
      "depth": 0,
      "sortOrder": 0
    }
  ]
}
```

要点：

- camelCase 字段、扁平 `tasks[] + parentUid`，内部嵌套树只在内存中存在
- 删除任务、子任务、分类均写 `deletedAt`，绝不物理删除
- 旧版 snake_case 嵌套格式会在读取时自动迁移到当前契约
- 分类色板与其它三端一致：`#34C7A8 / #5AA9E6 / #F2A65A / #A78BFA / #22B8CF / #F08A8A / #AACB5E / #7C96B0`

完整字段说明、合并语义和注意事项见 [AGENT.md](./AGENT.md)。

## 开发

```bash
cargo test --all-targets   # CI 同款
cargo test                 # 单元测试
cargo fmt
cargo clippy
cargo build --release
```

测试使用 `Store::with_dir()` 指向 `/tmp/doit-tui-*`，每个测试先清理目录，避免并行测试互相污染，也不应连接真实 WebDAV 或读写真实 `~/.doit-tui/`。

## 发版

发版由维护者显式执行：

```bash
./release.sh [patch|minor|major|1.2.3] [--dry-run] [--force]
```

脚本会从最近的 `vX.Y.Z` tag 推导下一版本，同步 `Cargo.toml` 与 `Cargo.lock`，创建并推送 tag；GitHub Actions 收到 `v*` tag 后自动构建 macOS / Windows / Linux 三平台二进制并发布 Release。日常修改只 `commit + push main`，不会触发发版。

## 相关项目

- 手机端：[nabaonan/doit-apk](https://github.com/nabaonan/doit-apk)
- 桌面端：[nabaonan/doit-desktop](https://github.com/nabaonan/doit-desktop)
- GUI 端：[nabaonan/doit-gpui](https://github.com/nabaonan/doit-gpui)
- 官网：[nabaonan/doit-website](https://github.com/nabaonan/doit-website)
