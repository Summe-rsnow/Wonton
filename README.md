<div align="center">

<img src="assets/logo.svg" alt="Wonton Logo" width="120" />

# Wonton

**轻量、原生的 Antigravity 多账号与额度管理工具**

在一个窗口内完成账号管理、实时模型额度查看、账号切换与指定代理启动。  
基于 **Rust + [NSH](https://github.com/longbridge/nsh)** 构建，纯原生渲染，无 JavaScript 运行时开销。

![Rust](https://img.shields.io/badge/Rust-2024-orange?style=flat-square&logo=rust)
![Platform](https://img.shields.io/badge/Platform-Windows%20%7C%20macOS%20%7C%20Linux-blue?style=flat-square)
![UI](https://img.shields.io/badge/UI-NSH%20Native-6366f1?style=flat-square)

</div>

---

## ✨ 核心特性

- **多账号一目了然**
  - 实时展示每个账号的订阅等级（`ULTRA` / `PRO` / `FREE` / `ENTERPRISE`）及各模型剩余额度百分比、进度条与重置倒计时。
  - 支持拖拽排序，按你的使用习惯自由排列账号卡片；内置浅色 / 深色主题，自动跟随系统外观。
- **三种账号导入方式**
  - **网页授权**：在浏览器完成 Google OAuth 登录，通过本地回环端口自动接收回调并立即拉取额度。
  - **手动 Token**：粘贴单个或多个 `refresh_token`（支持纯文本按行分隔或 JSON 数组批量导入）。
  - **本机导入**：直接读取本机已登录 Antigravity 客户端的 `state.vscdb`，自动提取 `refresh_token`（Windows）。
- **进程级独立代理**
  - 支持 `HTTP` / `SOCKS5` 代理配置并本地持久化。
  - 一键**切换账号并启动 Antigravity**：代理仅作用于该次启动的进程树，**不修改系统代理、不安装虚拟网卡**。
  - Wonton 自身的网络请求（OAuth 换取 Token、刷新凭证、拉取额度等）也全部直连所配代理（自建 CONNECT 隧道 + TLS），不受系统代理干扰。
- **企业版（GCP）客户端支持**
  - 添加账号时可勾选「使用企业版（GCP）客户端」，系统会记住每个 `refresh_token` 对应的 client 类型。
- **纯原生体验**
  - 全 Rust 编写，启动迅速、内存占用低，支持系统托盘常驻与快速操作。

---

## 🚀 快速开始

### 环境要求

- [Rust](https://www.rust-lang.org/tools/install)（Edition 2024）
- [NSH CLI](https://nsh.dev/docs/installation)（`0.6.0+`）

### 开发与运行

```bash
# 克隆仓库
git clone https://github.com/Summe-rsnow/Wonton.git
cd Wonton

# 以本机后端运行（推荐，会加载 nsh.json 与 theme.json 主题配置）
nsh run
```

> **提示**：直接使用 `cargo run` 也可以启动，但不会读取 `nsh.json` 和 `theme.json`，建议始终使用 `nsh run` 以获得完整的视觉与主题效果。

### 检查与测试

```bash
# 代码与类型检查
nsh check

# 运行单元测试与 headless UI 交互测试
nsh test
```

---

## 📁 项目结构

```text
Wonton/
├── assets/             # 应用图标、Logo 与托盘图标
├── nsh.json            # 应用元信息、窗口配置、主题与 Rust 入口声明
├── theme.json          # 浅色 / 深色语义色 Token（含 success / warning / scrim）
├── src/
│   ├── lib.rs          # 根组件 App（#[native_module]）
│   ├── main.rs         # 二进制入口
│   ├── shell.rs        # 主界面：顶栏、代理设置、账号列表、拖拽排序与提示层
│   ├── add_dialog.rs   # 添加账号弹窗（网页授权 / 手动 Token / 本机导入）
│   ├── store/          # 应用状态管理（mod.rs）与异步网络/启动动作（actions.rs）
│   ├── google.rs       # OAuth 授权、用户信息与额度查询接口
│   ├── localdb.rs      # state.vscdb → protobuf 解析 → 提取 refresh_token
│   ├── launch.rs       # Antigravity 客户端定位与带代理启动
│   ├── format.rs       # 倒计时与百分比文案格式化
│   └── ui/             # 样式与基础组件层（button / input / surface / quota / progress）
└── tests/
    └── screen.rs       # Headless 端到端界面操作测试
```

---

## ⚠️ 注意事项

1. **额度接口说明**：额度查询调用的是 Google 内部接口（`v1internal:fetchAvailableModels`，按 `daily → sandbox → prod` 顺序尝试），上游接口随时可能变更。
2. **本地存储**：`refresh_token` 与代理配置等本地状态均通过跨平台 `AES-256-GCM` 加密后保存在本地应用存储目录中，但仍建议妥善保管本机数据目录，切勿随意分享。
3. **本机导入**：依赖客户端 `state.vscdb` 的内部 protobuf 结构，客户端升级后可能需要同步适配；目前仅 Windows 构建默认链接了 SQLite（`rusqlite`）。

---

## 🤝 参与贡献

欢迎提交 Issue 或 Pull Request 来改进 Wonton！  
提交前请确保 `nsh check` 与 `nsh test` 均通过。

## 📄 开源协议

本项目基于 [Apache License 2.0](LICENSE) 开源。
