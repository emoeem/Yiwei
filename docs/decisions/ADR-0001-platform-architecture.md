# ADR-0001：跨平台架构

状态：Proposed（待评审）  
日期：2026-09-13

## 背景

需要覆盖 Windows、macOS、Linux、Android、iOS、Web，并尽可能共享核心逻辑、数据模型、阅读状态、插件协议和同步协议。

## 决策

采用 **Rust Core + React UI + Tauri 2 原生壳 + 独立 Web/PWA 壳**。

- 桌面/移动：Tauri 2。
- Web：同一 React UI 构建静态 PWA；Core 以 WASM 形式复用，浏览器不可用的能力显式降级。
- 阅读渲染：WebView/浏览器内使用 foliate-js + PDF.js + Readium CSS。
- Rust Core 负责解析索引、数据库、锚点、同步、插件宿主、AI/OCR/书源宿主边界。

## 考虑的替代方案

- Flutter + Rust Core：UI 一致、性能好，但 Web 文本选择/无障碍/EPUB 生态弱。
- React Native/Expo + Rust Core：移动强、桌面弱。
- Kotlin Multiplatform + Compose：桌面/移动强、Web 弱、工具链复杂。

## 后果

正面：最大化共享 UI 与阅读语义；复用成熟 MIT/Apache 阅读引擎；无障碍与标注基础最佳。

负面：依赖系统 WebView；Linux 可能需要 CEF 备选；Web 端需要能力降级；需要同时维护 Rust 与 TypeScript 两套工具链。

## 触发重新评估的条件

- Tauri 2 在任一目标平台出现无法绕过的阻塞问题。
- foliate-js 无法满足复杂 EPUB 兼容或性能要求。
- Web 端无障碍/文本选择验证失败。
- 移动端包体积或启动性能无法达标。
