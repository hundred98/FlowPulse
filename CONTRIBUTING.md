# Contributing Guide (贡献指南)

Thank you for your interest in contributing to the FlowPulse project! This document will help you understand how to participate in the project development.

---

## Table of Contents

- [Code of Conduct](#code-of-conduct)
- [How to Contribute](#how-to-contribute)
- [Development Environment Setup](#development-environment-setup)
- [Code Standards](#code-standards)
- [Submitting a Pull Request](#submitting-a-pull-request)
- [License](#license)

---

## Code of Conduct

### Our Pledge

To foster an open and welcoming environment, we pledge to:

- Use inclusive language
- Be respectful of differing viewpoints and experiences
- Gracefully accept constructive criticism
- Focus on what is best for the community
- Show empathy towards other community members

### Unacceptable Behavior

- The use of sexualized language or imagery
- Trolling, insulting/derogatory comments, and personal or political attacks
- Public or private harassment
- Publishing others' private information without explicit permission
- Other conduct which could reasonably be considered inappropriate in a professional setting

---

## How to Contribute

### Reporting Bugs

If you find a bug, please submit a report via GitHub Issues. The report should include:

1. **Bug Description**: A clear and concise description of the problem
2. **Steps to Reproduce**: Detailed steps to reproduce the issue
3. **Expected Behavior**: What you expected to happen
4. **Actual Behavior**: What actually happened
5. **Environment Information**:
   - Operating System
   - Rust Version
   - Project Version
6. **Screenshots**: If applicable, add screenshots to help explain the problem

### Suggesting New Features

We welcome new feature suggestions! Please submit via GitHub Issues, including:

1. **Feature Description**: A clear and concise description of the feature you want
2. **Use Case**: Describe how this feature would help you
3. **Alternatives**: Describe any alternative solutions you've considered
4. **Additional Information**: Any other relevant information or screenshots

### Submitting Code

1. Fork the repository
2. Create your feature branch (`git checkout -b feature/AmazingFeature`)
3. Commit your changes (`git commit -m 'Add some AmazingFeature'`)
4. Push to the branch (`git push origin feature/AmazingFeature`)
5. Create a Pull Request

---

## Development Environment Setup

### Prerequisites

- Rust 1.70 or higher
- Git
- Your preferred IDE (VS Code + rust-analyzer recommended)

### Clone the Repository

```bash
git clone https://github.com/hundred98/FlowPulse.git
cd FlowPulse
```

### Build the Project

```bash
cargo build
```

### Run Tests

```bash
cargo test
```

### Run Code Checks

```bash
cargo clippy
cargo fmt --check
```

---

## Code Standards

### Rust Code Style

- Follow the [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)
- Use `cargo fmt` to format code
- Use `cargo clippy` to check code quality

### Documentation Comments

- All public APIs must have documentation comments
- Use `///` for documentation comments
- Include example code

### Commit Messages

Use clear commit messages:

- Use the present tense ("Add feature" not "Added feature")
- Use the imperative mood ("Move cursor to..." not "Moves cursor to...")
- Limit the first line to 72 characters
- Reference relevant issues and pull requests

---

## Submitting a Pull Request

### PR Checklist

Before submitting a PR, please ensure:

- [ ] Code passes all tests (`cargo test`)
- [ ] Code passes Clippy checks (`cargo clippy`)
- [ ] Code is formatted (`cargo fmt`)
- [ ] Necessary documentation comments are added
- [ ] Related documentation is updated
- [ ] Project code style is followed
- [ ] Commit messages are clear

### PR Description Template

```markdown
## Description

Please briefly describe your changes.

## Type of Change

- [ ] Bug fix
- [ ] New feature
- [ ] Code refactoring
- [ ] Documentation update
- [ ] Other

## Testing

Please describe how you tested these changes.

## Related Issues

Closes #(issue number)

## Screenshots (if applicable)

## Additional Information

Any other relevant information.
```

---

## License

### Important Notice

This project uses a dual licensing strategy:

#### host & emb-public (Public Modules)

- **License**: MIT
- **Scope**: User interface, HTTP API, CLI tools, GCode parsing, temperature control, etc.
- **Commercial Use**: Allowed, but core engine requires separate commercial license

### Contributor License Agreement (CLA)

By submitting a contribution, you agree that:

1. Your contribution is your original work
2. You grant the project an irrevocable right to use it
3. Your contribution is licensed under MIT license
4. The project may use your contribution in commercially licensed products

For details, see [CLA.md](CLA.md).

---

## Contact

If you have any questions, you can reach us through:

- GitHub Issues: Submit questions or suggestions
- Email: hundred98@163.com

---

Thank you again for your contribution!

---

# 贡献指南 (中文版)

感谢您有兴趣为 FlowPulse 项目做出贡献！本文档将帮助您了解如何参与项目开发。

---

## 目录

- [行为准则](#行为准则)
- [如何贡献](#如何贡献)
- [开发环境设置](#开发环境设置)
- [代码规范](#代码规范)
- [提交 Pull Request](#提交-pull-request)
- [许可证](#许可证)

---

## 行为准则

### 我们的承诺

为了营造一个开放和友好的环境，我们承诺：

- 使用包容性语言
- 尊重不同的观点和经验
- 优雅地接受建设性批评
- 关注对社区最有利的事情
- 对其他社区成员表示同理心和友善

### 不可接受的行为

- 使用性化的语言或图像
- 捣乱、侮辱/贬损评论以及人身或政治攻击
- 公开或私下骚扰
- 未经明确许可，发布他人的私人信息
- 其他在专业环境中可能被合理认为不适当的行为

---

## 如何贡献

### 报告 Bug

如果您发现了 Bug，请通过 GitHub Issues 提交报告。报告应包含：

1. **Bug 描述**：清晰简洁地描述问题
2. **复现步骤**：详细的复现步骤
3. **预期行为**：您期望发生什么
4. **实际行为**：实际发生了什么
5. **环境信息**：
   - 操作系统
   - Rust 版本
   - 项目版本
6. **截图**：如果适用，添加截图帮助解释问题

### 建议新功能

我们欢迎新功能建议！请通过 GitHub Issues 提交，包含：

1. **功能描述**：清晰简洁地描述您想要的功能
2. **使用场景**：描述该功能如何帮助您
3. **替代方案**：描述您考虑过的替代方案
4. **附加信息**：任何其他相关信息或截图

### 提交代码

1. Fork 本仓库
2. 创建您的特性分支 (`git checkout -b feature/AmazingFeature`)
3. 提交您的更改 (`git commit -m 'Add some AmazingFeature'`)
4. 推送到分支 (`git push origin feature/AmazingFeature`)
5. 创建 Pull Request

---

## 开发环境设置

### 前置要求

- Rust 1.70 或更高版本
- Git
- 您喜欢的 IDE（推荐 VS Code + rust-analyzer）

### 克隆仓库

```bash
git clone https://github.com/hundred98/FlowPulse.git
cd FlowPulse
```

### 构建项目

```bash
cargo build
```

### 运行测试

```bash
cargo test
```

### 运行代码检查

```bash
cargo clippy
cargo fmt --check
```

---

## 代码规范

### Rust 代码风格

- 遵循 [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)
- 使用 `cargo fmt` 格式化代码
- 使用 `cargo clippy` 检查代码质量

### 文档注释

- 所有公共 API 必须有文档注释
- 使用 `///` 进行文档注释
- 包含示例代码

### 提交信息

使用清晰的提交信息：

- 使用现在时态（"Add feature" 而不是 "Added feature"）
- 使用祈使语气（"Move cursor to..." 而不是 "Moves cursor to..."）
- 限制第一行为 72 个字符
- 引用相关 Issues 和 Pull Requests

---

## 提交 Pull Request

### PR 检查清单

在提交 PR 之前，请确保：

- [ ] 代码通过所有测试 (`cargo test`)
- [ ] 代码通过 Clippy 检查 (`cargo clippy`)
- [ ] 代码已格式化 (`cargo fmt`)
- [ ] 添加了必要的文档注释
- [ ] 更新了相关文档
- [ ] 遵循项目的代码风格
- [ ] 提交信息清晰明了

### PR 描述模板

```markdown
## 变更描述

请简要描述您的变更。

## 变更类型

- [ ] Bug 修复
- [ ] 新功能
- [ ] 代码重构
- [ ] 文档更新
- [ ] 其他

## 测试

请描述您如何测试这些变更。

## 相关 Issues

关闭 #(issue number)

## 截图（如果适用）

## 附加信息

任何其他相关信息。
```

---

## 许可证

### 重要说明

本项目采用双许可证策略：

#### host & emb-public（公开模块）

- **许可证**：MIT
- **适用范围**：用户界面、HTTP API、CLI 工具、GCode 解析、温控等
- **商业使用**：允许，但核心引擎需单独商业授权

### 贡献者许可协议 (CLA)

通过提交贡献，您同意：

1. 您的贡献是您原创的作品
2. 您授予项目方不可撤销的使用权
3. 您的贡献适用于 MIT 许可证
4. 项目方可以将您的贡献用于商业授权产品

详细信息请参阅 [CLA.md](CLA.md)。

---

## 联系方式

如果您有任何问题，可以通过以下方式联系我们：

- GitHub Issues：提交问题或建议
- 电子邮件：hundred98@163.com

---

再次感谢您的贡献！
