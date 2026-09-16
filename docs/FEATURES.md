# 功能清单：现在支持什么、还要做什么

本页回答两个问题：**当前版本能做什么**、**我们还要做什么**。

- 「现在能用」的能力继承自上游 Readest（客户端实现见 `apps/readest-app/src`），本项目在此基础上做品牌化与自研能力。
- 「要做的能力」来自[路线图](implementation/ROADMAP.md)与 [ADR](decisions/)。
- 只想看一句话定位，读[项目简介](PROJECT.md)。

状态图例：✅ 可用 ｜ ⚠️ 可用但需要自己配置 ｜ 🚧 开发中 ｜ 📋 规划中 ｜ 🚫 明确不做

## 一、现在能用的能力

### 书籍格式

| 格式 | 说明 | 状态 |
| --- | --- | --- |
| EPUB `.epub` | 主流格式，保留原始 CSS/脚注/固定版式 | ✅ |
| PDF `.pdf` | PDF.js 渲染、文字层、批注；进入自有文档模型见 B1 | ✅ |
| MOBI / AZW / AZW3 | Kindle 格式（含 KF8） | ✅ |
| FB2 `.fb2` | FictionBook | ✅ |
| CBZ / ZIP `.cbz` `.zip` | 漫画，按图片页顺序阅读 | ✅ |
| TXT `.txt` | 纯文本，万章级分块 | ✅ |
| Markdown `.md` | 目前按文本渲染，标题层级语义化见 B1 | ✅ |
| 封面图 `.png` `.jpg` `.jpeg` | 手动替换书籍封面 | ✅ |

完整格式清单定义在 `apps/readest-app/src/services/constants.ts` 的 `SUPPORTED_BOOK_EXTS`。

### 阅读体验

| 能力 | 说明 | 状态 |
| --- | --- | --- |
| 翻页 / 滚动双模式 | 随时切换分页阅读与滚动阅读 | ✅ |
| 排版与主题自定义 | 字体、字号、行距、页边距、主题配色、背景 | ✅ |
| 代码语法高亮 | 阅读技术手册时的代码着色 | ✅ |
| 阅读辅助 | 阅读标尺、逐段阅读模式、速读（RSVP）、自动滚动 | ✅ |
| 并行阅读 | 同屏并排阅读两本书 | ✅ |
| 全文搜索 | 书内搜索与当前书架跨书搜索 | ✅ |
| E-ink 模式 | 高对比、去动画、清晰边框，适配 Android E-ink 设备 | ✅ |
| 无障碍 | 完整键盘导航，支持 VoiceOver / TalkBack / NVDA / Orca | ✅ |

### 标注与笔记

| 能力 | 说明 | 状态 |
| --- | --- | --- |
| 高亮 / 书签 / 笔记 | 选中即标注，支持即时模式与标注工具栏 | ✅ |
| 笔记与标注管理 | 侧边栏笔记本、跳转、导出 | ✅ |
| 手写批注 | 触控笔手写（上游在建） | 🚧 |

### 查询、词典与翻译

| 能力 | 说明 | 状态 |
| --- | --- | --- |
| 词典查询 | 内置词典 + 导入自定义词典（Yomitan ZIP、RDICT、StarDict、BGL 等） | ✅ |
| Wikipedia / Wiktionary | 选中词语直接查询 | ✅ |
| 全文翻译 | DeepL / Google / Azure / Yandex，从单句到整章 | ✅ |
| AI 助手 | RAG 问答、AI 翻译与词汇解释（Ollama / OpenRouter / 自建 OpenAI 兼容网关） | ⚠️ 需自备 key |

### 朗读与有声书

| 能力 | 说明 | 状态 |
| --- | --- | --- |
| TTS 朗读 | Edge TTS、系统原生 TTS、Web Speech，多语言、句/词高亮 | ✅ |
| 朗读控制 | 变速、段落间隔、按章节下载音频并缓存 | ✅ |
| 跟读（Read-Along） | EPUB 3 Media Overlays 定时高亮 | ✅ |
| 本地有声书配对 | 可重排 EPUB 与无 DRM 的 MP3 / M4A / M4B 配对播放 | ✅ |
| Audiobookshelf | 接入自建有声书服务 | ✅ |

### 书库、导入与集成

| 能力 | 说明 | 状态 |
| --- | --- | --- |
| 书库管理 | 分组、排序、筛选、搜索、封面管理 | ✅ |
| 文件导入 | 本地文件导入、系统「打开方式」与文件关联 | ✅ |
| OPDS / Calibre | 浏览与下载在线书库、Calibre 书库 | ✅ |
| 网页剪藏 | 用内置浏览器登录并剪藏网页，或抓取网页小说章节与图片 | ✅ |
| 浏览器扩展 | [Send to Readest](https://github.com/readest/readest/tree/main/apps/readest-app/extensions/send-to-readest) | ✅ |
| 局域网传输 | LocalSend 协议，手机与电脑直接传书 | ✅ |
| RSS 订阅 | 订阅源转成可读书籍 | ✅ |
| 第三方集成 | Readwise、Notion、Hardcover、BookOrbit | ✅ |

### 同步与备份

| 能力 | 说明 | 状态 |
| --- | --- | --- |
| 跨设备同步 | 书籍文件、阅读进度、标注、书签；后端可自选 Google Drive / OneDrive / WebDAV / S3 / iCloud | ⚠️ 见下 |
| 与 KOReader 同步 | KOSync：与 KOReader 设备互通进度、书签与笔记 | ✅ |
| 本地备份 | 书库与阅读数据备份、恢复 | ✅ |
| 官方云 | 上游云服务已断开，自有云与自托管服务端尚未上线 | 🚫 暂不可用 |

> 说明：本项目已切断上游的云服务、遥测与更新源（见[品牌化清单](implementation/BRANDING.md)），
> 并把内置的 Google Drive / OneDrive OAuth client id 一并移除。因此跨设备同步要自备后端配置：
> WebDAV / S3 可直接填地址，Drive / OneDrive 需要自己的 OAuth client id。自托管服务端是 B2 的目标。

### 平台与无障碍

| 平台 | 状态 |
| --- | --- |
| Windows / macOS / Linux | ✅ 桌面（Linux 上走 CEF 运行时） |
| Android / iOS | ✅ 移动端，含文件关联与分享入口 |
| Web / PWA | ✅ 浏览器直接使用，能力按平台降级 |

## 二、本项目要做的能力

### Rust Core（已落地）

`crates/` 是本项目自研的 Rust 核心，独立嵌套 workspace，测试与 clippy 均通过
（见[实现进度](implementation/README.md)）：

| 能力 | 说明 | 状态 |
| --- | --- | --- |
| 阅读模型 | 确定性 ID、EPUB CFI、Locator、阅读顺序 | ✅ |
| 文档解析与派生索引 | ZIP / EPUB / XHTML / TXT 解析，生成可重建索引 | ✅ |
| 标注锚点 | 锚点存储、重新定位、锚点合并 | ✅ |
| 存储 | SQLite schema 迁移与仓储（含 FTS5 能力探测） | ✅ |
| 同步内核 | HLC、字段级 LWW、墓碑/复活、进度与笔记合并 | ✅ |
| 协议契约 | Rust / TypeScript 共享的协议夹具与契约测试 | ✅ |

### B1：文档格式与解析能力

| 能力 | 说明 | 状态 |
| --- | --- | --- |
| PDF 文档模型 | 页码定位、文字层抽取、页 + 矩形锚点（当前锚点是 CFI，PDF 需要自己的方案） | 📋 |
| Markdown 语义 | 标题层级 → 目录与章节索引 | 📋 |
| HTML 直开 | 直接打开 `.html`：安全清洗、单文件与资源目录两种形态 | 📋 |
| TXT 索引 | 万章级分块与索引移到 Rust，前端只取窗口 | 📋 |

### B2：差异化能力

| 能力 | 说明 | 状态 |
| --- | --- | --- |
| 书源引擎 | Legado 规则 L0/L1 高覆盖兼容，L2 受限，L3 不承诺 | 📋 |
| 权限化插件宿主 | 桌面 WASM / QuickJS / Sidecar，Android 解释型 + WASM，iOS 声明式 | 📋 |
| 翻译 Job | Wenyi 翻译的任务状态机 + Sidecar / 远程 Worker | 📋 |
| OCR 与漫画翻译 | Provider 化接入，端侧轻量、重任务交给 Worker | 📋 |
| 自托管同步服务端 | Rust + Axum + PostgreSQL + S3 兼容存储，与官方云同一套协议 | 📋 |

## 三、现在还不能用

- **官方云服务**：已断开，未接通自有服务；登录、云同步、商店相关入口按未配置处理。
- **自动更新**：更新源与公钥已换成自有占位（`updates.invalid`），自有更新站点上线前不提供更新。
- **应用商店包**：签名、公证、App Store / Google Play 上架流程均未开始。
- **商标与域名**：`一苇 / Yiwei` 的商标与域名检索尚未完成（不阻塞开发）。

## 四、明确不做（至少前两年）

不做内容平台与盗版书源、不做书店与支付、不做实时协作编辑、不做社交动态与推荐算法、
不承诺 Legado 书源 100% 兼容、不追求所有平台的 E-ink 专用优化。完整理由见
[项目简介](PROJECT.md) 与[调研报告 §0.2](architecture/RESEARCH_AND_ARCHITECTURE.md)。

---

上游能力的原始清单见 [upstream-readme.md](upstream-readme.md) 的 Features 一节；
本项目自己的能力范围以[路线图](implementation/ROADMAP.md)与 [ADR](decisions/) 为准。
