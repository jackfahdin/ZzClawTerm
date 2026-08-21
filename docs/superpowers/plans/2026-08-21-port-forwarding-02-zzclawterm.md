# 端口转发 ZzClawTerm 应用侧实现计划（port-forwarding-02）

> **面向 AI 代理的工作者：** 必需子技能：使用 superpowers:subagent-driven-development（推荐）或 superpowers:executing-plans 逐任务实现此计划。步骤使用复选框（`- [ ]`）语法来跟踪进度。

**目标：** 把 ZzSshCore 已交付的 -L/-R/-D 端口转发能力接入 ZzClawTerm：规则随会话 profile 持久化、连接成功自动启动、断线销毁重连重建、单规则失败隔离，并提供规则编辑 UI 与状态栏活动隧道指示。

**架构：** 规则值类型 `ZzForwardRule` 入 `ZzSessionCore`；`ZzSessionProfile`/`ZzTransportEndpoint` 各加 `portForwards` 字段；新增 `ZzTunnelManager`（GUI 线程，每活动会话一个）经 `ZzTunnelHandle` 抽象操作隧道，生产侧 `ZzSshTunnelFactory` 包装 `ZzSshConnection`，测试侧 fake 工厂注入生命周期事件；`ZzSshTransportAdapter::onConnected()` 驱动 manager。状态栏加第四要素「隧道 N」；规则失败走新增的 `statusNotice` 瞬时消息链（不动错误横幅语义）。

**技术栈：** C++20 / Qt 6.8+（Core/Widgets/Network/Test）/ CMake 3.25+ / QTest。

**关联文档：**
- 规格：`docs/superpowers/specs/2026-08-21-ssh-port-forwarding-design.md`（用户已批准）
- 库侧计划（已完成）：`docs/superpowers/plans/2026-08-21-port-forwarding-01-zzsshcore.md`
- ZzSshCore 终态：`339b6d1`（gitcode master 已推送），隧道 API 见本文「附：ZzSshCore 接口速查」

---

## 全局约束（每个任务都适用）

- 仓库：主仓库 `/home/zz/Jackfahdin/github/ZzClawTerm`。本计划**不改动** `third_party/ZzSshCore` 内容（任务 1 只更新 gitlink）。
- 类名 `Zz` 前缀；文件名与类名严格一致（含大小写）；Doxygen 简体中文注释。
- commit message：Conventional Commits 前缀 + 中文首行简述 + 空行 + 中文详细说明。**不 push**（推送由用户单独确认）。
- TDD：先写失败测试，跑红，再实现，跑绿，commit。
- 既有全部测试为回归锚，一个不许改坏。回归命令：
  ```bash
  cmake --preset linux-gcc-release && cmake --build --preset linux-gcc-release
  ctest --preset linux-gcc-release
  ```
  （preset 名以根 CMakePresets.json 为准；以下步骤统一用 `linux-gcc-release`。）
- 测试注册两种基建：
  - 纯 Core 值类型 → `tests/session/CMakeLists.txt` 的 `zz_session_add_test(目标名)`（源码约定同名 .cpp，链 `ZzSessionCore`）。
  - Widget/应用级 → `tests/CMakeLists.txt` 的 `zz_add_qtest(名 路径)`（链 `ZzClawTermApp + ZzTestMocks`，离屏）。

## 文件结构（创建/修改总表）

| 文件 | 职责 | 任务 |
| ---- | ---- | ---- |
| `third_party/ZzSshCore`（gitlink） | 指向含隧道 API 的 339b6d1 | 1 |
| `src/session/ZzForwardRule.h/.cpp` | [新] 转发规则值类型 + 校验 | 2 |
| `src/session/CMakeLists.txt` | ZzSessionCore 加入新文件 | 2 |
| `tests/session/ZzForwardRuleTest.cpp` + `tests/session/CMakeLists.txt` | 规则序列化/校验单测 | 2 |
| `src/session/ZzSessionProfile.h/.cpp` | 加 `portForwards` 字段与序列化（version 仍为 1） | 3 |
| `tests/session/ZzSessionProfileTest.cpp` | 补 portForwards 往返/默认值用例 | 3 |
| `src/transport/ZzTransportEndpoint.h` | 加 `portForwards` 字段 | 3 |
| `src/tab/ZzTabManager.cpp` | `endpointFor()` 映射 portForwards | 3 |
| `tests/unit/tst_ZzConnectFlow.cpp` | 断言 endpoint 映射透传 | 3 |
| `src/transport/ZzTunnelHandle.h` | [新] 隧道句柄抽象（生产/测试双实现） | 4 |
| `src/transport/ZzTunnelManager.h/.cpp` | [新] 隧道集合生命周期管理 + `ZzTunnelFactory` 接口 | 4 |
| `tests/unit/tst_ZzTunnelManager.cpp` + `tests/CMakeLists.txt` | fake 工厂生命周期单测 | 4 |
| `src/transport/ZzSshTunnelHandle.h/.cpp` | [新] 生产句柄（包装 ZzSshTunnel/ZzSshForwardListener）+ `ZzSshTunnelFactory` | 5 |
| `src/transport/ZzTransportInterface.h` | 加 `tunnelCountChanged(int)` / `statusNotice(QString)` 两信号 | 5 |
| `src/transport/ZzSshTransport.h/.cpp` | onConnected 驱动 manager；close/重连废弃 manager | 5 |
| `src/CMakeLists.txt` | 注册新源文件 | 4/5 |
| `tests/unit/tst_ZzSshTunnelHandle.cpp` + `tests/CMakeLists.txt` | 生产句柄与工厂单测 | 5 |
| `tests/mocks/ZzMockTransport.h` | 加 `simulateTunnelCount` / `simulateStatusNotice` | 7 |
| `src/terminal/ZzTerminalView.h/.cpp` | 透传两个新信号 | 7 |
| `src/tab/ZzTabManager.h/.cpp` | `currentTunnelCountChanged` + 每标签计数 + statusNotice→statusMessage | 7 |
| `src/ZzAppShell.h/.cpp` | 状态栏第四要素「隧道 N」+ 观察口 | 7 |
| `tests/unit/tst_ZzTunnelIndicator.cpp` + `tests/CMakeLists.txt` | 状态栏链路单测 | 7 |
| `src/panel/ZzSessionEditDialog.h/.cpp` | 端口转发规则表（QTableWidget + 增删 + 校验） | 6 |
| `tests/unit/tst_ZzSessionEditDialog.cpp` + `tests/CMakeLists.txt` | 对话框规则表 Widget 测试 | 6 |
| `README.md` | 端口转发「开发中」→「已实现」 | 8 |

任务顺序即依赖顺序：1（gitlink）→ 2（规则类型）→ 3（配置贯通）→ 4（manager 核心）→ 5（生产接线）→ 6（编辑 UI）→ 7（状态栏）→ 8（收尾）。6 与 7 互不依赖，但为审查聚焦保持分开。

---

### 任务 1：更新 ZzSshCore gitlink 至 339b6d1 并全量验证

**文件：**
- 修改：`third_party/ZzSshCore`（gitlink）

- [ ] **步骤 1：更新子模块检出**

```bash
cd /home/zz/Jackfahdin/github/ZzClawTerm
git -C third_party/ZzSshCore fetch origin master
git -C third_party/ZzSshCore checkout 339b6d15ddfcc6341548bb14557b30a3715b2b41
git submodule status third_party/ZzSshCore
```

预期：检出 `339b6d1`，`git status` 显示 `third_party/ZzSshCore` 有新 commit（modified: new commits）。

- [ ] **步骤 2：验证隧道 API 可用**

```bash
grep -n "createTunnel\|createForwardListener" third_party/ZzSshCore/src/ZzSshConnection.h
ls third_party/ZzSshCore/src/ZzSshTunnel.h third_party/ZzSshCore/src/ZzSshForwardListener.h
```

预期：两行 grep 命中 + 两个头文件存在。

- [ ] **步骤 3：全量构建 + 回归（既有测试为锚）**

```bash
cmake --preset linux-gcc-release && cmake --build --preset linux-gcc-release
ctest --preset linux-gcc-release --output-on-failure
```

预期：构建通过；全部既有测试 `Passed`（gitlink 更新不应影响任何既有行为）。

- [ ] **步骤 4：Commit**

```bash
git add third_party/ZzSshCore
git commit -m "chore: 更新 ZzSshCore 子模块至 339b6d1（端口转发 API）

ZzSshCore 侧端口转发已交付（计划 port-forwarding-01）：ZzSshTunnel（-L/-D）、
ZzSshForwardListener（-R）、统一双向泵与背压、性能门控记录。
另含 flaky 根治（31c02c8）与中文 README（339b6d1）。"
```

---

### 任务 2：ZzForwardRule 值类型与校验

**文件：**
- 创建：`src/session/ZzForwardRule.h`
- 创建：`src/session/ZzForwardRule.cpp`
- 修改：`src/session/CMakeLists.txt`
- 测试：`tests/session/ZzForwardRuleTest.cpp`
- 修改：`tests/session/CMakeLists.txt`

- [ ] **步骤 1：编写失败的测试**

创建 `tests/session/ZzForwardRuleTest.cpp`：

```cpp
#include <QtTest>

#include "ZzForwardRule.h"

/**
 * @brief ZzForwardRule 序列化与校验单元测试（规格 §五）。
 */
class ZzForwardRuleTest : public QObject
{
    Q_OBJECT

private slots:
    /** @brief 三种类型全字段序列化后反序列化必须完全相等。 */
    void serializationRoundTrip()
    {
        const QVector<ZzForwardRule> rules = {
            {ZzForwardRule::Type::Local, QStringLiteral("127.0.0.1"), 13306,
             QStringLiteral("db.internal"), 3306},
            {ZzForwardRule::Type::Remote, QStringLiteral("0.0.0.0"), 8080,
             QStringLiteral("127.0.0.1"), 3000},
            {ZzForwardRule::Type::Dynamic, QStringLiteral("127.0.0.1"), 1080,
             QString(), 0},
        };
        for (const ZzForwardRule &rule : rules) {
            QCOMPARE(ZzForwardRule::fromJson(rule.toJson()), rule);
        }
    }

    /** @brief 空 JSON 反序列化取默认值（本地转发、127.0.0.1、端口 0）。 */
    void fromJsonUsesDefaults()
    {
        const ZzForwardRule rule = ZzForwardRule::fromJson(QJsonObject());
        QCOMPARE(rule.type, ZzForwardRule::Type::Local);
        QCOMPARE(rule.listenHost, QStringLiteral("127.0.0.1"));
        QCOMPARE(rule.listenPort, quint16(0));
        QVERIFY(rule.targetHost.isEmpty());
        QCOMPARE(rule.targetPort, quint16(0));
    }

    /** @brief 类型字符串往返；无法识别的字符串回退 Local。 */
    void typeStringRoundTrip()
    {
        QCOMPARE(zzForwardRuleTypeToString(ZzForwardRule::Type::Local), QStringLiteral("local"));
        QCOMPARE(zzForwardRuleTypeToString(ZzForwardRule::Type::Remote), QStringLiteral("remote"));
        QCOMPARE(zzForwardRuleTypeToString(ZzForwardRule::Type::Dynamic), QStringLiteral("dynamic"));
        QCOMPARE(zzForwardRuleTypeFromString(QStringLiteral("remote")), ZzForwardRule::Type::Remote);
        QCOMPARE(zzForwardRuleTypeFromString(QStringLiteral("垃圾")), ZzForwardRule::Type::Local);
    }

    /** @brief 三种类型的合法规则均通过校验（validate 返回空串）。 */
    void validateAcceptsValidRules()
    {
        QVERIFY(ZzForwardRule{ZzForwardRule::Type::Local, QStringLiteral("127.0.0.1"), 13306,
                              QStringLiteral("db.internal"), 3306}.validate().isEmpty());
        QVERIFY(ZzForwardRule{ZzForwardRule::Type::Remote, QStringLiteral("0.0.0.0"), 8080,
                              QStringLiteral("127.0.0.1"), 3000}.validate().isEmpty());
        // Dynamic 无目标地址，属合法（规格 §五）
        QVERIFY(ZzForwardRule{ZzForwardRule::Type::Dynamic, QStringLiteral("127.0.0.1"), 1080,
                              QString(), 0}.validate().isEmpty());
    }

    /** @brief 非法规则逐类拒绝：端口 0、缺目标、空监听地址。 */
    void validateRejectsInvalidRules()
    {
        // 监听端口 0
        QVERIFY(!ZzForwardRule{ZzForwardRule::Type::Local, QStringLiteral("127.0.0.1"), 0,
                               QStringLiteral("db.internal"), 3306}.validate().isEmpty());
        // Local/Remote 缺目标地址
        QVERIFY(!ZzForwardRule{ZzForwardRule::Type::Local, QStringLiteral("127.0.0.1"), 13306,
                               QString(), 3306}.validate().isEmpty());
        QVERIFY(!ZzForwardRule{ZzForwardRule::Type::Remote, QStringLiteral("0.0.0.0"), 8080,
                               QString(), 3000}.validate().isEmpty());
        // Local/Remote 目标端口 0
        QVERIFY(!ZzForwardRule{ZzForwardRule::Type::Local, QStringLiteral("127.0.0.1"), 13306,
                               QStringLiteral("db.internal"), 0}.validate().isEmpty());
        // 空监听地址
        QVERIFY(!ZzForwardRule{ZzForwardRule::Type::Dynamic, QString(), 1080,
                               QString(), 0}.validate().isEmpty());
    }

    /** @brief 列表级校验：同 (type, listenHost, listenPort) 不允许重复。 */
    void validateListDetectsDuplicate()
    {
        const ZzForwardRule a{ZzForwardRule::Type::Local, QStringLiteral("127.0.0.1"), 13306,
                              QStringLiteral("db.internal"), 3306};
        const ZzForwardRule b{ZzForwardRule::Type::Remote, QStringLiteral("127.0.0.1"), 13306,
                              QStringLiteral("127.0.0.1"), 3000}; // 类型不同，不算重复
        const ZzForwardRule c{ZzForwardRule::Type::Local, QStringLiteral("127.0.0.1"), 13306,
                              QStringLiteral("other.host"), 22}; // 与 a 同三元组
        QVERIFY(ZzForwardRule::validateList({a, b}).isEmpty());
        QVERIFY(!ZzForwardRule::validateList({a, c}).isEmpty());
    }

    /** @brief 规则描述串供状态栏提示使用。 */
    void describeFormatsReadable()
    {
        const ZzForwardRule local{ZzForwardRule::Type::Local, QStringLiteral("127.0.0.1"), 13306,
                                  QStringLiteral("db.internal"), 3306};
        QCOMPARE(local.describe(), QStringLiteral("本地 127.0.0.1:13306 → db.internal:3306"));
        const ZzForwardRule dynamic{ZzForwardRule::Type::Dynamic, QStringLiteral("127.0.0.1"), 1080,
                                    QString(), 0};
        QCOMPARE(dynamic.describe(), QStringLiteral("动态 127.0.0.1:1080"));
    }
};

QTEST_GUILESS_MAIN(ZzForwardRuleTest)

#include "ZzForwardRuleTest.moc"
```

在 `tests/session/CMakeLists.txt` 的 `zz_session_add_test(ZzCredentialStoreTest)` 行后追加：

```cmake
zz_session_add_test(ZzForwardRuleTest)
```

- [ ] **步骤 2：运行测试验证失败**

```bash
cmake --preset linux-gcc-release && cmake --build --preset linux-gcc-release
ctest --preset linux-gcc-release -R ZzForwardRuleTest --output-on-failure
```

预期：构建失败或测试不存在（`ZzForwardRule.h: No such file or directory` / 未注册用例）。

- [ ] **步骤 3：编写实现**

创建 `src/session/ZzForwardRule.h`：

```cpp
#pragma once

#include <QJsonObject>
#include <QString>
#include <QVector>

/**
 * @brief 端口转发规则（纯值类型，规格 §五）。
 *
 * 绑定在会话 profile 上：连接成功后自动启动，断线随会话销毁，重连自动重建。
 * 三种类型：Local（-L）、Remote（-R）、Dynamic（-D SOCKS5）。
 */
struct ZzForwardRule {
    /** @brief 转发类型。 */
    enum class Type {
        Local,   ///< 本地转发：监听本地端口 → 固定目标
        Remote,  ///< 远程转发：服务端监听 → 本地目标
        Dynamic  ///< 动态转发：本地 SOCKS5 入口，目标由握手解析
    };

    Type type = Type::Local;                            ///< 转发类型
    QString listenHost = QStringLiteral("127.0.0.1");   ///< 监听地址
    quint16 listenPort = 0;                             ///< 监听端口（1-65535）
    QString targetHost;                                 ///< 目标地址（Local/Remote 必填；Dynamic 忽略）
    quint16 targetPort = 0;                             ///< 目标端口（Local/Remote 必填；Dynamic 忽略）

    /**
     * @brief 序列化为 JSON 对象。
     * @return 包含全部字段的 JSON 对象（type 以字符串落盘）。
     */
    QJsonObject toJson() const;

    /**
     * @brief 从 JSON 对象反序列化。
     * @param obj 由 toJson() 产出的 JSON 对象；缺失或非法字段使用默认值。
     * @return 还原后的转发规则。
     */
    static ZzForwardRule fromJson(const QJsonObject &obj);

    /**
     * @brief 单条规则校验（规格 §五）。
     * @return 合法返回空串；否则返回中文错误描述。
     * @note 端口字段为 quint16，天然不超 65535；此处拒绝 0 与缺目标。
     */
    QString validate() const;

    /**
     * @brief 列表级校验：同 (type, listenHost, listenPort) 不允许重复（规格 §五）。
     * @param rules 待校验规则列表。
     * @return 无冲突返回空串；否则返回首个冲突的中文描述。
     */
    static QString validateList(const QVector<ZzForwardRule> &rules);

    /**
     * @brief 一行可读描述（状态栏/日志提示用）。
     * @return 如「本地 127.0.0.1:13306 → db.internal:3306」「动态 127.0.0.1:1080」。
     */
    QString describe() const;

    /** @brief 全字段相等比较。 */
    bool operator==(const ZzForwardRule &other) const = default;
};

/**
 * @brief 转发类型转 JSON 字符串（"local"/"remote"/"dynamic"）。
 */
QString zzForwardRuleTypeToString(ZzForwardRule::Type type);

/**
 * @brief JSON 字符串转转发类型；无法识别时回退 Local。
 */
ZzForwardRule::Type zzForwardRuleTypeFromString(const QString &text);
```

创建 `src/session/ZzForwardRule.cpp`：

```cpp
#include "ZzForwardRule.h"

namespace {

const QString kTypeKey = QStringLiteral("type");
const QString kListenHostKey = QStringLiteral("listenHost");
const QString kListenPortKey = QStringLiteral("listenPort");
const QString kTargetHostKey = QStringLiteral("targetHost");
const QString kTargetPortKey = QStringLiteral("targetPort");

/** @brief 类型 → 中文名（describe 用）。 */
QString typeDisplayName(ZzForwardRule::Type type)
{
    switch (type) {
    case ZzForwardRule::Type::Local:   return QStringLiteral("本地");
    case ZzForwardRule::Type::Remote:  return QStringLiteral("远程");
    case ZzForwardRule::Type::Dynamic: return QStringLiteral("动态");
    }
    return QStringLiteral("本地");
}

} // namespace

QString zzForwardRuleTypeToString(ZzForwardRule::Type type)
{
    switch (type) {
    case ZzForwardRule::Type::Local:   return QStringLiteral("local");
    case ZzForwardRule::Type::Remote:  return QStringLiteral("remote");
    case ZzForwardRule::Type::Dynamic: return QStringLiteral("dynamic");
    }
    return QStringLiteral("local");
}

ZzForwardRule::Type zzForwardRuleTypeFromString(const QString &text)
{
    if (text == QLatin1String("remote"))
        return ZzForwardRule::Type::Remote;
    if (text == QLatin1String("dynamic"))
        return ZzForwardRule::Type::Dynamic;
    return ZzForwardRule::Type::Local;
}

QJsonObject ZzForwardRule::toJson() const
{
    QJsonObject obj;
    obj.insert(kTypeKey, zzForwardRuleTypeToString(type));
    obj.insert(kListenHostKey, listenHost);
    obj.insert(kListenPortKey, static_cast<int>(listenPort));
    obj.insert(kTargetHostKey, targetHost);
    obj.insert(kTargetPortKey, static_cast<int>(targetPort));
    return obj;
}

ZzForwardRule ZzForwardRule::fromJson(const QJsonObject &obj)
{
    ZzForwardRule rule;
    rule.type = zzForwardRuleTypeFromString(obj.value(kTypeKey).toString());
    rule.listenHost = obj.value(kListenHostKey).toString(rule.listenHost);
    rule.listenPort = static_cast<quint16>(obj.value(kListenPortKey).toInt(rule.listenPort));
    rule.targetHost = obj.value(kTargetHostKey).toString();
    rule.targetPort = static_cast<quint16>(obj.value(kTargetPortKey).toInt(rule.targetPort));
    return rule;
}

QString ZzForwardRule::validate() const
{
    if (listenHost.trimmed().isEmpty())
        return QStringLiteral("监听地址不能为空");
    if (listenPort == 0)
        return QStringLiteral("监听端口必须在 1-65535 之间");
    if (type != Type::Dynamic) {
        if (targetHost.trimmed().isEmpty())
            return QStringLiteral("本地/远程转发必须填写目标地址");
        if (targetPort == 0)
            return QStringLiteral("目标端口必须在 1-65535 之间");
    }
    return QString();
}

QString ZzForwardRule::validateList(const QVector<ZzForwardRule> &rules)
{
    for (qsizetype i = 0; i < rules.size(); ++i) {
        for (qsizetype j = i + 1; j < rules.size(); ++j) {
            if (rules[i].type == rules[j].type
                && rules[i].listenHost == rules[j].listenHost
                && rules[i].listenPort == rules[j].listenPort) {
                return QStringLiteral("存在重复的转发规则：%1 %2:%3")
                    .arg(typeDisplayName(rules[i].type), rules[i].listenHost)
                    .arg(rules[i].listenPort);
            }
        }
    }
    return QString();
}

QString ZzForwardRule::describe() const
{
    const QString listen = QStringLiteral("%1:%2").arg(listenHost).arg(listenPort);
    if (type == Type::Dynamic)
        return QStringLiteral("%1 %2").arg(typeDisplayName(type), listen);
    const QString target = QStringLiteral("%1:%2").arg(targetHost).arg(targetPort);
    return QStringLiteral("%1 %2 → %3").arg(typeDisplayName(type), listen, target);
}
```

修改 `src/session/CMakeLists.txt`，在 `ZzCredentialStore.h` 行后追加两行：

```cmake
    ZzForwardRule.cpp
    ZzForwardRule.h
```

- [ ] **步骤 4：运行测试验证通过**

```bash
cmake --preset linux-gcc-release && cmake --build --preset linux-gcc-release
ctest --preset linux-gcc-release -R ZzForwardRuleTest --output-on-failure
```

预期：`ZzForwardRuleTest` 全部 `Passed`。

- [ ] **步骤 5：Commit**

```bash
git add src/session/ZzForwardRule.h src/session/ZzForwardRule.cpp \
        src/session/CMakeLists.txt \
        tests/session/ZzForwardRuleTest.cpp tests/session/CMakeLists.txt
git commit -m "feat(session): 新增 ZzForwardRule 端口转发规则值类型

规格 §五：三种类型（local/remote/dynamic）+ JSON 序列化 +
单条校验（端口 1-65535、local/remote 必须有目标、dynamic 无目标）+
列表级同三元组去重 + describe() 可读描述（状态栏提示用）。"
```

---

### 任务 3：portForwards 配置贯通（Profile → Endpoint → endpointFor）

**文件：**
- 修改：`src/session/ZzSessionProfile.h`、`src/session/ZzSessionProfile.cpp`
- 修改：`src/transport/ZzTransportEndpoint.h`
- 修改：`src/tab/ZzTabManager.cpp`（endpointFor，175-196 行区域）
- 测试：`tests/session/ZzSessionProfileTest.cpp`（追加用例）
- 测试：`tests/unit/tst_ZzConnectFlow.cpp`（追加映射断言）

- [ ] **步骤 1：编写失败的测试**

在 `tests/session/ZzSessionProfileTest.cpp` 的 `metatypeUsableInVariant` 用例后追加：

```cpp
    /** @brief portForwards 字段序列化往返（规格 §五配置格式）。 */
    void portForwardsRoundTrip()
    {
        ZzSessionProfile profile;
        profile.id = QUuid::createUuid();
        profile.name = QStringLiteral("带隧道");
        profile.portForwards = {
            {ZzForwardRule::Type::Local, QStringLiteral("127.0.0.1"), 13306,
             QStringLiteral("db.internal"), 3306},
            {ZzForwardRule::Type::Dynamic, QStringLiteral("127.0.0.1"), 1080,
             QString(), 0},
        };
        const ZzSessionProfile restored = ZzSessionProfile::fromJson(profile.toJson());
        QVERIFY(restored == profile); // operator== 全字段含 portForwards
    }

    /** @brief 旧版 sessions.json 无 portForwards 字段：默认空列表（version 仍为 1 兼容）。 */
    void portForwardsDefaultsToEmpty()
    {
        const ZzSessionProfile profile = ZzSessionProfile::fromJson(QJsonObject());
        QVERIFY(profile.portForwards.isEmpty());
    }
```

头部 include 区追加：

```cpp
#include "ZzForwardRule.h"
```

在 `tests/unit/tst_ZzConnectFlow.cpp` 的 `doubleClickToByteStream` 用例中，profile 构造后追加规则并在 mock 断言区追加映射断言：

```cpp
        profile.portForwards = {
            {ZzForwardRule::Type::Local, QStringLiteral("127.0.0.1"), 13306,
             QStringLiteral("db.internal"), 3306},
        };
```

断言区（`QCOMPARE(mock->lastEndpoint.keepaliveIntervalSeconds, 30);` 行后）追加：

```cpp
        // 端口转发规则必须透传到 endpoint（任务 3 规格 §三）
        QCOMPARE(mock->lastEndpoint.portForwards, profile.portForwards);
```

头部 include 区追加：

```cpp
#include "session/ZzForwardRule.h"
```

- [ ] **步骤 2：运行测试验证失败**

```bash
cmake --preset linux-gcc-release && cmake --build --preset linux-gcc-release
```

预期：编译失败（`ZzSessionProfile` 无 `portForwards` 成员 / `ZzTransportEndpoint` 无 `portForwards` 成员）。

- [ ] **步骤 3：编写实现**

`src/session/ZzSessionProfile.h`：
- include 区追加 `#include "ZzForwardRule.h"` 与 `#include <QVector>`；
- 字段区（`keepAliveIntervalSeconds` 之后）追加：

```cpp
    QVector<ZzForwardRule> portForwards;  ///< 端口转发规则（规格 §五；空=不启用转发）
```

`src/session/ZzSessionProfile.cpp`：
- 匿名命名空间 key 区追加：

```cpp
const QString kPortForwardsKey = QStringLiteral("portForwards");
```

- `toJson()` return 前追加：

```cpp
    QJsonArray forwards;
    for (const ZzForwardRule &rule : portForwards)
        forwards.append(rule.toJson());
    obj.insert(kPortForwardsKey, forwards);
```

- `fromJson()` return 前追加（缺失字段默认空列表，旧文件兼容，version 仍为 1）：

```cpp
    const QJsonArray forwards = obj.value(kPortForwardsKey).toArray();
    for (const auto &v : forwards)
        profile.portForwards.append(ZzForwardRule::fromJson(v.toObject()));
```

- include 区追加 `#include <QJsonArray>`。

`src/transport/ZzTransportEndpoint.h`：
- include 区追加：

```cpp
#include <QtCore/QVector>

#include "session/ZzForwardRule.h"
```

- 字段区（`shellProgram` 之后）追加：

```cpp
    QVector<ZzForwardRule> portForwards; ///< 端口转发规则（规格 §五）；localShell 时为空。
```

`src/tab/ZzTabManager.cpp` 的 `endpointFor()`，`if (endpoint.localShell)` 块之后追加：

```cpp
    // 端口转发仅 SSH 会话有效；local 会话保持空列表（契约：localShell 时为空）
    if (!endpoint.localShell) {
        endpoint.portForwards = profile.portForwards;
    }
```

- [ ] **步骤 4：运行测试验证通过**

```bash
cmake --preset linux-gcc-release && cmake --build --preset linux-gcc-release
ctest --preset linux-gcc-release -R "ZzSessionProfileTest|tst_ZzConnectFlow" --output-on-failure
```

预期：两测试全 `Passed`；随后跑全量 `ctest --preset linux-gcc-release` 确认无回归。

- [ ] **步骤 5：Commit**

```bash
git add src/session/ZzSessionProfile.h src/session/ZzSessionProfile.cpp \
        src/transport/ZzTransportEndpoint.h src/tab/ZzTabManager.cpp \
        tests/session/ZzSessionProfileTest.cpp tests/unit/tst_ZzConnectFlow.cpp
git commit -m "feat(session): 会话配置贯通端口转发规则字段

ZzSessionProfile/ZzTransportEndpoint 各加 portForwards（QVector<ZzForwardRule>）；
sessions.json 序列化缺失字段默认空列表，version 仍为 1（旧文件兼容，规格 §二）；
ZzTabManager::endpointFor 负责 profile→endpoint 映射，local 会话保持空列表。"
```

---

### 任务 4：ZzTunnelHandle 抽象 + ZzTunnelManager 生命周期核心

**文件：**
- 创建：`src/transport/ZzTunnelHandle.h`
- 创建：`src/transport/ZzTunnelManager.h`
- 创建：`src/transport/ZzTunnelManager.cpp`
- 修改：`src/CMakeLists.txt`
- 测试：`tests/unit/tst_ZzTunnelManager.cpp`
- 修改：`tests/CMakeLists.txt`

**设计要点：** manager 不直接依赖 ZzSshCore——经 `ZzTunnelHandle` 抽象 + `ZzTunnelFactory` 接口操作隧道，测试用 fake 注入 listening/failed/invalidated 事件（规格 §七要求 mock 覆盖生命周期）。重连重建由外部（适配器重建 manager）完成，manager 自身只管 startAll/stopAll 与失败隔离。

- [ ] **步骤 1：编写失败的测试**

创建 `tests/unit/tst_ZzTunnelManager.cpp`：

```cpp
#include <QtTest>

#include "transport/ZzTunnelHandle.h"
#include "transport/ZzTunnelManager.h"

namespace {

/**
 * @brief 测试用 fake 隧道句柄：脚本化启动行为，可手动注入事件。
 */
class ZzFakeTunnelHandle : public ZzTunnelHandle
{
    Q_OBJECT
public:
    using ZzTunnelHandle::ZzTunnelHandle;

    void start() override
    {
        ++startCallCount;
        if (failOnStart) {
            emit failed(1001, QStringLiteral("监听端口被占用"));
        } else {
            listening_ = true;
            emit listening(listenPort);
        }
    }
    void stop() override { ++stopCallCount; listening_ = false; }
    int activeConnectionCount() const override { return connectionCount; }

    /** @brief 注入断线失效（模拟 SSH 断开）。 */
    void simulateInvalidated() { listening_ = false; emit invalidated(); }

    bool failOnStart = false;      ///< start 时发射 failed 而非 listening
    quint16 listenPort = 0;        ///< listening 信号携带的端口
    int connectionCount = 0;       ///< 伪装的活动连接数
    int startCallCount = 0;
    int stopCallCount = 0;
    bool listening_ = false;
};

/**
 * @brief 测试用 fake 工厂：记录请求的规则，按序返回 fake 句柄。
 */
class ZzFakeTunnelFactory : public ZzTunnelFactory
{
public:
    ZzTunnelHandle *createHandle(const ZzForwardRule &rule, QObject *parent) override
    {
        requestedRules.append(rule);
        auto *handle = new ZzFakeTunnelHandle(parent);
        handle->failOnStart = failNext;
        failNext = false;
        handles.append(handle);
        return handle;
    }

    QVector<ZzForwardRule> requestedRules; ///< 依次收到的规则
    QList<ZzFakeTunnelHandle *> handles;   ///< 依次产出的句柄
    bool failNext = false;                 ///< 下一个句柄 start 即失败
};

/** @brief 造三条规则：本地/远程/动态各一。 */
QVector<ZzForwardRule> threeRules()
{
    return {
        {ZzForwardRule::Type::Local, QStringLiteral("127.0.0.1"), 13306,
         QStringLiteral("db.internal"), 3306},
        {ZzForwardRule::Type::Remote, QStringLiteral("0.0.0.0"), 8080,
         QStringLiteral("127.0.0.1"), 3000},
        {ZzForwardRule::Type::Dynamic, QStringLiteral("127.0.0.1"), 1080,
         QString(), 0},
    };
}

} // namespace

/**
 * @brief ZzTunnelManager 生命周期单元测试（规格 §七：连接→建隧道→断线→销毁→重建）。
 */
class tst_ZzTunnelManager : public QObject
{
    Q_OBJECT

private slots:
    /** @brief startAll 按规则逐一创建并启动句柄，三规则全部 listening。 */
    void startAllCreatesHandlePerRule()
    {
        ZzFakeTunnelFactory factory;
        ZzTunnelManager manager(&factory, threeRules());
        manager.startAll();

        QCOMPARE(factory.requestedRules.size(), 3);
        QCOMPARE(factory.handles.size(), 3);
        for (auto *h : factory.handles)
            QCOMPARE(h->startCallCount, 1);
        QCOMPARE(manager.activeTunnelCount(), 3);
        QVERIFY(manager.failedRules().isEmpty());
    }

    /** @brief startAll 幂等：重复调用不重复创建。 */
    void startAllIsIdempotent()
    {
        ZzFakeTunnelFactory factory;
        ZzTunnelManager manager(&factory, threeRules());
        manager.startAll();
        manager.startAll();
        QCOMPARE(factory.handles.size(), 3);
    }

    /** @brief 单规则失败隔离：失败规则入 failedRules 并报 ruleFailed，其余规则不受影响。 */
    void failedRuleIsolated()
    {
        ZzFakeTunnelFactory factory;
        ZzTunnelManager manager(&factory, threeRules());
        QSignalSpy failSpy(&manager, &ZzTunnelManager::ruleFailed);
        factory.failNext = true; // 第一条规则失败

        manager.startAll();

        QCOMPARE(failSpy.count(), 1);
        QCOMPARE(manager.failedRules().size(), 1);
        QCOMPARE(manager.failedRules().first().type, ZzForwardRule::Type::Local);
        QCOMPARE(manager.activeTunnelCount(), 2); // 其余两条正常
    }

    /** @brief 断线：句柄 invalidated 即从活动集移除（隧道本体由下层自毁）。 */
    void invalidatedDropsTunnel()
    {
        ZzFakeTunnelFactory factory;
        ZzTunnelManager manager(&factory, threeRules());
        QSignalSpy changeSpy(&manager, &ZzTunnelManager::tunnelsChanged);
        manager.startAll();
        changeSpy.clear();

        factory.handles.at(1)->simulateInvalidated();

        QCOMPARE(manager.activeTunnelCount(), 2);
        QVERIFY(changeSpy.count() >= 1);
        QVERIFY(manager.failedRules().isEmpty()); // 断线不算规则失败
    }

    /** @brief stopAll 停止并清空全部句柄（会话断开销毁路径）。 */
    void stopAllStopsEverything()
    {
        ZzFakeTunnelFactory factory;
        ZzTunnelManager manager(&factory, threeRules());
        manager.startAll();
        manager.stopAll();

        QCOMPARE(manager.activeTunnelCount(), 0);
        // stopAll 后句柄即销毁，只能经 startAll 前的快照断言 stop 被调用
        // （fake 随 delete 失效，故改为重建断言幂等性）
        manager.startAll(); // 重连重建语义：stopAll 后可再次 startAll
        QCOMPARE(manager.activeTunnelCount(), 3);
        QCOMPARE(factory.handles.size(), 6); // 全部重新创建
    }

    /** @brief 析构时自动停止全部隧道（会话关闭路径，不崩溃即通过）。 */
    void destructorStopsCleanly()
    {
        ZzFakeTunnelFactory factory;
        {
            ZzTunnelManager manager(&factory, threeRules());
            manager.startAll();
            QCOMPARE(manager.activeTunnelCount(), 3);
        }
        SUCCEED();
    }
};

QTEST_MAIN(tst_ZzTunnelManager)
#include "tst_ZzTunnelManager.moc"
```

在 `tests/CMakeLists.txt` 的 `zz_add_qtest(tst_ZzAppShell ...)` 行后追加：

```cmake
zz_add_qtest(tst_ZzTunnelManager unit/tst_ZzTunnelManager.cpp)
```

- [ ] **步骤 2：运行测试验证失败**

```bash
cmake --preset linux-gcc-release && cmake --build --preset linux-gcc-release
```

预期：编译失败（`transport/ZzTunnelHandle.h: No such file or directory`）。

- [ ] **步骤 3：编写实现**

创建 `src/transport/ZzTunnelHandle.h`：

```cpp
#pragma once

#include <QObject>

/**
 * @brief 隧道句柄抽象：一条已创建转发规则的运行时实体（GUI 线程）。
 *
 * 生产实现（ZzSshTunnelHandle）包装 ZzSshCore 的 ZzSshTunnel（-L/-D）与
 * ZzSshForwardListener（-R）；测试用 fake 注入生命周期事件。
 * 信号语义与 ZzSshCore 两侧逐一对齐（规格 §三/§六）。
 */
class ZzTunnelHandle : public QObject
{
    Q_OBJECT
public:
    using QObject::QObject;

    /** @brief 启动监听/转发（幂等）。结局为 listening() 或 failed()。 */
    virtual void start() = 0;

    /** @brief 停止并关闭全部转发连接（幂等；不发射信号）。 */
    virtual void stop() = 0;

    /** @brief 当前活动转发连接数。 */
    virtual int activeConnectionCount() const = 0;

signals:
    /** @brief 监听就绪（boundPort 为实际绑定端口）。 */
    void listening(quint16 boundPort);

    /** @brief 规则级失败：仅该规则受影响，会话保留（规格 §六）。 */
    void failed(int code, const QString &message);

    /** @brief 单连接级错误提示（目标不可达等）：该连接已关闭，隧道继续服务。 */
    void connectionError(const QString &message);

    /** @brief SSH 连接断开，隧道已自动停止；重连后由新 manager 重建。 */
    void invalidated();
};
```

创建 `src/transport/ZzTunnelManager.h`：

```cpp
#pragma once

#include <optional>

#include <QObject>
#include <QVector>

#include "session/ZzForwardRule.h"

class ZzTunnelHandle;

/**
 * @brief 隧道工厂：按规则创建隧道句柄。
 *
 * 生产实现 ZzSshTunnelFactory 包装 ZzSshConnection；测试注入 fake。
 * 创建失败（连接未就绪等）返回 nullptr，由 ZzTunnelManager 记为规则失败。
 */
class ZzTunnelFactory
{
public:
    virtual ~ZzTunnelFactory() = default;

    /**
     * @brief 按规则创建隧道句柄。
     * @param rule 转发规则。
     * @param parent 句柄的 QObject 父对象（manager 传入自身）。
     * @return 句柄；无法创建返回 nullptr。
     */
    virtual ZzTunnelHandle *createHandle(const ZzForwardRule &rule, QObject *parent) = 0;
};

/**
 * @brief 单个活动会话的隧道集合（规格 §三）：连接后建隧道、断线销毁、重连重建。
 *
 * startAll 按规则逐一经工厂创建句柄并启动；单规则失败只记入 failedRules
 * 并发射 ruleFailed，不影响其余规则（规格 §六错误矩阵）。
 * 断线时各句柄自下而上 invalidated，manager 将其移出活动集；
 * 重连重建由外部重建 manager 完成（ZzSshTransportAdapter 随新连接驱动）。
 */
class ZzTunnelManager : public QObject
{
    Q_OBJECT
public:
    /**
     * @brief 构造隧道管理器。
     * @param factory 隧道工厂（非拥有，须比 manager 活得久）。
     * @param rules 本会话的转发规则列表。
     */
    ZzTunnelManager(ZzTunnelFactory *factory, QVector<ZzForwardRule> rules,
                    QObject *parent = nullptr);

    /** @brief 析构即 stopAll（会话关闭释放全部监听）。 */
    ~ZzTunnelManager() override;

    /** @brief 启动全部规则（幂等）。 */
    void startAll();

    /** @brief 停止并销毁全部隧道（幂等；之后可再次 startAll 重建）。 */
    void stopAll();

    /** @brief 当前活动（listening）隧道数。 */
    int activeTunnelCount() const;

    /** @brief 启动失败的规则列表。 */
    QVector<ZzForwardRule> failedRules() const;

signals:
    /** @brief 活动隧道数或失败集合变化（状态栏刷新用）。 */
    void tunnelsChanged();

    /** @brief 单条规则启动失败（规格 §六：单规则失败隔离）。 */
    void ruleFailed(const ZzForwardRule &rule, const QString &message);

    /** @brief 隧道内单连接级错误提示（透传自句柄）。 */
    void tunnelConnectionError(const QString &message);

private:
    /** @brief 一条规则的运行时条目。 */
    struct Entry {
        ZzForwardRule rule;
        ZzTunnelHandle *handle = nullptr;
        bool listening = false;   ///< 是否已收到 listening 信号
    };

    /** @brief 按句柄移除条目并销毁句柄；返回被移除的规则，未找到返回 std::nullopt。 */
    std::optional<ZzForwardRule> dropEntry(ZzTunnelHandle *handle);

    ZzTunnelFactory *m_factory;          ///< 非拥有
    QVector<ZzForwardRule> m_rules;      ///< 全部规则
    QVector<ZzForwardRule> m_failed;     ///< 启动失败的规则
    QList<Entry> m_entries;              ///< 活动条目
    bool m_started = false;
};
```

创建 `src/transport/ZzTunnelManager.cpp`：

```cpp
#include "ZzTunnelManager.h"

#include <utility>

#include "ZzTunnelHandle.h"

ZzTunnelManager::ZzTunnelManager(ZzTunnelFactory *factory,
                                 QVector<ZzForwardRule> rules, QObject *parent)
    : QObject(parent)
    , m_factory(factory)
    , m_rules(std::move(rules))
{
}

ZzTunnelManager::~ZzTunnelManager()
{
    stopAll();
}

void ZzTunnelManager::startAll()
{
    if (m_started) {
        return;
    }
    m_started = true;

    for (const ZzForwardRule &rule : m_rules) {
        ZzTunnelHandle *handle = m_factory->createHandle(rule, this);
        if (!handle) {
            m_failed.append(rule);
            emit ruleFailed(rule, QStringLiteral("创建隧道失败（连接未就绪）"));
            continue;
        }

        Entry entry;
        entry.rule = rule;
        entry.handle = handle;
        m_entries.append(entry);

        // 信号接线必须先于 start()：失败可能同步发射（如 QTcpServer 绑定占用）
        connect(handle, &ZzTunnelHandle::listening, this,
                [this, handle](quint16 /*boundPort*/) {
                    for (Entry &e : m_entries) {
                        if (e.handle == handle) {
                            e.listening = true;
                            break;
                        }
                    }
                    emit tunnelsChanged();
                });
        connect(handle, &ZzTunnelHandle::failed, this,
                [this, handle](int /*code*/, const QString &message) {
                    const auto rule = dropEntry(handle);
                    if (!rule) {
                        return;
                    }
                    m_failed.append(*rule);
                    emit ruleFailed(*rule, message);
                    emit tunnelsChanged();
                });
        connect(handle, &ZzTunnelHandle::connectionError, this,
                &ZzTunnelManager::tunnelConnectionError);
        connect(handle, &ZzTunnelHandle::invalidated, this, [this, handle]() {
            if (dropEntry(handle)) {
                emit tunnelsChanged();
            }
        });

        handle->start();
    }
    emit tunnelsChanged();
}

void ZzTunnelManager::stopAll()
{
    m_started = false;
    m_failed.clear();
    // exchange 快照移出再逐个销毁：防 stop/delete 触发的信号重入修改 m_entries
    const QList<Entry> entries = std::exchange(m_entries, {});
    for (const Entry &e : entries) {
        e.handle->stop();
        delete e.handle; // 同线程、非事件处理中，直接 delete 安全
    }
}

int ZzTunnelManager::activeTunnelCount() const
{
    int count = 0;
    for (const Entry &e : m_entries) {
        if (e.listening) {
            ++count;
        }
    }
    return count;
}

QVector<ZzForwardRule> ZzTunnelManager::failedRules() const
{
    return m_failed;
}

std::optional<ZzForwardRule> ZzTunnelManager::dropEntry(ZzTunnelHandle *handle)
{
    for (qsizetype i = 0; i < m_entries.size(); ++i) {
        if (m_entries[i].handle == handle) {
            const ZzForwardRule rule = m_entries[i].rule;
            m_entries.removeAt(i);
            handle->deleteLater(); // 信号发射途中，延迟销毁
            return rule;
        }
    }
    return std::nullopt;
}
```

修改 `src/CMakeLists.txt` 的 `ZZCLAWTERM_APP_SOURCES`，在 `transport/ZzSshTransport.cpp` 行后追加：

```cmake
    transport/ZzTunnelHandle.h
    transport/ZzTunnelManager.h
    transport/ZzTunnelManager.cpp
```

- [ ] **步骤 4：运行测试验证通过**

```bash
cmake --preset linux-gcc-release && cmake --build --preset linux-gcc-release
ctest --preset linux-gcc-release -R tst_ZzTunnelManager --output-on-failure
```

预期：`tst_ZzTunnelManager` 6 用例全 `Passed`；随后全量 `ctest --preset linux-gcc-release` 无回归。

- [ ] **步骤 5：Commit**

```bash
git add src/transport/ZzTunnelHandle.h src/transport/ZzTunnelManager.h \
        src/transport/ZzTunnelManager.cpp src/CMakeLists.txt \
        tests/unit/tst_ZzTunnelManager.cpp tests/CMakeLists.txt
git commit -m "feat(transport): 新增 ZzTunnelManager 隧道生命周期管理

ZzTunnelHandle 抽象（信号语义对齐 ZzSshCore 隧道/监听器）+
ZzTunnelFactory 工厂接口，manager 经抽象操作隧道，
fake 工厂单测覆盖：按规则创建、startAll 幂等、单规则失败隔离、
断线 invalidated 移除、stopAll 销毁、析构清理（规格 §六/§七）。
重连重建由外部重建 manager 完成（ZzSshTransportAdapter 随新连接驱动）。"
```

---

### 任务 5：生产接线（ZzSshTunnelHandle / ZzSshTunnelFactory / 适配器驱动）

**文件：**
- 创建：`src/transport/ZzSshTunnelHandle.h`
- 创建：`src/transport/ZzSshTunnelHandle.cpp`
- 修改：`src/transport/ZzTransportInterface.h`（加两个信号）
- 修改：`src/transport/ZzSshTransport.h`、`src/transport/ZzSshTransport.cpp`
- 修改：`src/CMakeLists.txt`
- 测试：`tests/unit/tst_ZzSshTunnelHandle.cpp`
- 修改：`tests/CMakeLists.txt`

**设计要点：**
- `ZzSshTunnelHandle` 两个构造分别包装 `ZzSshTunnel`（-L/-D）与 `ZzSshForwardListener`（-R），**句柄取得被包装对象的所有权**（`setParent(handle)`），信号逐一转发。
- `ZzSshTunnelFactory` 按规则类型路由：`Local/Dynamic → ZzSshConnection::createTunnel`，`Remote → createForwardListener`（仅 Connected 可用，返回 nullptr 即创建失败）。
- 适配器在 `onConnected()` 尾部驱动 manager；`open()` 重连废弃块与 `close()` 中先销毁 manager 再废弃 `m_conn`（顺序不能反：manager 的句柄持有隧道，隧道观察连接）。
- 规则失败/单连接错误走新增的 `statusNotice` 信号（**不能**走 `errorOccurred`——那会触发标签内错误横幅，规格 §六 只要状态栏瞬时提示）。
- 活动隧道数经新增的 `tunnelCountChanged(int)` 信号上抛（任务 7 接到状态栏）。

- [ ] **步骤 1：编写失败的测试**

创建 `tests/unit/tst_ZzSshTunnelHandle.cpp`：

```cpp
#include <QtTest>

#include <ZzSshConnection.h>

#include "transport/ZzSshTunnelHandle.h"

/**
 * @brief ZzSshTunnelHandle / ZzSshTunnelFactory 单元测试。
 *
 * 注：createTunnel 任意连接状态可创建（本地 QTcpServer 先行监听）；
 * createForwardListener 仅 Connected 可用，未连接返回 nullptr（工厂记为创建失败）。
 */
class tst_ZzSshTunnelHandle : public QObject
{
    Q_OBJECT

private slots:
    /** @brief 工厂按类型路由：Local/Dynamic 出隧道句柄，Remote 未连接返回 nullptr。 */
    void factoryRoutesByRuleType()
    {
        ZzSshConnection conn; // 未连接：隧道可建，监听器不可建
        ZzSshTunnelFactory factory(&conn);

        ZzTunnelHandle *local = factory.createHandle(
            {ZzForwardRule::Type::Local, QStringLiteral("127.0.0.1"), 0,
             QStringLiteral("db.internal"), 3306}, this);
        QVERIFY(local != nullptr);

        ZzTunnelHandle *dynamic = factory.createHandle(
            {ZzForwardRule::Type::Dynamic, QStringLiteral("127.0.0.1"), 0,
             QString(), 0}, this);
        QVERIFY(dynamic != nullptr);

        ZzTunnelHandle *remote = factory.createHandle(
            {ZzForwardRule::Type::Remote, QStringLiteral("0.0.0.0"), 8080,
             QStringLiteral("127.0.0.1"), 3000}, this);
        QVERIFY(remote == nullptr); // 未连接时监听器不可创建
    }

    /** @brief 句柄 start/stop 委托到隧道：本地隧道真实监听（端口 0=系统分配）。 */
    void handleDelegatesStartStop()
    {
        ZzSshConnection conn;
        ZzSshTunnelFactory factory(&conn);
        ZzTunnelHandle *handle = factory.createHandle(
            {ZzForwardRule::Type::Local, QStringLiteral("127.0.0.1"), 0,
             QStringLiteral("127.0.0.1"), 22}, this);
        QVERIFY(handle != nullptr);

        QSignalSpy listenSpy(handle, &ZzTunnelHandle::listening);
        handle->start();
        // QTcpServer 本地监听为同步路径，信号可能已发射
        QVERIFY(listenSpy.count() == 1 || listenSpy.wait(3000));
        QVERIFY(listenSpy.first().at(0).toUInt() > 0); // 实际绑定端口

        handle->stop(); // 幂等不崩溃
        handle->stop();
        QCOMPARE(handle->activeConnectionCount(), 0);
    }

    /** @brief 句柄销毁级联销毁被包装隧道（setParent 所有权语义）。 */
    void handleOwnsTunnel()
    {
        auto *conn = new ZzSshConnection;
        ZzSshTunnelFactory factory(conn);
        ZzTunnelHandle *handle = factory.createHandle(
            {ZzForwardRule::Type::Dynamic, QStringLiteral("127.0.0.1"), 0,
             QString(), 0}, nullptr);
        QVERIFY(handle != nullptr);
        QVERIFY(handle->parent() == nullptr);
        delete handle; // 隧道随之销毁（隧道已是句柄子对象）；不崩溃即通过
        delete conn;
        SUCCEED();
    }
};

QTEST_MAIN(tst_ZzSshTunnelHandle)
#include "tst_ZzSshTunnelHandle.moc"
```

在 `tests/CMakeLists.txt` 的 `zz_add_qtest(tst_ZzTunnelManager ...)` 行后追加：

```cmake
zz_add_qtest(tst_ZzSshTunnelHandle unit/tst_ZzSshTunnelHandle.cpp)
```

- [ ] **步骤 2：运行测试验证失败**

```bash
cmake --preset linux-gcc-release && cmake --build --preset linux-gcc-release
```

预期：编译失败（`transport/ZzSshTunnelHandle.h: No such file or directory`）。

- [ ] **步骤 3：编写实现**

创建 `src/transport/ZzSshTunnelHandle.h`：

```cpp
#pragma once

#include "ZzTunnelHandle.h"
#include "session/ZzForwardRule.h"

class ZzSshConnection;
class ZzSshForwardListener;
class ZzSshTunnel;

/**
 * @brief 生产隧道句柄：包装 ZzSshTunnel（-L/-D）或 ZzSshForwardListener（-R）。
 *
 * 构造即取得被包装对象所有权（setParent 到本句柄），信号逐一转发。
 * 两种实体信号集一致（listening/failed/connectionError/invalidated），
 * 仅创建方式不同，故共用一个句柄类。
 */
class ZzSshTunnelHandle : public ZzTunnelHandle
{
    Q_OBJECT
public:
    /** @brief 包装本地/动态隧道（-L/-D）。tunnel 不可为空。 */
    explicit ZzSshTunnelHandle(ZzSshTunnel *tunnel, QObject *parent = nullptr);

    /** @brief 包装远程转发监听器（-R）。listener 不可为空。 */
    explicit ZzSshTunnelHandle(ZzSshForwardListener *listener, QObject *parent = nullptr);

    void start() override;
    void stop() override;
    int activeConnectionCount() const override;

private:
    /** @brief 统一接线：源对象四信号转发为本句柄信号。 */
    void wireSignals(QObject *source);

    ZzSshTunnel *m_tunnel = nullptr;            ///< -L/-D 实体（本句柄为父）
    ZzSshForwardListener *m_listener = nullptr; ///< -R 实体（本句柄为父）
};

/**
 * @brief 生产隧道工厂：经 ZzSshConnection 按规则类型创建句柄（规格 §三）。
 *
 * Local/Dynamic → createTunnel（任意连接状态可创建）；
 * Remote → createForwardListener（仅 Connected，否则返回 nullptr）。
 */
class ZzSshTunnelFactory : public ZzTunnelFactory
{
public:
    /** @brief 构造工厂。connection 非拥有，须比工厂活得久。 */
    explicit ZzSshTunnelFactory(ZzSshConnection *connection);

    ZzTunnelHandle *createHandle(const ZzForwardRule &rule, QObject *parent) override;

private:
    ZzSshConnection *m_connection; ///< 非拥有
};
```

创建 `src/transport/ZzSshTunnelHandle.cpp`：

```cpp
#include "ZzSshTunnelHandle.h"

#include <ZzSshConnection.h>
#include <ZzSshForwardListener.h>
#include <ZzSshTunnel.h>

ZzSshTunnelHandle::ZzSshTunnelHandle(ZzSshTunnel *tunnel, QObject *parent)
    : ZzTunnelHandle(parent)
    , m_tunnel(tunnel)
{
    Q_ASSERT(m_tunnel);
    m_tunnel->setParent(this); // 句柄取得所有权（原 parent 为连接）
    wireSignals(m_tunnel);
}

ZzSshTunnelHandle::ZzSshTunnelHandle(ZzSshForwardListener *listener, QObject *parent)
    : ZzTunnelHandle(parent)
    , m_listener(listener)
{
    Q_ASSERT(m_listener);
    m_listener->setParent(this);
    wireSignals(m_listener);
}

void ZzSshTunnelHandle::wireSignals(QObject *source)
{
    // 两侧实体信号集一致，按名连接逐一转发
    connect(source, SIGNAL(listening(quint16)), this, SIGNAL(listening(quint16)));
    connect(source, SIGNAL(failed(int,QString)), this, SIGNAL(failed(int,QString)));
    connect(source, SIGNAL(connectionError(QString)), this, SIGNAL(connectionError(QString)));
    connect(source, SIGNAL(invalidated()), this, SIGNAL(invalidated()));
}

void ZzSshTunnelHandle::start()
{
    if (m_tunnel) {
        m_tunnel->start();
    } else if (m_listener) {
        m_listener->start();
    }
}

void ZzSshTunnelHandle::stop()
{
    if (m_tunnel) {
        m_tunnel->stop();
    } else if (m_listener) {
        m_listener->stop();
    }
}

int ZzSshTunnelHandle::activeConnectionCount() const
{
    if (m_tunnel) {
        return m_tunnel->activeConnectionCount();
    }
    if (m_listener) {
        return m_listener->activeConnectionCount();
    }
    return 0;
}

ZzSshTunnelFactory::ZzSshTunnelFactory(ZzSshConnection *connection)
    : m_connection(connection)
{
}

ZzTunnelHandle *ZzSshTunnelFactory::createHandle(const ZzForwardRule &rule, QObject *parent)
{
    switch (rule.type) {
    case ZzForwardRule::Type::Local:
        if (auto *tunnel = m_connection->createTunnel(
                ZzSshTunnel::Type::Local, rule.listenHost, rule.listenPort,
                rule.targetHost, rule.targetPort)) {
            return new ZzSshTunnelHandle(tunnel, parent);
        }
        return nullptr;
    case ZzForwardRule::Type::Dynamic:
        if (auto *tunnel = m_connection->createTunnel(
                ZzSshTunnel::Type::Dynamic, rule.listenHost, rule.listenPort)) {
            return new ZzSshTunnelHandle(tunnel, parent);
        }
        return nullptr;
    case ZzForwardRule::Type::Remote:
        if (auto *listener = m_connection->createForwardListener(
                rule.listenHost, rule.listenPort, rule.targetHost, rule.targetPort)) {
            return new ZzSshTunnelHandle(listener, parent);
        }
        return nullptr;
    }
    return nullptr;
}
```

修改 `src/transport/ZzTransportInterface.h`，signals 区（`disconnected` 声明之后）追加：

```cpp
    /** @brief 活动隧道数变化（仅支持转发的传输发射；无规则时不发射）。 */
    void tunnelCountChanged(int count);
    /** @brief 瞬时提示消息（状态栏展示，不弹窗不横幅；如转发规则失败/单连接错误）。 */
    void statusNotice(const QString &message);
```

修改 `src/transport/ZzSshTransport.h`：
- 前置声明区追加 `class ZzTunnelManager; class ZzSshTunnelFactory;`；
- include 区追加 `#include <memory>`（若已有则跳过）；
- 私有成员区追加：

```cpp
    std::unique_ptr<ZzSshTunnelFactory> m_tunnelFactory; ///< 随 m_conn 重建
    ZzTunnelManager *m_tunnelManager = nullptr;          ///< 本对象为父；随 m_conn 重建
```

- 私有方法区追加：

```cpp
    /** @brief 销毁隧道管理器（先 stopAll 释放监听，再 deleteLater）。 */
    void destroyTunnelManager();
    /** @brief connected 后按 endpoint.portForwards 创建并启动隧道管理器。 */
    void startTunnels();
```

修改 `src/transport/ZzSshTransport.cpp`：
- include 区追加：

```cpp
#include "ZzSshTunnelHandle.h"
#include "ZzTunnelManager.h"
```

- `open()` 的重连废弃块（`if (m_conn)` 内、`m_conn->disconnectFromHost()` 之前）追加：

```cpp
        destroyTunnelManager(); // 隧道先于连接销毁（句柄持有观察连接的隧道）
```

- `close()` 的 `if (m_conn)` 块前追加：

```cpp
    destroyTunnelManager();
```

- 文件末尾追加两个方法：

```cpp
void ZzSshTransportAdapter::destroyTunnelManager()
{
    if (!m_tunnelManager) {
        return;
    }
    m_tunnelManager->stopAll();
    m_tunnelManager->deleteLater();
    m_tunnelManager = nullptr;
    m_tunnelFactory.reset();
}

void ZzSshTransportAdapter::startTunnels()
{
    if (m_endpoint.portForwards.isEmpty()) {
        return;
    }
    m_tunnelFactory = std::make_unique<ZzSshTunnelFactory>(m_conn);
    m_tunnelManager = new ZzTunnelManager(m_tunnelFactory.get(),
                                          m_endpoint.portForwards, this);
    // 规则级失败 → 状态栏瞬时提示（规格 §六：单规则失败隔离，不动错误横幅）
    connect(m_tunnelManager, &ZzTunnelManager::ruleFailed, this,
            [this](const ZzForwardRule &rule, const QString &message) {
                emit statusNotice(QStringLiteral("转发规则 %1 启动失败：%2")
                                      .arg(rule.describe(), message));
            });
    connect(m_tunnelManager, &ZzTunnelManager::tunnelConnectionError, this,
            [this](const QString &message) { emit statusNotice(message); });
    connect(m_tunnelManager, &ZzTunnelManager::tunnelsChanged, this, [this]() {
        emit tunnelCountChanged(m_tunnelManager ? m_tunnelManager->activeTunnelCount() : 0);
    });
    m_tunnelManager->startAll();
}
```

- `onConnected()` 末尾（`m_channel->openShell(...)` 调用之后）追加：

```cpp
    startTunnels(); // 连接已就绪：createTunnel/createForwardListener 均可用
```

修改 `src/CMakeLists.txt` 的 `ZZCLAWTERM_APP_SOURCES`，在任务 4 追加的 `transport/ZzTunnelManager.cpp` 行后追加：

```cmake
    transport/ZzSshTunnelHandle.h
    transport/ZzSshTunnelHandle.cpp
```

- [ ] **步骤 4：运行测试验证通过**

```bash
cmake --preset linux-gcc-release && cmake --build --preset linux-gcc-release
ctest --preset linux-gcc-release -R "tst_ZzSshTunnelHandle|tst_ZzTunnelManager|tst_ZzSshTransport" --output-on-failure
```

预期：全 `Passed`；随后全量 `ctest --preset linux-gcc-release` 无回归。

- [ ] **步骤 5：Commit**

```bash
git add src/transport/ZzSshTunnelHandle.h src/transport/ZzSshTunnelHandle.cpp \
        src/transport/ZzTransportInterface.h \
        src/transport/ZzSshTransport.h src/transport/ZzSshTransport.cpp \
        src/CMakeLists.txt tests/unit/tst_ZzSshTunnelHandle.cpp tests/CMakeLists.txt
git commit -m "feat(transport): SSH 适配器接入端口转发隧道管理

ZzSshTunnelHandle 包装 ZzSshTunnel/ZzSshForwardListener 并取得所有权；
ZzSshTunnelFactory 按规则类型路由 createTunnel/createForwardListener；
适配器 onConnected 驱动 ZzTunnelManager，close/重连先销毁 manager 再废弃连接；
ZzTransportInterface 新增 tunnelCountChanged/statusNotice 两信号——
规则失败走瞬时提示（规格 §六），不经 errorOccurred 避免触发错误横幅。"
```

---

### 任务 6：ZzSessionEditDialog 端口转发规则表

**文件：**
- 修改：`src/panel/ZzSessionEditDialog.h`、`src/panel/ZzSessionEditDialog.cpp`
- 测试：`tests/unit/tst_ZzSessionEditDialog.cpp`
- 修改：`tests/CMakeLists.txt`

**设计要点：** 当前对话框为单一 QFormLayout（无页签），按最小改动在密码行后追加「端口转发」分组区块：QTableWidget 五列（类型/监听地址/监听端口/目标地址/目标端口）+ 添加/删除按钮。类型列用 QComboBox cell widget，端口列文本输入在 accept 时校验。非法规则（含重复）经 QMessageBox 提示并拒绝保存（规格 §五：非法规则禁止保存）。local 协议会话同样允许编辑规则（endpointFor 已为 local 清空，不生效但不阻断编辑——保持 UI 简单）。

- [ ] **步骤 1：编写失败的测试**

创建 `tests/unit/tst_ZzSessionEditDialog.cpp`：

```cpp
#include <QtTest>
#include <QComboBox>
#include <QDialogButtonBox>
#include <QDir>
#include <QMessageBox>
#include <QPushButton>
#include <QTableWidget>
#include <QTimer>

#include "panel/ZzSessionEditDialog.h"
#include "session/ZzCredentialStore.h"

/**
 * @brief ZzSessionEditDialog 端口转发规则表单元测试（规格 §五）。
 */
class tst_ZzSessionEditDialog : public QObject
{
    Q_OBJECT

    /** @brief 临时目录凭据库（对话框构造需要；本组用例不涉及密码）。 */
    std::unique_ptr<ZzCredentialStore> makeStore()
    {
        const QString dir = QDir(QDir::tempPath())
            .filePath(QStringLiteral("zzclawterm-editdlg-%1").arg(QCoreApplication::applicationPid()));
        QDir(dir).removeRecursively();
        QDir().mkpath(dir);
        return std::make_unique<ZzCredentialStore>(dir + QStringLiteral("/credentials.dat"));
    }

    /** @brief 造带两条规则的 profile。 */
    static ZzSessionProfile profileWithRules()
    {
        ZzSessionProfile profile;
        profile.id = QUuid::createUuid();
        profile.name = QStringLiteral("隧道机");
        profile.host = QStringLiteral("10.0.0.1");
        profile.portForwards = {
            {ZzForwardRule::Type::Local, QStringLiteral("127.0.0.1"), 13306,
             QStringLiteral("db.internal"), 3306},
            {ZzForwardRule::Type::Dynamic, QStringLiteral("127.0.0.1"), 1080,
             QString(), 0},
        };
        return profile;
    }

    /** @brief 点击对话框 OK 按钮。 */
    static void clickOk(QDialog &dlg)
    {
        auto *buttons = dlg.findChild<QDialogButtonBox *>();
        QVERIFY(buttons);
        buttons->button(QDialogButtonBox::Ok)->click();
    }

    /** @brief 安排自动关闭即将弹出的模态 QMessageBox（校验失败提示会阻塞嵌套事件循环）。 */
    static void autoDismissMessageBox()
    {
        // QMessageBox::warning 走嵌套事件循环，singleShot 在其中触发并关闭弹窗
        QTimer::singleShot(0, [] {
            if (auto *box = qobject_cast<QMessageBox *>(QApplication::activeModalWidget())) {
                box->accept();
            }
        });
    }

private slots:
    /** @brief 构造时表格按 profile.portForwards 填充。 */
    void ctorPopulatesTableFromProfile()
    {
        auto store = makeStore();
        ZzSessionEditDialog dlg(store.get(), profileWithRules());
        auto *table = dlg.findChild<QTableWidget *>(QStringLiteral("forwardTable"));
        QVERIFY(table);
        QCOMPARE(table->rowCount(), 2);
        QCOMPARE(table->item(0, 1)->text(), QStringLiteral("127.0.0.1"));
        QCOMPARE(table->item(0, 2)->text(), QStringLiteral("13306"));
        QCOMPARE(table->item(0, 3)->text(), QStringLiteral("db.internal"));
        QCOMPARE(table->item(0, 4)->text(), QStringLiteral("3306"));
        // 类型列为下拉框
        auto *typeCombo = qobject_cast<QComboBox *>(table->cellWidget(0, 0));
        QVERIFY(typeCombo);
        QCOMPARE(typeCombo->currentData().toInt(),
                 static_cast<int>(ZzForwardRule::Type::Local));
    }

    /** @brief 合法规则：accept 后 profile().portForwards 与表格一致。 */
    void acceptSavesValidRules()
    {
        auto store = makeStore();
        ZzSessionEditDialog dlg(store.get(), profileWithRules());
        QSignalSpy finishSpy(&dlg, &QDialog::finished);
        clickOk(dlg);
        QCOMPARE(finishSpy.count(), 1);
        QCOMPARE(dlg.result(), QDialog::Accepted);
        QCOMPARE(dlg.profile().portForwards.size(), 2);
        QCOMPARE(dlg.profile().portForwards.at(0).targetHost, QStringLiteral("db.internal"));
    }

    /** @brief 非法规则（监听端口 0）禁止保存：accept 被拒绝，profile 不变。 */
    void invalidRuleBlocksAccept()
    {
        auto store = makeStore();
        ZzSessionEditDialog dlg(store.get(), profileWithRules());
        auto *table = dlg.findChild<QTableWidget *>(QStringLiteral("forwardTable"));
        table->item(0, 2)->setText(QStringLiteral("0")); // 非法监听端口

        autoDismissMessageBox(); // 校验失败的 QMessageBox 为模态，自动关闭防挂起
        QSignalSpy finishSpy(&dlg, &QDialog::finished);
        clickOk(dlg);
        QCOMPARE(finishSpy.count(), 0); // 未 finished = accept 被拒绝
        QCOMPARE(dlg.profile().portForwards.size(), 2); // 工作副本未写回
    }

    /** @brief 重复规则（同 type+listenHost+listenPort）禁止保存。 */
    void duplicateRuleBlocksAccept()
    {
        auto store = makeStore();
        ZzSessionEditDialog dlg(store.get(), profileWithRules());
        auto *table = dlg.findChild<QTableWidget *>(QStringLiteral("forwardTable"));
        // 把第二行改成与第一行同三元组
        auto *typeCombo = qobject_cast<QComboBox *>(table->cellWidget(1, 0));
        typeCombo->setCurrentIndex(0); // Local
        table->item(1, 1)->setText(QStringLiteral("127.0.0.1"));
        table->item(1, 2)->setText(QStringLiteral("13306"));

        autoDismissMessageBox();
        QSignalSpy finishSpy(&dlg, &QDialog::finished);
        clickOk(dlg);
        QCOMPARE(finishSpy.count(), 0);
    }

    /** @brief 添加/删除按钮增减行。 */
    void addRemoveButtonsWork()
    {
        auto store = makeStore();
        ZzSessionEditDialog dlg(store.get(), ZzSessionProfile{});
        auto *table = dlg.findChild<QTableWidget *>(QStringLiteral("forwardTable"));
        auto *addBtn = dlg.findChild<QPushButton *>(QStringLiteral("addForwardButton"));
        auto *removeBtn = dlg.findChild<QPushButton *>(QStringLiteral("removeForwardButton"));
        QVERIFY(table && addBtn && removeBtn);
        QCOMPARE(table->rowCount(), 0);

        addBtn->click();
        QCOMPARE(table->rowCount(), 1); // 默认新增一行 Local 规则
        table->selectRow(0);
        removeBtn->click();
        QCOMPARE(table->rowCount(), 0);
    }
};

QTEST_MAIN(tst_ZzSessionEditDialog)
#include "tst_ZzSessionEditDialog.moc"
```

在 `tests/CMakeLists.txt` 的 `zz_add_qtest(tst_ZzSshTunnelHandle ...)` 行后追加：

```cmake
zz_add_qtest(tst_ZzSessionEditDialog unit/tst_ZzSessionEditDialog.cpp)
```

- [ ] **步骤 2：运行测试验证失败**

```bash
cmake --preset linux-gcc-release && cmake --build --preset linux-gcc-release
ctest --preset linux-gcc-release -R tst_ZzSessionEditDialog --output-on-failure
```

预期：编译失败或断言失败（`forwardTable` 不存在）。

- [ ] **步骤 3：编写实现**

`src/panel/ZzSessionEditDialog.h`：
- 前置声明区追加 `class QTableWidget;`；
- 私有方法区追加：

```cpp
    /** @brief 用 m_profile.portForwards 填充规则表。 */
    void populateForwardTable();
    /** @brief 向表格追加一行（默认值或给定规则）。 */
    void appendForwardRow(const ZzForwardRule &rule);
    /** @brief 从表格读出规则列表（未校验）。 */
    QVector<ZzForwardRule> rulesFromTable() const;
```

- 私有成员区追加：

```cpp
    QTableWidget *m_forwardTable = nullptr; ///< 端口转发规则表
```

`src/panel/ZzSessionEditDialog.cpp`：
- include 区追加：

```cpp
#include <QtWidgets/QHBoxLayout>
#include <QtWidgets/QHeaderView>
#include <QtWidgets/QLabel>
#include <QtWidgets/QPushButton>
#include <QtWidgets/QTableWidget>
#include <QtWidgets/QVBoxLayout>
```

- 构造函数中，密码行（`layout->addRow(QStringLiteral("密码："), m_passwordEdit);`）之后、按钮盒之前追加：

```cpp
    // 端口转发规则表（规格 §三/§五）：五列 + 增删按钮
    auto *forwardSection = new QWidget(this);
    auto *forwardLayout = new QVBoxLayout(forwardSection);
    forwardLayout->setContentsMargins(0, 0, 0, 0);
    m_forwardTable = new QTableWidget(0, 5, forwardSection);
    m_forwardTable->setObjectName(QStringLiteral("forwardTable"));
    m_forwardTable->setHorizontalHeaderLabels({
        QStringLiteral("类型"), QStringLiteral("监听地址"), QStringLiteral("监听端口"),
        QStringLiteral("目标地址"), QStringLiteral("目标端口")});
    m_forwardTable->horizontalHeader()->setSectionResizeMode(QHeaderView::Stretch);
    m_forwardTable->setSelectionBehavior(QAbstractItemView::SelectRows);
    forwardLayout->addWidget(m_forwardTable);
    auto *forwardButtons = new QHBoxLayout;
    auto *addButton = new QPushButton(QStringLiteral("添加"), forwardSection);
    addButton->setObjectName(QStringLiteral("addForwardButton"));
    auto *removeButton = new QPushButton(QStringLiteral("删除"), forwardSection);
    removeButton->setObjectName(QStringLiteral("removeForwardButton"));
    forwardButtons->addWidget(addButton);
    forwardButtons->addWidget(removeButton);
    forwardButtons->addStretch();
    forwardLayout->addLayout(forwardButtons);
    layout->addRow(QStringLiteral("端口转发："), forwardSection);
    populateForwardTable();
    connect(addButton, &QPushButton::clicked, this, [this]() {
        appendForwardRow(ZzForwardRule{});
    });
    connect(removeButton, &QPushButton::clicked, this, [this]() {
        const int row = m_forwardTable->currentRow();
        if (row >= 0) {
            m_forwardTable->removeRow(row);
        }
    });
```

- 文件末尾追加三个方法：

```cpp
void ZzSessionEditDialog::populateForwardTable()
{
    for (const ZzForwardRule &rule : m_profile.portForwards) {
        appendForwardRow(rule);
    }
}

void ZzSessionEditDialog::appendForwardRow(const ZzForwardRule &rule)
{
    const int row = m_forwardTable->rowCount();
    m_forwardTable->insertRow(row);

    auto *typeCombo = new QComboBox(m_forwardTable);
    typeCombo->addItem(QStringLiteral("本地 -L"), static_cast<int>(ZzForwardRule::Type::Local));
    typeCombo->addItem(QStringLiteral("远程 -R"), static_cast<int>(ZzForwardRule::Type::Remote));
    typeCombo->addItem(QStringLiteral("动态 -D"), static_cast<int>(ZzForwardRule::Type::Dynamic));
    const int typeIndex = typeCombo->findData(static_cast<int>(rule.type));
    typeCombo->setCurrentIndex(typeIndex >= 0 ? typeIndex : 0);
    m_forwardTable->setCellWidget(row, 0, typeCombo);

    m_forwardTable->setItem(row, 1, new QTableWidgetItem(rule.listenHost));
    m_forwardTable->setItem(row, 2, new QTableWidgetItem(
        rule.listenPort ? QString::number(rule.listenPort) : QString()));
    m_forwardTable->setItem(row, 3, new QTableWidgetItem(rule.targetHost));
    m_forwardTable->setItem(row, 4, new QTableWidgetItem(
        rule.targetPort ? QString::number(rule.targetPort) : QString()));
}

QVector<ZzForwardRule> ZzSessionEditDialog::rulesFromTable() const
{
    QVector<ZzForwardRule> rules;
    for (int row = 0; row < m_forwardTable->rowCount(); ++row) {
        auto *typeCombo = qobject_cast<QComboBox *>(m_forwardTable->cellWidget(row, 0));
        ZzForwardRule rule;
        rule.type = static_cast<ZzForwardRule::Type>(typeCombo->currentData().toInt());
        const auto *listenHostItem = m_forwardTable->item(row, 1);
        const auto *listenPortItem = m_forwardTable->item(row, 2);
        const auto *targetHostItem = m_forwardTable->item(row, 3);
        const auto *targetPortItem = m_forwardTable->item(row, 4);
        rule.listenHost = listenHostItem ? listenHostItem->text().trimmed() : QString();
        // 非法数字/空串 → 0，交由 validate() 拒绝（quint16 超界由 toUInt 失败兜底）
        bool ok = false;
        const uint listenPort = listenPortItem ? listenPortItem->text().toUInt(&ok) : 0;
        rule.listenPort = (ok && listenPort <= 65535) ? static_cast<quint16>(listenPort) : 0;
        rule.targetHost = targetHostItem ? targetHostItem->text().trimmed() : QString();
        const uint targetPort = targetPortItem ? targetPortItem->text().toUInt(&ok) : 0;
        rule.targetPort = (ok && targetPort <= 65535) ? static_cast<quint16>(targetPort) : 0;
        rules.append(rule);
    }
    return rules;
}
```

- `accept()` 中，密码处理块之前（`m_profile.privateKeyPath = ...` 行之后）追加规则校验与写回：

```cpp
    // 端口转发规则：逐条校验 + 列表去重，非法禁止保存（规格 §五）
    const QVector<ZzForwardRule> rules = rulesFromTable();
    for (const ZzForwardRule &rule : rules) {
        const QString error = rule.validate();
        if (!error.isEmpty()) {
            QMessageBox::warning(this, QStringLiteral("转发规则无效"), error);
            return;
        }
    }
    const QString dupError = ZzForwardRule::validateList(rules);
    if (!dupError.isEmpty()) {
        QMessageBox::warning(this, QStringLiteral("转发规则无效"), dupError);
        return;
    }
    m_profile.portForwards = rules;
```

- [ ] **步骤 4：运行测试验证通过**

```bash
cmake --preset linux-gcc-release && cmake --build --preset linux-gcc-release
ctest --preset linux-gcc-release -R "tst_ZzSessionEditDialog|tst_ZzSessionPanel" --output-on-failure
```

预期：全 `Passed`；随后全量 `ctest --preset linux-gcc-release` 无回归。

- [ ] **步骤 5：Commit**

```bash
git add src/panel/ZzSessionEditDialog.h src/panel/ZzSessionEditDialog.cpp \
        tests/unit/tst_ZzSessionEditDialog.cpp tests/CMakeLists.txt
git commit -m "feat(panel): 会话编辑对话框新增端口转发规则表

QTableWidget 五列（类型/监听地址/监听端口/目标地址/目标端口）+
添加/删除按钮；类型列为下拉框，端口文本在 accept 时解析校验；
非法规则（含同三元组重复）QMessageBox 提示并拒绝保存（规格 §五）；
构造时按 profile.portForwards 回填，accept 时写回工作副本。"
```

---

### 任务 7：状态栏隧道指示与瞬时提示链路

**文件：**
- 修改：`tests/mocks/ZzMockTransport.h`（加两个注入方法）
- 修改：`src/terminal/ZzTerminalView.h`、`src/terminal/ZzTerminalView.cpp`
- 修改：`src/tab/ZzTabManager.h`、`src/tab/ZzTabManager.cpp`
- 修改：`src/ZzAppShell.h`、`src/ZzAppShell.cpp`
- 测试：`tests/unit/tst_ZzTunnelIndicator.cpp`
- 修改：`tests/CMakeLists.txt`

**设计要点：** 链路为 `ZzTransportInterface::tunnelCountChanged/statusNotice`（任务 5 已加）→ `ZzTerminalView` 同名透传 → `ZzTabManager::currentTunnelCountChanged`（仅当前标签）/ `statusMessage` → `ZzAppShell` 第四状态栏要素「隧道 N」/ 状态栏瞬时消息。mock 增加注入方法，使整链可离屏测试。

- [ ] **步骤 1：编写失败的测试**

创建 `tests/unit/tst_ZzTunnelIndicator.cpp`：

```cpp
#include <QtTest>
#include <QDir>
#include <QLabel>
#include <QMainWindow>

#include "ZzAppShell.h"
#include "ZzMockTransport.h"
#include "session/ZzSessionProfile.h"
#include "tab/ZzTabManager.h"
#include "terminal/ZzTerminalView.h"
#include "transport/ZzTransportRegistry.h"

/**
 * @brief 状态栏隧道指示链路测试：mock 注入 → 视图透传 → 标签管理器 → 状态栏。
 */
class tst_ZzTunnelIndicator : public QObject
{
    Q_OBJECT

private slots:
    void initTestCase()
    {
        qRegisterMetaType<ZzTransportInterface::State>();
        ZzTransportRegistry::instance().registerTransport(
            QStringLiteral("mock"),
            [](QObject *parent) { return new ZzMockTransport(parent); });
    }

    void cleanupTestCase()
    {
        ZzTransportRegistry::instance().clear();
    }

    /** @brief mock 注入隧道计数 → 当前标签信号 → 状态栏第四要素文案。 */
    void tunnelCountReachesStatusBar()
    {
        // 组合根装配（临时配置目录，避免读写真实用户配置）
        const QString dir = QDir(QDir::tempPath()).filePath(
            QStringLiteral("zzclawterm-tunnel-ind-%1").arg(QCoreApplication::applicationPid()));
        QDir(dir).removeRecursively();
        QDir().mkpath(dir);
        ZzAppShell shell(dir);
        QMainWindow window;
        QVERIFY(shell.assemble(window));
        QWidget container;
        // 页面实例必须存活到用例结束（析构会连带销毁 View）
        auto page = shell.createTerminalPage(&container);
        QVERIFY(page.hasValue());
        auto *tabManager = shell.tabManager();
        QVERIFY(tabManager);
        QLabel *tunnelLabel = shell.statusTunnelLabel();
        QVERIFY(tunnelLabel);
        QCOMPARE(tunnelLabel->text(), QStringLiteral("隧道: 0"));

        // 开一个 mock 会话并注入隧道计数
        ZzSessionProfile profile;
        profile.id = QUuid::createUuid();
        profile.name = QStringLiteral("隧道机");
        profile.protocol = QStringLiteral("mock");
        tabManager->openSession(profile);
        auto *view = tabManager->viewAt(0);
        QVERIFY(view);
        QTRY_COMPARE(view->transportState(), ZzTransportInterface::State::Connected);

        auto *mock = static_cast<ZzMockTransport *>(view->transport());
        QSignalSpy countSpy(tabManager, &ZzTabManager::currentTunnelCountChanged);
        // 全链为同线程直接连接，注入后同步到位
        mock->simulateTunnelCount(2);
        QCOMPARE(countSpy.count(), 1);
        QCOMPARE(tunnelLabel->text(), QStringLiteral("隧道: 2"));

        mock->simulateTunnelCount(0);
        QCOMPARE(tunnelLabel->text(), QStringLiteral("隧道: 0"));
    }

    /** @brief statusNotice 链路到状态栏瞬时消息（不经错误横幅）。 */
    void statusNoticeBecomesTransientMessage()
    {
        ZzTabManager tabs;
        ZzSessionProfile profile;
        profile.id = QUuid::createUuid();
        profile.name = QStringLiteral("提示机");
        profile.protocol = QStringLiteral("mock");
        tabs.openSession(profile);
        auto *view = tabs.viewAt(0);
        QVERIFY(view);
        QTRY_COMPARE(view->transportState(), ZzTransportInterface::State::Connected);

        auto *mock = static_cast<ZzMockTransport *>(view->transport());
        QSignalSpy msgSpy(&tabs, &ZzTabManager::statusMessage);
        mock->simulateStatusNotice(QStringLiteral("转发规则 本地 127.0.0.1:13306 启动失败：监听端口被占用"));
        QTRY_VERIFY(msgSpy.count() >= 1);
        QCOMPARE(msgSpy.last().at(0).toString(),
                 QStringLiteral("转发规则 本地 127.0.0.1:13306 启动失败：监听端口被占用"));
        // 不触发错误横幅
        QVERIFY(!view->errorBanner()->isVisible());
    }
};

QTEST_MAIN(tst_ZzTunnelIndicator)
#include "tst_ZzTunnelIndicator.moc"
```

在 `tests/CMakeLists.txt` 的 `zz_add_qtest(tst_ZzSessionEditDialog ...)` 行后追加：

```cmake
zz_add_qtest(tst_ZzTunnelIndicator unit/tst_ZzTunnelIndicator.cpp)
```

- [ ] **步骤 2：运行测试验证失败**

```bash
cmake --preset linux-gcc-release && cmake --build --preset linux-gcc-release
```

预期：编译失败（`statusTunnelLabel` / `simulateTunnelCount` / `currentTunnelCountChanged` 不存在）。

- [ ] **步骤 3：编写实现**

`tests/mocks/ZzMockTransport.h` 公有区追加：

```cpp
    /** @brief 注入一次活动隧道数变化。 */
    void simulateTunnelCount(int count) { emit tunnelCountChanged(count); }
    /** @brief 注入一条瞬时提示消息。 */
    void simulateStatusNotice(const QString &message) { emit statusNotice(message); }
```

`src/terminal/ZzTerminalView.h` signals 区追加：

```cpp
    /** @brief 活动隧道数透传（ZzTabManager 据此刷新状态栏第四要素）。 */
    void tunnelCountChanged(int count);
    /** @brief 瞬时提示透传（转发规则失败等；不触发错误横幅）。 */
    void statusNotice(const QString &message);
```

`src/terminal/ZzTerminalView.cpp` 的 `setTransport()` 接线区（`errorOccurred` 连接之后）追加：

```cpp
    connect(m_transport, &ZzTransportInterface::tunnelCountChanged, this,
            &ZzTerminalView::tunnelCountChanged);
    connect(m_transport, &ZzTransportInterface::statusNotice, this,
            &ZzTerminalView::statusNotice);
```

`src/tab/ZzTabManager.h`：
- signals 区（`statusMessage` 之后）追加：

```cpp
    /** @brief 当前标签活动隧道数变化（状态栏第四要素）。 */
    void currentTunnelCountChanged(int count);
```

- 私有成员区追加：

```cpp
    QHash<ZzTerminalView *, int> m_tabTunnelCounts; ///< 每标签活动隧道数
```

`src/tab/ZzTabManager.cpp`：
- 标签切换 lambda（发射 `currentStateChanged`/`currentEncodingChanged`/`currentSizeChanged` 的 `connect(tabBar(), &QTabBar::currentChanged, ...)` 块内）追加：

```cpp
        emit currentTunnelCountChanged(m_tabTunnelCounts.value(view, 0));
```

- `wireView()` 末尾追加：

```cpp
    connect(view, &ZzTerminalView::tunnelCountChanged, this,
            [this, view](int count) {
                m_tabTunnelCounts.insert(view, count);
                if (indexOf(view) == currentIndex()) {
                    emit currentTunnelCountChanged(count);
                }
            });
    connect(view, &ZzTerminalView::statusNotice, this,
            [this](const QString &message) { emit statusMessage(message); });
```

- `closeTab()` 中 `m_tabProfiles.remove(view)` 附近（视图移除前）追加：

```cpp
    m_tabTunnelCounts.remove(view);
```

`src/ZzAppShell.h`：
- 观察口区追加：

```cpp
    [[nodiscard]] QLabel *statusTunnelLabel() const;
```

- 私有成员区追加：

```cpp
    QPointer<QLabel> m_tunnelLabel;
```

`src/ZzAppShell.cpp`：
- `assemble()` 状态栏区（`addPermanentWidget(m_sizeLabel)` 之后）追加：

```cpp
    m_tunnelLabel = new QLabel(QStringLiteral("隧道: 0"), m_statusBar);
    m_statusBar->addPermanentWidget(m_tunnelLabel);
```

- `wireTabManager()` 状态栏接线区（`currentSizeChanged` 连接之后）追加：

```cpp
    tabs->connect(tabs, &ZzTabManager::currentTunnelCountChanged, this,
                  [this](int count) {
                      if (m_tunnelLabel) {
                          m_tunnelLabel->setText(QStringLiteral("隧道: %1").arg(count));
                      }
                  });
```

- 访问器区（`statusSizeLabel()` 实现之后）追加：

```cpp
QLabel *ZzAppShell::statusTunnelLabel() const { return m_tunnelLabel; }
```

- [ ] **步骤 4：运行测试验证通过**

```bash
cmake --preset linux-gcc-release && cmake --build --preset linux-gcc-release
ctest --preset linux-gcc-release -R "tst_ZzTunnelIndicator|tst_ZzAppShell|tst_ZzTabManager|tst_ZzTerminalView|tst_ZzConnectFlow" --output-on-failure
```

预期：全 `Passed`；随后全量 `ctest --preset linux-gcc-release` 无回归。

- [ ] **步骤 5：Commit**

```bash
git add tests/mocks/ZzMockTransport.h \
        src/terminal/ZzTerminalView.h src/terminal/ZzTerminalView.cpp \
        src/tab/ZzTabManager.h src/tab/ZzTabManager.cpp \
        src/ZzAppShell.h src/ZzAppShell.cpp \
        tests/unit/tst_ZzTunnelIndicator.cpp tests/CMakeLists.txt
git commit -m "feat: 状态栏新增活动隧道指示与转发瞬时提示链路

链路：ZzTransportInterface::tunnelCountChanged/statusNotice →
ZzTerminalView 透传 → ZzTabManager（每标签计数 + 当前标签转发 +
statusNotice 归并 statusMessage）→ ZzAppShell 第四状态栏要素「隧道 N」。
mock 传输加 simulateTunnelCount/simulateStatusNotice 注入方法。
statusNotice 不触发标签内错误横幅（与 errorOccurred 语义区分）。"
```

---

### 任务 8：全量回归与收尾

- [ ] **步骤 1：全量回归**

```bash
cmake --preset linux-gcc-release && cmake --build --preset linux-gcc-release
ctest --preset linux-gcc-release --output-on-failure
```

预期：全部测试 `Passed`（含本计划新增的 ZzForwardRuleTest / tst_ZzTunnelManager / tst_ZzSshTunnelHandle / tst_ZzSessionEditDialog / tst_ZzTunnelIndicator）。

- [ ] **步骤 2：规格覆盖核对**

逐项核对并在任务报告中给出覆盖表：

- §三 组件（ZzClawTerm 侧）：`ZzForwardRule` ✔（任务 2）、`ZzSessionProfile.portForwards` ✔（任务 3）、`ZzTunnelManager` ✔（任务 4）、`ZzTransportEndpoint.portForwards` + `ZzTabManager::endpointFor` ✔（任务 3）、`ZzSshTransportAdapter` 驱动 ✔（任务 5）、`ZzSessionEditDialog` 规则表 ✔（任务 6）、状态栏指示 ✔（任务 7）
- §五 配置格式与校验：序列化往返 ✔、单条校验 ✔、列表去重 ✔、非法禁止保存 ✔
- §六 错误矩阵（应用侧行为）：单规则失败状态栏提示且会话保留 ✔（任务 5 接线 + 任务 7 链路）、断线销毁/重连重建 ✔（任务 4 manager + 任务 5 适配器重建）、单连接错误只关该连接 ✔（库侧语义经 tunnelConnectionError 透传为提示）
- §七 测试：规则序列化/校验 ✔、manager 生命周期 mock 测试 ✔
- §一 生命周期：连接成功自动启动 ✔、断线销毁 ✔、重连重建 ✔

- [ ] **步骤 3：更新 README**

`README.md` 的「开发中」一节：把 SSH 端口转发条目移入「已实现」，措辞改为：

```markdown
- SSH 端口转发：本地 -L / 远程 -R / 动态 -D（SOCKS5），规则绑定会话 profile、
  连接成功自动启动、断线销毁重连重建、单规则失败隔离（状态栏「隧道 N」指示 +
  失败瞬时提示），规则在会话编辑对话框的端口转发规则表中维护
```

「已实现」的状态栏条目更新为四要素（连接状态 / 编码 / 终端尺寸 / 活动隧道数）。

- [ ] **步骤 4：Commit**

```bash
git add README.md
git commit -m "docs: README 端口转发功能状态更新为已实现

应用侧交付完成（计划 port-forwarding-02 任务 1-7）：
规则配置持久化/自动启动/断线销毁重连重建/失败隔离/
会话编辑对话框规则表/状态栏隧道指示。"
```

- [ ] **步骤 5：收尾确认**

```bash
git log --oneline -10
```

预期：任务 1-8 的 commit 依次在列。向用户汇报：应用侧全部完成；**推送远端需用户确认**。

---

## 附：ZzSshCore 接口速查（339b6d1，实现时逐字以头文件为准）

```cpp
// ZzSshConnection（third_party/ZzSshCore/src/ZzSshConnection.h）
ZzSshTunnel *createTunnel(ZzSshTunnel::Type type, const QString &listenHost, quint16 listenPort,
                          const QString &targetHost = QString(), quint16 targetPort = 0);
    // 任意连接状态可创建；parent 为连接；Local 需要目标，Dynamic 忽略目标
ZzSshForwardListener *createForwardListener(const QString &listenHost, quint16 listenPort,
                                            const QString &targetHost, quint16 targetPort);
    // 仅 Connected 状态可用，否则返回 nullptr；parent 为连接

// ZzSshTunnel / ZzSshForwardListener（信号集一致）
void start();   // 幂等；结局 listening() 或 failed()
void stop();    // 幂等；不发射信号
int activeConnectionCount() const;
// 信号：listening(quint16) / failed(int, QString) / connectionError(QString) / invalidated()
// ZzSshTunnel::Type { Local, Dynamic }；ZzSshTunnel::MaxConnections = 256
```

## 附：自检记录（计划落笔后执行）

- 规格覆盖：§三 ZzClawTerm 侧 7 组件全部有任务承载；§五/§六/§七/§一 应用侧条目均有任务；§八性能门控为库侧指标，已在 port-forwarding-01 任务 9 落地，本计划无新增门控。
- 「状态栏/会话菜单」取状态栏实现：会话菜单需面板感知活动会话计数，侵入大于收益，状态栏已满足「活动状态可见」意图；如用户要菜单项再补。
- 类型一致性：`ZzForwardRule`（任务 2 定义）→ 任务 3/4/5/6 引用同名同字段；`ZzTunnelHandle`/`ZzTunnelFactory`（任务 4 定义）→ 任务 5 生产实现同名；`tunnelCountChanged`/`statusNotice`（任务 5 接口定义）→ 任务 7 链路同名；`ZzSshTunnelFactory`/`ZzSshTunnelHandle`（任务 5）被任务 5 测试同名引用。
- 占位符扫描：无 TODO/待定；所有代码块为可直接落盘的完整实现。
