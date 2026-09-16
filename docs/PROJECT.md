# 项目简介：我们要做什么

这一页写给新加入的协作者和旁观者，读完应该能回答三个问题：**这是什么、为什么这么做、现在做到哪了**。
技术论证不在这一页展开，见[调研与架构设计报告](architecture/RESEARCH_AND_ARCHITECTURE.md)（Phase 0 报告，
已从 reader-platform 仓库导入）与 [ADR](decisions/)。

## 一句话定位

做一个**以阅读器为核心、Local-first、跨平台、插件化**的现代阅读平台：本地电子书与网络小说都能读，
支持书源、AI、OCR、翻译、批注，以及官方云 / 自托管同步。

产品名 **一苇 / Yiwei**（桌面 `identifier` 为 `com.yiwei.reader`，深链 `yiwei://`）。

它不是 EPUB 阅读器的换皮，也不是某个现成客户端的跨平台移植。真正的资产是三样：

1. **阅读体验**：排版、字体、翻页、批注、跨设备阅读连续性。
2. **开放协议**：Document / Annotation / Sync / Plugin / Provider。
3. **生态**：书源、AI Provider、OCR Provider、词典、TTS、同步后端、插件。

## 目标用户

- 本地电子书重度读者：EPUB / PDF / TXT，重视排版、批注、导出与隐私。
- 网络小说读者：需要书源、目录更新、离线缓存、TTS。
- 多设备用户：手机 / 平板 / 电脑 / 网页之间连续阅读。
- AI 辅助阅读用户：翻译、词汇、章节总结、全书分析、双语阅读。
- 漫画 / 扫描件用户：OCR、翻译、气泡文本覆盖。
- 自托管与隐私用户：自己部署同步服务，或纯本地不登录。
- 插件开发者：书源、OCR、AI、词典、主题、同步 Provider。

## 非目标（至少前两年）

- 不做内容平台，不内置盗版书库或书源。
- 不做电子书商店与支付（架构预留 Entitlement，不实现支付）。
- 不做实时协作编辑（只预留 CRDT 扩展点）。
- 不做社交动态与推荐算法。
- 不追求所有平台的 E-ink 专用优化，先支持 Android E-ink 设备与高对比 / 无动画模式。
- 不承诺与 Legado 书源 100% 兼容。

## 架构总览

**共享 Rust Core + 同一套 Web Reader UI + Tauri 2 原生壳 + 独立 Web/PWA 壳**：

```text
apps/readest-app/    客户端 UI 与阅读体验（React / Next.js），六端共用
  src-tauri/         原生壳、插件、平台适配（Tauri 2）
crates/              Rust Core（独立嵌套 workspace，edition 2024）
  reader-model/      ID、EPUB CFI、Locator、阅读顺序
  document/          ZIP/EPUB 解析、派生索引、TXT 分块
  annotation/        批注锚点与重新定位
  storage/           SQLite schema 与仓储
  sync/              HLC、字段级 LWW、合并规则
  protocol/          与 TypeScript 共享的协议夹具与契约测试
packages/            foliate-js、tauri 等 submodule 与共享包
docs/                调研报告、ADR、实现进度、品牌化清单
server/              自托管服务端（未开始）
```

- **渲染层**：EPUB / FB2 / MOBI / CBZ 用 foliate-js，PDF 用 PDF.js，排版基线用 Readium CSS。
  阅读器的排版、选区、标注本质上是 DOM/CSS 问题，Web 引擎是唯一能在六端共享同一套锚点语义的路线。
- **Rust Core**：书籍解析与索引、数据库、标注锚点、同步合并、插件宿主，以及 AI / OCR / 书源的安全边界；
  Web 端以 WASM 形式复用同一份 Core。
- **服务端**：Rust + Axum + PostgreSQL + S3 兼容对象存储，官方云与自托管使用同一套 Sync Protocol。
- **原生壳**：Tauri 2，覆盖 Windows / macOS / Linux / Android / iOS；Web 端构建静态 PWA，复用同一套 UI。

本仓库是上游 [Readest](https://github.com/readest/readest) 的 fork（见
[ADR-0007](decisions/ADR-0007-readest-as-product-base.md)），许可证为 **AGPL-3.0**。

## 设计原则

1. **本地优先**：本地数据库是 Source of Truth，云服务只做同步、备份与中继；登录与联网不是阅读的前提。
2. **原始文件不可变**：书籍文件按内容哈希存储，内部 Document Model 是可重建的**派生索引**，不写回原书。
3. **一切通过接口接入**：AI、OCR、书源、同步、渲染引擎都是可替换的 Provider / 引擎，不写死在核心里。
4. **书籍文件默认不上传**：默认只同步元数据与阅读数据，文件同步是可选能力。
5. **插件按平台分级**：桌面端 WASM / QuickJS / Sidecar，Android 允许解释型与 WASM，iOS 只保证数据型 / 声明式
   插件与内置精选插件，避免触犯商店对动态下载可执行代码的限制。
6. **协议先行**：Document / Annotation / Sync / Plugin / Provider 都有独立协议与夹具，客户端与服务端共享。

## 已被否定的做法

这些做法看起来省事，但会破坏长期可维护性，报告中已明确列出，不要重复提案：

- 把所有格式转成自有格式后丢弃原结构（会永久丢失 EPUB 样式、脚注、锚点与固定版式）。
- 承诺 100% 兼容 Legado 书源（依赖 Android WebView / Rhino / OkHttp 等平台能力，跨平台不可能，法律与安全上也不应承诺）。
- 把 Wenyi 作为 Python 库嵌进客户端（iOS 无法运行 Python sidecar），应做 Job Runner + Sidecar / 远程 Worker。
- 允许插件热插拔任意可执行代码（与 App Store 2.5.2、Google Play 动态代码政策冲突）。
- 默认把所有书籍文件上传云端（版权、成本、隐私与冲突都会失控）。
- 让登录、云服务或在线校验成为阅读前提。

> 报告 §0.2 里「不以现成项目为底座」一条已被
> [ADR-0007](decisions/ADR-0007-readest-as-product-base.md) 取代：项目现在以 Readest 上游为产品底座。

## 现在做到哪了

- **B0 基础收尾（进行中）**：上游设施隔离（云 / 遥测 / 更新源 / OAuth）、应用身份与图标、Rust Core 并入 `crates/`。
- **B1 文档格式与解析能力（下一步）**：PDF 进入自有文档模型（页码定位、文字层、页 + 矩形锚点）、
  Markdown / HTML / TXT 的语义与索引。
- **B2 差异化能力**：书源引擎、权限化插件宿主、Wenyi 翻译 Job、自托管同步服务端。

能做什么、还要做什么（逐项状态）见[功能清单](FEATURES.md)；详细进度、验证状态与待办见
[路线图](implementation/ROADMAP.md)与[实现进度](implementation/README.md)。

## 文档地图

| 想知道什么 | 看哪里 |
| --- | --- |
| 支持哪些功能、还要做什么 | [FEATURES.md](FEATURES.md) |
| 为什么这么设计（完整调研与论证） | [architecture/RESEARCH_AND_ARCHITECTURE.md](architecture/RESEARCH_AND_ARCHITECTURE.md) |
| 已拍板的架构决策 | [decisions/](decisions/)（ADR-0001～0007） |
| 调研引用来源 | [research/EVIDENCE.md](research/EVIDENCE.md) |
| 实现进度与验证方式 | [implementation/README.md](implementation/README.md) |
| 当前阶段要做什么 | [implementation/ROADMAP.md](implementation/ROADMAP.md) |
| 发布前的品牌化 / 去上游化清单 | [implementation/BRANDING.md](implementation/BRANDING.md) |
| Linux 桌面端窗口空白、GBM 报错排查 | [implementation/LINUX-DEV.md](implementation/LINUX-DEV.md) |
| 上游 Readest README 原文（下载渠道、赞助信息） | [upstream-readme.md](upstream-readme.md) |

## 本地跑起来

```bash
git submodule update --init --recursive
pnpm install --frozen-lockfile
pnpm --filter @readest/readest-app setup-vendors   # simplecc / pdfjs / jieba → public/vendor
pnpm dev-web                                       # http://localhost:3000
pnpm tauri dev                                     # 桌面端（Linux 上走 CEF 运行时）
```

Rust Core 单独构建与测试：

```bash
cd crates && cargo test --offline      # 含契约夹具
cd crates && cargo clippy --offline --all-targets
```

常见坑见[实现进度](implementation/README.md)的「本地启动」一节与
[Linux 桌面端排查](implementation/LINUX-DEV.md)。
