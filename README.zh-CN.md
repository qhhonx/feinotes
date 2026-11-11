# 飞记 · Fei Notes

原生 macOS Markdown 笔记应用，支持按需开启 iCloud 同步与多人协作。

[官网](https://feinotes.app/) · [TestFlight 体验](https://feinotes.app/#testflight) · [支持](https://feinotes.app/support/) · [English](README.md)

- **本机笔记。** 离线写作，支持文件夹、全文搜索、回收站和整库备份与恢复。
- **按需开启 iCloud。** 在自己的 Mac 之间同步笔记、目录和附件，离线修改同段不同位置后可自动合并。
- **共享一篇笔记。** 邀请指定 Apple 账户查看或编辑，并管理访问权限。
- **Markdown 与丰富内容。** 原生块编辑器支持标题、清单、代码、表格、图片和附件。
- **从 Quiet.md 导入。** 在文档库设置中选择原文档库，将内容复制到飞记。

## 体验

macOS 分发包同时包含 Apple 芯片和 Intel 架构，要求 macOS 14 或更高版本。Intel 已纳入编译与打包，尚未经 Intel 真机验证。在官网的 TestFlight 区域加入测试，官网显示当前分发与审核状态。

## 开发与来源

源码公开，可在适用许可范围内自行构建用于个人非商业用途：

```sh
cargo build --release --locked
```

开发说明以英文为主，见 [CONTRIBUTING.md](CONTRIBUTING.md) / [Fei Notes additions](docs/FEINOTES-CONTRIBUTING.md)、[AGENTS.md](AGENTS.md) / [Fei Notes rules](docs/FEINOTES-AGENTS.md)、[维护流程](docs/MAINTENANCE.md)和[Apple 发布流程](docs/APPLE-RELEASE.md)。

飞记基于 [Quiet.md](https://github.com/s1dashu/quiet-md)。上游代码保留 [MIT 许可](LICENSE)。清单中独立原创的飞记模块以 [PolyForm Noncommercial 1.0.0](LICENSE-FEINOTES.md) 提供源码，保留商业使用权，商业用途需另行取得许可；这不改变上游 MIT 代码已有的使用权限。文件范围见 [LICENSES.md](LICENSES.md)，第三方声明见 [THIRD_PARTY.md](THIRD_PARTY.md)。
