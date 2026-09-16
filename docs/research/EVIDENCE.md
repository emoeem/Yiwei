# 调研证据与来源清单

调研日期：2026-09-13  
方法：优先读取官方仓库、README、LICENSE、docs 与源码；商业产品优先官方页面；政策条款优先官方原文。  
限制：GitHub 未认证 API 出现共享出口速率限制，因此改为 `git clone --depth 1` 与 `raw.githubusercontent.com` 读取。部分商业产品页面有反爬（Kobo 返回 Challenged，PocketBook 证书过期，Amazon 页面为空），这些产品的功能矩阵在报告中已标注为公开资料或产品体验，需在实现前复核。

## 1. 源码仓库（已本地精读）

| 仓库 | 版本/提交 | 许可证 | 精读内容 |
| --- | --- | --- | --- |
| https://github.com/BigDawnGhost/wenyi | v0.7.0；commit 818e70b（2026-09-10） | MIT | README、docs/pipeline.md、docs/configuration.md、docs/usage.md、trans_novel/pipeline/runstore.py、assemble/epub_writer.py、cli.py、llm 配置与 operations |
| https://github.com/readest/readest | v0.12.8；commit c3a95ba（2026-09-11） | AGPL-3.0 | README、LICENSE、package.json、Cargo.toml、tauri.conf.json、src/types/book.ts、src/libs/crdt.README.md、src/libs/replicaSchemas.ts、同步与数据库测试线索 |
| https://github.com/gedoor/legado | 官方仓库仅剩维权公告 | GPL-3.0（历史） | README 公告 |
| https://github.com/yudonw/Read3.0 | commit 8503854（2025-06-26） | GPL-3.0 | BookSource.kt、规则实体、analyzeRule/*.kt、AnalyzeUrl.kt、api.md、LICENSE |
| https://github.com/koreader/koreader | v2026.07.1；commit 7e6c9a4（2026-09-13） | AGPL-3.0 | README、COPYING、frontend/pluginloader.lua、reader modules 列表、statistics 插件 |
| https://github.com/johnfactotum/foliate-js | commit 78914ae（2026-05-01）；无正式 Release | MIT | README（格式、接口、安全警告）、LICENSE、epubcfi.js、paginator.js、overlayer.js、search.js、view.js |
| https://github.com/futurepress/epub.js | commit eee359d（2026-03-23） | BSD-3-Clause | README、LICENSE |
| https://github.com/readium/readium-css | 活跃 | BSD-3-Clause | README（paged/scrolled、theming、user settings、accessibility、i18n） |
| https://github.com/readium/webpub-manifest | 活跃 | BSD-3-Clause | LICENSE |
| https://github.com/hiroi-sora/Umi-OCR | 活跃 | MIT | README（Windows 7 x64/Linux x64、HTTP/CLI） |

## 2. 版本与发布证据

| 项目 | 证据 |
| --- | --- |
| Readest | `https://github.com/readest/readest/releases/latest` 重定向到 `v0.12.8` |
| Wenyi | `https://github.com/BigDawnGhost/wenyi/releases/latest` 重定向到 `v0.7.0` |
| KOReader | `https://github.com/koreader/koreader/releases/latest` 重定向到 `v2026.07.1` |
| foliate-js | Releases 页面无正式 Release；本地 commit 78914ae |
| Legado 官方 | 仓库 README 仅维权公告，宣布删除项目内容 |

## 3. 官方文档/页面

| 来源 | 用途 |
| --- | --- |
| https://www.apple.com/apple-books/ | Apple Books 官方定位与跨设备体验 |
| https://weread.qq.com/ | 微信读书官方描述：正版书籍、小说、漫画、公众号、听书、多设备同步 |
| https://www.duokan.com/ | 多看阅读官方描述：Android/iPhone/iPad/Kindle |
| https://du.163.com/ | 网易蜗牛读书官方站点 |
| https://play.google.com/books | Google Play Books 官方描述：电子书、有声书、漫画、跨设备 |
| https://v2.tauri.app/start/ | Tauri 2 官方文档 |
| https://www.jetbrains.com/help/kotlin-multiplatform-dev/get-started.html | Kotlin Multiplatform 官方文档 |
| https://capacitorjs.com/docs | Capacitor 官方文档 |
| https://reactnative.dev/docs/getting-started | React Native 官方文档 |
| https://docs.flutter.dev/platform-integration | Flutter 平台集成官方文档 |
| https://docs.expo.dev/ | Expo 官方文档 |
| https://specs.opds.io/opds-2.0 | OPDS 2.0 规范 |
| https://www.w3.org/TR/annotation-model/ | W3C Web Annotation Data Model |
| https://www.w3.org/TR/epub-33/ | EPUB 3.3 |
| https://idpf.org/epub/linking/cfi/epub-cfi.html | EPUB Canonical Fragment Identifiers 1.1 |
| https://developer.apple.com/app-store/review/guidelines/ | Apple App Store Review Guidelines 2.5.2 原文 |
| https://support.google.com/googleplay/android-developer/answer/9888379 | Google Play Device and Network Abuse / 动态代码限制 |
| https://en.wikipedia.org/wiki/Z-Library | Z-Library 的公开法律状态与“shadow library”描述（仅用于风险评估，非法律依据） |
| https://manual.calibre-ebook.com/server.html | Calibre Content Server 官方文档 |

## 4. 许可证证据

| 组件 | 证据 |
| --- | --- |
| Wenyi | 本地 LICENSE：MIT |
| Readest | 本地 LICENSE：AGPL-3.0 |
| KOReader | 本地 COPYING：AGPL-3.0 |
| Read3.0/Legado | 本地 LICENSE：GPL-3.0 |
| foliate-js | 本地 LICENSE：MIT |
| PDF.js | raw LICENSE：Apache-2.0 |
| Readium Kotlin/Swift/CSS/Manifest | raw LICENSE：BSD-3-Clause |
| Tauri | README 徽章与 Licenses 章节：MIT 或 Apache-2.0 |
| Flutter | raw LICENSE：BSD-3-Clause（Flutter 版权头） |
| React Native | raw LICENSE：MIT |
| Expo | raw LICENSE：MIT |
| Capacitor | raw LICENSE：MIT |
| Compose Multiplatform / KMP | raw LICENSE.txt：Apache-2.0 |
| Wasmtime | raw LICENSE：Apache-2.0 with LLVM exception |
| Extism | raw LICENSE：BSD-3-Clause |
| Automerge | raw LICENSE：MIT |
| Yjs | raw LICENSE：MIT |
| mlua | raw LICENSE：MIT |
| QuickJS | raw LICENSE：MIT |
| Umi-OCR | raw LICENSE：MIT |
| PaddleOCR / Tesseract / EasyOCR / RapidOCR | raw LICENSE：Apache-2.0 |
| MinerU | raw LICENSE.md：Apache-2.0 + 附加条款（MAU > 1 亿或月收入 > 2000 万美元需商业许可；在线服务需标注） |
| MinIO | raw LICENSE：AGPL-3.0 |
| SeaweedFS | raw LICENSE：Apache-2.0 |
| PostgreSQL | raw COPYRIGHT：PostgreSQL License |

## 5. 商店政策原文摘要

Apple App Store Review Guidelines 2.5.2：

> Apps should be self-contained in their bundles, and may not read or write data outside the designated container area, nor may they download, install, or execute code which introduces or changes features or functionality of the app, including other apps.

Google Play Device and Network Abuse 政策：

> An app distributed via Google Play may not modify, replace, or update itself using any method other than Google Play's update mechanism. Likewise, an app may not download executable code (such as dex, JAR, .so files) from a source other than Google Play. This restriction does not apply to code that runs in a virtual machine or an interpreter where either provides indirect access to Android APIs (such as JavaScript in a webview or browser).

> Apps or third-party code, like SDKs, with interpreted languages (JavaScript, Python, Lua, etc.) loaded at run time (for example, not packaged with the app) must not allow potential violations of Google Play policies.

## 6. 未完成/待复核

- Kobo、PocketBook、Amazon 部分页面抓取失败，竞品功能需在实现前人工复核。
- GitHub API 速率限制导致 stars/forks 等元数据未作为决策依据；本报告不依赖这些数字。
- 商业产品的 AI 功能随版本变化，矩阵中的“AI”列只能作为方向参考。
- 本报告不是法律意见；MinerU 附加条款、AGPL/GPL 边界、App Store 分发必须由律师复核。
- OCR/LLM 模型权重许可证未逐项审计，必须在打包前完成。
