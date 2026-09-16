# ADR-0007：以 Readest 上游为产品底座

状态：Accepted（2026-09-16）
日期：2026-09-16
取代：报告 §0.2 第 6 条、§4.2 第二条、§25.3、§25.4 第 1 条（详见「被本 ADR 取代的结论」）

## 背景

Phase 0 报告与 ADR-0001～0006 的共同前提是「自研客户端，只借鉴设计、不复制受限代码」。
该前提在 ADR-0005 修订为全项目 AGPL-3.0 之后已经失效：许可证不再构成复用障碍。

实现推进到 MVP 1 前端阶段后，暴露了两个事实：

1. **自研 UI 是本项目最大的成本项，且不产生任何差异化。** 排版、翻页、批注交互、设置面板、
   无障碍、e-ink、i18n、六端适配都是成熟阅读器已经解决的问题。
2. **reader-platform 已有的 Rust Core 与 Readest 的实现大面积重复：**

| reader-platform（自研） | Readest 上游 |
| --- | --- |
| `crates/sync`：HLC、字段级 LWW、墓碑、Op 日志 | `src/libs/crdt.ts`、`replicaSyncClient/Server.ts`、`hlcStore.ts`、`replicas` 表 |
| `crates/annotation`：锚点模型与重定位流水线 | `src/services/annotation/` + 批注/笔记本 UI |
| `crates/storage`：客户端 SQLite schema 与仓储 | `src/services/database/`（Turso/libSQL 迁移） |
| `crates/document`：EPUB/TXT 解析与派生索引 | `src/services/bookContent.ts`、`src/libs/document.ts`、foliate-js |
| `packages/reader-web`：ReaderEngine 适配层 | `src/app/reader/components/FoliateViewer.tsx` + `packages/foliate-js` fork |

并行维护两套等价实现直接违背项目原则 5（不以牺牲长期可维护性换取 MVP 速度）。

## 决策

1. **产品底座 = `emoeem/readest` fork（本仓库）。** 客户端 UI、阅读体验、六端打包、i18n、
   无障碍、e-ink、设置系统均以上游实现为准，不再自研。
2. **Rust Core 收窄为「上游没有的能力」**，不再承担客户端本地存储、客户端同步、批注 UI
   与渲染引擎职责。收窄后的 Rust 落点：书源引擎、权限化插件宿主、Wenyi/OCR Job 编排、
   自托管同步服务端，以及需要跨端与服务端一致性的文档模型与锚点算法。
3. **客户端数据层短期不改。** 本地库、阅读状态、批注继续使用上游 Turso/TS 实现；只有当某项
   能力由 Rust 提供且能证明收益时才替换，并且逐项走 ADR，不批量改写上游文件。
4. **差异代码以新增目录为主**：`crates/`、`server/`、`plugins/`。尽量不修改上游文件，以保住
   `git rebase upstream/main` 的能力。
5. **本项目文档并入本仓库 `docs/`**，作为产品规范与实现记录；自研 Vite 客户端与 vendor 的
   foliate-js 退役（见「退役清单」）。

### Rust workspace 安排

`crates/` 是嵌套的独立 Cargo workspace，并由根 `Cargo.toml` 的 `exclude` 排除在 readest
workspace 之外。理由：

- 上游 `Cargo.toml`、`Cargo.lock`、`Cargo.cef.lock` 完全不动，rebase 时不会与 Cargo
  依赖图解析冲突（加到 `exclude` 是本 ADR 对上游清单的唯一改动）。
- Rust Core 可以保留 edition 2024 与自己的 `[workspace.lints]`；上游 Tauri 壳是 edition 2021
  且通过 `Cargo.cef.lock` 钉版本，强行合并会互相牵制。
- 代价是 CI 需要跑两条构建命令，以及未来若客户端需要直接依赖某个 crate，需要显式处理
  跨 workspace 的路径依赖。

## 后果

正面：

- 直接获得已交付的六端阅读器产品，MVP 1 从「数人月自研」变为「集成 + 品牌化」。
- 不必再实现排版、TTS、词典、AI 助手、OPDS、阅读统计、笔记本等已完成能力。
- Rust Core 的每一行代码都有明确且不重复的消费者。

负面：

- **承担上游追踪成本。** 缓解手段：新增目录为主 + 逐项 ADR + 定期 `rebase upstream/main`。
- **产品身份与上游耦合**（README、包名、i18n key、遥测默认值、Supabase/PostHog 默认端点、
  Apple 团队 ID、自动更新源），必须完成 [品牌化清单](../implementation/BRANDING.md) 才能发布。
- 上游是 AGPL-3.0，且已上架 App Store / Google Play（`apps.apple.com/app/id6738622779`）。
  本项目继承同一条分发路径，也继承同一套合规义务。ADR-0005 中「AGPL 与 App Store 分发机制
  冲突、不能上架 App Store」的结论与上游事实不符，已在本次修订中更正。

## 被本 ADR 取代的结论

阅读报告时以本 ADR 为准，以下结论不再有效：

- §0.2 第 6 条「以某个现成项目为『底座』直接改造」——已改为本项目明确采用的方案。
- §4.2「Readest 只能参考设计、不能复制代码」——同为 AGPL-3.0，可直接复用。
- §25.3 分层许可证（客户端 Apache-2.0）——已由 ADR-0005 修订版取代，全项目 AGPL-3.0。
- §25.4 第 1 条「不要复制 Readest 代码到 Apache-2.0 模块」——前提消失。
- §34 第 1～2 条「先冻结 ADR 再进入实现」——实现按 Accepted 记录推进，不再阻塞。

## 退役清单

| 资产 | 处理 | 原因 |
| --- | --- | --- |
| `reader-platform/apps/client`（自研 Vite UI，约 3.4k 行） | 退役，原仓库保留作参考 | 由上游 UI 取代 |
| `reader-platform/packages/reader-web` + vendor 的 foliate-js | 退役 | 上游有 `packages/foliate-js` fork，避免两份引擎长期分叉 |
| `crates/sync` 的客户端角色 | 移出客户端 | 上游 `libs/crdt.ts` 已在产线运行；Rust 侧保留给服务端 |
| `crates/storage` 的客户端角色 | 移出客户端 | 上游 Turso 数据层不改；Rust 侧保留给服务端 Postgres |

## 不在本 ADR 范围内

产品名称已定为 **一苇 / Yiwei**（2026-09-16）。命名过程与排除理由、以及发布前仍必须完成的
品牌化事项记录在 [品牌化清单](../implementation/BRANDING.md)；商标与域名检索仍未完成，
不阻塞开发。上游技术目录名仍沿用，未做重命名。
