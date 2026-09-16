# ADR-0005：许可证策略

状态：Accepted（2026-09-16 修订，见文末）  
日期：2026-09-13（2026-09-15 修订）

## 背景

项目需要开源，同时希望最大化复用成熟阅读器（Readest、KOReader）的界面实现与交互代码。原计划分层许可（客户端 Apache-2.0、服务端 AGPL-3.0）以兼容 App Store，但该限制严重阻碍了开发效率——无法直接借鉴 Readest 的 UI 组件实现，导致前端必须从零构建。

## 决策（修订）

- **整个项目统一为 AGPL-3.0**：Rust Core、TS 客户端、桌面壳、未来的服务端与插件 SDK。
- **可以复制/借鉴 Readest、KOReader 等 AGPL-3.0 项目的代码**——它们是 AGPL-3.0，与我们的许可证兼容。
- **文档**：AGPL-3.0。
- **第三方插件**：作者自选，但必须与 AGPL-3.0 兼容。

## 后果

正面：开发效率大幅提升；可以直接复用 Readest 的 UI 组件、交互实现、foliate-js 适配层经验；网络服务源码义务倒逼官方云同步。

负面：不能上架 App Store（AGPL 与 App Store 分发机制冲突）；任何网络服务端也必须开源；企业客户采用门槛提高。

## 约束

- 所有依赖必须与 AGPL-3.0 兼容（MIT/Apache/BSD/MPL 等宽松许可均兼容；GPL-2.0/GPL-3.0 需评估组合义务与版本升级兼容性）。
- SBOM 许可证扫描持续运行。
- 商业发布前律师复核。

## 2026-09-16 修订

1. **更正「不能上架 App Store」的结论。** 该说法与事实不符：上游 Readest 本身即为
   AGPL-3.0，并已上架 App Store 与 Google Play
   （<https://apps.apple.com/app/id6738622779>）。以 Readest 为底座
   （[ADR-0007](ADR-0007-readest-as-product-base.md)）不改变其分发路径，也不额外引入
   App Store 相关的许可证障碍。AGPL 的真实义务在源码提供与网络服务开源，而非商店分发本身。
2. **明确复用范围。** 因统一为 AGPL-3.0，Readest 与 KOReader（均为 AGPL-3.0）的代码可直接
   进入本项目；GPL-3.0 项目（Legado、Calibre、Calibre-Web）仍需逐项评估。
3. **修正上一版约束条目的措辞笔误**：原文「排除 MIT/Apache/BSD/MPL 等宽松许可均兼容」
   语义自相矛盾，已改为「MIT/Apache/BSD/MPL 等宽松许可均兼容」。
