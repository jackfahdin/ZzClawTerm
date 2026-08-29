#include <QtTest/QtTest>
#include <QDir>
#include <QFile>
#include <QProcess>
#include <QRegularExpression>
#include <QStandardPaths>

#include "x11/ZzXAuthority.h"

/**
 * @brief ZzXAuthority 单元测试：cookie 生成、xauthority 二进制格式、文件权限。
 */
class tst_ZzXAuthority : public QObject
{
    Q_OBJECT
private slots:
    /** @brief generateCookie() 两次结果不同、纯小写 hex、长度 32。 */
    void cookieIs32HexChars()
    {
        ZzXAuthority auth;
        const QString c1 = auth.generateCookie();
        const QString c2 = auth.generateCookie();
        QCOMPARE(c1.size(), 32);
        QCOMPARE(c2.size(), 32);
        QVERIFY(c1 != c2);
        const QRegularExpression hex(QStringLiteral("^[0-9a-f]{32}$"));
        QVERIFY(hex.match(c1).hasMatch());
        QVERIFY(hex.match(c2).hasMatch());
    }

    /** @brief 写入临时文件后系统 xauth 能读回（无 xauth 时 QSKIP）。 */
    void writtenFileAcceptedByXauth()
    {
        if (QStandardPaths::findExecutable(QStringLiteral("xauth")).isEmpty())
            QSKIP("系统无 xauth，跳过二进制格式回读验证");
        ZzXAuthority auth;
        const QString cookie = auth.generateCookie();
        const QString path = QDir::temp().filePath(QStringLiteral("zzxauth-test"));
        QVERIFY(auth.writeXauthorityFile(path, 99, cookie));
        QProcess p;
        p.start(QStringLiteral("xauth"), {QStringLiteral("-f"), path, QStringLiteral("list")});
        QVERIFY(p.waitForFinished(5000));
        const QString out = QString::fromUtf8(p.readAllStandardOutput());
        QVERIFY(out.contains(QStringLiteral("MIT-MAGIC-COOKIE-1")));
        QVERIFY(out.contains(cookie));
        QFile::remove(path);
    }

    /** @brief 写出的授权文件权限必须为 0600（仅属主可读写）。 */
    void filePermissions0600()
    {
#ifdef Q_OS_WIN
        // Windows 无 POSIX 权限位：QFile::permissions 恒返回全量权限，
        // 0600 语义不可验证（xauthority 文件内容格式仍由上一个用例覆盖）
        QSKIP("POSIX 0600 权限语义在 Windows 不存在");
#endif
        ZzXAuthority auth;
        const QString path = QDir::temp().filePath(QStringLiteral("zzxauth-perm-test"));
        QVERIFY(auth.writeXauthorityFile(path, 1, auth.generateCookie()));
        const auto perms = QFile::permissions(path);
        // POSIX 0600 在 Qt 中同时置位 Owner 与 User 两组属主位
        QCOMPARE(perms.toInt(),
                 QFileDevice::Permissions(QFileDevice::ReadOwner | QFileDevice::WriteOwner |
                                          QFileDevice::ReadUser | QFileDevice::WriteUser).toInt());
        QFile::remove(path);
    }
};

QTEST_MAIN(tst_ZzXAuthority)
#include "tst_ZzXAuthority.moc"
