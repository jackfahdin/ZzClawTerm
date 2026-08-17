# 会话模型与凭据存储（计划C） 实现计划

> **面向 AI 代理的工作者：** 必需子技能：使用 superpowers:subagent-driven-development（推荐）或 superpowers:executing-plans 逐任务实现此计划。步骤使用复选框（`- [ ]`）语法来跟踪进度。

**目标：** 实现规格 §六 的纯 Qt Core 后端库：`ZzSessionProfile`（会话配置档案值类型）、`ZzSessionModel`（JSON 持久化 + 分组树操作）、`ZzCredentialStore`（AES-256-GCM + 主密码的凭据存储），全部附带 QTest 测试与性能门控记录。

**架构：** 三个类组成静态库目标 `ZzSessionCore`（`src/session/`），只依赖 Qt6::Core 与 OpenSSL::Crypto，不依赖 Widgets；测试集中放在 `tests/session/`。会话以 JSON 文件持久化（QSaveFile 原子写），密码明文不落盘——会话只保存指向 `ZzCredentialStore` 条目的 `credentialId` 引用；凭据文件 `credentials.dat` 用 PBKDF2-HMAC-SHA256（60 万次迭代）从主密码派生密钥，OpenSSL EVP 接口做 AES-256-GCM 加解密，解锁后密钥驻留内存，`lock()` 用 `OPENSSL_cleanse` 清零。

**技术栈：** C++20 / Qt 6.8+（Core、Test）/ CMake 3.25+ / OpenSSL 3.x EVP / QTest。

**执行前提（硬性）：** 应用仓库骨架已由**计划 04 任务 1** 提供——根 `CMakeLists.txt`（含 `project()`、C++20 全局设置、`find_package(Qt6 ...)`、`enable_testing()`）、`CMakePresets.json`、`CMakeUserPresets.json.example`、`.gitignore` 均已存在且 Qt6 / OpenSSL 工具链可用。**本计划不创建、不改写这些根构建文件**；唯一例外是在根 `CMakeLists.txt` 末尾追加两行 `add_subdirectory`（任务 1 步骤 3）。`OpenSSL` 的 `find_package` 由本计划在 `src/session/CMakeLists.txt` 内自行完成（CMake 幂等，与根或其他模块重复调用无害）。

**构建与测试命令约定（preset 无关的显式形式；若骨架提供等价 preset 可替换）：**

```bash
# Debug 配置 + 构建
cmake -S . -B build/debug -G Ninja -DCMAKE_BUILD_TYPE=Debug && cmake --build build/debug
# 跑全部测试
ctest --test-dir build/debug --output-on-failure
# 跑单个测试
ctest --test-dir build/debug -R ZzSessionModelTest --output-on-failure
# Release 同理：-B build/release -DCMAKE_BUILD_TYPE=Release
```

若 configure 报 `Could not find a package configuration file provided by "Qt6"`，在 configure 命令追加 `-DCMAKE_PREFIX_PATH=<Qt 6.8+ 安装前缀>`（骨架的 preset 一般已处理）。

**设计决策（规格未明言处，本计划内冻结；其中 id / protocol / credentialId / Q_DECLARE_METATYPE 为计划 04 的冻结契约，必须一致）：**

- `ZzSessionProfile` 包含契约字段：`id`（`QUuid`，由 `ZzSessionModel::addSession` 生成，null 表示未分配）、`protocol`（`QString`，取值 `"ssh"` / `"local"`，默认 `"ssh"`）、`credentialId`（`QUuid`，null 表示无密码引用）；结构体声明后紧跟 `Q_DECLARE_METATYPE(ZzSessionProfile)`，保证可进 QVariant / 信号槽跨线程传递。
- 会话 id 与凭据 id 均使用 `QUuid`；JSON 中以 `QUuid::toString(QUuid::WithoutBraces)` 字符串形式落盘。
- `load()` 在文件不存在时返回 `true` 并视为空模型（首次启动场景）；文件存在但 JSON 非法时返回 `false`。
- `renameGroup` 按前缀重命名（含子分组），禁止把分组重命名为自身的子分组；无匹配会话时视为幂等成功。
- `sessionsInGroup` 只返回直接位于该分组的会话（不含子分组）。
- 凭据文件格式：`"ZZCT"(4B) || version(u32 BE=1) || kdfIterations(u32 BE) || salt(16B) || iv(12B) || 密文 || GCM tag(16B)`；密文为 JSON `{"verifier":"zzclawterm-v1","credentials":[...]}`。GCM tag 校验失败即判定主密码错误或文件损坏。
- 存储路径用 `QStandardPaths::AppConfigLocation`（Linux `~/.config/ZzClawTerm`、Windows `%APPDATA%/ZzClawTerm`、macOS `~/Library/Application Support/ZzClawTerm`），文件名 `sessions.json` / `credentials.dat`，路径经构造函数注入以便测试。

---

## 文件结构

| 文件 | 职责 | 所属任务 |
| ---- | ---- | -------- |
| `CMakeLists.txt`（根，计划 04 所有） | 仅末尾追加两行 `add_subdirectory` | 任务 1 |
| `src/session/CMakeLists.txt` | 静态库 `ZzSessionCore`；`find_package(OpenSSL)`（幂等） | 任务 1、2、4 |
| `src/session/ZzSessionProfile.h/.cpp` | 会话配置档案值类型 + JSON 序列化 + Q_DECLARE_METATYPE | 任务 1 |
| `src/session/ZzSessionModel.h/.cpp` | 会话增删改查、JSON 持久化、分组树操作 | 任务 2、3 |
| `src/session/ZzCredentialStore.h/.cpp` | 主密码初始化/解锁/锁定、凭据增删改查、EVP AES-256-GCM 加解密 | 任务 4、5 |
| `tests/session/CMakeLists.txt` | `zz_session_add_test` 辅助函数、git hash 探测、性能测试编译定义注入 | 任务 1、2、4、6 |
| `tests/session/ZzSessionProfileTest.cpp` | 序列化往返、缺省值、metatype 可用性 | 任务 1 |
| `tests/session/ZzSessionModelTest.cpp` | 增删改查、持久化往返、分组树操作、信号 | 任务 2、3 |
| `tests/session/ZzCredentialStoreTest.cpp` | 初始化/解锁/错误主密码/锁定拒绝、凭据加解密往返 | 任务 4、5 |
| `tests/session/ZzCredentialPerfTest.cpp` | 解锁与加解密往返性能门控，写记录 JSON | 任务 6 |
| `tests/perf/records/.gitkeep` | 保证记录目录入库（规格 §9.1 指定路径） | 任务 6 |

---

### 任务 1：`ZzSessionProfile` 值类型与 JSON 序列化（含接入计划 04 骨架）

**文件：**
- 创建：`src/session/ZzSessionProfile.h`
- 创建：`src/session/ZzSessionProfile.cpp`
- 创建：`src/session/CMakeLists.txt`
- 创建：`tests/session/CMakeLists.txt`
- 创建：`tests/session/ZzSessionProfileTest.cpp`
- 修改：`CMakeLists.txt`（根，仅末尾追加两行 `add_subdirectory`）

- [ ] **步骤 1：编写失败的测试 `tests/session/ZzSessionProfileTest.cpp`**

```cpp
#include <QtTest>
#include <QVariant>

#include "ZzSessionProfile.h"

/**
 * @brief ZzSessionProfile 序列化单元测试。
 */
class ZzSessionProfileTest : public QObject
{
    Q_OBJECT

private slots:
    /** @brief 全字段非默认值，序列化后反序列化必须完全相等。 */
    void serializationRoundTrip()
    {
        ZzSessionProfile profile;
        profile.id = QUuid::createUuid();
        profile.name = QStringLiteral("生产 Web-01");
        profile.groupPath = QStringLiteral("生产环境/Web 服务器");
        profile.protocol = QStringLiteral("local");
        profile.host = QStringLiteral("10.0.0.1");
        profile.port = 2222;
        profile.userName = QStringLiteral("deploy");
        profile.authMethod = ZzAuthMethod::PrivateKey;
        profile.privateKeyPath = QStringLiteral("/home/zz/.ssh/id_ed25519");
        profile.credentialId = QUuid::createUuid();
        profile.terminalType = QStringLiteral("xterm");
        profile.encoding = QStringLiteral("GBK");
        profile.colorSchemeName = QStringLiteral("Solarized Dark");
        profile.keepAliveIntervalSeconds = 30;

        const ZzSessionProfile restored = ZzSessionProfile::fromJson(profile.toJson());
        QVERIFY(restored == profile);
    }

    /** @brief 空 JSON 对象反序列化时所有字段取默认值（含计划 04 冻结契约字段）。 */
    void fromJsonUsesDefaults()
    {
        const ZzSessionProfile profile = ZzSessionProfile::fromJson(QJsonObject());
        QVERIFY(profile.id.isNull());
        QVERIFY(profile.name.isEmpty());
        QVERIFY(profile.groupPath.isEmpty());
        QCOMPARE(profile.protocol, QStringLiteral("ssh"));
        QVERIFY(profile.host.isEmpty());
        QCOMPARE(profile.port, quint16(22));
        QVERIFY(profile.userName.isEmpty());
        QCOMPARE(profile.authMethod, ZzAuthMethod::Agent);
        QVERIFY(profile.privateKeyPath.isEmpty());
        QVERIFY(profile.credentialId.isNull());
        QCOMPARE(profile.terminalType, QStringLiteral("xterm-256color"));
        QCOMPARE(profile.encoding, QStringLiteral("UTF-8"));
        QVERIFY(profile.colorSchemeName.isEmpty());
        QCOMPARE(profile.keepAliveIntervalSeconds, 0);
    }

    /** @brief 三种认证方式序列化为字符串后均可无损还原。 */
    void authMethodStringRoundTrip()
    {
        for (ZzAuthMethod method : {ZzAuthMethod::Agent, ZzAuthMethod::PrivateKey, ZzAuthMethod::Password}) {
            ZzSessionProfile profile;
            profile.authMethod = method;
            const ZzSessionProfile restored = ZzSessionProfile::fromJson(profile.toJson());
            QCOMPARE(restored.authMethod, method);
        }
    }

    /** @brief 计划 04 冻结契约：Q_DECLARE_METATYPE 生效，可装入 QVariant 往返。 */
    void metatypeUsableInVariant()
    {
        QVERIFY(qMetaTypeId<ZzSessionProfile>() != QMetaType::UnknownType);

        ZzSessionProfile profile;
        profile.id = QUuid::createUuid();
        profile.name = QStringLiteral("契约检查");
        const QVariant variant = QVariant::fromValue(profile);
        QVERIFY(variant.canConvert<ZzSessionProfile>());
        QVERIFY(variant.value<ZzSessionProfile>() == profile);
    }
};

QTEST_GUILESS_MAIN(ZzSessionProfileTest)

#include "ZzSessionProfileTest.moc"
```

- [ ] **步骤 2：创建 `src/session/CMakeLists.txt` 与 `tests/session/CMakeLists.txt`**

`src/session/CMakeLists.txt`：

```cmake
# Qt6::Core 与 OpenSSL 的 find_package 均为幂等，允许与根或其他模块重复调用
find_package(Qt6 6.8 REQUIRED COMPONENTS Core)
find_package(OpenSSL 3.0 REQUIRED COMPONENTS Crypto)

set(CMAKE_AUTOMOC ON)

add_library(ZzSessionCore STATIC
    ZzSessionProfile.cpp
    ZzSessionProfile.h
)
target_compile_features(ZzSessionCore PUBLIC cxx_std_20)
target_link_libraries(ZzSessionCore PUBLIC Qt6::Core PRIVATE OpenSSL::Crypto)
target_include_directories(ZzSessionCore PUBLIC ${CMAKE_CURRENT_SOURCE_DIR})
```

`tests/session/CMakeLists.txt`：

```cmake
set(CMAKE_AUTOMOC ON)

find_package(Qt6 6.8 REQUIRED COMPONENTS Test)

# 性能记录（规格 9.1）需要代码版本；探测失败时记为 unknown
find_package(Git QUIET)
set(ZZ_GIT_COMMIT_HASH "unknown")
if(Git_FOUND)
    execute_process(
        COMMAND ${GIT_EXECUTABLE} rev-parse --short HEAD
        WORKING_DIRECTORY ${CMAKE_SOURCE_DIR}
        OUTPUT_VARIABLE ZZ_GIT_COMMIT_HASH_DETECTED
        OUTPUT_STRIP_TRAILING_WHITESPACE
        ERROR_QUIET)
    if(ZZ_GIT_COMMIT_HASH_DETECTED)
        set(ZZ_GIT_COMMIT_HASH "${ZZ_GIT_COMMIT_HASH_DETECTED}")
    endif()
endif()

# 注册一个 QTest 可执行目标：源码为同名 .cpp，链接 ZzSessionCore，并登记进 ctest。
function(zz_session_add_test target)
    add_executable(${target} ${target}.cpp)
    target_link_libraries(${target} PRIVATE ZzSessionCore Qt6::Core Qt6::Test)
    add_test(NAME ${target} COMMAND ${target})
endfunction()

zz_session_add_test(ZzSessionProfileTest)
```

- [ ] **步骤 3：修改根 `CMakeLists.txt`（计划 04 任务 1 已创建），在文件末尾追加**

```cmake
add_subdirectory(src/session)
add_subdirectory(tests/session)
```

追加前先确认根文件已含 `enable_testing()`；若没有（骨架疏漏），在两行之前补一行 `enable_testing()`。除此之外不改动根文件的任何既有内容。

- [ ] **步骤 4：运行测试验证失败**

运行：`cmake -S . -B build/debug -G Ninja -DCMAKE_BUILD_TYPE=Debug && cmake --build build/debug`
预期：编译失败，报错 `'ZzSessionProfile' file not found`（`ZzSessionProfile.h` 尚未创建）。

- [ ] **步骤 5：创建 `src/session/ZzSessionProfile.h`**

```cpp
#pragma once

#include <QJsonObject>
#include <QMetaType>
#include <QString>
#include <QUuid>

/**
 * @brief 会话认证方式。
 */
enum class ZzAuthMethod {
    Agent,      ///< 使用 SSH agent 认证
    PrivateKey, ///< 使用私钥文件认证
    Password    ///< 使用密码认证（密码密文存于 ZzCredentialStore，此处仅保存引用）
};

/**
 * @brief 会话配置档案（纯值类型）。
 *
 * 描述一个会话的全部静态配置。密码等敏感信息不保存在此，
 * 仅通过 credentialId 引用 ZzCredentialStore 中的条目。
 * 分组用路径字符串表示（如 "生产环境/Web 服务器"），空串表示未分组。
 * id / protocol / credentialId 字段与 Q_DECLARE_METATYPE 为计划 04 冻结契约。
 */
struct ZzSessionProfile {
    QUuid id;                       ///< 全局唯一标识（由 ZzSessionModel::addSession 生成；null 表示未分配）
    QString name;                   ///< 会话显示名称
    QString groupPath;              ///< 分组路径，空串表示未分组
    QString protocol = QStringLiteral("ssh"); ///< 协议类型："ssh"（SSH 远程会话）或 "local"（本地 shell）
    QString host;                   ///< 主机地址（IP 或域名；protocol == "local" 时忽略）
    quint16 port = 22;              ///< 端口号（1-65535）
    QString userName;               ///< 登录用户名
    ZzAuthMethod authMethod = ZzAuthMethod::Agent; ///< 认证方式
    QString privateKeyPath;         ///< 私钥文件路径（authMethod == PrivateKey 时有效）
    QUuid credentialId;             ///< 密码引用（authMethod == Password 时有效；null 表示无）
    QString terminalType = QStringLiteral("xterm-256color"); ///< 终端类型（TERM）
    QString encoding = QStringLiteral("UTF-8");              ///< 字符编码
    QString colorSchemeName;        ///< 配色方案名，空串表示使用全局默认
    int keepAliveIntervalSeconds = 0; ///< keepalive 间隔（秒），0 表示禁用

    /**
     * @brief 序列化为 JSON 对象。
     * @return 包含全部字段的 JSON 对象（QUuid 以 WithoutBraces 字符串落盘）。
     */
    QJsonObject toJson() const;

    /**
     * @brief 从 JSON 对象反序列化。
     * @param obj 由 toJson() 产出的 JSON 对象；缺失或非法字段使用默认值。
     * @return 还原后的会话配置档案。
     */
    static ZzSessionProfile fromJson(const QJsonObject &obj);

    /** @brief 全字段相等比较。 */
    bool operator==(const ZzSessionProfile &other) const = default;
};

Q_DECLARE_METATYPE(ZzSessionProfile)
```

- [ ] **步骤 6：创建 `src/session/ZzSessionProfile.cpp`**

```cpp
#include "ZzSessionProfile.h"

namespace {

const QString kIdKey = QStringLiteral("id");
const QString kNameKey = QStringLiteral("name");
const QString kGroupPathKey = QStringLiteral("groupPath");
const QString kProtocolKey = QStringLiteral("protocol");
const QString kHostKey = QStringLiteral("host");
const QString kPortKey = QStringLiteral("port");
const QString kUserNameKey = QStringLiteral("userName");
const QString kAuthMethodKey = QStringLiteral("authMethod");
const QString kPrivateKeyPathKey = QStringLiteral("privateKeyPath");
const QString kCredentialIdKey = QStringLiteral("credentialId");
const QString kTerminalTypeKey = QStringLiteral("terminalType");
const QString kEncodingKey = QStringLiteral("encoding");
const QString kColorSchemeNameKey = QStringLiteral("colorSchemeName");
const QString kKeepAliveKey = QStringLiteral("keepAliveIntervalSeconds");

/**
 * @brief 认证方式转 JSON 字符串。
 */
QString authMethodToString(ZzAuthMethod method)
{
    switch (method) {
    case ZzAuthMethod::Agent:      return QStringLiteral("agent");
    case ZzAuthMethod::PrivateKey: return QStringLiteral("privateKey");
    case ZzAuthMethod::Password:   return QStringLiteral("password");
    }
    return QStringLiteral("agent");
}

/**
 * @brief JSON 字符串转认证方式；无法识别时回退为 Agent。
 */
ZzAuthMethod authMethodFromString(const QString &text)
{
    if (text == QLatin1String("privateKey"))
        return ZzAuthMethod::PrivateKey;
    if (text == QLatin1String("password"))
        return ZzAuthMethod::Password;
    return ZzAuthMethod::Agent;
}

} // namespace

QJsonObject ZzSessionProfile::toJson() const
{
    QJsonObject obj;
    obj.insert(kIdKey, id.toString(QUuid::WithoutBraces));
    obj.insert(kNameKey, name);
    obj.insert(kGroupPathKey, groupPath);
    obj.insert(kProtocolKey, protocol);
    obj.insert(kHostKey, host);
    obj.insert(kPortKey, static_cast<int>(port));
    obj.insert(kUserNameKey, userName);
    obj.insert(kAuthMethodKey, authMethodToString(authMethod));
    obj.insert(kPrivateKeyPathKey, privateKeyPath);
    obj.insert(kCredentialIdKey, credentialId.toString(QUuid::WithoutBraces));
    obj.insert(kTerminalTypeKey, terminalType);
    obj.insert(kEncodingKey, encoding);
    obj.insert(kColorSchemeNameKey, colorSchemeName);
    obj.insert(kKeepAliveKey, keepAliveIntervalSeconds);
    return obj;
}

ZzSessionProfile ZzSessionProfile::fromJson(const QJsonObject &obj)
{
    ZzSessionProfile profile;
    profile.id = QUuid::fromString(obj.value(kIdKey).toString()); // 非法串得 null QUuid
    profile.name = obj.value(kNameKey).toString();
    profile.groupPath = obj.value(kGroupPathKey).toString();
    profile.protocol = obj.value(kProtocolKey).toString(profile.protocol);
    profile.host = obj.value(kHostKey).toString();
    profile.port = static_cast<quint16>(obj.value(kPortKey).toInt(profile.port));
    profile.userName = obj.value(kUserNameKey).toString();
    profile.authMethod = authMethodFromString(obj.value(kAuthMethodKey).toString());
    profile.privateKeyPath = obj.value(kPrivateKeyPathKey).toString();
    profile.credentialId = QUuid::fromString(obj.value(kCredentialIdKey).toString());
    profile.terminalType = obj.value(kTerminalTypeKey).toString(profile.terminalType);
    profile.encoding = obj.value(kEncodingKey).toString(profile.encoding);
    profile.colorSchemeName = obj.value(kColorSchemeNameKey).toString();
    profile.keepAliveIntervalSeconds = obj.value(kKeepAliveKey).toInt(profile.keepAliveIntervalSeconds);
    return profile;
}
```

- [ ] **步骤 7：运行测试验证通过**

运行：`cmake --build build/debug && ctest --test-dir build/debug --output-on-failure`
预期：`100% tests passed, 0 tests failed out of 1`；详细输出含 `PASS   : ZzSessionProfileTest::serializationRoundTrip()`、`PASS   : ZzSessionProfileTest::fromJsonUsesDefaults()`、`PASS   : ZzSessionProfileTest::authMethodStringRoundTrip()`、`PASS   : ZzSessionProfileTest::metatypeUsableInVariant()`。

- [ ] **步骤 8：Commit**

```bash
git add CMakeLists.txt src/session/ tests/session/
git commit -m "feat: 新增 ZzSessionProfile 会话档案值类型与 JSON 序列化"
```

---

### 任务 2：`ZzSessionModel` 增删改查与 JSON 持久化

**文件：**
- 创建：`src/session/ZzSessionModel.h`
- 创建：`src/session/ZzSessionModel.cpp`
- 创建：`tests/session/ZzSessionModelTest.cpp`
- 修改：`src/session/CMakeLists.txt`（库源文件追加）
- 修改：`tests/session/CMakeLists.txt`（注册新测试）

- [ ] **步骤 1：编写失败的测试 `tests/session/ZzSessionModelTest.cpp`**

```cpp
#include <QtTest>
#include <QFile>
#include <QFileInfo>
#include <QSignalSpy>
#include <QTemporaryDir>

#include "ZzSessionModel.h"

namespace {

/**
 * @brief 构造一个用于测试的会话档案。
 * @param name 会话名称。
 * @param groupPath 分组路径。
 */
ZzSessionProfile makeProfile(const QString &name, const QString &groupPath)
{
    ZzSessionProfile profile;
    profile.name = name;
    profile.groupPath = groupPath;
    profile.host = QStringLiteral("192.168.1.10");
    profile.port = 22;
    profile.userName = QStringLiteral("root");
    profile.authMethod = ZzAuthMethod::Password;
    profile.credentialId = QUuid::createUuid();
    return profile;
}

} // namespace

/**
 * @brief ZzSessionModel 增删改查与持久化单元测试。
 */
class ZzSessionModelTest : public QObject
{
    Q_OBJECT

private slots:
    /** @brief 添加会话后可通过返回的 id 查询到，且触发 sessionsChanged 信号。 */
    void addAndQuery()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        ZzSessionModel model(dir.filePath(QStringLiteral("sessions.json")));
        QSignalSpy spy(&model, &ZzSessionModel::sessionsChanged);

        const QUuid id = model.addSession(makeProfile(QStringLiteral("Web-01"),
                                                      QStringLiteral("生产环境/Web 服务器")));
        QVERIFY(!id.isNull());
        QCOMPARE(model.allSessions().size(), 1);
        QCOMPARE(spy.count(), 1);

        const std::optional<ZzSessionProfile> fetched = model.session(id);
        QVERIFY(fetched.has_value());
        QVERIFY(fetched->id == id);
        QCOMPARE(fetched->name, QStringLiteral("Web-01"));
        QCOMPARE(fetched->groupPath, QStringLiteral("生产环境/Web 服务器"));
    }

    /** @brief 调用方自带 id 时尊重该 id；id 冲突时拒绝添加。 */
    void addDuplicateIdRejected()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        ZzSessionModel model(dir.filePath(QStringLiteral("sessions.json")));

        const QUuid fixed = QUuid::createUuid();
        ZzSessionProfile first = makeProfile(QStringLiteral("A"), QString());
        first.id = fixed;
        QVERIFY(model.addSession(first) == fixed);

        ZzSessionProfile second = makeProfile(QStringLiteral("B"), QString());
        second.id = fixed;
        QVERIFY(model.addSession(second).isNull());
        QVERIFY(!model.errorString().isEmpty());
        QCOMPARE(model.allSessions().size(), 1);
    }

    /** @brief 更新已存在的会话；更新不存在的 id 返回 false。 */
    void updateSession()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        ZzSessionModel model(dir.filePath(QStringLiteral("sessions.json")));
        const QUuid id = model.addSession(makeProfile(QStringLiteral("Web-01"), QString()));
        QVERIFY(!id.isNull());

        ZzSessionProfile updated = model.session(id).value();
        updated.port = 2222;
        updated.host = QStringLiteral("10.0.0.99");
        QVERIFY(model.updateSession(updated));
        QCOMPARE(model.session(id)->port, quint16(2222));
        QCOMPARE(model.session(id)->host, QStringLiteral("10.0.0.99"));

        ZzSessionProfile ghost = updated;
        ghost.id = QUuid::createUuid();
        QVERIFY(!model.updateSession(ghost));
    }

    /** @brief 删除已存在的会话；重复删除返回 false。 */
    void removeSession()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        ZzSessionModel model(dir.filePath(QStringLiteral("sessions.json")));
        QSignalSpy spy(&model, &ZzSessionModel::sessionsChanged);

        const QUuid id = model.addSession(makeProfile(QStringLiteral("Web-01"), QString()));
        QVERIFY(model.removeSession(id));
        QVERIFY(model.allSessions().isEmpty());
        QVERIFY(!model.session(id).has_value());
        QCOMPARE(spy.count(), 2); // add + remove

        QVERIFY(!model.removeSession(id));
        QVERIFY(!model.errorString().isEmpty());
    }

    /** @brief 保存后由新模型实例加载，内容必须逐条相等（序列化往返）。 */
    void persistenceRoundTrip()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        const QString path = dir.filePath(QStringLiteral("sessions.json"));

        ZzSessionModel writer(path);
        writer.addSession(makeProfile(QStringLiteral("Web-01"), QStringLiteral("生产环境/Web 服务器")));
        writer.addSession(makeProfile(QStringLiteral("DB-01"), QStringLiteral("生产环境/数据库")));
        writer.addSession(makeProfile(QStringLiteral("本地"), QString()));
        QVERIFY(writer.save());

        ZzSessionModel reader(path);
        QVERIFY(reader.load());
        QVERIFY(reader.allSessions() == writer.allSessions());
    }

    /** @brief 文件不存在（首次启动）时 load 返回 true 且模型为空。 */
    void loadMissingFileIsEmptyModel()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        ZzSessionModel model(dir.filePath(QStringLiteral("sessions.json")));
        QVERIFY(model.load());
        QVERIFY(model.allSessions().isEmpty());
    }

    /** @brief 文件存在但内容不是合法 JSON 对象时 load 返回 false 并给出错误信息。 */
    void loadCorruptFileFails()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        const QString path = dir.filePath(QStringLiteral("sessions.json"));
        QFile file(path);
        QVERIFY(file.open(QIODevice::WriteOnly));
        file.write("this is not json");
        file.close();

        ZzSessionModel model(path);
        QVERIFY(!model.load());
        QVERIFY(!model.errorString().isEmpty());
    }

    /** @brief save 在目录不存在时自动创建目录。 */
    void saveCreatesParentDirectory()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        const QString path = dir.filePath(QStringLiteral("nested/deep/sessions.json"));

        ZzSessionModel model(path);
        model.addSession(makeProfile(QStringLiteral("Web-01"), QString()));
        QVERIFY(model.save());
        QVERIFY(QFileInfo::exists(path));
    }
};

QTEST_GUILESS_MAIN(ZzSessionModelTest)

#include "ZzSessionModelTest.moc"
```

- [ ] **步骤 2：注册测试与库源文件**

修改 `tests/session/CMakeLists.txt`，在 `zz_session_add_test(ZzSessionProfileTest)` 行后追加：

```cmake
zz_session_add_test(ZzSessionModelTest)
```

修改 `src/session/CMakeLists.txt`，把 `add_library` 块替换为：

```cmake
add_library(ZzSessionCore STATIC
    ZzSessionProfile.cpp
    ZzSessionProfile.h
    ZzSessionModel.cpp
    ZzSessionModel.h
)
```

- [ ] **步骤 3：运行测试验证失败**

运行：`cmake -S . -B build/debug -G Ninja -DCMAKE_BUILD_TYPE=Debug && cmake --build build/debug`
预期：编译失败，报错 `'ZzSessionModel' file not found`。

- [ ] **步骤 4：创建 `src/session/ZzSessionModel.h`**

```cpp
#pragma once

#include "ZzSessionProfile.h"

#include <QObject>
#include <QList>
#include <QString>
#include <QStringList>
#include <QUuid>
#include <optional>

/**
 * @brief 会话配置档案模型，负责会话的内存管理、增删改查与 JSON 持久化。
 *
 * 纯 Qt Core 后端类，不依赖 Widgets。持久化路径经构造函数注入；
 * 生产代码使用 defaultFilePath()（平台配置目录下的 sessions.json），
 * 测试注入临时目录路径。每次数据变更后发射 sessionsChanged() 信号。
 * 分组用路径字符串表示（如 "生产环境/Web 服务器"），重命名分组即改字符串前缀。
 */
class ZzSessionModel : public QObject
{
    Q_OBJECT

public:
    /**
     * @brief 构造会话模型。
     * @param filePath 持久化 JSON 文件路径。
     * @param parent Qt 父对象。
     */
    explicit ZzSessionModel(const QString &filePath, QObject *parent = nullptr);

    /**
     * @brief 默认持久化路径（Linux ~/.config/ZzClawTerm/sessions.json，
     *        Windows %APPDATA%/ZzClawTerm/sessions.json，
     *        macOS ~/Library/Application Support/ZzClawTerm/sessions.json）。
     * @return 平台配置目录下的 sessions.json 绝对路径。
     */
    static QString defaultFilePath();

    /**
     * @brief 从磁盘加载会话。文件不存在时视为空模型并返回 true（首次启动）。
     * @return 加载成功返回 true；文件存在但内容非法返回 false，错误原因见 errorString()。
     */
    bool load();

    /**
     * @brief 原子保存到磁盘（QSaveFile），父目录不存在时自动创建。
     * @return 保存成功返回 true；失败返回 false，错误原因见 errorString()。
     */
    bool save() const;

    /** @brief 返回全部会话（按添加顺序）。 */
    QList<ZzSessionProfile> allSessions() const;

    /**
     * @brief 按 id 查询会话。
     * @param id addSession 返回的会话 id。
     * @return 找到返回会话副本，否则返回 std::nullopt。
     */
    std::optional<ZzSessionProfile> session(const QUuid &id) const;

    /**
     * @brief 添加会话。id 为 null 时自动生成；id 冲突时拒绝。
     * @param profile 会话档案（id 字段可为 null）。
     * @return 成功返回该会话的 id；失败返回 null QUuid，错误原因见 errorString()。
     */
    QUuid addSession(ZzSessionProfile profile);

    /**
     * @brief 按 id 整体更新会话。
     * @param profile 新档案，id 字段必须指向已存在的会话。
     * @return 成功返回 true；id 不存在返回 false，错误原因见 errorString()。
     */
    bool updateSession(const ZzSessionProfile &profile);

    /**
     * @brief 按 id 删除会话。
     * @param id 会话 id。
     * @return 成功返回 true；id 不存在返回 false，错误原因见 errorString()。
     * @note 不级联删除 ZzCredentialStore 中对应的凭据（由上层按需处理）。
     */
    bool removeSession(const QUuid &id);

    /** @brief 最近一次失败的错误信息（简体中文）。 */
    QString errorString() const;

signals:
    /** @brief 会话数据发生任何增删改后发射。 */
    void sessionsChanged();

private:
    QList<ZzSessionProfile> m_sessions; ///< 内存中的全部会话
    QString m_filePath;                 ///< 持久化文件路径
    mutable QString m_errorString;      ///< 最近一次失败的错误信息
};
```

- [ ] **步骤 5：创建 `src/session/ZzSessionModel.cpp`**

```cpp
#include "ZzSessionModel.h"

#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QJsonArray>
#include <QJsonDocument>
#include <QSaveFile>
#include <QStandardPaths>

namespace {

const QString kVersionKey = QStringLiteral("version");
const QString kSessionsKey = QStringLiteral("sessions");
constexpr int kFormatVersion = 1;

} // namespace

ZzSessionModel::ZzSessionModel(const QString &filePath, QObject *parent)
    : QObject(parent)
    , m_filePath(filePath)
{
}

QString ZzSessionModel::defaultFilePath()
{
    const QString dir = QStandardPaths::writableLocation(QStandardPaths::AppConfigLocation);
    return dir + QStringLiteral("/sessions.json");
}

bool ZzSessionModel::load()
{
    QFile file(m_filePath);
    if (!file.exists()) {
        // 首次启动：文件不存在视为空模型
        m_sessions.clear();
        return true;
    }
    if (!file.open(QIODevice::ReadOnly)) {
        m_errorString = QStringLiteral("无法打开会话文件：%1").arg(file.errorString());
        return false;
    }
    const QJsonDocument doc = QJsonDocument::fromJson(file.readAll());
    if (!doc.isObject()) {
        m_errorString = QStringLiteral("会话文件格式非法（不是 JSON 对象）");
        return false;
    }
    const QJsonArray array = doc.object().value(kSessionsKey).toArray();
    QList<ZzSessionProfile> loaded;
    loaded.reserve(array.size());
    for (const QJsonValue &value : array) {
        if (value.isObject())
            loaded.append(ZzSessionProfile::fromJson(value.toObject()));
    }
    m_sessions = loaded;
    return true;
}

bool ZzSessionModel::save() const
{
    const QFileInfo info(m_filePath);
    if (!info.dir().exists() && !QDir().mkpath(info.absolutePath())) {
        m_errorString = QStringLiteral("无法创建会话目录：%1").arg(info.absolutePath());
        return false;
    }

    QJsonArray array;
    for (const ZzSessionProfile &profile : m_sessions)
        array.append(profile.toJson());

    QJsonObject root;
    root.insert(kVersionKey, kFormatVersion);
    root.insert(kSessionsKey, array);

    QSaveFile file(m_filePath);
    if (!file.open(QIODevice::WriteOnly)) {
        m_errorString = QStringLiteral("无法写入会话文件：%1").arg(file.errorString());
        return false;
    }
    file.write(QJsonDocument(root).toJson(QJsonDocument::Indented));
    if (!file.commit()) {
        m_errorString = QStringLiteral("会话文件落盘失败：%1").arg(file.errorString());
        return false;
    }
    return true;
}

QList<ZzSessionProfile> ZzSessionModel::allSessions() const
{
    return m_sessions;
}

std::optional<ZzSessionProfile> ZzSessionModel::session(const QUuid &id) const
{
    for (const ZzSessionProfile &profile : m_sessions) {
        if (profile.id == id)
            return profile;
    }
    return std::nullopt;
}

QUuid ZzSessionModel::addSession(ZzSessionProfile profile)
{
    if (profile.id.isNull())
        profile.id = QUuid::createUuid();
    if (session(profile.id).has_value()) {
        m_errorString = QStringLiteral("会话 id 已存在：%1").arg(profile.id.toString(QUuid::WithoutBraces));
        return QUuid();
    }
    m_sessions.append(profile);
    emit sessionsChanged();
    return profile.id;
}

bool ZzSessionModel::updateSession(const ZzSessionProfile &profile)
{
    for (qsizetype i = 0; i < m_sessions.size(); ++i) {
        if (m_sessions[i].id == profile.id) {
            m_sessions[i] = profile;
            emit sessionsChanged();
            return true;
        }
    }
    m_errorString = QStringLiteral("会话不存在：%1").arg(profile.id.toString(QUuid::WithoutBraces));
    return false;
}

bool ZzSessionModel::removeSession(const QUuid &id)
{
    for (qsizetype i = 0; i < m_sessions.size(); ++i) {
        if (m_sessions[i].id == id) {
            m_sessions.removeAt(i);
            emit sessionsChanged();
            return true;
        }
    }
    m_errorString = QStringLiteral("会话不存在：%1").arg(id.toString(QUuid::WithoutBraces));
    return false;
}

QString ZzSessionModel::errorString() const
{
    return m_errorString;
}
```

- [ ] **步骤 6：运行测试验证通过**

运行：`cmake --build build/debug && ctest --test-dir build/debug --output-on-failure`
预期：`100% tests passed, 0 tests failed out of 2`；`ZzSessionModelTest` 下 7 个用例全部 PASS。

- [ ] **步骤 7：Commit**

```bash
git add src/session/ tests/session/
git commit -m "feat: 新增 ZzSessionModel 会话增删改查与 JSON 原子持久化"
```

---

### 任务 3：`ZzSessionModel` 分组树操作

**文件：**
- 修改：`src/session/ZzSessionModel.h`（追加 4 个分组方法声明）
- 修改：`src/session/ZzSessionModel.cpp`（追加 4 个分组方法实现）
- 修改：`tests/session/ZzSessionModelTest.cpp`（追加分组测试用例）

- [ ] **步骤 1：编写失败的测试——在 `tests/session/ZzSessionModelTest.cpp` 的 `private slots:` 区块末尾（`saveCreatesParentDirectory` 声明之后）追加声明**

```cpp
    /** @brief allGroupPaths 返回去重并排序的全部分组路径（含嵌套路径全量，不含空分组）。 */
    void allGroupPaths();

    /** @brief sessionsInGroup 只返回直接位于该分组的会话，不含子分组。 */
    void sessionsInGroup();

    /** @brief 重命名分组时同路径及子路径前缀一并改写。 */
    void renameGroup();

    /** @brief 非法重命名（空路径、同名、重命名为自身子分组）被拒绝。 */
    void renameGroupInvalidRejected();

    /** @brief 删除分组时级联删除该分组及其子分组下的全部会话。 */
    void removeGroup();
```

并在文件末尾（`saveCreatesParentDirectory()` 实现之后、`QTEST_GUILESS_MAIN` 之前）追加实现：

```cpp
void ZzSessionModelTest::allGroupPaths()
{
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    ZzSessionModel model(dir.filePath(QStringLiteral("sessions.json")));

    model.addSession(makeProfile(QStringLiteral("Web-01"), QStringLiteral("生产环境/Web 服务器")));
    model.addSession(makeProfile(QStringLiteral("Web-02"), QStringLiteral("生产环境/Web 服务器")));
    model.addSession(makeProfile(QStringLiteral("DB-01"), QStringLiteral("生产环境/数据库")));
    model.addSession(makeProfile(QStringLiteral("Test-01"), QStringLiteral("测试环境")));
    model.addSession(makeProfile(QStringLiteral("本地"), QString()));

    const QStringList groups = model.allGroupPaths();
    QCOMPARE(groups.size(), 3);
    QVERIFY(groups.contains(QStringLiteral("生产环境/Web 服务器")));
    QVERIFY(groups.contains(QStringLiteral("生产环境/数据库")));
    QVERIFY(groups.contains(QStringLiteral("测试环境")));
}

void ZzSessionModelTest::sessionsInGroup()
{
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    ZzSessionModel model(dir.filePath(QStringLiteral("sessions.json")));

    model.addSession(makeProfile(QStringLiteral("Web-01"), QStringLiteral("生产环境")));
    model.addSession(makeProfile(QStringLiteral("Web-02"), QStringLiteral("生产环境/Web 服务器")));
    model.addSession(makeProfile(QStringLiteral("Web-03"), QStringLiteral("生产环境/Web 服务器")));
    model.addSession(makeProfile(QStringLiteral("本地"), QString()));

    QCOMPARE(model.sessionsInGroup(QStringLiteral("生产环境")).size(), 1);
    QCOMPARE(model.sessionsInGroup(QStringLiteral("生产环境/Web 服务器")).size(), 2);
    QVERIFY(model.sessionsInGroup(QStringLiteral("不存在")).isEmpty());
    QVERIFY(model.sessionsInGroup(QString()).isEmpty());
}

void ZzSessionModelTest::renameGroup()
{
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    ZzSessionModel model(dir.filePath(QStringLiteral("sessions.json")));

    const QUuid idTop = model.addSession(makeProfile(QStringLiteral("Prod"), QStringLiteral("生产")));
    const QUuid idSub = model.addSession(makeProfile(QStringLiteral("ProdWeb"), QStringLiteral("生产/Web")));
    const QUuid idOther = model.addSession(makeProfile(QStringLiteral("ProdMirror"), QStringLiteral("生产环境-镜像")));
    const QUuid idNone = model.addSession(makeProfile(QStringLiteral("Local"), QString()));

    // spy 在 addSession 之后创建，只统计 renameGroup 触发的信号
    QSignalSpy spy(&model, &ZzSessionModel::sessionsChanged);
    QVERIFY(model.renameGroup(QStringLiteral("生产"), QStringLiteral("生产环境")));
    QCOMPARE(spy.count(), 1);

    QCOMPARE(model.session(idTop)->groupPath, QStringLiteral("生产环境"));
    QCOMPARE(model.session(idSub)->groupPath, QStringLiteral("生产环境/Web"));
    // "生产环境-镜像" 不是 "生产" 的子路径（前缀边界必须是 '/'），不得被误改
    QCOMPARE(model.session(idOther)->groupPath, QStringLiteral("生产环境-镜像"));
    QVERIFY(model.session(idNone)->groupPath.isEmpty());
}

void ZzSessionModelTest::renameGroupInvalidRejected()
{
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    ZzSessionModel model(dir.filePath(QStringLiteral("sessions.json")));
    model.addSession(makeProfile(QStringLiteral("Web-01"), QStringLiteral("生产/Web")));

    QVERIFY(!model.renameGroup(QString(), QStringLiteral("新分组")));
    QVERIFY(!model.renameGroup(QStringLiteral("生产"), QString()));
    QVERIFY(!model.renameGroup(QStringLiteral("生产"), QStringLiteral("生产")));
    QVERIFY(!model.renameGroup(QStringLiteral("生产"), QStringLiteral("生产/Web/子分组")));

    // 无匹配分组时视为幂等成功
    QVERIFY(model.renameGroup(QStringLiteral("不存在"), QStringLiteral("任意")));
}

void ZzSessionModelTest::removeGroup()
{
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    ZzSessionModel model(dir.filePath(QStringLiteral("sessions.json")));

    model.addSession(makeProfile(QStringLiteral("Prod"), QStringLiteral("生产")));
    model.addSession(makeProfile(QStringLiteral("ProdWeb"), QStringLiteral("生产/Web")));
    const QUuid idOther = model.addSession(makeProfile(QStringLiteral("Test"), QStringLiteral("测试")));

    QVERIFY(model.removeGroup(QStringLiteral("生产")));
    QCOMPARE(model.allSessions().size(), 1);
    QVERIFY(model.allSessions().first().id == idOther);

    // 空路径拒绝；无匹配返回 false
    QVERIFY(!model.removeGroup(QString()));
    QVERIFY(!model.removeGroup(QStringLiteral("不存在")));
}
```

- [ ] **步骤 2：运行测试验证失败**

运行：`cmake --build build/debug && ctest --test-dir build/debug -R ZzSessionModelTest --output-on-failure`
预期：链接失败，报错 `undefined reference to 'ZzSessionModel::allGroupPaths() const'`（等方法未定义）。

- [ ] **步骤 3：修改 `src/session/ZzSessionModel.h`，在 `removeSession` 声明之后、`errorString` 声明之前追加**

```cpp
    /**
     * @brief 返回全部非空分组路径（去重、字典序排序）。
     * @return 分组路径列表，元素为完整路径字符串（如 "生产环境/Web 服务器"）。
     * @note 只返回实际被会话使用的路径，不从嵌套路径推导父分组；
     *       UI 构建分组树时自行按 '/' 拆分归并。
     */
    QStringList allGroupPaths() const;

    /**
     * @brief 返回直接位于指定分组的会话（不含子分组中的会话）。
     * @param groupPath 分组路径；空串查询未分组会话。
     * @return 会话列表（按添加顺序）。
     */
    QList<ZzSessionProfile> sessionsInGroup(const QString &groupPath) const;

    /**
     * @brief 重命名分组：groupPath 等于 oldPath 或以 "oldPath/" 为前缀的会话，前缀改写为 newPath。
     * @param oldPath 原分组路径，不能为空。
     * @param newPath 新分组路径，不能为空、不能与 oldPath 相同、不能是 oldPath 的子路径。
     * @return 参数非法返回 false；无匹配会话视为幂等成功返回 true。
     */
    bool renameGroup(const QString &oldPath, const QString &newPath);

    /**
     * @brief 删除分组：级联删除 groupPath 等于该路径或以 "groupPath/" 为前缀的全部会话。
     * @param groupPath 分组路径，不能为空。
     * @return 删除了至少一个会话返回 true；路径为空或无匹配返回 false。
     */
    bool removeGroup(const QString &groupPath);
```

- [ ] **步骤 4：修改 `src/session/ZzSessionModel.cpp`，在 `removeSession` 实现之后、`errorString` 实现之前追加**

```cpp
QStringList ZzSessionModel::allGroupPaths() const
{
    QStringList paths;
    for (const ZzSessionProfile &profile : m_sessions) {
        if (!profile.groupPath.isEmpty() && !paths.contains(profile.groupPath))
            paths.append(profile.groupPath);
    }
    paths.sort();
    return paths;
}

QList<ZzSessionProfile> ZzSessionModel::sessionsInGroup(const QString &groupPath) const
{
    QList<ZzSessionProfile> result;
    for (const ZzSessionProfile &profile : m_sessions) {
        if (profile.groupPath == groupPath)
            result.append(profile);
    }
    return result;
}

bool ZzSessionModel::renameGroup(const QString &oldPath, const QString &newPath)
{
    if (oldPath.isEmpty() || newPath.isEmpty() || oldPath == newPath) {
        m_errorString = QStringLiteral("分组重命名参数非法（空路径或与原路径相同）");
        return false;
    }
    if (newPath.startsWith(oldPath + QLatin1Char('/'))) {
        m_errorString = QStringLiteral("不能将分组重命名为自身的子分组");
        return false;
    }

    bool changed = false;
    const QString oldPrefix = oldPath + QLatin1Char('/');
    for (ZzSessionProfile &profile : m_sessions) {
        if (profile.groupPath == oldPath) {
            profile.groupPath = newPath;
            changed = true;
        } else if (profile.groupPath.startsWith(oldPrefix)) {
            profile.groupPath = newPath + profile.groupPath.mid(oldPath.size());
            changed = true;
        }
    }
    if (changed)
        emit sessionsChanged();
    return true; // 无匹配分组视为幂等成功
}

bool ZzSessionModel::removeGroup(const QString &groupPath)
{
    if (groupPath.isEmpty()) {
        m_errorString = QStringLiteral("不能删除空分组路径");
        return false;
    }

    const QString prefix = groupPath + QLatin1Char('/');
    const qsizetype before = m_sessions.size();
    m_sessions.removeIf([&](const ZzSessionProfile &profile) {
        return profile.groupPath == groupPath || profile.groupPath.startsWith(prefix);
    });
    if (m_sessions.size() == before) {
        m_errorString = QStringLiteral("分组不存在或为空：%1").arg(groupPath);
        return false;
    }
    emit sessionsChanged();
    return true;
}
```

- [ ] **步骤 5：运行测试验证通过**

运行：`cmake --build build/debug && ctest --test-dir build/debug --output-on-failure`
预期：`100% tests passed, 0 tests failed out of 2`；`ZzSessionModelTest` 下 12 个用例全部 PASS，含 `PASS   : ZzSessionModelTest::renameGroup()`。

- [ ] **步骤 6：Commit**

```bash
git add src/session/ZzSessionModel.h src/session/ZzSessionModel.cpp tests/session/ZzSessionModelTest.cpp
git commit -m "feat: ZzSessionModel 支持分组树查询、前缀重命名与级联删除"
```

---

### 任务 4：`ZzCredentialStore` 主密码初始化与解锁（OpenSSL EVP AES-256-GCM）

**文件：**
- 创建：`src/session/ZzCredentialStore.h`
- 创建：`src/session/ZzCredentialStore.cpp`
- 创建：`tests/session/ZzCredentialStoreTest.cpp`
- 修改：`src/session/CMakeLists.txt`（库源文件追加）
- 修改：`tests/session/CMakeLists.txt`（注册新测试）

凭据文件二进制格式（本任务实现，后续任务复用）：

```text
"ZZCT"(4B) || version(u32 大端, 当前为 1) || kdfIterations(u32 大端) || salt(16B)
|| iv(12B) || AES-256-GCM 密文 || GCM tag(16B)
```

密文解密后为 JSON：`{"verifier":"zzclawterm-v1","credentials":[{"id","name","secret"},...]}`（`id` 为 QUuid WithoutBraces 字符串）。密钥由 `PBKDF2-HMAC-SHA256(主密码, salt, 600000 次迭代)` 派生 32 字节。GCM tag 校验失败即判定主密码错误或文件损坏。

- [ ] **步骤 1：编写失败的测试 `tests/session/ZzCredentialStoreTest.cpp`**

```cpp
#include <QtTest>
#include <QFile>
#include <QFileInfo>
#include <QTemporaryDir>

#include "ZzCredentialStore.h"

/**
 * @brief ZzCredentialStore 主密码生命周期单元测试。
 */
class ZzCredentialStoreTest : public QObject
{
    Q_OBJECT

private slots:
    /** @brief 首次初始化生成凭据文件并直接处于解锁状态。 */
    void initializeCreatesFileAndUnlocks()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        const QString path = dir.filePath(QStringLiteral("credentials.dat"));

        ZzCredentialStore store(path);
        QVERIFY(!store.hasMasterPassword());
        QVERIFY(!store.isUnlocked());

        QVERIFY(store.initialize(QStringLiteral("主密码-abc123")));
        QVERIFY(store.hasMasterPassword());
        QVERIFY(store.isUnlocked());
        QVERIFY(QFileInfo::exists(path));
        QVERIFY(QFileInfo(path).size() > 0);
    }

    /** @brief 已存在凭据文件时拒绝重复初始化。 */
    void initializeTwiceRejected()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        ZzCredentialStore store(dir.filePath(QStringLiteral("credentials.dat")));

        QVERIFY(store.initialize(QStringLiteral("主密码-abc123")));
        QVERIFY(!store.initialize(QStringLiteral("另一个密码")));
        QVERIFY(!store.errorString().isEmpty());
    }

    /** @brief 空主密码被拒绝。 */
    void initializeEmptyPasswordRejected()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        ZzCredentialStore store(dir.filePath(QStringLiteral("credentials.dat")));

        QVERIFY(!store.initialize(QString()));
        QVERIFY(!store.hasMasterPassword());
    }

    /** @brief 正确主密码解锁成功，密钥驻留内存直到 lock()。 */
    void unlockWithCorrectPassword()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        const QString path = dir.filePath(QStringLiteral("credentials.dat"));

        {
            ZzCredentialStore store(path);
            QVERIFY(store.initialize(QStringLiteral("正确密码")));
        } // 析构即锁定

        ZzCredentialStore store(path);
        QVERIFY(!store.isUnlocked());
        QVERIFY(store.unlock(QStringLiteral("正确密码")));
        QVERIFY(store.isUnlocked());

        store.lock();
        QVERIFY(!store.isUnlocked());
    }

    /** @brief 错误主密码解锁失败（GCM tag 校验不通过），且不驻留任何密钥。 */
    void unlockWithWrongPasswordFails()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        const QString path = dir.filePath(QStringLiteral("credentials.dat"));

        {
            ZzCredentialStore store(path);
            QVERIFY(store.initialize(QStringLiteral("正确密码")));
        }

        ZzCredentialStore store(path);
        QVERIFY(!store.unlock(QStringLiteral("错误密码")));
        QVERIFY(!store.isUnlocked());
        QVERIFY(!store.errorString().isEmpty());
    }

    /** @brief 文件内容被篡改后解锁失败（GCM 认证失败或格式非法）。 */
    void unlockWithCorruptedFileFails()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        const QString path = dir.filePath(QStringLiteral("credentials.dat"));

        {
            ZzCredentialStore store(path);
            QVERIFY(store.initialize(QStringLiteral("正确密码")));
        }

        // 篡改文件最后一个字节（位于 GCM tag 区）
        QFile file(path);
        QVERIFY(file.open(QIODevice::ReadWrite));
        QVERIFY(file.seek(file.size() - 1));
        char byte = 0;
        QVERIFY(file.getChar(&byte));
        QVERIFY(file.seek(file.size() - 1));
        const char flipped = byte ^ 0xFF;
        QVERIFY(file.putChar(flipped));
        file.close();

        ZzCredentialStore store(path);
        QVERIFY(!store.unlock(QStringLiteral("正确密码")));
        QVERIFY(!store.isUnlocked());
    }

    /** @brief 已解锁状态下重复 unlock 被拒绝。 */
    void unlockTwiceRejected()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        ZzCredentialStore store(dir.filePath(QStringLiteral("credentials.dat")));

        QVERIFY(store.initialize(QStringLiteral("主密码-abc123")));
        QVERIFY(!store.unlock(QStringLiteral("主密码-abc123")));
        QVERIFY(!store.errorString().isEmpty());
    }
};

QTEST_GUILESS_MAIN(ZzCredentialStoreTest)

#include "ZzCredentialStoreTest.moc"
```

- [ ] **步骤 2：注册测试与库源文件**

修改 `tests/session/CMakeLists.txt`，在 `zz_session_add_test(ZzSessionModelTest)` 行后追加：

```cmake
zz_session_add_test(ZzCredentialStoreTest)
```

修改 `src/session/CMakeLists.txt`，把 `add_library` 块替换为：

```cmake
add_library(ZzSessionCore STATIC
    ZzSessionProfile.cpp
    ZzSessionProfile.h
    ZzSessionModel.cpp
    ZzSessionModel.h
    ZzCredentialStore.cpp
    ZzCredentialStore.h
)
```

- [ ] **步骤 3：运行测试验证失败**

运行：`cmake -S . -B build/debug -G Ninja -DCMAKE_BUILD_TYPE=Debug && cmake --build build/debug`
预期：编译失败，报错 `'ZzCredentialStore' file not found`。

- [ ] **步骤 4：创建 `src/session/ZzCredentialStore.h`**

```cpp
#pragma once

#include <QObject>
#include <QByteArray>
#include <QList>
#include <QString>
#include <QUuid>
#include <optional>

/**
 * @brief 凭据存储：AES-256-GCM 加密 + 主密码，敏感信息的唯一出入口。
 *
 * 凭据保存在 credentials.dat（平台配置目录），文件整体加密：
 * PBKDF2-HMAC-SHA256 从主密码派生 32 字节密钥（60 万次迭代），
 * OpenSSL EVP 接口执行 AES-256-GCM 加解密。
 * 首次启动 initialize() 设主密码；unlock() 一次后密钥驻留内存；
 * lock()（或析构）用 OPENSSL_cleanse 清零密钥。锁定状态下一切凭据操作被拒绝。
 * GCM tag 提供完整性认证：主密码错误或文件被篡改都会导致 unlock 失败。
 *
 * v0.2 可将系统密钥环实现为同一抽象的另一后端，应用层无感。
 */
class ZzCredentialStore : public QObject
{
    Q_OBJECT

public:
    /**
     * @brief 构造凭据存储。
     * @param filePath 凭据文件路径（测试注入临时路径）。
     * @param parent Qt 父对象。
     */
    explicit ZzCredentialStore(const QString &filePath, QObject *parent = nullptr);

    /** @brief 析构时锁定（清零内存中的密钥与明文凭据）。 */
    ~ZzCredentialStore() override;

    /**
     * @brief 默认凭据文件路径（平台配置目录下的 credentials.dat）。
     * @return 绝对路径。
     */
    static QString defaultFilePath();

    /** @brief 是否已设置过主密码（凭据文件存在）。 */
    bool hasMasterPassword() const;

    /**
     * @brief 首次初始化：设置主密码并创建凭据文件，成功后直接处于解锁状态。
     * @param masterPassword 主密码，不能为空。
     * @return 成功返回 true；文件已存在或密码为空返回 false，错误原因见 errorString()。
     */
    bool initialize(const QString &masterPassword);

    /**
     * @brief 用主密码解锁：派生密钥、解密并校验凭据文件，成功后密钥驻留内存。
     * @param masterPassword 主密码。
     * @return 成功返回 true；主密码错误、文件损坏或已处于解锁状态返回 false，
     *         错误原因见 errorString()。
     */
    bool unlock(const QString &masterPassword);

    /** @brief 锁定：清零内存中的密钥与全部明文凭据。幂等。 */
    void lock();

    /** @brief 当前是否处于解锁状态。 */
    bool isUnlocked() const;

    /**
     * @brief 新增凭据。
     * @param name 凭据显示名（如 "root@web-01"）。
     * @param secret 明文密码（仅内存中短暂存在，落盘前整体加密）。
     * @return 成功返回凭据 id（QUuid，供 ZzSessionProfile::credentialId 引用）；
     *         失败返回 null QUuid，错误原因见 errorString()。锁定状态下必然失败。
     */
    QUuid addCredential(const QString &name, const QString &secret);

    /**
     * @brief 更新凭据明文。
     * @param credentialId addCredential 返回的 id。
     * @param secret 新明文密码。
     * @return 成功返回 true；id 不存在或未解锁返回 false，错误原因见 errorString()。
     */
    bool updateCredential(const QUuid &credentialId, const QString &secret);

    /**
     * @brief 读取凭据明文。
     * @param credentialId addCredential 返回的 id。
     * @return 找到且已解锁返回明文，否则返回 std::nullopt。
     */
    std::optional<QString> credential(const QUuid &credentialId) const;

    /**
     * @brief 删除凭据。
     * @param credentialId addCredential 返回的 id。
     * @return 成功返回 true；id 不存在或未解锁返回 false，错误原因见 errorString()。
     */
    bool removeCredential(const QUuid &credentialId);

    /** @brief 最近一次失败的错误信息（简体中文）。 */
    QString errorString() const;

private:
    /**
     * @brief 内存中的凭据条目（明文，仅在解锁期间存在）。
     */
    struct Credential {
        QUuid id;       ///< 凭据 id
        QString name;   ///< 显示名
        QString secret; ///< 明文密码
    };

    /**
     * @brief 用内存中的密钥把全部凭据加密并原子落盘。
     * @return 成功返回 true；失败返回 false，错误原因见 errorString()。
     */
    bool persist() const;

    QString m_filePath;                 ///< 凭据文件路径
    QByteArray m_key;                   ///< 派生密钥，仅解锁期间驻留内存
    QByteArray m_salt;                  ///< PBKDF2 盐（随文件头读写）
    quint32 m_kdfIterations = 600000;   ///< PBKDF2 迭代次数
    QList<Credential> m_credentials;    ///< 解锁期间的明文凭据
    mutable QString m_errorString;      ///< 最近一次失败的错误信息
};
```

- [ ] **步骤 5：创建 `src/session/ZzCredentialStore.cpp`（完整文件，含本任务与任务 5 的全部实现）**

```cpp
#include "ZzCredentialStore.h"

#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QSaveFile>
#include <QStandardPaths>

#include <openssl/crypto.h>
#include <openssl/evp.h>
#include <openssl/rand.h>

namespace {

constexpr quint32 kFormatVersion = 1;
constexpr int kKeyLength = 32;   // AES-256
constexpr int kSaltLength = 16;
constexpr int kIvLength = 12;    // GCM 推荐 IV 长度
constexpr int kTagLength = 16;
const QByteArray kMagic = QByteArrayLiteral("ZZCT");
const QString kVerifier = QStringLiteral("zzclawterm-v1");
const QString kVerifierKey = QStringLiteral("verifier");
const QString kCredentialsKey = QStringLiteral("credentials");
const QString kIdKey = QStringLiteral("id");
const QString kNameKey = QStringLiteral("name");
const QString kSecretKey = QStringLiteral("secret");

/**
 * @brief PBKDF2-HMAC-SHA256 从主密码派生 32 字节密钥。
 * @param password 主密码。
 * @param salt 随机盐。
 * @param iterations 迭代次数。
 * @param keyOut 输出 32 字节密钥。
 * @return 成功返回 true。
 */
bool deriveKey(const QString &password, const QByteArray &salt, quint32 iterations, QByteArray &keyOut)
{
    keyOut = QByteArray(kKeyLength, Qt::Uninitialized);
    const QByteArray passwordUtf8 = password.toUtf8();
    return PKCS5_PBKDF2_HMAC(passwordUtf8.constData(),
                             passwordUtf8.size(),
                             reinterpret_cast<const unsigned char *>(salt.constData()),
                             salt.size(),
                             static_cast<int>(iterations),
                             EVP_sha256(),
                             kKeyLength,
                             reinterpret_cast<unsigned char *>(keyOut.data())) == 1;
}

/**
 * @brief AES-256-GCM 加密。输出格式：iv(12B) || 密文 || tag(16B)。
 * @param key 32 字节密钥。
 * @param plain 明文。
 * @param blobOut 输出加密块。
 * @return 成功返回 true。
 */
bool aesGcmEncrypt(const QByteArray &key, const QByteArray &plain, QByteArray &blobOut)
{
    QByteArray iv(kIvLength, Qt::Uninitialized);
    if (RAND_bytes(reinterpret_cast<unsigned char *>(iv.data()), kIvLength) != 1)
        return false;

    EVP_CIPHER_CTX *ctx = EVP_CIPHER_CTX_new();
    if (!ctx)
        return false;

    bool ok = false;
    QByteArray cipher(plain.size(), Qt::Uninitialized);
    QByteArray tag(kTagLength, Qt::Uninitialized);
    int len = 0;
    int total = 0;
    do {
        if (EVP_EncryptInit_ex(ctx, EVP_aes_256_gcm(), nullptr, nullptr, nullptr) != 1)
            break;
        if (EVP_CIPHER_CTX_ctrl(ctx, EVP_CTRL_GCM_SET_IVLEN, kIvLength, nullptr) != 1)
            break;
        if (EVP_EncryptInit_ex(ctx, nullptr, nullptr,
                               reinterpret_cast<const unsigned char *>(key.constData()),
                               reinterpret_cast<const unsigned char *>(iv.constData())) != 1)
            break;
        if (!plain.isEmpty()) {
            if (EVP_EncryptUpdate(ctx,
                                  reinterpret_cast<unsigned char *>(cipher.data()), &len,
                                  reinterpret_cast<const unsigned char *>(plain.constData()),
                                  plain.size()) != 1)
                break;
            total = len;
        }
        if (EVP_EncryptFinal_ex(ctx, reinterpret_cast<unsigned char *>(cipher.data()) + total, &len) != 1)
            break;
        total += len;
        if (EVP_CIPHER_CTX_ctrl(ctx, EVP_CTRL_GCM_GET_TAG, kTagLength, tag.data()) != 1)
            break;
        cipher.truncate(total);
        blobOut = iv + cipher + tag;
        ok = true;
    } while (false);

    EVP_CIPHER_CTX_free(ctx);
    return ok;
}

/**
 * @brief AES-256-GCM 解密（输入格式同 aesGcmEncrypt 输出）。
 * @param key 32 字节密钥。
 * @param blob 加密块。
 * @param plainOut 输出明文。
 * @return 成功返回 true；GCM tag 校验失败（主密码错误或数据损坏）返回 false。
 */
bool aesGcmDecrypt(const QByteArray &key, const QByteArray &blob, QByteArray &plainOut)
{
    if (blob.size() < kIvLength + kTagLength)
        return false;

    const QByteArray iv = blob.left(kIvLength);
    const QByteArray tag = blob.right(kTagLength);
    const QByteArray cipher = blob.mid(kIvLength, blob.size() - kIvLength - kTagLength);

    EVP_CIPHER_CTX *ctx = EVP_CIPHER_CTX_new();
    if (!ctx)
        return false;

    bool ok = false;
    QByteArray plain(cipher.size(), Qt::Uninitialized);
    int len = 0;
    int total = 0;
    do {
        if (EVP_DecryptInit_ex(ctx, EVP_aes_256_gcm(), nullptr, nullptr, nullptr) != 1)
            break;
        if (EVP_CIPHER_CTX_ctrl(ctx, EVP_CTRL_GCM_SET_IVLEN, kIvLength, nullptr) != 1)
            break;
        if (EVP_DecryptInit_ex(ctx, nullptr, nullptr,
                               reinterpret_cast<const unsigned char *>(key.constData()),
                               reinterpret_cast<const unsigned char *>(iv.constData())) != 1)
            break;
        if (!cipher.isEmpty()) {
            if (EVP_DecryptUpdate(ctx,
                                  reinterpret_cast<unsigned char *>(plain.data()), &len,
                                  reinterpret_cast<const unsigned char *>(cipher.constData()),
                                  cipher.size()) != 1)
                break;
            total = len;
        }
        if (EVP_CIPHER_CTX_ctrl(ctx, EVP_CTRL_GCM_SET_TAG, kTagLength,
                                const_cast<char *>(tag.constData())) != 1)
            break;
        // GCM tag 校验发生在 Final：主密码错误或数据被篡改时这里返回 0
        if (EVP_DecryptFinal_ex(ctx, reinterpret_cast<unsigned char *>(plain.data()) + total, &len) != 1)
            break;
        total += len;
        plain.truncate(total);
        plainOut = plain;
        ok = true;
    } while (false);

    EVP_CIPHER_CTX_free(ctx);
    return ok;
}

/**
 * @brief 向缓冲区追加一个大端 u32。
 */
void appendU32(QByteArray &out, quint32 value)
{
    out.append(static_cast<char>(value >> 24));
    out.append(static_cast<char>(value >> 16));
    out.append(static_cast<char>(value >> 8));
    out.append(static_cast<char>(value));
}

/**
 * @brief 从缓冲区指定偏移读取一个大端 u32。
 */
quint32 readU32(const QByteArray &data, qsizetype offset)
{
    const auto *p = reinterpret_cast<const unsigned char *>(data.constData()) + offset;
    return (quint32(p[0]) << 24) | (quint32(p[1]) << 16) | (quint32(p[2]) << 8) | quint32(p[3]);
}

} // namespace

ZzCredentialStore::ZzCredentialStore(const QString &filePath, QObject *parent)
    : QObject(parent)
    , m_filePath(filePath)
{
}

ZzCredentialStore::~ZzCredentialStore()
{
    lock();
}

QString ZzCredentialStore::defaultFilePath()
{
    const QString dir = QStandardPaths::writableLocation(QStandardPaths::AppConfigLocation);
    return dir + QStringLiteral("/credentials.dat");
}

bool ZzCredentialStore::hasMasterPassword() const
{
    return QFileInfo::exists(m_filePath);
}

bool ZzCredentialStore::initialize(const QString &masterPassword)
{
    if (hasMasterPassword()) {
        m_errorString = QStringLiteral("主密码已存在，不能重复初始化");
        return false;
    }
    if (masterPassword.isEmpty()) {
        m_errorString = QStringLiteral("主密码不能为空");
        return false;
    }

    m_salt = QByteArray(kSaltLength, Qt::Uninitialized);
    if (RAND_bytes(reinterpret_cast<unsigned char *>(m_salt.data()), kSaltLength) != 1) {
        m_errorString = QStringLiteral("生成随机盐失败");
        return false;
    }
    if (!deriveKey(masterPassword, m_salt, m_kdfIterations, m_key)) {
        m_errorString = QStringLiteral("密钥派生失败");
        return false;
    }

    m_credentials.clear();
    if (!persist()) {
        lock(); // 落盘失败不能留下驻留密钥
        return false;
    }
    return true;
}

bool ZzCredentialStore::unlock(const QString &masterPassword)
{
    if (isUnlocked()) {
        m_errorString = QStringLiteral("凭据存储已处于解锁状态");
        return false;
    }

    QFile file(m_filePath);
    if (!file.open(QIODevice::ReadOnly)) {
        m_errorString = QStringLiteral("无法打开凭据文件：%1").arg(file.errorString());
        return false;
    }
    const QByteArray raw = file.readAll();

    // 文件头：magic(4B) || version(4B) || kdfIterations(4B) || salt(16B)
    constexpr qsizetype kHeaderLength = 4 + 4 + 4 + kSaltLength;
    if (raw.size() < kHeaderLength || !raw.startsWith(kMagic)) {
        m_errorString = QStringLiteral("凭据文件格式非法");
        return false;
    }
    const quint32 version = readU32(raw, 4);
    if (version != kFormatVersion) {
        m_errorString = QStringLiteral("不支持的凭据文件版本：%1").arg(version);
        return false;
    }
    const quint32 iterations = readU32(raw, 8);
    const QByteArray salt = raw.mid(12, kSaltLength);
    const QByteArray blob = raw.mid(kHeaderLength);

    QByteArray key;
    if (!deriveKey(masterPassword, salt, iterations, key)) {
        m_errorString = QStringLiteral("密钥派生失败");
        return false;
    }

    QByteArray plain;
    if (!aesGcmDecrypt(key, blob, plain)) {
        OPENSSL_cleanse(key.data(), key.size());
        m_errorString = QStringLiteral("主密码错误或凭据文件已损坏");
        return false;
    }

    const QJsonObject root = QJsonDocument::fromJson(plain).object();
    if (root.value(kVerifierKey).toString() != kVerifier) {
        OPENSSL_cleanse(key.data(), key.size());
        m_errorString = QStringLiteral("凭据文件校验失败");
        return false;
    }

    QList<Credential> loaded;
    const QJsonArray array = root.value(kCredentialsKey).toArray();
    loaded.reserve(array.size());
    for (const QJsonValue &value : array) {
        const QJsonObject obj = value.toObject();
        loaded.append(Credential{QUuid::fromString(obj.value(kIdKey).toString()),
                                 obj.value(kNameKey).toString(),
                                 obj.value(kSecretKey).toString()});
    }

    m_key = key;
    m_salt = salt;
    m_kdfIterations = iterations;
    m_credentials = loaded;
    return true;
}

void ZzCredentialStore::lock()
{
    if (!m_key.isEmpty()) {
        OPENSSL_cleanse(m_key.data(), m_key.size());
        m_key.clear();
    }
    m_credentials.clear();
}

bool ZzCredentialStore::isUnlocked() const
{
    return !m_key.isEmpty();
}

bool ZzCredentialStore::persist() const
{
    QJsonArray array;
    for (const Credential &cred : m_credentials) {
        QJsonObject obj;
        obj.insert(kIdKey, cred.id.toString(QUuid::WithoutBraces));
        obj.insert(kNameKey, cred.name);
        obj.insert(kSecretKey, cred.secret);
        array.append(obj);
    }
    QJsonObject root;
    root.insert(kVerifierKey, kVerifier);
    root.insert(kCredentialsKey, array);
    const QByteArray plain = QJsonDocument(root).toJson(QJsonDocument::Compact);

    QByteArray blob;
    if (!aesGcmEncrypt(m_key, plain, blob)) {
        m_errorString = QStringLiteral("凭据加密失败");
        return false;
    }

    QByteArray out;
    out.reserve(4 + 4 + 4 + kSaltLength + blob.size());
    out.append(kMagic);
    appendU32(out, kFormatVersion);
    appendU32(out, m_kdfIterations);
    out.append(m_salt);
    out.append(blob);

    const QFileInfo info(m_filePath);
    if (!info.dir().exists() && !QDir().mkpath(info.absolutePath())) {
        m_errorString = QStringLiteral("无法创建凭据目录：%1").arg(info.absolutePath());
        return false;
    }
    QSaveFile file(m_filePath);
    if (!file.open(QIODevice::WriteOnly)) {
        m_errorString = QStringLiteral("无法写入凭据文件：%1").arg(file.errorString());
        return false;
    }
    file.write(out);
    if (!file.commit()) {
        m_errorString = QStringLiteral("凭据文件落盘失败：%1").arg(file.errorString());
        return false;
    }
    return true;
}

QUuid ZzCredentialStore::addCredential(const QString &name, const QString &secret)
{
    if (!isUnlocked()) {
        m_errorString = QStringLiteral("凭据存储未解锁");
        return QUuid();
    }

    const Credential cred{QUuid::createUuid(), name, secret};
    m_credentials.append(cred);
    if (!persist()) {
        m_credentials.removeLast(); // 落盘失败回滚内存状态
        return QUuid();
    }
    return cred.id;
}

bool ZzCredentialStore::updateCredential(const QUuid &credentialId, const QString &secret)
{
    if (!isUnlocked()) {
        m_errorString = QStringLiteral("凭据存储未解锁");
        return false;
    }

    for (Credential &cred : m_credentials) {
        if (cred.id == credentialId) {
            const QString oldSecret = cred.secret;
            cred.secret = secret;
            if (!persist()) {
                cred.secret = oldSecret; // 落盘失败回滚内存状态
                return false;
            }
            return true;
        }
    }
    m_errorString = QStringLiteral("凭据不存在：%1").arg(credentialId.toString(QUuid::WithoutBraces));
    return false;
}

std::optional<QString> ZzCredentialStore::credential(const QUuid &credentialId) const
{
    if (!isUnlocked())
        return std::nullopt;
    for (const Credential &cred : m_credentials) {
        if (cred.id == credentialId)
            return cred.secret;
    }
    return std::nullopt;
}

bool ZzCredentialStore::removeCredential(const QUuid &credentialId)
{
    if (!isUnlocked()) {
        m_errorString = QStringLiteral("凭据存储未解锁");
        return false;
    }

    for (qsizetype i = 0; i < m_credentials.size(); ++i) {
        if (m_credentials[i].id == credentialId) {
            const Credential backup = m_credentials[i];
            m_credentials.removeAt(i);
            if (!persist()) {
                m_credentials.insert(i, backup); // 落盘失败回滚内存状态
                return false;
            }
            return true;
        }
    }
    m_errorString = QStringLiteral("凭据不存在：%1").arg(credentialId.toString(QUuid::WithoutBraces));
    return false;
}

QString ZzCredentialStore::errorString() const
{
    return m_errorString;
}
```

- [ ] **步骤 6：运行测试验证通过**

运行：`cmake --build build/debug && ctest --test-dir build/debug --output-on-failure`
预期：`100% tests passed, 0 tests failed out of 3`；`ZzCredentialStoreTest` 下 7 个用例全部 PASS。

注意：`ZzCredentialStore` 的凭据增删改查方法已一并实现（避免任务 5 全量重写本文件），任务 5 用测试驱动验证这些方法的正确性，如发现缺陷再修复。

- [ ] **步骤 7：Commit**

```bash
git add src/session/ tests/session/
git commit -m "feat: 新增 ZzCredentialStore 主密码初始化/解锁与 AES-256-GCM 加密落盘"
```

---

### 任务 5：凭据增删改查与加解密往返验证

**文件：**
- 修改：`tests/session/ZzCredentialStoreTest.cpp`（追加 4 个测试用例）
- 修改：`src/session/ZzCredentialStore.cpp`（仅在测试暴露缺陷时修复，预期无需改动）

说明：CRUD 实现已随任务 4 落盘（理由见任务 4 步骤 6），本任务以测试驱动验证其正确性；任何 FAIL 都回到实现修复。

- [ ] **步骤 1：编写测试——在 `tests/session/ZzCredentialStoreTest.cpp` 的 `private slots:` 区块末尾（`unlockTwiceRejected` 声明之后）追加声明**

```cpp
    /** @brief 加解密往返：写入凭据后锁定、重建实例、解锁，读回的明文必须一致（含中文与特殊字符）。 */
    void credentialRoundTrip();

    /** @brief 更新凭据后重新解锁读到新明文。 */
    void updateCredentialRoundTrip();

    /** @brief 删除凭据后重新解锁不再可读；删除不存在的 id 返回 false。 */
    void removeCredentialRoundTrip();

    /** @brief 锁定（或未解锁）状态下一切凭据操作被拒绝。 */
    void lockedStoreRejectsOperations();
```

并在文件末尾（`unlockTwiceRejected()` 实现之后、`QTEST_GUILESS_MAIN` 之前）追加实现：

```cpp
void ZzCredentialStoreTest::credentialRoundTrip()
{
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    const QString path = dir.filePath(QStringLiteral("credentials.dat"));

    QUuid idAlpha;
    QUuid idBeta;
    QUuid idGamma;
    {
        ZzCredentialStore store(path);
        QVERIFY(store.initialize(QStringLiteral("主密码-xyz")));
        idAlpha = store.addCredential(QStringLiteral("root@web-01"), QStringLiteral("s3cret!@#"));
        idBeta = store.addCredential(QStringLiteral("deploy@db-01"), QStringLiteral("密码含中文与换行\n第二行"));
        idGamma = store.addCredential(QStringLiteral("空密码"), QString());
        QVERIFY(!idAlpha.isNull());
        QVERIFY(!idBeta.isNull());
        QVERIFY(!idGamma.isNull());
        // 同一实例内立即可读
        QCOMPARE(store.credential(idAlpha).value(), QStringLiteral("s3cret!@#"));
    } // 析构锁定，明文与密钥清零

    ZzCredentialStore store(path);
    QVERIFY(!store.credential(idAlpha).has_value()); // 未解锁不可读
    QVERIFY(store.unlock(QStringLiteral("主密码-xyz")));
    QCOMPARE(store.credential(idAlpha).value(), QStringLiteral("s3cret!@#"));
    QCOMPARE(store.credential(idBeta).value(), QStringLiteral("密码含中文与换行\n第二行"));
    QCOMPARE(store.credential(idGamma).value(), QString());
    QVERIFY(!store.credential(QUuid::createUuid()).has_value());
}

void ZzCredentialStoreTest::updateCredentialRoundTrip()
{
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    const QString path = dir.filePath(QStringLiteral("credentials.dat"));

    QUuid id;
    {
        ZzCredentialStore store(path);
        QVERIFY(store.initialize(QStringLiteral("主密码-xyz")));
        id = store.addCredential(QStringLiteral("root@web-01"), QStringLiteral("old"));
        QVERIFY(!id.isNull());
        QVERIFY(store.updateCredential(id, QStringLiteral("new-密码")));
        QVERIFY(!store.updateCredential(QUuid::createUuid(), QStringLiteral("x")));
    }

    ZzCredentialStore store(path);
    QVERIFY(store.unlock(QStringLiteral("主密码-xyz")));
    QCOMPARE(store.credential(id).value(), QStringLiteral("new-密码"));
}

void ZzCredentialStoreTest::removeCredentialRoundTrip()
{
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    const QString path = dir.filePath(QStringLiteral("credentials.dat"));

    QUuid idKeep;
    QUuid idDrop;
    {
        ZzCredentialStore store(path);
        QVERIFY(store.initialize(QStringLiteral("主密码-xyz")));
        idKeep = store.addCredential(QStringLiteral("保留"), QStringLiteral("keep"));
        idDrop = store.addCredential(QStringLiteral("删除"), QStringLiteral("drop"));
        QVERIFY(store.removeCredential(idDrop));
        QVERIFY(!store.removeCredential(idDrop)); // 重复删除返回 false
        QVERIFY(!store.removeCredential(QUuid::createUuid()));
    }

    ZzCredentialStore store(path);
    QVERIFY(store.unlock(QStringLiteral("主密码-xyz")));
    QCOMPARE(store.credential(idKeep).value(), QStringLiteral("keep"));
    QVERIFY(!store.credential(idDrop).has_value());
}

void ZzCredentialStoreTest::lockedStoreRejectsOperations()
{
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    const QString path = dir.filePath(QStringLiteral("credentials.dat"));

    QUuid id;
    {
        ZzCredentialStore store(path);
        QVERIFY(store.initialize(QStringLiteral("主密码-xyz")));
        id = store.addCredential(QStringLiteral("root@web-01"), QStringLiteral("s3cret"));
        QVERIFY(!id.isNull());
    }

    // 未解锁
    ZzCredentialStore locked(path);
    QVERIFY(locked.addCredential(QStringLiteral("x"), QStringLiteral("y")).isNull());
    QVERIFY(!locked.errorString().isEmpty());
    QVERIFY(!locked.updateCredential(id, QStringLiteral("y")));
    QVERIFY(!locked.credential(id).has_value());
    QVERIFY(!locked.removeCredential(id));

    // 解锁后再 lock()，同样全部拒绝
    QVERIFY(locked.unlock(QStringLiteral("主密码-xyz")));
    QVERIFY(locked.credential(id).has_value());
    locked.lock();
    QVERIFY(locked.addCredential(QStringLiteral("x"), QStringLiteral("y")).isNull());
    QVERIFY(!locked.updateCredential(id, QStringLiteral("y")));
    QVERIFY(!locked.credential(id).has_value());
    QVERIFY(!locked.removeCredential(id));
}
```

- [ ] **步骤 2：运行测试验证通过**

运行：`cmake --build build/debug && ctest --test-dir build/debug -R ZzCredentialStoreTest --output-on-failure`
预期：`100% tests passed`；`ZzCredentialStoreTest` 下 11 个用例全部 PASS，含 `PASS   : ZzCredentialStoreTest::credentialRoundTrip()`。

若有 FAIL：回到 `src/session/ZzCredentialStore.cpp` 修复对应方法，重复本步骤直至全绿。

- [ ] **步骤 3：Commit**

```bash
git add tests/session/ZzCredentialStoreTest.cpp src/session/ZzCredentialStore.cpp
git commit -m "test: ZzCredentialStore 凭据增删改查与加解密往返测试"
```
（若步骤 2 未改动实现，则 `git add` 只有测试文件，属正常。）

---

### 任务 6：性能测试门控与记录（规格 §9.1）

**文件：**
- 创建：`tests/session/ZzCredentialPerfTest.cpp`
- 创建：`tests/perf/records/.gitkeep`
- 修改：`tests/session/CMakeLists.txt`（追加性能测试目标与编译定义注入）

阈值定义（凭据加解密，规格 §9.1 要求达标方可验收）：

| 测试项 | 阈值 | 说明 |
| ------ | ---- | ---- |
| credential-unlock | ≤ 2000 ms | 单次解锁（含 PBKDF2-HMAC-SHA256 60 万次迭代 + GCM 解密校验） |
| credential-roundtrip | ≤ 3000 ms | 1000 条凭据逐条写入（每条触发整体加密落盘）+ 冷启动解锁 + 全量读取 |

- [ ] **步骤 1：修改 `tests/session/CMakeLists.txt`，在文件末尾追加**

```cmake
# 规格 9.1：记录需包含构建类型 / 编译器 / git commit hash；Release 数字才有效
add_executable(ZzCredentialPerfTest ZzCredentialPerfTest.cpp)
target_link_libraries(ZzCredentialPerfTest PRIVATE ZzSessionCore Qt6::Core Qt6::Test)
target_compile_definitions(ZzCredentialPerfTest PRIVATE
    ZZ_BUILD_TYPE="$<CONFIG>"
    ZZ_COMPILER_ID="${CMAKE_CXX_COMPILER_ID}"
    ZZ_COMPILER_VERSION="${CMAKE_CXX_COMPILER_VERSION}"
    ZZ_GIT_COMMIT_HASH="${ZZ_GIT_COMMIT_HASH}"
    ZZ_RECORDS_DIR="${CMAKE_SOURCE_DIR}/tests/perf/records")
add_test(NAME ZzCredentialPerfTest COMMAND ZzCredentialPerfTest)
```

（`ZZ_GIT_COMMIT_HASH` 变量由任务 1 已写入本文件的 git 探测块提供。）

- [ ] **步骤 2：创建 `tests/perf/records/.gitkeep`（空文件，保证规格 §9.1 指定记录目录入库）**

```bash
mkdir -p tests/perf/records && touch tests/perf/records/.gitkeep
```

- [ ] **步骤 3：创建 `tests/session/ZzCredentialPerfTest.cpp`**

```cpp
#include <QtTest>
#include <QDate>
#include <QDateTime>
#include <QDir>
#include <QElapsedTimer>
#include <QFile>
#include <QJsonDocument>
#include <QJsonObject>
#include <QRegularExpression>
#include <QSysInfo>
#include <QTemporaryDir>

#include "ZzCredentialStore.h"

namespace {

/** @brief 解锁阈值：单次解锁（含 PBKDF2 60 万次迭代）不得超过 2000ms。 */
constexpr qint64 kUnlockThresholdMs = 2000;

/** @brief 往返阈值：1000 条凭据写入 + 冷启动解锁 + 全量读取不得超过 3000ms。 */
constexpr qint64 kRoundTripThresholdMs = 3000;

/** @brief 往返测试的凭据条数。 */
constexpr int kCredentialCount = 1000;

/**
 * @brief 读取物理内存总量描述（尽力而为，非 Linux 平台返回 unknown）。
 * @return 如 "16254000 kB"。
 */
QString totalMemoryString()
{
#ifdef Q_OS_LINUX
    QFile file(QStringLiteral("/proc/meminfo"));
    if (file.open(QIODevice::ReadOnly)) {
        const QString content = QString::fromLatin1(file.readAll());
        const QRegularExpression re(QStringLiteral("MemTotal:\\s*(\\d+\\s*kB)"));
        const QRegularExpressionMatch match = re.match(content);
        if (match.hasMatch())
            return match.captured(1);
    }
#endif
    return QStringLiteral("unknown");
}

/**
 * @brief 把一条性能记录写入 tests/perf/records/YYYY-MM-DD-<记录名>.json（规格 9.1）。
 * @param recordName 记录名（文件名后缀）。
 * @param thresholdMs 通过阈值（毫秒）。
 * @param measuredMs 实测值（毫秒）。
 * @param passed 是否达标。
 * @return 写入成功返回 true。
 */
bool writeRecord(const QString &recordName, qint64 thresholdMs, qint64 measuredMs, bool passed)
{
    QJsonObject env;
    env.insert(QStringLiteral("cpu"), QSysInfo::currentCpuArchitecture());
    env.insert(QStringLiteral("memory"), totalMemoryString());
    env.insert(QStringLiteral("os"), QSysInfo::prettyProductName());
    env.insert(QStringLiteral("qtVersion"), QStringLiteral(QT_VERSION_STR));
    env.insert(QStringLiteral("compiler"),
               QStringLiteral(ZZ_COMPILER_ID) + QLatin1Char(' ') + QStringLiteral(ZZ_COMPILER_VERSION));
    env.insert(QStringLiteral("buildType"), QStringLiteral(ZZ_BUILD_TYPE));

    QJsonObject root;
    root.insert(QStringLiteral("testItem"), recordName);
    root.insert(QStringLiteral("thresholdMs"), thresholdMs);
    root.insert(QStringLiteral("measuredMs"), measuredMs);
    root.insert(QStringLiteral("passed"), passed);
    root.insert(QStringLiteral("environment"), env);
    root.insert(QStringLiteral("gitCommit"), QStringLiteral(ZZ_GIT_COMMIT_HASH));
    root.insert(QStringLiteral("timestamp"), QDateTime::currentDateTimeUtc().toString(Qt::ISODate));

    QDir dir(QStringLiteral(ZZ_RECORDS_DIR));
    if (!dir.exists() && !dir.mkpath(QStringLiteral(".")))
        return false;

    const QString path = dir.filePath(QDate::currentDate().toString(QStringLiteral("yyyy-MM-dd"))
                                      + QLatin1Char('-') + recordName + QStringLiteral(".json"));
    QFile file(path);
    if (!file.open(QIODevice::WriteOnly | QIODevice::Truncate))
        return false;
    file.write(QJsonDocument(root).toJson(QJsonDocument::Indented));
    return true;
}

} // namespace

/**
 * @brief 凭据加解密性能门控测试（规格 9.1：阈值失败即测试失败；仅 Release 构建有效）。
 */
class ZzCredentialPerfTest : public QObject
{
    Q_OBJECT

private slots:
    /** @brief 单次解锁耗时（PBKDF2 60 万次迭代 + GCM 解密校验）不超过阈值。 */
    void unlockPerformance()
    {
        if (QStringLiteral(ZZ_BUILD_TYPE) != QLatin1String("Release"))
            QSKIP("性能数字仅 Release 构建有效（规格 9.1），当前构建跳过阈值判定");

        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        const QString path = dir.filePath(QStringLiteral("credentials.dat"));
        {
            ZzCredentialStore store(path);
            QVERIFY(store.initialize(QStringLiteral("PerfMaster-密码")));
        }

        qint64 elapsed = 0;
        {
            ZzCredentialStore store(path);
            QElapsedTimer timer;
            timer.start();
            QVERIFY(store.unlock(QStringLiteral("PerfMaster-密码")));
            elapsed = timer.elapsed();
        }

        const bool passed = elapsed <= kUnlockThresholdMs;
        QVERIFY(writeRecord(QStringLiteral("credential-unlock"), kUnlockThresholdMs, elapsed, passed));
        QVERIFY2(passed, qPrintable(QStringLiteral("解锁耗时 %1ms 超过阈值 %2ms")
                                        .arg(elapsed)
                                        .arg(kUnlockThresholdMs)));
    }

    /** @brief 1000 条凭据逐条加密落盘 + 冷启动解锁 + 全量读取的总耗时不超过阈值。 */
    void encryptDecryptRoundTripPerformance()
    {
        if (QStringLiteral(ZZ_BUILD_TYPE) != QLatin1String("Release"))
            QSKIP("性能数字仅 Release 构建有效（规格 9.1），当前构建跳过阈值判定");

        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        const QString path = dir.filePath(QStringLiteral("credentials.dat"));
        const QString secret = QStringLiteral("perf-secret-0123456789-abcdefghijklmnopqrstuvwxyz-中文填充数据");

        QElapsedTimer timer;
        timer.start();

        QList<QUuid> ids;
        ids.reserve(kCredentialCount);
        {
            ZzCredentialStore store(path);
            QVERIFY(store.initialize(QStringLiteral("PerfMaster-密码")));
            for (int i = 0; i < kCredentialCount; ++i) {
                const QUuid id = store.addCredential(QStringLiteral("cred-%1").arg(i), secret);
                QVERIFY(!id.isNull());
                ids.append(id);
            }
        }
        {
            ZzCredentialStore store(path);
            QVERIFY(store.unlock(QStringLiteral("PerfMaster-密码")));
            for (const QUuid &id : ids)
                QCOMPARE(store.credential(id).value(), secret);
        }

        const qint64 elapsed = timer.elapsed();
        const bool passed = elapsed <= kRoundTripThresholdMs;
        QVERIFY(writeRecord(QStringLiteral("credential-roundtrip"), kRoundTripThresholdMs, elapsed, passed));
        QVERIFY2(passed, qPrintable(QStringLiteral("加解密往返耗时 %1ms 超过阈值 %2ms")
                                        .arg(elapsed)
                                        .arg(kRoundTripThresholdMs)));
    }
};

QTEST_GUILESS_MAIN(ZzCredentialPerfTest)

#include "ZzCredentialPerfTest.moc"
```

- [ ] **步骤 4：Debug 构建下验证测试注册（性能用例被跳过，不计阈值）**

运行：`cmake -S . -B build/debug -G Ninja -DCMAKE_BUILD_TYPE=Debug && cmake --build build/debug && ctest --test-dir build/debug --output-on-failure`
预期：`100% tests passed, 0 tests failed out of 4`；`ZzCredentialPerfTest` 输出含 `QSKIP : ... 性能数字仅 Release 构建有效`。

- [ ] **步骤 5：Release 构建下运行性能门控并生成记录**

运行：`cmake -S . -B build/release -G Ninja -DCMAKE_BUILD_TYPE=Release && cmake --build build/release && ctest --test-dir build/release -R ZzCredentialPerfTest --output-on-failure`
预期：`100% tests passed, 0 tests failed out of 1`；`tests/perf/records/` 下生成 `<当天日期>-credential-unlock.json` 与 `<当天日期>-credential-roundtrip.json`。

然后检查记录内容：`cat tests/perf/records/*-credential-unlock.json`，确认包含 `thresholdMs`、`measuredMs`、`passed: true`、`environment`（cpu/memory/os/qtVersion/compiler/buildType=Release）、`gitCommit`（当前 commit 短 hash）、`timestamp`。

若不达标（阈值失败）：优化实现（如调整迭代次数需重新评估安全性，先排查机器负载）后重跑，直到 Release 下通过为止——规格 9.1 不达标不验收。

- [ ] **步骤 6：Commit（含性能记录）**

```bash
git add tests/session/CMakeLists.txt tests/session/ZzCredentialPerfTest.cpp tests/perf/
git commit -m "test: 凭据加解密性能门控测试与首份 Release 性能记录"
```

---

### 任务 7：全量回归与收尾

**文件：**
- 无新增；仅验证与必要的修复。

- [ ] **步骤 1：Debug 全量回归**

运行：`cmake -S . -B build/debug -G Ninja -DCMAKE_BUILD_TYPE=Debug --fresh && cmake --build build/debug && ctest --test-dir build/debug --output-on-failure`
预期：`100% tests passed, 0 tests failed out of 4`（`ZzSessionProfileTest`、`ZzSessionModelTest`、`ZzCredentialStoreTest` 全 PASS，`ZzCredentialPerfTest` 为 QSKIP）。

- [ ] **步骤 2：Release 全量回归（性能门控生效）**

运行：`cmake -S . -B build/release -G Ninja -DCMAKE_BUILD_TYPE=Release --fresh && cmake --build build/release && ctest --test-dir build/release --output-on-failure`
预期：`100% tests passed, 0 tests failed out of 4`，`ZzCredentialPerfTest` 两个用例 PASS，记录 JSON 已更新。

- [ ] **步骤 3：编码规范自查**

运行：`grep -rn "QtWidgets\|QWidget" src/session/`（应无输出，确认纯 Qt Core）；并逐文件确认：

- `src/session/ZzSessionProfile.h` 定义 `ZzAuthMethod` 与 `ZzSessionProfile`，且声明后紧跟 `Q_DECLARE_METATYPE(ZzSessionProfile)`（计划 04 冻结契约），文件名与主类名一致
- `src/session/ZzSessionModel.h/.cpp` ↔ `ZzSessionModel`
- `src/session/ZzCredentialStore.h/.cpp` ↔ `ZzCredentialStore`
- `ZzSessionProfile` 含契约字段 `id`（QUuid）、`protocol`（"ssh"/"local"）、`credentialId`（QUuid）
- 头文件注释均为 Doxygen 风格简体中文（`@brief` / `@param` / `@return`）

- [ ] **步骤 4：三平台验证说明**

三平台 preset 矩阵与可执行包由计划 04 骨架与后续装配计划负责；本计划交付的库与测试均为纯 Qt Core + OpenSSL，无平台相关代码（`/proc/meminfo` 读取有 `#ifdef Q_OS_LINUX` 兜底）。Windows / macOS 实际构建与运行留待三平台人工验收清单环节（规格 §九），本计划不要求在本机验证非 Linux 平台。

- [ ] **步骤 5：最终 Commit（若步骤 1-4 有修复）**

```bash
git add -A
git commit -m "chore: 会话模型与凭据存储模块全量回归收尾"
```
（若无任何修复则跳过本步，直接汇报完成。）

---

## 附录：公开 API 清单（计划 04 冻结契约 + 供其他计划消费）

库目标：`ZzSessionCore`（静态库，`src/session/`；`PUBLIC Qt6::Core`、`PRIVATE OpenSSL::Crypto`，头文件经 `target_include_directories` 暴露为 `#include "ZzSessionModel.h"` 形式）。

### `ZzSessionProfile`（`src/session/ZzSessionProfile.h`）

契约字段（与计划 04 一致）：`id`（`QUuid`）、`protocol`（`QString`，`"ssh"` / `"local"`，默认 `"ssh"`）、`credentialId`（`QUuid`，null 表示无密码引用）。结构体声明后紧跟 `Q_DECLARE_METATYPE(ZzSessionProfile)`。

完整字段：`QUuid id` / `QString name` / `QString groupPath` / `QString protocol` / `QString host` / `quint16 port = 22` / `QString userName` / `ZzAuthMethod authMethod`（`Agent` / `PrivateKey` / `Password`）/ `QString privateKeyPath` / `QUuid credentialId` / `QString terminalType = "xterm-256color"` / `QString encoding = "UTF-8"` / `QString colorSchemeName` / `int keepAliveIntervalSeconds = 0`。

方法：`QJsonObject toJson() const`、`static ZzSessionProfile fromJson(const QJsonObject &)`、`operator==`（C++20 defaulted）。

### `ZzSessionModel`（`src/session/ZzSessionModel.h`）

- `static QString defaultFilePath()` — 平台配置目录下 `sessions.json`
- `bool load()` / `bool save() const`
- `QList<ZzSessionProfile> allSessions() const`
- `std::optional<ZzSessionProfile> session(const QUuid &id) const`
- `QUuid addSession(ZzSessionProfile profile)` — 失败返回 null QUuid
- `bool updateSession(const ZzSessionProfile &profile)`
- `bool removeSession(const QUuid &id)`
- `QStringList allGroupPaths() const`
- `QList<ZzSessionProfile> sessionsInGroup(const QString &groupPath) const`
- `bool renameGroup(const QString &oldPath, const QString &newPath)`
- `bool removeGroup(const QString &groupPath)`
- `QString errorString() const`
- 信号：`void sessionsChanged()`

### `ZzCredentialStore`（`src/session/ZzCredentialStore.h`）

- `static QString defaultFilePath()` — 平台配置目录下 `credentials.dat`
- `bool hasMasterPassword() const`
- `bool initialize(const QString &masterPassword)` — 首次设主密码，成功后即解锁
- `bool unlock(const QString &masterPassword)` / `void lock()` / `bool isUnlocked() const`
- `QUuid addCredential(const QString &name, const QString &secret)` — 失败返回 null QUuid
- `bool updateCredential(const QUuid &credentialId, const QString &secret)`
- `std::optional<QString> credential(const QUuid &credentialId) const`
- `bool removeCredential(const QUuid &credentialId)`
- `QString errorString() const`

连接流程中的凭据取用（规格 §七）：会话 `authMethod == ZzAuthMethod::Password` 时，用 `credentialId` 向已解锁的 `ZzCredentialStore` 取明文；未解锁则弹主密码框。`protocol == "local"` 的会话不经 SSH 与凭据存储，由壳层直接起本地 PTY。
