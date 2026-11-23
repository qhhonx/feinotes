# 同步第一期：需要用户动手的准备

配合 [sync.md](sync.md)。两件事：注册 Quiet.md 的 GitHub App（2.2 节），以及 D12 的三项实测（1.1 节）。都写成照着做的步骤，做完把结果告诉同步会话。

## 一、注册 GitHub App

GitHub 同步用它来实现「登录 GitHub」：用户在浏览器里确认一次，只授权自己选的仓库。我们**不需要服务器**，也**不需要任何密钥**。

### 步骤

1. 打开 GitHub → 右上角头像 → **Settings** → 左栏最下面 **Developer settings** → **GitHub Apps** → **New GitHub App**。
   - 想让 App 挂在组织名下，就从组织的 Settings 进入；挂在个人账号下也可以，用户看到的发布者名字不同而已。
2. 按下表填写。表里没提到的项保持默认。

| 字段 | 填什么 | 说明 |
|---|---|---|
| GitHub App name | `Quiet.md` | 全 GitHub 唯一，被占用就用 `Quiet.md Sync`。这个名字会出现在用户的授权页上 |
| Description | `Sync your Quiet.md notes with a GitHub repository.` | 授权页上显示 |
| Homepage URL | 官网地址 | 必填 |
| **Callback URL** | **留空** | 我们用设备码登录，不走浏览器回调 |
| Expire user authorization tokens | **勾选**（默认已勾） | token 8 小时过期，应用用 refresh token 自动续期 |
| Request user authorization (OAuth) during installation | 不勾 | |
| **Enable Device Flow** | **勾选** | 「登录 GitHub」靠的就是它 |
| Setup URL / Redirect on update | 留空 | |
| **Webhook → Active** | **取消勾选** | 我们没有服务器接收事件 |
| Webhook URL / secret | 留空 | 取消 Active 以后就不要求填了 |

3. **Permissions → Repository permissions**，只开这两项：

| 权限 | 级别 | 用途 |
|---|---|---|
| **Contents** | **Read and write** | 读写仓库里的文件、建提交、更新分支 |
| Metadata | Read-only | GitHub 强制要求，选了 Contents 会自动带上 |

   - **Administration 先不开**。开了它才能替用户新建仓库，但会让授权页上多一行"管理仓库"，看着吓人。第一期新建仓库走网页（下面"实测"里有一项专门确认这条路顺不顺）。
   - Account permissions、Organization permissions **全部不开**。提交作者用 GitHub 账号默认的 noreply 邮箱，不用读邮箱权限。
4. **Subscribe to events**：全部不勾。
5. **Where can this GitHub App be installed?** 选 **Any account**，否则只有你自己能用。
6. 点 **Create GitHub App**。

### 创建以后

- 页面顶部的 **App ID**（一串数字）、**Client ID**（以 `Iv` 开头），以及浏览器地址栏里 App 的网址名（`github.com/apps/<这一段>`），**这三项都发给同步会话**。它们都是公开信息，会写进代码里的常量（GitHub 后端模块，1e 实现时建）。
- 页面下方有 **Generate a new client secret** 和 **Generate a private key**：**都不要点**。设备码登录用不到，生成了反而多一份要保管的密钥。万一点了，删掉即可，不要发给任何人，也不要放进仓库。
- 可选：上传应用图标（Display information → Upload a logo），授权页上会显示。

### 顺便实测一下新建仓库这条路

注册完以后，用你自己的账号走一遍下面的流程，告诉同步会话哪一步别扭：

1. 打开 `https://github.com/new?name=quiet-notes&visibility=private`，点 **Create repository**。
2. 打开 `https://github.com/apps/<App 网址名>/installations/new`，选自己的账号，**Only select repositories** 选 `quiet-notes`，点 **Install**。

这就是用户第一次开 GitHub 同步时要在网页上点的两下。如果觉得太绕，备选方案是给 App 开 Administration 权限，让应用直接建仓库，代价是授权页多一行权限。

## 二、D12 实测：Mac 能不能直接按路径用 Quiet.md 的 iCloud 容器

背景：iOS 版准备把库放在 app 自己的 iCloud 容器里（在"文件"App 和访达里都显示成 iCloud 云盘下的 `Quiet.md` 文件夹）。Mac 版没有沙盒，我们希望它不加任何 iCloud 权限，直接按路径打开这个文件夹。下面三项全通过，D12 就按这个做；有一项不通过，就退回"Mac 版也加 iCloud 权限"。

### 准备（一次）

1. 开发者后台建 iCloud 容器 `iCloud.com.s1dashu.quietmd`、给 iOS App ID 打开 iCloud Documents，并在 iPad 上装好**把默认库放在 iCloud 容器里的 Quiet.md iOS 版**：步骤见移动端的真机清单 `docs/design/mobile-device.md`（分支 `claude/ios-device-checklist`）第一节，两份清单写的是同一件事，做一次就行。
2. 下面"在 iPad 上新建文档"都是在 iPad 上的 Quiet.md 里做；Quiet.md 只显示 `.xml` 和 `.md`，所以 Mac 这边写的测试文件用 `.md`。
3. iPad（或 iPhone）和 Mac 登录**同一个 Apple ID**，都打开了 iCloud 云盘。
4. Mac 上**不要装任何带这个容器权限的 app**（现在发布的 Quiet.md 是 Developer ID 签名、没有 iCloud 权限，正好）。

下面的命令里，`C` 代表容器的路径：

```sh
C=~/Library/Mobile\ Documents/iCloud~com~s1dashu~quietmd/Documents
```

### 实测 1：容器会不会同步到一台"没装带权限 app"的 Mac 上，并且双向同步

1. 在 iPad 上的 Quiet.md 里新建一篇文档，标题写 `from-ipad`，随便写一行字。
2. 在 Mac 上等最多 5 分钟，看两处：
   - 访达左栏 iCloud 云盘里，是否出现 `Quiet.md` 文件夹，里面有 `from-ipad.xml`；
   - 终端里 `ls "$C"` 是否列出 `from-ipad.xml`。
3. 在 Mac 上写一个文件：`echo "# from-mac" > "$C/from-mac.md"`。
4. 在 iPad 的 Quiet.md 里（也可以看"文件"App → iCloud 云盘 → Quiet.md），等最多 5 分钟，看是否出现 `from-mac` 这篇文档，内容对不对。
5. 重启 Mac，在 iPad 上再改一下 `from-ipad`，看 Mac 上 `cat "$C/from-ipad.xml"` 的内容是否更新。

**通过**：第 2、4、5 步都在 5 分钟内看到。**不通过**：Mac 上一直没有这个文件夹，或者只能单向同步。

### 实测 2：Mac 上按路径写进去的文件，文件协调和冲突版本是否照常

这一项确认 sync.md 1.3 的做法（经过 `NSFileCoordinator` 写入，冲突由 `NSFileVersion` 交给我们，而不是生成 `X 2.txt`）在这个路径上也成立。

1. 把下面的脚本存成 `~/conflicts.swift`：

   ```swift
   import Foundation
   let url = URL(fileURLWithPath: CommandLine.arguments[1])
   if CommandLine.arguments.count > 2 {   // 第二个参数：经过文件协调写入这段内容
       var err: NSError?
       NSFileCoordinator().coordinate(writingItemAt: url, options: .forReplacing, error: &err) { u in
           try! CommandLine.arguments[2].write(to: u, atomically: true, encoding: .utf8)
       }
       if let err { print("写入出错：\(err)") } else { print("已写入") }
   }
   let v = NSFileVersion.unresolvedConflictVersionsOfItem(at: url) ?? []
   print("未解决的冲突版本：\(v.count)")
   for x in v { print(" -", x.localizedNameOfSavingComputer ?? "?", x.modificationDate ?? Date()) }
   ```

2. Mac 上：`swift ~/conflicts.swift "$C/same.md" "Mac 第一版"`，等 iPad 的 Quiet.md 里出现 `same` 这篇文档。
3. **Mac 断网**（关掉 Wi-Fi）。
4. iPad 上在 Quiet.md 里打开 `same`，随便改一句（等自动保存）。
5. Mac 上（仍然断网）：`swift ~/conflicts.swift "$C/same.md" "Mac 断网时改的"`。
6. Mac 重新联网，等 2–5 分钟，然后：
   - `ls "$C"`：看有没有出现 `same 2.md`；
   - `swift ~/conflicts.swift "$C/same.md"`：看"未解决的冲突版本"是几。

**通过**：没有 `same 2.md`，并且冲突版本数 ≥ 1（列出来的设备名是 iPad）。**部分通过**：出现了 `same 2.md`，冲突版本数是 0。这说明这个路径上 iCloud 不按文件协调处理冲突，我们得走 1.3 的兜底规则（按文件名识别副本），方案仍可行，但要告诉同步会话。**不通过**：脚本写入报错。

### 实测 3：打开"优化 Mac 存储"后，容器里的文件会不会变成占位

1. 系统设置 → 你的名字 → iCloud → iCloud 云盘 → 打开 **优化 Mac 存储**。
2. 手动把一个文件清出本地：`brctl evict "$C/from-ipad.xml"`（不想等系统自动清理）。
3. 看它是不是变成了占位：`ls -lO "$C"`，`from-ipad.xml` 那一行的标志里应有 `dataless`；或者在访达里看，文件名旁边有云朵图标。
4. 读一次：`cat "$C/from-ipad.xml"`。应该会停顿一下（在下载），然后正常输出内容；再 `ls -lO`，`dataless` 消失。
5. 测完可以关掉"优化 Mac 存储"。

**通过**：第 3 步看到占位，第 4 步能读出来。这说明容器里的文件和 iCloud 云盘其他地方一样会被清出本地，sync.md 1.2 的处理（读之前查下载状态、放后台读）同样适用。第 3 步就没变成占位也记下来，不影响 D12，只说明这里不会出现占位。

### 结果怎么报

把三项各自的结果（通过 / 部分通过 / 不通过）和看到的现象（截图或终端输出）发给同步会话，它会写回 sync.md 的 D12 和 1.1 节。
