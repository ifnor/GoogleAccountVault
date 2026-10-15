# Google 账号管家 v3.0.0 (Rust + Tauri 2.0 原生版)

采用 **Rust + Tauri 2.0 + Vue 3 + Tailwind CSS v4 + Lucide Icons** 全面重构的新一代 Google 账号密码与 2FA 本地保险库。

## ✨ 核心特性

- **极致轻量**：独立便携版 EXE 仅 **4.4 MB**，极速安装包仅 **1.45 MB**（相较原 Python 版缩小超过 96%）。
- **秒级启动**：冷启动仅需 **~0.08 秒**，内存占用极低（~25 MB）。
- **Google Material 3 现代美学**：彻底消除复古死黑线框与 Emoji 乱码降级，具备纯净柔和圆角卡片、高质感字印排版与平滑动效。
- **100% 数据格式向下兼容**：完全沿用 AES-256-GCM + Scrypt 密钥派生规范，可无缝直接解锁并读取原 Python 版生成的 `accounts.gvault`，数据零丢失。
- **RFC 6238 实时 2FA**：毫秒级实时计算 TOTP 动态码与倒计时，支持一键安全复制。

## 🚀 启动与构建

### 快捷运行
双击根目录下的 `启动程序.bat` 即可直接拉起已编译的原生程序。

### 开发模式（热重载）
```powershell
npm run tauri dev
```

### 生产打包
```powershell
npm run tauri build
```
产物将输出在：
- **便携版独立程序**：`src-tauri/target/release/google-account-vault.exe` (4.4 MB)
- **NSIS 一键安装包**：`src-tauri/target/release/bundle/nsis/Google 账号管家_3.0.0_x64-setup.exe` (1.45 MB)
