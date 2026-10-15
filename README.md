# Google 账号管家 (Google Account Vault) v3.0.0

[![Tauri](https://img.shields.io/badge/Tauri-2.0-24C8D8?style=flat-square&logo=tauri&logoColor=white)](https://tauri.app/)
[![Rust](https://img.shields.io/badge/Rust-2024_Edition-black?style=flat-square&logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![Vue](https://img.shields.io/badge/Vue-3.5-4FC08D?style=flat-square&logo=vuedotjs&logoColor=white)](https://vuejs.org/)
[![TypeScript](https://img.shields.io/badge/TypeScript-5.6-3178C6?style=flat-square&logo=typescript&logoColor=white)](https://www.typescriptlang.org/)
[![Tailwind CSS](https://img.shields.io/badge/TailwindCSS-v4-38B2AC?style=flat-square&logo=tailwind-css&logoColor=white)](https://tailwindcss.com/)
[![License](https://img.shields.io/badge/License-MIT-blue.svg?style=flat-square)](#)

新一代高性能、零依赖、现代化设计的 Google 账号与二次验证（2FA）桌面端保险库。采用 **Rust 2.0 + Tauri 2.0 + Vue 3 + Tailwind CSS** 原生重构，提供极致轻量、亚秒级启动与军工级本地端到端加密保护。

---

## ✨ 核心亮点与功能特性

### 1. 🛡️ 军工级本地端到端加密
- **AES-256-GCM + Scrypt 密钥派生**：主密码绝不落盘，全字段高强加密保护。
- **100% 数据向下兼容**：完全兼容旧版 `accounts.gvault` 格式规范，平滑升级无缝迁移。
- **快照回滚与数据安全**：任何变更前自动创建防灾快照，支持历史版本一键回滚与 CSV/JSON 导出。

### 2. 🔑 Windows Hello / PIN 硬件级免密解锁
- **Win32 DPAPI 硬件绑定凭证保护**：主密码安全封装于当前 Windows 用户安全上下文。
- **原生焦点锚定与置顶**：接入 Windows `IUserConsentVerifierInterop` COM 互操作层，生物识别/PIN 验证弹窗前台秒级唤起并自动聚焦光标，无需二次点击。

### 3. ⏱️ RFC 6238 实时 2FA TOTP 动态码
- **精准时间戳倒计时环**：毫秒级实时计算 30 秒 TOTP 六位动态口令。
- **一键复制与即时反馈**：支持带动画的复制状态与剪贴板即时清理机制。

### 4. 🎫 8位备用验证码智能管理
- **可视化网格与状态卡片**：清晰标记已使用与未使用备用验证码。
- **一键使用下一个**：自动定位并复制首个有效码，一键完成安全核销。
- **智能文本解析**：支持多行复制粘贴自动剥离识别 8 位纯数字验证码。

### 5. 🎲 全功能独立强密码生成器
- **快捷预设模板**：一键切换「标准强密 (20位)」、「超高强度 (32位)」、「纯字母数字 (18位)」与「PIN码 (6位)」。
- **细粒度自定义规则**：6~64位无级滑块调节、字符集自由复选与防混淆字符（`1, l, I, 0, O, o`）过滤。
- **实时安全熵值评分**：动态色彩指示条与安全等级评估。
- **一键应用至当前账号**：选中账号后可直接将新生成密码一键应用，省去反复粘贴步骤。

### 6. 🌐 隔离环境独立浏览器 (会话形态 A)
- **多账号独立持久存储**：为每个 Google 账号分配独立的 WebView2 存储上下文与隔离会话环境。
- **自动化安全灌入**：智能检测 Google 登录界面表单，自动识别灌入邮箱、密码及当前最新的 2FA TOTP 动态口令。

### 7. ☁️ WebDAV 端到端纯密文云备份
- **跨设备无缝同步**：基于 `reqwest` + `rustls` 实现标准化 WebDAV 协议互通（支持坚果云、Nextcloud、Synology 等）。
- **零知识云存储**：云端仅传输和存储高强度加密密文，服务器端无法探测任何明文信息。

### 8. 🎨 现代化交互与 UI 设计
- **Google Material 3 / Fluent 视觉美学**：彻底消除复古死黑线框与 Emoji 乱码降级，柔和圆角卡片、高质感字印排版与平滑动效。
- **全自研弹窗与通知系统**：彻底移除浏览器原生阻塞式 `alert` 与 `confirm`，全系配备 Material/Fluent 风格毛玻璃确认框与优雅 Toast 通知。
- **全平台多尺寸图标**：内嵌 16×16 至 256×256 复合 7 阶 Windows 原生图标与全端高清 Logo 适配。

---

## 🚀 快速上手与运行

### 1. 运行环境准备
- **Node.js**: v18.0.0 或更高版本
- **Rust / Cargo**: 1.80+ (推荐 2024 Edition 工具链)
- **WebView2 运行时** (Windows 10/11 系统通常自带)

### 2. 安装项目依赖
```powershell
npm install
```

### 3. 开发模式（支持热重载）
```powershell
npm run tauri dev
```

### 4. 生产环境构建与打包
```powershell
npm run tauri build
```
编译完成后，可在以下路径找到构建产物：
- **绿色单文件（免安装便携版）**：`src-tauri/target/release/google-account-vault.exe`
- **官方 NSIS 一键安装包**：`src-tauri/target/release/bundle/nsis/Google 账号管家_3.0.0_x64-setup.exe`

---

## 📂 项目结构概览

```text
├── public/                 # 静态资源 (app.ico, logo.png, favicon.ico)
├── src/                    # 前端代码 (Vue 3 + TypeScript + Tailwind CSS)
│   ├── App.vue             # 主界面、模态弹窗系统与全功能视图组件
│   ├── main.ts             # 应用前端入口
│   └── style.css           # 基础样式与 Tailwind 指令
├── src-tauri/              # Rust 后端 (Tauri 2.0 桌面端架构)
│   ├── src/
│   │   ├── lib.rs          # Tauri 命令调度中心与生命周期初始化
│   │   ├── models.rs       # 账号实体、备份结构等数据模型
│   │   ├── vault.rs        # AES-256-GCM 与 Scrypt 密文存储引擎
│   │   ├── hello.rs        # DPAPI 凭证加密与 WinRT Interop 窗口句柄置顶
│   │   ├── totp.rs         # RFC 6238 TOTP 动态口令计算与验证
│   │   ├── utils.rs        # 强随机密码生成引擎与密码安全熵值计算
│   │   ├── webdav.rs       # WebDAV 客户端纯密文同步上传与下载
│   │   └── backups.rs      # 本地历史快照防灾管理与回滚
│   ├── icons/              # 全平台自适应图标集合
│   ├── Cargo.toml          # Rust 依赖声明与编译优化配置
│   └── tauri.conf.json     # Tauri 2.0 应用配置文件
├── 启动程序.bat            # 便捷启动脚本
└── package.json            # 前端依赖配置
```

---

## 🔒 安全性说明

1. 本项目为**纯本地运行应用**，除用户主动配置的 WebDAV 同步服务以及使用内置独立浏览器访问 Google 官网外，**不连接任何第三方统计、追踪或数据分析服务器**。
2. 主密码采用强密码学哈希派生保护，请务必妥善保管主密码。丢失主密码且未配置 Windows Hello / 离线备份的情况下，数据将不可恢复。

---

## 📄 开源许可证

本项目基于 [MIT License](LICENSE) 开源。
