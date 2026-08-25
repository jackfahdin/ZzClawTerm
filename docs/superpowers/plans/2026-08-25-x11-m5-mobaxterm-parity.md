# X11 体验对齐 MobaXterm（M5）实现计划

> **面向 AI 代理的工作者：** 必需子技能：使用 superpowers:subagent-driven-development（推荐）或 superpowers:executing-plans 逐任务实现此计划。步骤使用复选框（`- [ ]`）语法来跟踪进度。

**目标：** 把 ZzClawTerm 的 X11 体验对齐 MobaXterm——应用级共享单例 ZzXsrv（启动即拉起、可全局关闭）、X11 转发默认开、独立窗口为默认形态。

**架构：** 新增 `ZzX11Service` 应用级门面持有共享 `ZzXServerManager`（multiwindow 模式），`ZzSshTransport` 非嵌入会话只读引用它；M4b 嵌入路径保留为实验选项、会话自带独立 server。ZzXsrv 仓补 `-parent`/`-multiwindow` 互斥校验、安装包快捷方式默认 `-multiwindow`。

**技术栈：** C++20 / Qt 6.8+ / CMake Presets / Qt Test / NSIS / ZzXsrv（vcxsrv 魔改，meson 构建）。

**规格：** `docs/superpowers/specs/2026-08-25-x11-m5-mobaxterm-parity-design.md`（任务与规格的对应：任务 1-5 = M5a 主仓，任务 6-7 = M5b ZzXsrv 仓）。

**全局约定（每个主仓任务都要遵守）：**

- 构建：`cmake --build --preset linux-gcc-release`
- 定向测试：`ctest --preset linux-gcc-release -R <测试名> --output-on-failure`
- 全量回归：`ctest --preset linux-gcc-release`（基线 44 项全绿）
- **全量 ctest 后必须恢复 perf 记录**：`git checkout -- tests/perf/records/`，并删除当天新生成的未跟踪文件（按当天日期前缀 `tests/perf/records/$(date +%F)-*.json` 逐一核对删除，**绝不用月份通配**）
- commit：Conventional Commits 前缀 + 中文首行 + 空行 + 中文详述；类名 Zz 前缀、文件名=类名、Doxygen 简体中文注释
- **push 一律等用户明确确认**，计划中不得自动推送

---

### 任务 1：会话默认值翻转（x11Forwarding 默认开、嵌入默认关）

**文件：**
- 修改：`src/session/ZzSessionProfile.h:45-46`
- 修改：`src/panel/ZzSessionEditDialog.cpp:153-168`
- 测试：`tests/session/ZzSessionProfileTest.cpp`（追加用例）、`tests/unit/tst_ZzSessionEditDialog.cpp`（追加用例）

- [ ] **步骤 1：编写失败的测试**

在 `tests/session/ZzSessionProfileTest.cpp` 的 `private slots:` 追加：

```cpp
    void x11DefaultsAlignMobaXterm()
    {
        // M5 规格 §三决策 2/3：转发默认开、嵌入默认关
        const ZzSessionProfile profile;
        QVERIFY(profile.x11Forwarding);
        QVERIFY(!profile.x11EmbedMode);
    }

    void oldJsonWithoutX11KeysTakesNewDefaults()
    {
        // M5 规格 §九：旧 JSON 缺键取代码新默认
        QJsonObject obj;
        obj.insert(QStringLiteral("name"), QStringLiteral("s"));
        const ZzSessionProfile parsed = ZzSessionProfile::fromJson(obj);
        QVERIFY(parsed.x11Forwarding);
        QVERIFY(!parsed.x11EmbedMode);
    }

    void explicitX11ValuesSurviveRoundtrip()
    {
        // 显式存过的值不受默认值翻转影响
        ZzSessionProfile profile;
        profile.x11Forwarding = false;
        profile.x11EmbedMode = true;
        const ZzSessionProfile parsed = ZzSessionProfile::fromJson(profile.toJson());
        QVERIFY(!parsed.x11Forwarding);
        QVERIFY(parsed.x11EmbedMode);
    }
```

在 `tests/unit/tst_ZzSessionEditDialog.cpp` 追加（构造方式参照该文件已有用例——`ZzSessionEditDialog` 构造签名为 `(ZzCredentialStore *store, ...)`，复用文件中现成的 store 构造）：

```cpp
    void x11CheckBoxesMatchNewDefaults()
    {
        // 默认 profile 打开对话框：转发勾选、嵌入不勾选（M5 默认值翻转）
        // …按本文件已有用例的方式构造 dialog…
        auto *x11 = dialog.findChild<QCheckBox *>(QStringLiteral("x11CheckBox"));
        auto *embed = dialog.findChild<QCheckBox *>(QStringLiteral("x11EmbedCheckBox"));
        QVERIFY(x11 && x11->isChecked());
        QVERIFY(embed && !embed->isChecked());
    }
```

- [ ] **步骤 2：运行测试验证失败**

运行：`cmake --build --preset linux-gcc-release && ctest --preset linux-gcc-release -R "ZzSessionProfile|ZzSessionEditDialog" --output-on-failure`
预期：新增 3+1 用例 FAIL（当前默认 false/true 与断言相反）

- [ ] **步骤 3：修改默认值与对话框文案**

`src/session/ZzSessionProfile.h:45-46` 改为：

```cpp
    bool x11Forwarding = true;        ///< 是否启用 X11 转发（缺省 true 对齐 MobaXterm；旧 JSON 无此字段时兼容）
    bool x11EmbedMode = false;        ///< X11 嵌入标签页显示（实验；false=独立窗口，缺省 false，旧 JSON 兼容）
```

`src/panel/ZzSessionEditDialog.cpp:153-168` 文案调整（`ZzSessionEditDialog.h` 顶部类注释若有"实验性"表述同步更新）：

```cpp
    // X11 转发开关：默认开启对齐 MobaXterm；Windows 走内建 X server，Linux/macOS 依赖本机 X server
    m_x11CheckBox = new QCheckBox(QStringLiteral("X11 转发"), this);
```

嵌入复选框标注实验：

```cpp
    auto *x11EmbedCheckBox =
        new QCheckBox(QStringLiteral("嵌入标签页显示（实验；否则独立窗口）"), this);
```

- [ ] **步骤 4：运行测试验证通过**

运行：`ctest --preset linux-gcc-release -R "ZzSessionProfile|ZzSessionEditDialog" --output-on-failure`
预期：PASS

- [ ] **步骤 5：Commit**

```bash
git add src/session/ZzSessionProfile.h src/panel/ZzSessionEditDialog.cpp tests/session/ZzSessionProfileTest.cpp tests/unit/tst_ZzSessionEditDialog.cpp
git commit -m "feat(session): X11 转发默认开启、嵌入模式默认关闭并标注实验

对齐 MobaXterm（M5 规格 §三决策 2/3）：
- ZzSessionProfile::x11Forwarding 默认 false→true，x11EmbedMode 默认 true→false
- 旧 JSON 缺键取新默认（规格 §九行为变化，release notes 声明）
- 会话编辑对话框：转发去实验性标注，嵌入加实验标注
- 测试：默认值、旧 JSON 缺键、显式值往返三组用例"
```

---

### 任务 2：全局开关"启用 X server"（ZzAppSettings + ZzSettingsPage）

**文件：**
- 修改：`src/settings/ZzAppSettings.h`、`src/settings/ZzAppSettings.cpp`
- 修改：`src/settings/ZzSettingsPage.h`、`src/settings/ZzSettingsPage.cpp`
- 测试：`tests/unit/tst_ZzAppSettings.cpp`、`tests/unit/tst_ZzSettingsPage.cpp`

- [ ] **步骤 1：编写失败的测试**

`tests/unit/tst_ZzAppSettings.cpp` 追加（该文件已有用例展示了如何用临时 INI 路径构造 `ZzAppSettings`，照其模式）：

```cpp
    void x11ServerEnabledDefaultsTrue()
    {
        ZzAppSettings settings(m_dir.filePath(QStringLiteral("s.ini")));
        QVERIFY(settings.x11ServerEnabled()); // M5 规格 §三决策 1：默认开
    }

    void x11ServerEnabledRoundtrip()
    {
        ZzAppSettings settings(m_dir.filePath(QStringLiteral("s.ini")));
        QSignalSpy spy(&settings, &ZzAppSettings::settingsChanged);
        settings.setX11ServerEnabled(false);
        QVERIFY(!settings.x11ServerEnabled());
        QCOMPARE(spy.count(), 1);
        settings.setX11ServerEnabled(false); // 同值短路不再发射
        QCOMPARE(spy.count(), 1);
    }
```

`tests/unit/tst_ZzSettingsPage.cpp` 追加（构造模式照该文件已有用例）：

```cpp
    void x11ServerCheckBoxReflectsAndWrites()
    {
        // …按本文件已有用例的方式构造 ZzAppSettings（临时 INI）与 ZzSettingsPage…
        auto *box = page.x11ServerCheck();
        QVERIFY(box && box->isChecked());      // 默认开
        box->setChecked(false);                // toggled 即写设置
        QVERIFY(!settings.x11ServerEnabled());
    }
```

- [ ] **步骤 2：运行测试验证失败**

运行：`cmake --build --preset linux-gcc-release && ctest --preset linux-gcc-release -R "ZzAppSettings|ZzSettingsPage" --output-on-failure`
预期：编译失败（`x11ServerEnabled`/`x11ServerCheck` 不存在）——此即失败验证

- [ ] **步骤 3：实现设置项与复选框**

`src/settings/ZzAppSettings.h`：类注释字段列表加"启用 X server"；追加声明：

```cpp
    /** @brief 启用内建 X server（应用启动时自动拉起共享实例），默认 true。 */
    [[nodiscard]] bool x11ServerEnabled() const;
    void setX11ServerEnabled(bool enabled);
```

`src/settings/ZzAppSettings.cpp` 追加（键 `x11/serverEnabled`，同值短路模式照 `setCredentialBackend`）：

```cpp
bool ZzAppSettings::x11ServerEnabled() const
{
    return m_settings->value(QStringLiteral("x11/serverEnabled"), true).toBool();
}

void ZzAppSettings::setX11ServerEnabled(bool enabled)
{
    if (x11ServerEnabled() == enabled) {
        return;
    }
    m_settings->setValue(QStringLiteral("x11/serverEnabled"), enabled);
    emit settingsChanged();
}
```

`src/settings/ZzSettingsPage.h`：include 前置声明 `class QCheckBox;`；追加：

```cpp
    [[nodiscard]] QCheckBox *x11ServerCheck() const;
```

私有成员追加 `QCheckBox *m_x11ServerCheck;`

`src/settings/ZzSettingsPage.cpp`：构造内凭据后端行之后、note 之前追加（include `<QtWidgets/QCheckBox>`）：

```cpp
    m_x11ServerCheck = new QCheckBox(QStringLiteral("启用 X server（启动时自动运行）"), this);
    m_x11ServerCheck->setChecked(m_settings->x11ServerEnabled());
    m_x11ServerCheck->setToolTip(QStringLiteral(
        "关闭后停止内建 X server，新会话不再发起 X11 转发；重新开启即恢复"));
    layout->addRow(QStringLiteral("X11："), m_x11ServerCheck);
```

连接区追加：

```cpp
    connect(m_x11ServerCheck, &QCheckBox::toggled,
            m_settings, &ZzAppSettings::setX11ServerEnabled);
```

文件尾部追加访问器：

```cpp
QCheckBox *ZzSettingsPage::x11ServerCheck() const { return m_x11ServerCheck; }
```

- [ ] **步骤 4：运行测试验证通过**

运行：`cmake --build --preset linux-gcc-release && ctest --preset linux-gcc-release -R "ZzAppSettings|ZzSettingsPage" --output-on-failure`
预期：PASS

- [ ] **步骤 5：Commit**

```bash
git add src/settings/ZzAppSettings.h src/settings/ZzAppSettings.cpp src/settings/ZzSettingsPage.h src/settings/ZzSettingsPage.cpp tests/unit/tst_ZzAppSettings.cpp tests/unit/tst_ZzSettingsPage.cpp
git commit -m "feat(settings): 新增全局开关\"启用 X server\"（默认开）

M5 规格 §三决策 1\"允许关闭\"的落地：
- ZzAppSettings::x11ServerEnabled（键 x11/serverEnabled，默认 true，同值短路）
- ZzSettingsPage 复选框即改即存，tooltip 说明关闭语义
- 拉起/停止联动在任务 5 由 ZzAppShell 接线"
```

---

### 任务 3：ZzX11Service 应用级共享 X server 门面（新类）

**文件：**
- 创建：`src/x11/ZzX11Service.h`
- 创建：`src/x11/ZzX11Service.cpp`
- 修改：`src/CMakeLists.txt`（x11 源文件清单处，照 `ZzXServerManager` 两行模式追加）
- 测试：创建 `tests/unit/tst_ZzX11Service.cpp`；修改 `tests/CMakeLists.txt`（照 `tst_ZzXServerManager` 注册模式追加）

**设计要点（规格 §4.1/§4.3）：** 拥有 `ZzXServerManager`（multiwindow 共享）与惰性 `ZzXServerDownloader`（仅 Windows）；`setEnabled` 驱动拉起/停止；`start()` 幂等；cookie 在 server 就绪时生成一次供全部会话共用；崩溃只发信号不自动热恢复，`ensureRunning()` 提供懒重拉。

- [ ] **步骤 1：编写失败的测试**

创建 `tests/unit/tst_ZzX11Service.cpp`（桩脚本模式照 `tests/unit/tst_ZzXServerManager.cpp:21-42`）：

```cpp
#include <QtTest/QtTest>
#include <QFile>
#include <QTemporaryDir>
#include <QStandardPaths>

#include "x11/ZzX11Service.h"

/**
 * @brief ZzX11Service 单元测试：开关语义、幂等拉起、共享 cookie、停用停止。
 *
 * 桩 server 为 POSIX shell 脚本（记录参数后 sleep），进程类用例仅 Unix 有效。
 */
class tst_ZzX11Service : public QObject
{
    Q_OBJECT
private:
    QTemporaryDir m_dir;

    QString makeSleepingStub(const QString &argsFile)
    {
        const QString path = m_dir.filePath(QStringLiteral("stub-xserver.sh"));
        QFile f(path);
        if (!f.open(QIODevice::WriteOnly))
            return {};
        f.write("#!/bin/sh\n"
                "printf '%s\\n' \"$@\" > '" + argsFile.toUtf8() + "'\n"
                "sleep 60\n");
        f.close();
        f.setPermissions(QFileDevice::ReadOwner | QFileDevice::WriteOwner |
                         QFileDevice::ExeOwner | QFileDevice::ReadUser |
                         QFileDevice::WriteUser | QFileDevice::ExeUser);
        return path;
    }

private slots:
    void initTestCase()
    {
        QStandardPaths::setTestModeEnabled(true); // xauth 落测试目录，不碰真实用户目录
    }

    /** @brief 全局开关关闭时 start/ensureRunning 为空操作（规格 §七错误处理）。 */
    void disabledStartIsNoop()
    {
        ZzX11Service service;
        QVERIFY(!service.isEnabled());
        service.start();
        service.ensureRunning();
        QVERIFY(!service.isRunning());
        QCOMPARE(service.display(), -1);
    }

    /** @brief 开启即拉起（幂等），multiwindow 参数组，cookie 共享一份。 */
    void enableStartsSharedServer()
    {
#ifdef Q_OS_UNIX
        const QString argsFile = m_dir.filePath(QStringLiteral("args.txt"));
        const QString stub = makeSleepingStub(argsFile);
        QVERIFY(!stub.isEmpty());
        ZzX11Service service;
        service.setServerProgramForTesting(stub);
        QSignalSpy startedSpy(&service, &ZzX11Service::serverStarted);

        service.setEnabled(true);
        QTRY_VERIFY_WITH_TIMEOUT(service.isRunning(), 5000);
        QCOMPARE(startedSpy.count(), 1);
        QVERIFY(service.display() >= 0);
        QVERIFY(!service.cookie().isEmpty());

        // 幂等：运行中重复 start/ensureRunning 不再拉起、不重发信号
        service.start();
        service.ensureRunning();
        service.setEnabled(true); // 同值短路
        QTest::qWait(200);
        QCOMPARE(startedSpy.count(), 1);

        // 参数组：-multiwindow（非嵌入共享形态，规格 §4.2）
        QTRY_VERIFY(QFile::exists(argsFile));
        QFile f(argsFile);
        QVERIFY(f.open(QIODevice::ReadOnly));
        const QString args = QString::fromUtf8(f.readAll());
        QVERIFY(args.contains(QStringLiteral("-multiwindow")));
        QVERIFY(!args.contains(QStringLiteral("-parent")));
#else
        QSKIP("桩 server 为 POSIX 脚本，仅 Unix 有效");
#endif
    }

    /** @brief 关闭开关停止共享 server（规格 §三决策 1 允许关闭）。 */
    void disableStopsServer()
    {
#ifdef Q_OS_UNIX
        const QString argsFile = m_dir.filePath(QStringLiteral("args2.txt"));
        const QString stub = makeSleepingStub(argsFile);
        QVERIFY(!stub.isEmpty());
        ZzX11Service service;
        service.setServerProgramForTesting(stub);
        service.setEnabled(true);
        QTRY_VERIFY_WITH_TIMEOUT(service.isRunning(), 5000);

        service.setEnabled(false);
        QTRY_VERIFY_WITH_TIMEOUT(!service.isRunning(), 8000);
        QCOMPARE(service.display(), -1);
        QVERIFY(service.cookie().isEmpty());
#else
        QSKIP("桩 server 为 POSIX 脚本，仅 Unix 有效");
#endif
    }
};

QTEST_MAIN(tst_ZzX11Service)
#include "tst_ZzX11Service.moc"
```

`tests/CMakeLists.txt` 照 `tst_ZzXServerManager` 的注册方式追加 `tst_ZzX11Service`（同源码 include 目录与链接库）。

- [ ] **步骤 2：运行测试验证失败**

运行：`cmake --build --preset linux-gcc-release`
预期：编译失败（`src/x11/ZzX11Service.h` 不存在）——此即失败验证

- [ ] **步骤 3：实现 ZzX11Service**

创建 `src/x11/ZzX11Service.h`：

```cpp
#pragma once

#include <QObject>
#include <QString>

#include "x11/ZzXAuthority.h"
#include "x11/ZzXServerManager.h" // ZzXLocalEndpoint

class ZzXServerDownloader;

/**
 * @brief 应用级共享 X server 门面（M5 规格 §4.1：对齐 MobaXterm 单例语义）。
 *
 * 全局单实例（ZzAppShell 持有）：全局开关开启时拉起 ZzXsrv（-multiwindow）供全部
 * 会话共享；会话只读查询 display/cookie/localEndpoint 并经 ensureRunning() 懒重拉，
 * 不拥有生命周期（关会话不杀，应用退出随析构终止）。全局开关关闭时
 * start()/ensureRunning() 为空操作。嵌入实验模式不经由本类（会话自带独立 server）。
 * cookie 在 server 就绪时生成一次，所有会话共用（xauth 0600 + 仅 127.0.0.1，
 * 威胁模型与现状同级，规格 §4.1）。
 */
class ZzX11Service : public QObject
{
    Q_OBJECT
public:
    explicit ZzX11Service(QObject *parent = nullptr);

    /** @brief 全局开关：开→立即拉起（幂等）；关→停止并阻止后续拉起。 */
    void setEnabled(bool enabled);
    /** @brief 当前开关状态。 */
    [[nodiscard]] bool isEnabled() const { return m_enabled; }

    /** @brief 拉起共享 server（幂等：已禁用/启动中/运行中均为空操作）。 */
    void start();
    /** @brief 停止共享 server（异步收尾，语义同 ZzXServerManager::stop）。 */
    void stop();
    /** @brief 会话侧懒重拉入口：等价 start()（规格 §4.3 lazy 重拉）。 */
    void ensureRunning() { start(); }

    /** @brief server 是否在运行。 */
    [[nodiscard]] bool isRunning() const;
    /** @brief 当前 display 号；未运行返回 -1。 */
    [[nodiscard]] int display() const { return m_display; }
    /** @brief 共享 MIT-MAGIC-COOKIE-1（hex）；未运行返回空串。 */
    [[nodiscard]] QString cookie() const { return m_cookie; }
    /** @brief 本地接入端点（透传 ZzXServerManager）。 */
    [[nodiscard]] ZzXLocalEndpoint localEndpoint() const;

    /** @brief 测试注入：以桩可执行替代真实 server 程序（透传 ZzXServerManager）。 */
    void setServerProgramForTesting(const QString &program);

signals:
    void serverStarted(int display);              ///< server 就绪（含懒重拉成功）
    void startFailed(const QString &message);     ///< 启动失败（下载失败/无 display/授权写入失败）
    void serverCrashed(const QString &message);   ///< 非预期退出（不自动热恢复）

private:
    void onDownloaderReady(const QString &executablePath); ///< 仅 Windows

    ZzXServerManager *m_manager = nullptr;        ///< 本对象为父
    ZzXServerDownloader *m_downloader = nullptr;  ///< 仅 Windows：本对象为父，惰性创建
    ZzXAuthority m_authority;                     ///< 值成员：无状态 cookie/xauth 工具
    QString m_programOverride;                    ///< 测试注入的桩程序路径（空=真实 server）
    QString m_cookie;                             ///< 共享 cookie（server 就绪时生成）
    int m_display = -1;                           ///< 当前 display 号
    bool m_enabled = false;                       ///< 全局开关（ZzAppShell 按设置驱动）
    bool m_starting = false;                      ///< 下载/拉起进行中（start 幂等判定）
};
```

创建 `src/x11/ZzX11Service.cpp`：

```cpp
#include "ZzX11Service.h"

#include <QFileInfo>
#include <QStandardPaths>

#include "x11/ZzXServerDownloader.h"

ZzX11Service::ZzX11Service(QObject *parent)
    : QObject(parent)
    , m_manager(new ZzXServerManager(this))
{
    connect(m_manager, &ZzXServerManager::started, this, [this](int display) {
        m_starting = false;
        m_display = display;
        if (m_cookie.isEmpty()) {
            m_cookie = m_authority.generateCookie();
        }
#ifndef Q_OS_WIN
        // Unix 复用系统 X server：cookie 需并入用户授权库（桩程序测试路径跳过）
        if (m_programOverride.isEmpty()) {
            QString error;
            if (!m_authority.addToSystemAuthority(display, m_cookie, &error)) {
                m_display = -1;
                m_cookie.clear();
                emit startFailed(tr("X11 授权写入失败：%1").arg(error));
                return;
            }
        }
#endif
        emit serverStarted(display);
    });
    connect(m_manager, &ZzXServerManager::crashed, this,
            [this](const QString &message) {
                m_starting = false;
                m_display = -1;
                m_cookie.clear();
                emit serverCrashed(message);
            });
    connect(m_manager, &ZzXServerManager::stopped, this, [this] {
        m_starting = false;
        m_display = -1;
        m_cookie.clear();
    });
}

void ZzX11Service::setEnabled(bool enabled)
{
    if (m_enabled == enabled) {
        return;
    }
    m_enabled = enabled;
    if (m_enabled) {
        start();
    } else {
        stop();
    }
}

void ZzX11Service::start()
{
    if (!m_enabled || m_starting || m_manager->isRunning()) {
        return;
    }
#if defined(Q_OS_WIN)
    m_starting = true;
    if (!m_downloader) {
        m_downloader = new ZzXServerDownloader(this);
        connect(m_downloader, &ZzXServerDownloader::ready,
                this, &ZzX11Service::onDownloaderReady);
        connect(m_downloader, &ZzXServerDownloader::downloadFailed, this,
                [this](const QString &message) {
                    m_starting = false;
                    emit startFailed(tr("X11 转发不可用：%1").arg(message));
                });
    }
    m_downloader->ensureAvailable();
#else
    if (m_programOverride.isEmpty()) {
#if defined(Q_OS_LINUX)
        // 无本地 X server（纯 Wayland/无头）时提前提示并跳过
        if (qgetenv("DISPLAY").isEmpty()) {
            emit startFailed(tr("X11 转发已跳过：未检测到本地 X server（$DISPLAY 为空）"));
            return;
        }
#elif defined(Q_OS_MAC)
        if (!QFileInfo::exists(QStringLiteral("/tmp/.X11-unix"))) {
            emit startFailed(tr("X11 转发已跳过：未检测到 XQuartz（/tmp/.X11-unix 不存在）"));
            return;
        }
#endif
    }
    m_starting = true;
    m_manager->start(QString(), QString(), 0); // Unix：解析 $DISPLAY 或拉起桩程序
#endif
}

void ZzX11Service::stop()
{
    m_starting = false;
    m_manager->stop();
}

bool ZzX11Service::isRunning() const
{
    return m_manager->isRunning();
}

ZzXLocalEndpoint ZzX11Service::localEndpoint() const
{
    return m_manager->localEndpoint();
}

void ZzX11Service::setServerProgramForTesting(const QString &program)
{
    m_programOverride = program;
    m_manager->setServerProgramForTesting(program);
}

#if defined(Q_OS_WIN)
void ZzX11Service::onDownloaderReady(const QString &executablePath)
{
    if (!m_enabled) {
        m_starting = false; // 下载期间开关被关闭：放弃拉起
        return;
    }
    const int d = ZzXServerManager::allocateDisplay();
    if (d < 0) {
        m_starting = false;
        emit startFailed(tr("X11 转发不可用：无空闲 display 号"));
        return;
    }
    m_cookie = m_authority.generateCookie();
    const QString xauthPath =
        QStandardPaths::writableLocation(QStandardPaths::AppDataLocation)
        + QStringLiteral("/xserver/xauth-shared-%1").arg(d);
    if (!m_authority.writeXauthorityFile(xauthPath, d, m_cookie)) {
        m_starting = false;
        m_cookie.clear();
        emit startFailed(tr("X11 授权写入失败：%1").arg(xauthPath));
        return;
    }
    m_manager->start(executablePath, xauthPath, d);
}
#endif
```

`src/CMakeLists.txt`：在 `x11/ZzXServerManager.h/.cpp` 两行旁追加：

```cmake
    x11/ZzX11Service.h
    x11/ZzX11Service.cpp
```

- [ ] **步骤 4：运行测试验证通过**

运行：`cmake --build --preset linux-gcc-release && ctest --preset linux-gcc-release -R "ZzX11Service" --output-on-failure`
预期：PASS（3 用例）

- [ ] **步骤 5：Commit**

```bash
git add src/x11/ZzX11Service.h src/x11/ZzX11Service.cpp src/CMakeLists.txt tests/unit/tst_ZzX11Service.cpp tests/CMakeLists.txt
git commit -m "feat(x11): 新增 ZzX11Service 应用级共享 X server 门面

M5 规格 §4.1 核心组件：
- 持有共享 ZzXServerManager（-multiwindow）与惰性下载器，全局单例语义
- setEnabled 驱动拉起/停止；start 幂等；ensureRunning 供会话懒重拉
- cookie 就绪时生成一次全部会话共用；崩溃只发信号不自动热恢复
- Unix 保留 \$DISPLAY/XQuartz 预检与用户授权库写入（桩程序路径跳过）
- 单测：禁用空操作/开启拉起+参数组+幂等/停用停止"
```

---

### 任务 4：ZzSshTransport 接入共享服务（非嵌入路径改注入）

**文件：**
- 修改：`src/transport/ZzSshTransport.h`（setter/观察口/成员注释/方法声明）
- 修改：`src/transport/ZzSshTransport.cpp:240-389`（startX11Forwarding 重构、onX11ServerReady 收窄为嵌入专用、requestX11Forwarding 双源、destroyX11 注释）
- 测试：`tests/unit/tst_ZzSshTransport.cpp`（追加用例）

**前提：** 任务 3 已完成（`ZzX11Service` 存在）。

- [ ] **步骤 1：编写失败的测试**

`tests/unit/tst_ZzSshTransport.cpp` 追加（include `"x11/ZzX11Service.h"`）：

```cpp
    void x11ServiceInjectionRoundtrip()
    {
        // M5：共享门面经 ZzTabManager 注入，适配器只观察不拥有
        ZzSshTransport transport;
        QVERIFY(transport.x11Service() == nullptr);
        ZzX11Service service;
        transport.setX11Service(&service);
        QCOMPARE(transport.x11Service(), &service);
    }

    void closeWithX11AndNoServiceIsSafe()
    {
        // x11Forwarding 开启但未注入服务：走"未启用跳过"分支，不得崩溃、照常断开
        ZzSshTransport transport;
        ZzTransportEndpoint endpoint;
        endpoint.host = QStringLiteral("127.0.0.1");
        endpoint.port = 1;
        endpoint.user = QStringLiteral("nobody");
        endpoint.x11Forwarding = true;
        transport.open(endpoint);
        transport.close();
        QCOMPARE(transport.state(), ZzTransportInterface::State::Disconnected);
    }
```

- [ ] **步骤 2：运行测试验证失败**

运行：`cmake --build --preset linux-gcc-release`
预期：编译失败（`setX11Service`/`x11Service` 不存在）——此即失败验证

- [ ] **步骤 3：重构适配器**

`src/transport/ZzSshTransport.h`：

头部前置声明区追加 `class ZzX11Service;`

公开区（`setHostKeyConfirmer` 附近）追加：

```cpp
    /** @brief 注入应用级共享 X server 门面（ZzTabManager 装配；观察指针不拥有）。 */
    void setX11Service(ZzX11Service *service) { m_x11Service = service; }
    /** @brief 测试观察口：当前注入的共享 X server 门面。 */
    [[nodiscard]] ZzX11Service *x11Service() const { return m_x11Service; }
```

私有方法区 `startX11Forwarding()` 声明后追加：

```cpp
    /** @brief 嵌入实验路径（仅 Windows）：会话自带独立 server（M4b 原流程）。 */
    void startX11ForwardingEmbedded();
```

成员区修改注释并追加成员：

```cpp
    ZzXServerManager *m_x11Manager = nullptr;    ///< 本对象为父；仅嵌入实验路径创建（会话自带独立 server）
    ZzX11Service *m_x11Service = nullptr;        ///< 观察指针：应用级共享 server（ZzAppShell 持有，M5）
    ZzXServerDownloader *m_x11Downloader = nullptr; ///< 仅 Windows 嵌入路径：按需下载 ZzXsrv
```

（`m_x11Downloader` 原注释"仅 Windows：按需下载 vcxsrv"按上行替换；`m_x11Cookie` 注释改为"嵌入路径的 MIT-MAGIC-COOKIE-1（hex）"。）

`src/transport/ZzSshTransport.cpp`：

include 区追加 `#include "x11/ZzX11Service.h"`。

`startX11Forwarding()`（:240-305）整体替换为：

```cpp
void ZzSshTransportAdapter::startX11Forwarding()
{
    // 契约：无论装配成败，本函数保证以 openShellChannel() 收尾（同步或经服务
    // 信号异步续接），任何失败只瞬时提示、不阻断会话
    if (!m_channel) {
        return;
    }
#if defined(Q_OS_WIN)
    if (m_endpoint.x11ParentWindow != 0) {
        startX11ForwardingEmbedded(); // 嵌入实验路径：会话自带独立 server
        return;
    }
#endif
    // 非嵌入会话走应用级共享 server（M5 规格 §4.2）
    if (!m_x11Service || !m_x11Service->isEnabled()) {
        emit statusNotice(QStringLiteral("X11 转发已跳过：X server 未启用"));
        openShellChannel();
        return;
    }
    if (m_x11Service->isRunning()) {
        requestX11Forwarding();
        openShellChannel();
        return;
    }
    // server 未就绪（首次下载/懒重拉中）：一次性续接，失败不阻断会话
    connect(m_x11Service, &ZzX11Service::serverStarted, this, [this](int) {
        if (!m_channel || !m_endpoint.x11Forwarding) {
            return;
        }
        requestX11Forwarding();
        openShellChannel();
    }, Qt::SingleShotConnection);
    connect(m_x11Service, &ZzX11Service::startFailed, this,
            [this](const QString &message) {
                emit statusNotice(message);
                openShellChannel();
            }, Qt::SingleShotConnection);
    m_x11Service->ensureRunning();
}

#if defined(Q_OS_WIN)
void ZzSshTransportAdapter::startX11ForwardingEmbedded()
{
    // 嵌入实验路径（M4b 原流程）：按会话生成 cookie、按需下载、拉起独立 server
    m_x11Cookie = m_x11Authority.generateCookie();
    if (!m_x11Downloader) {
        m_x11Downloader = new ZzXServerDownloader(this);
        connect(m_x11Downloader, &ZzXServerDownloader::ready, this,
                &ZzSshTransportAdapter::onX11ServerReady);
        connect(m_x11Downloader, &ZzXServerDownloader::downloadFailed, this,
                [this](const QString &message) {
                    emit statusNotice(QStringLiteral("X11 转发不可用：%1").arg(message));
                    openShellChannel(); // X11 失败不阻断会话
                });
    }
    m_x11Downloader->ensureAvailable();
}
#endif
```

（原 :247-263 的 Unix `$DISPLAY`/XQuartz 预检与原 :264 的 cookie 生成随之删除——预检已移入 `ZzX11Service::start()`，cookie 改由共享服务持有。原 :278-304 的 Unix 每会话 manager 分支整体删除。）

`onX11ServerReady()`（:307-349）收窄为嵌入专用：

```cpp
void ZzSshTransportAdapter::onX11ServerReady(const QString &executablePath)
{
#if defined(Q_OS_WIN)
    // 仅嵌入路径使用本地下载器/管理器；非嵌入走共享服务不经此处
    if (!m_channel || !m_endpoint.x11Forwarding || m_endpoint.x11ParentWindow == 0) {
        return;
    }
    if (!m_x11Manager) {
        m_x11Manager = new ZzXServerManager(this);
        connect(m_x11Manager, &ZzXServerManager::crashed, this,
                [this](const QString &message) {
                    emit statusNotice(QStringLiteral("X11 本地 server 异常：%1").arg(message));
                });
    }
    const int display = ZzXServerManager::allocateDisplay();
    if (display < 0) {
        emit statusNotice(QStringLiteral("X11 转发不可用：无空闲 display 号"));
        openShellChannel();
        return;
    }
    const QString xauthPath =
        QStandardPaths::writableLocation(QStandardPaths::AppDataLocation)
        + QStringLiteral("/xserver/xauth-%1").arg(display);
    if (!m_x11Authority.writeXauthorityFile(xauthPath, display, m_x11Cookie)) {
        emit statusNotice(QStringLiteral("X11 授权写入失败：%1").arg(xauthPath));
        openShellChannel();
        return;
    }
    // 桥按 channel 到达时才连本地端点，无需等待 server 完全就绪
    m_x11Manager->startEmbedded(executablePath, xauthPath, display,
                                m_endpoint.x11ParentWindow,
                                m_endpoint.x11InitialSize);
    requestX11Forwarding();
    openShellChannel();
#else
    Q_UNUSED(executablePath);
#endif
}
```

`requestX11Forwarding()`（:351-375）改双源：

```cpp
void ZzSshTransportAdapter::requestX11Forwarding()
{
    if (!m_channel) {
        return;
    }
    ZzXLocalEndpoint local;
    QString cookie;
    if (m_x11Manager) {
        // 嵌入实验路径：会话自带 server 与 cookie
        local = m_x11Manager->localEndpoint();
        cookie = m_x11Cookie;
    } else if (m_x11Service && m_x11Service->isRunning()) {
        // 共享路径：应用级 server 与 cookie（M5 规格 §4.1）
        local = m_x11Service->localEndpoint();
        cookie = m_x11Service->cookie();
    } else {
        return;
    }
    // 应用侧 ZzXLocalEndpoint → 库侧 ZzSshX11Bridge::LocalEndpoint 字段映射
    ZzSshX11Bridge::LocalEndpoint endpoint;
    endpoint.host = local.host;
    endpoint.port = local.port;
    endpoint.localSocketPath = local.localSocketPath;
    m_x11Bridge = new ZzSshX11Bridge(m_conn, endpoint, this);
    connect(m_x11Bridge, &ZzSshX11Bridge::bridgeFailed, this,
            [this](quint32 /*channelId*/, int /*code*/, const QString &message) {
                emit statusNotice(QStringLiteral("X11 转发通道失败：%1").arg(message));
            });
    connect(m_channel, &ZzSshShellChannel::x11ForwardingReady, this, [this]() {
        emit statusNotice(QStringLiteral("X11 转发已启用"));
    });
    connect(m_channel, &ZzSshShellChannel::x11ForwardingFailed, this,
            [this](int /*code*/, const QString &message) {
                emit statusNotice(QStringLiteral("X11 转发被服务端拒绝：%1").arg(message));
            });
    m_channel->requestX11Forwarding(cookie);
}
```

`destroyX11()`（:377-389）：函数体不变，仅把 `m_x11Manager` 分支注释改为：

```cpp
    if (m_x11Manager) {
        // 仅嵌入路径存在会话级 manager；共享服务生命周期归 ZzAppShell，不在此触碰
        // stop 为异步收尾；deleteLater 后 QObject 树兜底回收（Windows 子进程随析构终止）
        m_x11Manager->stop();
        m_x11Manager->deleteLater();
        m_x11Manager = nullptr;
    }
```

- [ ] **步骤 4：运行测试验证通过**

运行：`cmake --build --preset linux-gcc-release && ctest --preset linux-gcc-release -R "ZzSshTransport" --output-on-failure`
预期：PASS（含既有 2 用例不回退）

- [ ] **步骤 5：全量回归 + 恢复 perf 记录**

运行：`ctest --preset linux-gcc-release` → 44 基线 + 新增全绿；随后 `git checkout -- tests/perf/records/` 并按当天日期前缀删除新生成的未跟踪 perf json。

- [ ] **步骤 6：Commit**

```bash
git add src/transport/ZzSshTransport.h src/transport/ZzSshTransport.cpp tests/unit/tst_ZzSshTransport.cpp
git commit -m "refactor(transport): 非嵌入 X11 会话改接应用级共享 server

M5 规格 §4.2 双路径拆分：
- startX11Forwarding：Windows 嵌入（x11ParentWindow!=0）走 M4b 原流程
  （抽为 startX11ForwardingEmbedded），其余一律经注入的 ZzX11Service
- 服务未就绪时一次性挂 serverStarted/startFailed 续接，失败不阻断会话
- requestX11Forwarding 改双源（嵌入=会话级 manager，共享=服务）
- 删除 Unix 每会话 manager/cookie 分支（预检与授权写入并入 ZzX11Service）
- destroyX11 明确不触碰共享服务生命周期"
```

---

### 任务 5：装配接线（ZzTabManager 注入 + ZzAppShell 持有与开关联动）

**文件：**
- 修改：`src/tab/ZzTabManager.h`、`src/tab/ZzTabManager.cpp:147-172`
- 修改：`src/ZzAppShell.h`、`src/ZzAppShell.cpp`（构造/析构/wireTabManager/访问器）
- 测试：`tests/unit/tst_ZzTabManager.cpp`、`tests/unit/tst_ZzAppShell.cpp`

**前提：** 任务 3、4 已完成。

- [ ] **步骤 1：编写失败的测试**

`tests/unit/tst_ZzTabManager.cpp` 追加（include `"x11/ZzX11Service.h"` 与 `"transport/ZzSshTransport.h"`；本文件已有 `openSession` 开标签的用例模式可参照）：

```cpp
    void x11ServiceInjectedIntoSshTransport()
    {
        // M5 装配契约：ZzTabManager 把共享门面注入每个 SSH 传输
        ZzX11Service service;
        ZzTabManager tabs;
        tabs.setX11Service(&service);

        ZzSessionProfile profile;
        profile.name = QStringLiteral("ssh-x11");
        profile.protocol = QStringLiteral("ssh");
        profile.host = QStringLiteral("127.0.0.1");
        profile.port = 1; // 不可达即可：只需传输实例被创建，不需真连接
        profile.userName = QStringLiteral("nobody");
        tabs.openSession(profile);

        ZzTerminalView *view = tabs.viewAt(0);
        QVERIFY(view);
        auto *ssh = qobject_cast<ZzSshTransport *>(view->transport());
        QVERIFY(ssh);
        QCOMPARE(ssh->x11Service(), &service);
        tabs.closeTab(0);
    }
```

（`ZzTerminalView::transport()` 访问器已存在于 `src/terminal/ZzTerminalView.h:35`，直接使用。）

`tests/unit/tst_ZzAppShell.cpp` 追加（构造模式照该文件已有用例，include `"x11/ZzX11Service.h"`）：

```cpp
    void x11ServiceFollowsGlobalSetting()
    {
        // M5 规格 §三决策 1：全局开关驱动共享服务启停
        const bool original = ZzAppSettings::instance().x11ServerEnabled();
        ZzAppShell shell; // 照本文件已有用例的构造参数
        QVERIFY(shell.x11Service());
        QCOMPARE(shell.x11Service()->isEnabled(), original);

        ZzAppSettings::instance().setX11ServerEnabled(false);
        QTRY_VERIFY(!shell.x11Service()->isEnabled());
        ZzAppSettings::instance().setX11ServerEnabled(true);
        QTRY_VERIFY(shell.x11Service()->isEnabled());

        ZzAppSettings::instance().setX11ServerEnabled(original); // 还原，免污染其他用例
    }
```

- [ ] **步骤 2：运行测试验证失败**

运行：`cmake --build --preset linux-gcc-release`
预期：编译失败（`setX11Service`/`x11Service()` 不存在）——此即失败验证

- [ ] **步骤 3：实现接线**

`src/tab/ZzTabManager.h`：前置声明 `class ZzX11Service;`；公开区追加：

```cpp
    /** @brief 注入应用级共享 X server 门面（ZzAppShell 装配；观察指针不拥有）。 */
    void setX11Service(ZzX11Service *service) { m_x11Service = service; }
```

私有成员追加：`ZzX11Service *m_x11Service = nullptr; ///< 观察指针：注入各 SSH 传输`

`src/tab/ZzTabManager.cpp`：`createTransport()` 的 SSH 装配块内（`ssh->setHostKeyConfirmer(...)` 之后）追加：

```cpp
        ssh->setX11Service(m_x11Service); // 共享 X server 门面（M5；nullptr 时传输内跳过 X11）
```

`src/ZzAppShell.h`：前置声明 `class ZzX11Service;`；公开访问器区追加：

```cpp
    [[nodiscard]] ZzX11Service *x11Service() const;
```

私有成员追加：`ZzX11Service *m_x11Service = nullptr; ///< 本对象为父：应用级共享 X server（M5）`

`src/ZzAppShell.cpp`：

include 区追加 `#include "x11/ZzX11Service.h"`。

构造函数尾部（凭据库创建之后）追加：

```cpp
    // 应用级共享 X server（M5 对齐 MobaXterm：启动即拉起、全局开关可关闭、
    // 关会话不杀；应用退出时随本对象析构，QProcess 析构终止 ZzXsrv）
    m_x11Service = new ZzX11Service(this);
    m_x11Service->setEnabled(ZzAppSettings::instance().x11ServerEnabled());
    connect(&ZzAppSettings::instance(), &ZzAppSettings::settingsChanged, this, [this] {
        m_x11Service->setEnabled(ZzAppSettings::instance().x11ServerEnabled());
    });
```

`wireTabManager()` 内（setHostKeyConfirmer 之后）追加：

```cpp
    // 共享 X server 门面注入各 SSH 传输；服务级异常上状态栏（无会话时也能感知）
    tabs->setX11Service(m_x11Service);
    connect(m_x11Service, &ZzX11Service::startFailed, this,
            &ZzAppShell::showStatusMessage);
    connect(m_x11Service, &ZzX11Service::serverCrashed, this,
            [this](const QString &message) {
                showStatusMessage(QStringLiteral("X server 异常退出：%1").arg(message));
            });
```

文件尾部访问器区追加：

```cpp
ZzX11Service *ZzAppShell::x11Service() const { return m_x11Service; }
```

- [ ] **步骤 4：运行测试验证通过**

运行：`cmake --build --preset linux-gcc-release && ctest --preset linux-gcc-release -R "ZzTabManager|ZzAppShell" --output-on-failure`
预期：PASS（含既有用例不回退）

- [ ] **步骤 5：全量回归 + 恢复 perf 记录**

运行：`ctest --preset linux-gcc-release` → 全绿；随后 `git checkout -- tests/perf/records/` 并按当天日期前缀删除新生成的未跟踪 perf json。

- [ ] **步骤 6：Commit**

```bash
git add src/tab/ZzTabManager.h src/tab/ZzTabManager.cpp src/ZzAppShell.h src/ZzAppShell.cpp tests/unit/tst_ZzTabManager.cpp tests/unit/tst_ZzAppShell.cpp
git commit -m "feat(x11): 装配共享 X server 门面（启动拉起+开关联动+注入传输）

M5 规格 §4.1/§4.3 接线：
- ZzAppShell 持有 ZzX11Service，构造时按全局设置拉起，settingsChanged 联动启停
- 服务级 startFailed/serverCrashed 上状态栏（无会话时用户可感知）
- ZzTabManager 把门面注入每个 SSH 传输（观察指针不拥有）
- 应用退出随析构终止 ZzXsrv（关会话不杀，退出应用才杀）"
```

---

### 任务 6：ZzXsrv——`-parent`/`-multiwindow` 互斥校验 + 快捷方式默认 multiwindow

**仓库：** `third_party/vcxsrv`（ZzXsrv 工作副本，remote `zzxsrv`；**绝不推 `origin`**）

**文件：**
- 修改：`xorg-server/hw/xwin/winvalargs.c:78-165`（屏幕循环内）
- 修改：`xorg-server/installer/zzxsrv-64.nsi:176`

- [ ] **步骤 1：互斥校验**

`winvalargs.c` 屏幕循环内、`-multiwindow`/`-rootless` 计数块（:84-104）之后追加（照既有 ErrorF 风格）：

```c
        /* Check for -parent and -multiwindow */
        if (g_hwndParent != NULL && g_ScreenInfo[i].fMultiWindow) {
            ErrorF("winValidateArgs - -parent is invalid with -multiwindow.\n");
            return FALSE;
        }
```

确认 `g_hwndParent` 可见：其声明在 `xorg-server/hw/xwin/winglobals.h:96-97`；若 `winvalargs.c` 头部未包含 `winglobals.h` 则补 `#include "winglobals.h"`。

依据（防回归注释写入 commit message）：调研发现 `-multiwindow -parent` 同给时，M4b 嵌入分支会把 multiwindow 的隐藏 screen 窗口 `SetParent` 进 Qt 容器，行为未定义。

- [ ] **步骤 2：快捷方式默认 `-multiwindow`**

`zzxsrv-64.nsi:176` 改为：

```nsis
  CreateShortCut "$SMPROGRAMS\ZzXsrv\ZzXsrv.lnk" "$INSTDIR\ZzXsrv.exe" "-multiwindow" "$INSTDIR\ZzXsrv.exe" 0
```

（M5 规格 §三决策 4：手动从开始菜单启动即进入受管窗口模式；exe 本体无参数缺省行为不变。）

- [ ] **步骤 3：验证**

本机无法构建（Windows/meson），验证 = 代码审查 + 推送后 CI：
- `git -C third_party/vcxsrv diff` 自查两处改动
- **经用户确认后**推送 `zzxsrv` remote，确认 CI build workflow 绿（含双模式冒烟）
- `gh run watch` 在本环境会 API 抖动早退，用轮询循环：`gh run list --limit 3` 观察状态；token 异常时先 `gh auth status`

- [ ] **步骤 4：Commit**

```bash
cd third_party/vcxsrv
git add xorg-server/hw/xwin/winvalargs.c xorg-server/installer/zzxsrv-64.nsi
git commit -m "feat: -parent 与 -multiwindow 互斥校验 + 快捷方式默认 multiwindow

- winvalargs.c：同给 -parent -multiwindow 时报错退出。调研发现该组合会让
  M4b 嵌入分支把 multiwindow 的隐藏 screen 窗口 SetParent 进 Qt 容器，行为未定义
- 安装包开始菜单快捷方式追加 -multiwindow 参数：手动启动即受管窗口模式
  （对齐 MobaXterm/VcXsrv 默认体验），exe 本体无参缺省行为不变"
```

---

### 任务 7：ZzXsrv release zz3 + 主仓下载常量升级 + 规格核销

**前提：** 任务 6 已推送且 CI 绿。

- [ ] **步骤 1：打 tag 出 release（需用户确认推送）**

```bash
cd third_party/vcxsrv
git tag zz-21.1.16.1-3
git push zzxsrv master --tags   # 先向用户说明并经确认
```

轮询 release workflow 至完成，确认四资产上传（含 `zzxsrv-64.21.1.16.1.installer.noadmin.exe` 及随附 .sha256）。

- [ ] **步骤 2：实测新资产 SHA256 与大小**

```bash
gh release download zz-21.1.16.1-3 --pattern "zzxsrv-64.21.1.16.1.installer.noadmin.exe" --dir /tmp/zz3-asset
sha256sum /tmp/zz3-asset/zzxsrv-64.21.1.16.1.installer.noadmin.exe
stat -c %s /tmp/zz3-asset/zzxsrv-64.21.1.16.1.installer.noadmin.exe
```

与 release 随附 .sha256 交叉核验一致后才继续（沿用 zz2 交叉核验流程）。

- [ ] **步骤 3：升级主仓下载常量**

`src/x11/ZzXServerDownloader.h`：
- `kUrl` 中 tag `zz-21.1.16.1-2` → `zz-21.1.16.1-3`（资产文件名 `zzxsrv-64.21.1.16.1.installer.noadmin.exe` 不含 tag 后缀，不变）
- `kExpectedSha256` 替换为步骤 2 实测值；注释中字节数同步更新
- 文件头部注释（:36 附近"2026-08-22 实际下载后本地 sha256sum"与 :70-71）日期更新为实测日期

- [ ] **步骤 4：主仓回归**

运行：`cmake --build --preset linux-gcc-release && ctest --preset linux-gcc-release`（`tst_ZzXServerDownloader` 等全绿）；恢复 perf 记录（`git checkout -- tests/perf/records/` + 按当天日期前缀清理）。

- [ ] **步骤 5：规格核销与文档更新**

- `docs/superpowers/specs/2026-08-25-x11-m5-mobaxterm-parity-design.md`：状态行改为"已实现（自动化部分全部核销）"；§十一完成定义 1-3 勾选；第 4 条人工验收保持挂起并注明待用户时间
- `docs/superpowers/specs/2026-08-24-x11-m4b-zzxsrv-embed-design.md` §2.2 手动启动条目：标注"已由 M5b（任务 6）解决：快捷方式默认 -multiwindow"

- [ ] **步骤 6：Commit**

```bash
git add src/x11/ZzXServerDownloader.h docs/superpowers/specs/2026-08-25-x11-m5-mobaxterm-parity-design.md docs/superpowers/specs/2026-08-24-x11-m4b-zzxsrv-embed-design.md
git commit -m "feat(x11): ZzXsrv 下载常量升级 zz3 + M5 规格核销

- kUrl tag → zz-21.1.16.1-3，kExpectedSha256/字节数更新为实测值
  （与 release 随附 .sha256 交叉核验一致）
- zz3 内容：-parent/-multiwindow 互斥校验 + 快捷方式默认 -multiwindow
- M5 规格自动化条目核销；人工验收（§八清单）挂起待用户时间
- M4b 规格 §2.2 手动启动待办标注已由 M5b 解决"
```

---

## 自检记录（计划作者已完成）

- **规格覆盖度：** §三决策 1→任务 2/3/5；决策 2/3→任务 1；决策 4→任务 6；§4.1→任务 3/5；§4.2 互斥→任务 4；§4.3 状态机→任务 3/5；§五主仓清单→任务 1-5；§六 ZzXsrv 清单→任务 6/7；§七错误处理→任务 3/4；§八测试→各任务测试步骤；§九兼容→任务 1 测试；§十一完成定义→任务 7。无遗漏。
- **类型一致性：** `ZzX11Service::setEnabled/isEnabled/start/stop/ensureRunning/isRunning/display/cookie/localEndpoint/setServerProgramForTesting` 在任务 3 定义，任务 4/5 使用一致；`ZzAppSettings::x11ServerEnabled/setX11ServerEnabled` 任务 2 定义，任务 5 使用一致；`ZzSshTransport::setX11Service/x11Service` 任务 4 定义，任务 5 使用一致。
- **已知留白（执行时确认，非占位符）：** 任务 1/2/5 中各测试文件的构造样板（临时 INI、ZzCredentialStore 传参等）以各测试文件既有用例为准。
