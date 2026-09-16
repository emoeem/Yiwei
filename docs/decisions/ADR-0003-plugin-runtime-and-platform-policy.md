# ADR-0003：插件运行时与平台策略

状态：Proposed（待评审）  
日期：2026-09-13

## 背景

插件是一等公民，但 iOS/Android 商店政策不允许任意下载/执行会改变应用功能的代码。KOReader 式无沙箱 Lua 插件不可接受。

## 决策

分层插件运行时：

1. `declarative`：纯数据规则，全平台。
2. `quickjs`：受限 JS，桌面/Android/Web；iOS 仅内置精选。
3. `wasm`：WASI 能力制，桌面/Android/Web；iOS 使用解释器或内置 AOT。
4. `sidecar`：独立进程/HTTP，桌面与服务器。
5. `ui`：CSP + iframe sandbox 的 UI 插件。

插件必须带 Manifest、权限、签名、版本与平台可用性；宿主在边界强制执行权限。

## 后果

正面：跨端安全模型可控；符合商店政策；插件能力可以渐进增强。

负面：插件 API 设计成本上升；iOS 插件体验弱于桌面；需要签名与市场审核基础设施。

## 明确禁止

- 插件直接访问文件系统、数据库、Keychain。
- 插件获取 AI API Key。
- 下载并执行原生 .so/.dll/.dylib。
- 无上限的 CPU/内存/网络。
