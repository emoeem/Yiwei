# 品牌化清单

本仓库是 Readest 的 fork。**截至 2026-09-16，P0 已完成**：应用不再连接上游的
Supabase / PostHog / Stripe / 更新源，不会再向 `*.readest.com` 发起请求，也不会把自己
更新回原版 Readest。

判定依据：ADR-0007（以 Readest 上游为产品底座）。产品名已定为 **一苇 / Yiwei**，
正式发布前仍需商标与域名检索（见文末「发布前仍需完成」）。

## 已完成（2026-09-16）

### 1. 云服务与遥测不再指向上游

| 位置 | 改动 |
| --- | --- |
| `apps/readest-app/.env` | 移除上游 Supabase / PostHog / Stripe 的 base64 凭据，改为自建配置说明 |
| `src/utils/supabase.ts` | base64 解码容错；未配置时指向 RFC 2606 保留的 `*.invalid`，导出 `isCloudConfigured()` |
| `src/context/PHContext.tsx` | 同上；无 key 时完全不初始化 PostHog |
| `src/libs/payment/stripe/client.ts` | 无 publishable key 时返回 `null` client，而不是抛错 |
| `src/services/constants.ts` | `READEST_WEB_BASE_URL` / `NODE` / 更新源 / 存储 / 资源域名全部改为保留域名；`SEND_EMAIL_DOMAIN` 同理 |
| `src/middleware.ts` | CORS 允许的来源列表移除 `web.readest.com` |
| `src/services/sync/providers/{gdrive,onedrive}` | 移除上游内置 OAuth client id，未配置时该同步后端不可用（UI 自动隐藏） |

### 2. 自动更新源

| 位置 | 改动 |
| --- | --- |
| `src-tauri/tauri.conf.json` → `plugins.updater.endpoints` | 改为 `https://updates.invalid/latest.json`（自有更新源就绪后替换） |
| `src-tauri/tauri.conf.json` → `plugins.updater.pubkey` | 换成自有 minisign 公钥 |
| `src/services/constants.ts` → `READEST_UPDATER_PUBKEY` | 同步为同一把自有公钥（此前与上游 pubkey 重复，必须一致） |
| `bundle.createUpdaterArtifacts` | 置为 `false`：尚无签名密钥的本地构建不会产出更新包 |
| `wrangler.toml` | 路由改为注释状态，worker 名与 R2 bucket 改自有命名 |

新建的密钥对：

```
私钥：~/.tauri/yiwei-updater.key        ← 不要提交，不要丢失
公钥：~/.tauri/yiwei-updater.key.pub    ← 已写入 tauri.conf.json 与 constants.ts
```

发布更新前必须：把私钥存入 CI secret，以 `TAURI_SIGNING_PRIVATE_KEY` 注入，
把 `createUpdaterArtifacts` 改回 `true`，并把 endpoints 指向自有 release 站点。

### 3. 应用身份

| 位置 | 由 → 改为 |
| --- | --- |
| `src-tauri/tauri.conf.json`：`productName` / `mainBinaryName` / `identifier` | `Readest` / `readest` / `com.bilingify.readest` → `Yiwei` / `yiwei` / `com.yiwei.reader` |
| `.env.tauri` `DBUS_ID`、`src-tauri/src/lib.rs` 的 `dbus_id` | `com.bilingify.readest` → `com.yiwei.reader` |
| `src-tauri/Info-ios.plist` | URL name、scheme、iCloud 容器、权限说明文案 → Yiwei |
| `plugins.deep-link`（mobile + desktop） | `readest` / `readest-onedrive` → `yiwei` / `yiwei-onedrive`；移除上游 Google OAuth reverse-DNS scheme |
| `bundle.fileAssociations` 的 `com.readest.fb2` / `.cbz` | → `com.yiwei.fb2` / `.cbz` |
| `plugins.cli.description` | → `Yiwei CLI` |
| `bundle.assetProtocol.scope` 的 `**/Readest/**/*` | → `**/Yiwei/**/*`（必须与 `DATA_SUBDIR` 一致） |
| `src/services/constants.ts`：`DATA_SUBDIR` | `Readest` → `Yiwei` |
| `public/manifest.json` | PWA `name` / `short_name` / `description` |
| `data/metainfo/appdata.xml` + `.sha256` | id / name / summary / URL / keyword，sha256 已重算 |
| `public/.well-known/apple-app-site-association` | appID 改为 `TEAMID.com.yiwei.reader`（需填真实 Team ID） |
| `src-tauri/src/discord_rpc.rs` | 按钮文案与链接 |
| 深链 scheme 全量改名 | `src/**` 与 `src-tauri` 内 69 处 `readest://` → `yiwei://`；`src/utils/{deeplink,share}.ts`、`hooks/useOpenWithBooks.ts` 的协议判断、`services/send/conversion/convertToEpub.ts` 的标识前缀 |
| `src/utils/share.ts` | 分享链接的 host 校验改为从 `READEST_WEB_BASE_URL` 推导，不再硬编码 `.readest.com` |

验证：`pnpm --filter @readest/readest-app lint`（tsc + biome）通过；
前端全量测试 11085 passed / 0 failed（改动前基线为 11072 passed / 13 failed，
13 个失败全部是本清单引起的硬编码期望值，已随之更新）。

### 4. 原生工程身份（2026-09-16）

`src-tauri/gen` 在 `.gitignore` 里，但其中若干部件是被强制跟踪的——它们就是原生工程的**事实来源**，
Tauri 构建时不会重新生成（只在缺失时 `tauri android/ios init`）。因此这里做了手工迁移：

| 位置 | 改动 |
| --- | --- |
| `gen/android/app/build.gradle.kts` | `namespace` / `applicationId` → `com.yiwei.reader` |
| `gen/android/app/src/{main,test}/java/com/yiwei/reader/` | Kotlin 包路径由 `com.bilingify.readest` 迁入 |
| `gen/android/.../AndroidManifest.xml` | scheme `readest` / `readest-onedrive` → `yiwei` / `yiwei-onedrive`，host → `yiwei.invalid`；移除上游 Google Drive OAuth 的 reverse-DNS filter |
| `gen/android/.../res/values/themes.xml` | `Theme.readest` → `Theme.yiwei` |
| `gen/apple/Yiwei.xcodeproj`（原 `Readest.xcodeproj`） | 工程与 target 改名：`Readest_iOS` → `Yiwei_iOS`、`ReadestWidget` → `YiweiWidget`；`PRODUCT_NAME` → `Yiwei` |
| `gen/apple/project.yml` | `name`、`bundleIdPrefix`、target 名与源路径同步；移除上游 `DEVELOPMENT_TEAM: J5W48D69VR` |
| `gen/apple/**/*.entitlements` | App Group → `group.com.yiwei.reader`；iCloud 容器 → `iCloud.com.yiwei.reader`；associated domains → `applinks:yiwei.invalid` |
| `src-tauri/Info.plist`（macOS） | iCloud 容器、UTI `com.readest.fb2/cbz` → `com.yiwei.*`、权限说明文案 |
| `src-tauri/plugins/**` | iOS App Group suite 名、Android/iOS widget 深链 `readest://book/` → `yiwei://book/`、CarPlay UserDefaults key `readest.carplay.*` → `yiwei.carplay.*` |
| `src-tauri/src/{dir_scanner,transfer_file}.rs` | 数据目录回退判断 `"Readest"` → `"Yiwei"`。**这是功能性改动**：它与前端 `DATA_SUBDIR` 必须一致，否则自有目录会被文件访问校验拒绝 |

> **未验证项**：Xcode 工程只能在 macOS 上验证。第一次在 Mac 上打开时请先跑 `xcodebuild -list`；
> 若工程名/target 引用有出入，在 `gen/apple` 下用 `env -u FORCE_COLOR xcodegen generate` 从 `project.yml`
> 重新生成 pbxproj（`FORCE_COLOR` 会被 xcodegen 展开进构建脚本，必须在干净 shell 执行——上游踩过这个坑）。

### 5. 图标与启动图

源图与派生素材都入库在 `data/icons/yiwei/`：

| 文件 | 用途 |
| --- | --- |
| `icon.png` | 原始方图（1254×1254，不透明：暖纸底 `#F6F3EB` + 墨黑苇秆 + 青瓷绿 `#33706F` 苇叶与水线） |
| `android-fg.png` | 抠掉纸底的前景层（带透明通道），供 Android 自适应图标使用 |
| `android-monochrome.png` | 单色前景层（Android 13+ 主题图标） |
| `manifest.json` | `tauri icon` 的输入清单（`bg_color: #F6F3EB`） |

生成命令（在 `apps/readest-app` 下执行）：

```bash
./node_modules/.bin/tauri icon ../../data/icons/yiwei/manifest.json
```

产出 `src-tauri/icons/*`（含 `.icns` / `.ico` / Windows 磁贴）、`icons/ios/*`、
`gen/android/.../res/mipmap-*`，并写入 `res/values/ic_launcher_background.xml` 的品牌底色。

两个坑必须知道：

1. **`tauri icon` 会重写 `res/mipmap-anydpi-v26/ic_launcher.xml` 并丢掉 `inset`。** 苇梢在画布上的最大半径是
   半画布的 68.9%，超过 Android 保证可见的 61% 安全圆，圆形/圆角遮罩会裁到苇叶；本仓库保留 `inset 9%`
   把最大半径压到 57%。**重新生成图标后要确认这两行 `inset` 还在。**
2. Web/PWA 与启动图不由 `tauri icon` 管理，另行生成：`public/icon.png`(512)、`apple-touch-icon.png`(180)、
   `favicon.ico`(16/32/48/64)、`icon-tiny.png`(224，Discord 小图)、
   `gen/android/.../drawable/splash_icon.png`(432，透明前景)。启动背景同时改成纸色 `#FFF6F3EB`——
   墨黑图标在原来的深灰 `#FF323130` 上几乎看不见。

### 6. 构建产物名与发布脚本

`productName` 变为 `Yiwei` 后产物名同步为 `Yiwei.app` / `Yiwei.ipa` / `Yiwei.pkg` / `Yiwei_*.apk`。
已同步：`apps/readest-app/package.json`（`dev-ios` / `dev-ios-sim` / `dev-macos`）、
`scripts/{fix-ios-appstore-appgroup,release-ios-appstore,release-mac-appstore,verify-ios-appstore-entitlements}.sh`、
`.github/workflows/{nightly,release,try-appimage}.yml`、`src-tauri/tauri.macos-nonestore.conf.json`，
以及 provisioning profile 文件名 `ReadestDeveloperID` → `YiweiDeveloperID`。

## 发布前仍需完成

### P1 — 需要你的账号、证书与后端

| # | 位置 | 说明 |
| --- | --- | --- |
| 1 | `src-tauri/profiles/YiweiDeveloperID.provisionprofile` | 文件名已改，但**内容仍是上游签发的 profile**（含其开发者身份）。必须用 Apple 后台为 `com.yiwei.reader` 签发的替换 |
| 2 | iOS / Android 签名与 Team ID | `DEVELOPMENT_TEAM` 已移除。打包前回填自有 Team ID，并在 Apple / Google 后台注册 `com.yiwei.reader`、`com.yiwei.reader.YiweiWidget`、`com.yiwei.reader.ShareExtension`、App Group `group.com.yiwei.reader`、iCloud 容器 `iCloud.com.yiwei.reader` |
| 3 | `fastlane/`（`Appfile`/`Fastfile`/`metadata*`/`metadata-play`/`screenshots`） | 商店列表、截图与发布流程 |
| 4 | `public/.well-known/{assetlinks.json,apple-app-site-association}` | Android App Links 的 `package_name` 已改为 `com.yiwei.reader`，两个证书指纹留了 `REPLACE_WITH_...` 占位（原值是上游 keystore 的）；Universal Links 的 appID 为 `TEAMID.com.yiwei.reader`，需填真实 Team ID。两者都必须在自有域名下提供 |
| 5 | 包 scope `@readest/monorepo`、`@readest/readest-app` | 改动需一次性覆盖全部 `--filter` 脚本与 CI。Rust **包名**仍为 `Readest`（`cargo -p Readest`）——dev 二进制名已通过 `[[bin]] name = "yiwei"` 与 `mainBinaryName` 对齐，但包名如需一并改，必须同步 `package.json` 的三个 cargo 脚本与 Sentry release 名（`sentry_config.rs` 用 `CARGO_PKG_NAME`，`scripts/upload-sourcemaps.mjs` 里是 `Readest@<version>`），并重写 `Cargo.lock` 与 `Cargo.cef.lock` |
| 5.1 | Linux 任务栏图标 | dev 下已解决：`src-tauri/Cargo.toml` 显式声明 `[[bin]] name = "yiwei"`（app_id 随之变为 `yiwei`），窗口图标显式取 `bundle.icon` 的嵌入值，并把 `yiwei.png` 按尺寸装入 `~/.local/share/icons/hicolor/*/apps/`。**打包发行版后要复核**安装到 `/usr/share/icons/hicolor/*/apps/` 的图标文件名与 app_id 一致（Waybar 按 app_id 查主题图标，见 [LINUX-DEV.md](LINUX-DEV.md)） |
| 6 | 自有云后端 | 按 `.env` 注释设置 `NEXT_PUBLIC_SUPABASE_URL` / `NEXT_PUBLIC_API_BASE_URL` 后方可启用同步、分享、元数据搜索、Edge TTS |
| 7 | 自有 OAuth | `NEXT_PUBLIC_GOOGLE_CLIENT_ID` / `_WEB_CLIENT_ID` / `NEXT_PUBLIC_MICROSOFT_CLIENT_ID`，并注册对应 redirect URI（含 `yiwei-onedrive://auth`） |
| 8 | 自有更新源 | endpoints + `TAURI_SIGNING_PRIVATE_KEY`（私钥在 `~/.tauri/yiwei-updater.key`）+ `createUpdaterArtifacts: true` |

### P2 — 文案与素材

| # | 位置 | 说明 |
| --- | --- | --- |
| 9 | `public/locales/*` | 用户可见文案中的产品名（i18n 为 key-as-content，key 本身即英文原文） |
| 10 | 仍指向上游的链接与文案 | `app/layout.tsx` 元数据、`components/LegalLinks.tsx` 服务条款/隐私、`app/error.tsx` 与订阅页的 `support@readest.com`、`components/landing/PageFooter.tsx`、`app/o/page.tsx`、`api/share/.../og.png/render.tsx`、`services/ai/providers/OpenRouterProvider.ts` 的 HTTP-Referer、`pages/api/send/fetch-url.ts` 的 UA、`services/notion/NotionClient.ts` 的 marker origin |
| 11 | 仍指向上游 CDN 的资源（已被 CSP 拦截，表现为功能降级而非静默请求） | `styles/fonts.ts` 的 `DEFAULT_FONT_BASE_URL`（`storage.readest.com/public/font/dist`）、`services/wordlens/glossPacks.ts` 的 `WORDLENS_CDN_BASE`（`cdn.readest.com/wordlens`）。换成自有 CDN 或改用打包内置资源 |
| 12 | `README.md`、`SECURITY.md`、`CONTRIBUTING.md`、`CODE_OF_CONDUCT.md` | 仓库首页与社区文档 |
| 13 | `apps/readest-app/e2e/`、`.github/workflows/*` | 快照与选择器（产物名已改，断言里的品牌文案未改） |

### 名称与合规

- **商标检索**：`一苇` / `Yiwei` 需在 CNIPA（第 9、41、42 类）与目标市场检索；
  `yiwei.com` 已被占用，`yiwei.app` / `yiwei.dev` / `yiwei.io` 的 DNS 未解析（需在注册商确认）。
- 已排除的候选：`余白 / Yohaku`（与 `Innei/Yohaku` 排版设计系统冲突）、
  `涵泳`（与 `yhnocoder/grasp`「涵泳」阅读批注产品冲突）。
- 域名与商店名称确认后，`identifier`（`com.yiwei.reader`）一旦发布不可更改。

## 保留项（不要改）

- **LICENSE 与上游归属。** 本项目与上游同为 AGPL-3.0。上游版权声明、`NOTICE` 类信息与
  foliate-js 等第三方许可证必须保留。
- **AGPL 源码义务。** 分发二进制并提供网络服务时，必须提供完整对应源码。
- **`upstream` remote。** 保留 `https://github.com/readest/readest.git` 以持续 `rebase`。

## 验证方式

改完后必须确认：

1. 构建产物启动后不向 `*.readest.com`、`*.supabase.co`、`*.posthog.com` 发起请求
   （抓包或检查 CSP 报告）。
2. 自动更新检查指向自有更新源，且签名公钥与自有私钥匹配。
3. 打包产物在 Windows / macOS / Linux 上安装后 `productName` 与图标正确。
4. 移动端 `identifier` 与签名团队正确，能通过各自商店的上传校验。
