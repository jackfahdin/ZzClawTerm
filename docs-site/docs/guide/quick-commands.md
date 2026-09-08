# 快捷命令

快捷命令功能让你可以把常用命令保存成可复用动作，在工作区里快速发送到当前终端。

## 用途

- 高频执行固定运维命令
- 保存带参数模板的部署 / 排障脚本
- 把常见命令按产品、环境或团队分类
- 把危险命令先放到输入行检查，再决定是否执行

## 创建快捷命令

1. 打开底部或侧边的 **快捷命令** 区域
2. 点击 **添加**
3. 在独立子窗口中填写命令信息

可配置字段包括：

| 字段 | 说明 |
|------|------|
| 标签名称 | 命令的显示名称 |
| 分类 | 命令所属分类 |
| 描述 | 命令说明（可选） |
| 颜色标签 | 自定义显示颜色 |
| 图标 | 自定义图标 |
| 置顶显示 | 是否优先显示在列表顶部 |
| 执行模式 | 立即执行或追加到输入行 |
| 命令脚本 | 要发送到终端的命令内容 |

保存后，命令会出现在快捷命令列表中，可继续编辑或删除。

## 执行模式

### 立即执行

点击后直接发送到当前终端并执行，用于：

- 确认无误的常用命令
- 日常巡检
- 固定格式的只读查询

### 追加到输入行

点击后只把命令放到输入行，不自动回车，用于：

- 还要再检查参数的命令
- 可能需要二次修改的脚本片段
- 有一定风险、希望人工确认的操作

## 变量提示

命令脚本支持 `{{变量名}}` 语法注入动态参数，例如：

```bash
docker exec -it {{容器名}} bash
```

执行时会弹出变量填写对话框，让你把模板命令补全后再发送。

## 分类、搜索、排序与视图

快捷命令面板支持这些管理方式：

- 通过搜索框按标签、命令内容或描述过滤
- 通过分类下拉框只看某一类命令
- 创建、重命名、移动和排序分类
- 置顶命令优先显示在列表顶部
- 按名称、使用频率、最近使用等排序模式整理列表
- 在紧凑视图和常规视图之间切换，以适配不同面板宽度
- 已保存分类会被复用，新命令也可以继续补充分类；导入时也会尽量保留外部文件中的分类结构

常见的分类方式：

- K8s
- Docker
- 数据库
- 发布脚本
- 环境巡检

## 导出快捷命令

在快捷命令面板右上角点击 **导出快捷命令**，把当前快捷命令和分类导出为 NyaTerm JSON 文件。导出文件可用于：

- 在另一台设备上通过 **导入快捷命令** 合并
- 作为团队模板分发
- 在做大规模分类整理前留一份可回退副本

导出不会包含终端历史或其他应用设置；它只覆盖快捷命令和分类数据。

## 导入快捷命令

在快捷命令面板右上角点击 **导入快捷命令**，可以从外部文件批量导入。导入会按命令 ID 合并覆盖，不会清空现有快捷命令。

### 从 WindTerm 导入

WindTerm 的快捷命令文件通常位于：

```text
~/.wind/profiles/default.v10/terminal/quickbar.config
```

在 Windows 上可以在用户目录下找到对应路径：

```text
C:\Users\<用户名>\.wind\profiles\default.v10\terminal\quickbar.config
```

选择 **WindTerm Quickbar** 后，选中这个 `quickbar.config` 文件即可导入。NyaTerm 会读取 `quick.label`、`quick.text`、`quick.group` 和 `quick.uuid` 等字段，并把 WindTerm 的 `Send Text` 类型导入为 **追加到输入行**，避免点击后立即执行脚本。

### 从 JSON 文件导入

选择 **NyaTerm JSON** 后，可以导入下面这种 JSON 文件：

```json
{
  "categories": [
    {
      "id": "general",
      "name": "General"
    },
    {
      "id": "k8s",
      "name": "Kubernetes"
    }
  ],
  "commands": [
    {
      "id": "cmd-list-files",
      "label": "List files",
      "command": "ls -la",
      "category_id": "general",
      "description": "List files with details",
      "color_tag": "blue",
      "icon_tag": "terminal",
      "pinned": true,
      "execution_mode": "execute",
      "source": "manual",
      "risk_level": "low"
    },
    {
      "label": "Kubernetes pods",
      "command": "kubectl get pods -A",
      "category": "Kubernetes",
      "execution_mode": "append",
      "risk_level": "low"
    }
  ]
}
```

说明：

- `categories` 可以省略；使用 `category` 名称时，分类不存在会自动创建
- `id` 可以省略；省略后会自动生成
- `execution_mode` 支持 `execute` 或 `append`
- `source` 支持 `manual` 或 `ai`
- `risk_level` 支持 `low`、`medium`、`high`、`critical`

## 与工作区配合的使用方式

快捷命令并不绑定某一类会话。只要当前终端可接收输入，你就可以把命令发送到：

- SSH 会话
- 本地终端
- 某些需要批量发指令的串口场景

常见搭配方式：

- 左边看日志，右边通过快捷命令触发诊断脚本
- 远程 SSH 执行部署命令，本地终端同时做构建或 Git 操作
- 把变量化命令做成团队共享模板，减少人工拼写错误
- 将 AI Assistant 生成并通过审批的命令保存为快捷命令，后续按固定模板复用
