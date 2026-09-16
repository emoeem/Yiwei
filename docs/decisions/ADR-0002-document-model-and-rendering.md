# ADR-0002：Document Model 与渲染

状态：Proposed（待评审）  
日期：2026-09-13

## 背景

需要统一支持 EPUB/PDF/TXT/HTML，并为搜索、AI、翻译、批注提供稳定数据模型；同时不能破坏 EPUB 原始信息。

## 决策

- 原始书籍文件不可变，按内容哈希存储。
- 内部 Document Model 是**派生索引**，可重建，不写回原书。
- EPUB/FB2/MOBI/CBZ 使用 foliate-js；PDF 使用 PDF.js；排版基线使用 Readium CSS。
- 阅读位置使用 Locator：CFI/XPointer/Text Quote/block 偏移/文档指纹组合。
- 批注锚点独立存储，支持重定位与 orphan 状态。

## 考虑的替代方案

- 全部转换为自有格式：会丢失 EPUB 样式、脚注、锚点与固定版式信息。
- 全部使用原生引擎：跨端语义分裂，插件与无障碍成本高。
- epub.js 作为主引擎：维护与性能不如 foliate-js。

## 后果

正面：信息无损；可重建索引；跨端渲染语义一致；便于批注与 AI 对齐。

负面：需要处理原始 EPUB 的复杂性；foliate-js API 未稳定，需要固定版本与适配层；PDF 仍需单独优化内存。
