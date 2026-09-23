# 更新签名密钥与轮换

应用内更新只接受用固定公钥验证通过的安装包，所以**签名密钥就是发布链的根信任**：谁能拿到私钥，谁就能签出被所有已安装版本当作正式更新接受的包。这一页说明这把密钥放在哪、被哪些文件引用、以及轮换时必须同步什么。

## 密钥的职责

更新链路是这样的：

1. 发布流程用私钥为每个可更新产物生成 minisign 签名；
2. 签名值写进 `latest.json` 的 `signature` 字段；
3. 已安装的应用下载产物，用**编译进二进制**的公钥验签，通过才安装。

应用只认这一把公钥。公钥漏换或换错时的表现是：更新检查能发现新版本，一安装就报 `unsupported update signature`（`minisign` 的 `UnexpectedKeyId`）。项目早期就出现过一次这样的事故：CI 侧换了密钥，应用源码里的常量没跟着换。

## 当前密钥

- minisign key id：`CA3A638733F9C706`
- 生成方式：`npx @tauri-apps/cli signer generate`
- 私钥只存在于 GitHub Actions secrets；仓库、文档与日志里都不允许出现私钥材料

## 密钥值存放在哪里

| 值 | 位置 | 说明 |
| --- | --- | --- |
| 私钥 | GitHub Actions secret `TAURI_SIGNING_PRIVATE_KEY_B64` | 发布流程签名的唯一来源 |
| 私钥口令 | GitHub Actions secret `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | 与私钥成对使用 |
| 公钥 | 见"哪些文件引用签名公钥" | 公钥不是机密，随仓库分发 |

私钥与口令**必须**在仓库之外另存一份（密码管理器或加密备份）。GitHub secrets 丢失且没有本地备份时，就再也签不出能被已安装版本接受的更新。

## 哪些文件引用签名公钥

轮换密钥时下面几处必须同时改：

- `.github/workflows/release.yml` 的 `TAURI_UPDATER_PUBLIC_KEY_B64`：发布流程用它验签
- `.github/workflows/continuous-build.yml` 的同一常量：每日快照的预览通道共用这把公钥
- `crates/zzclawterm-desktop/src/features/update/download.rs` 的 `PUBLIC_KEY`：应用内置、编译进二进制

前两处是工作流里的字面量，第三处是同一份公钥的 base64 包装。单测 `embedded_signing_key_matches_the_publishing_workflows` 会扫描 `.github/workflows/*.yml` 里所有 `TAURI_UPDATER_PUBLIC_KEY_B64` 并与应用常量逐字节比对，所以漏改应用常量会在 `cargo test` 阶段被拦下。

## 轮换步骤

1. 生成新密钥：`npx @tauri-apps/cli signer generate -w <仓库外路径> -p '<口令>'`；
2. 把新私钥与口令写进上面两个 GitHub secret；
3. 同步更新"哪些文件引用签名公钥"列出的三处；
4. 本地自检：`cargo test -p zzclawterm-desktop --lib update::download`；
5. 打一个 tag 走一次正式发布：流程自身会用新公钥对签名跑一次 `minisign -Vm`，不配对会在上传前失败；
6. 按下一节的顺序安排过渡版本。

## 轮换的代价：必须发一个过渡版本

应用把公钥写死，所以**用新密钥签出的包，老版本一律拒绝**。安全顺序是：

1. 先发布一个同时信任新旧两把公钥的过渡版本；
2. 等过渡版本铺开后再用新密钥签名；
3. 之后某个版本再移除旧公钥。

在过渡版本铺开之前直接换密钥，只会让已安装用户只能手动下载重装。

## 丢失私钥怎么办

私钥与口令都丢失时无法补签，只能按上一节做一次强制轮换：先发一个信任新公钥的版本，让用户手动安装一次，之后再回到正常发布节奏。

## 校验手段

- `cargo test` 里的密钥一致性测试：防止公钥常量与工作流漂移；
- 发布流程签名后立刻用公钥跑 `minisign -Vm`，签名与公钥不配对会在上传前失败；
- 正式发布在公开前逐个比对远端产物的大小与 sha256（`scripts/ci/verify_remote_assets.py`）。
