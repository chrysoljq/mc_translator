# MC Mod Translator (Minecraft 模组汉化助手)

基于 Rust 与 egui 构建的跨平台图形化 Minecraft 模组翻译工具。集成大语言模型实现上下文感知的智能汉化，支持 JAR、JSON、LANG 及 SNBT 多种格式的自动化处理，旨在为玩家与汉化者提供高效、精准的翻译体验。支持全量翻译和增量式翻译（SNBT格式除外）。
![alt text](image.png)

## ✨ 功能特性

* **多格式全面支持**：
  - 📦 **JAR**: 自动扫描 Mod 文件，读取内部 `en_us.json` 自动识别 ModID 并生成对应的汉化资源结构。
  - 📄 **JSON**: 标准 Minecraft 语言文件翻译。
  - 📝 **LANG**: 旧版 Minecraft 语言文件 (`.lang`) 支持。
  - 📜 **SNBT**: 专为 **FTB Quests** 设计，智能提取任务标题、描述与副标题进行翻译，完美保留原有数据结构。

* **智能翻译流程**：
  - 🧠 **自定义提示词**：支持用户自定义 System Prompt，可针对不同类型的整合包和模组灵活调整翻译风格与术语表。
  - 🛡️ **格式保护**：自动识别并保留格式代码（如 `§a`, `%s`, `{{0}}`、`\n`），防止破坏游戏内显示。
  - 🔄 **增量更新**：支持"更新翻译"模式，**读取旧汉化文件和内置汉化文件**，仅翻译新增的条目，保留原有的人工校对内容。
  - ⏭️ **智能跳过**：支持跳过已存在的文件，防止重复作业。
  - 📦 **自动生成**：生成标准格式的汉化文件，可以直接放入模组的资源文件夹中。

* **🚀 高性能并发处理**：
  - **多文件并行**：同时处理多个 Mod 或文件，充分利用网络带宽。
  - **多批次并行**：大文件自动切分，并发请求 API，大幅提升长文本翻译速度。

* **现代图形界面 (GUI)**：
  - 基于 `egui` 的跨平台界面，简洁直观。
  - 实时日志反馈与进度监控。



## 🛠️ 安装与构建

### 预编译版本

请前往 [Releases](https://github.com/chrysoljq/mc_translator/releases) 页面下载适用于 Windows / Linux / macOS 的最新版本。

### 从源码构建
可以通过 fork 本项目自动构建，也可以手动编译本项目：
```bash
# 1. 克隆仓库
git clone https://github.com/chrysoljq/mc_translator.git
cd mc_translator

# 2. 编译发布版本
cargo build --release
```

编译完成后，可执行文件位于 `target/release/` 目录下。

## 📂 支持的目录结构
程序会自动识别输入文件夹中的以下内容：
* `mods/*.jar` (自动解压读取)
* `assets/*/lang/en_us.json`
* `assets/*/lang/en_us.lang`
* `resources/*/lang/en_us.json`
* `kubejs/assets/*/lang/*.json`
* `config/ftbquests/**/*.snbt` (任务文件)

## 🤝 贡献
欢迎提交 Issue 反馈 Bug 或提交 Pull Request 改进代码。

## 📜 许可证
本项目采用 **GPL-3.0** 许可证。详见 [LICENSE](https://www.google.com/search?q=LICENSE) 文件。