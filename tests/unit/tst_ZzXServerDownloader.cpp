#include <QtTest/QtTest>
#include <QCryptographicHash>
#include <QDir>
#include <QFile>
#include <QNetworkAccessManager>
#include <QTcpServer>
#include <QTcpSocket>
#include <QTemporaryDir>

#include "x11/ZzXServerDownloader.h"

namespace {

/**
 * @brief 极简 HTTP 桩服务器：每个连接回复预设状态码与包体，并记录请求数。
 */
class ZzStubHttpServer : public QTcpServer
{
    Q_OBJECT
public:
    int status = 200;          ///< 回复的 HTTP 状态码
    QByteArray body;           ///< 回复包体
    int requestCount = 0;      ///< 已服务的请求数

    bool start()
    {
        return listen(QHostAddress::LocalHost, 0);
    }

    QUrl url() const
    {
        return QUrl(QStringLiteral("http://127.0.0.1:%1/vcxsrv-installer").arg(serverPort()));
    }

protected:
    void incomingConnection(qintptr socketDescriptor) override
    {
        auto *sock = new QTcpSocket(this);
        sock->setSocketDescriptor(socketDescriptor);
        connect(sock, &QTcpSocket::readyRead, this, [this, sock] {
            QByteArray req = sock->property("req").toByteArray();
            req += sock->readAll();
            if (!req.contains("\r\n\r\n")) {
                sock->setProperty("req", req); // 请求头未收全，等待下一段
                return;
            }
            ++requestCount;
            const QByteArray reason = status == 200 ? "OK" : "Not Found";
            sock->write("HTTP/1.1 " + QByteArray::number(status) + " " + reason + "\r\n"
                        "Content-Length: " + QByteArray::number(body.size()) + "\r\n"
                        "Connection: close\r\n\r\n");
            sock->write(body);
            sock->flush();
            sock->disconnectFromHost();
        });
    }
};

/**
 * @brief 生成桩 NSIS 安装包：记录参数到 argsPath，并在 /D= 目录落 vcxsrv.exe。
 */
QByteArray makeStubInstaller(const QString &argsPath)
{
    QByteArray script;
    script += "#!/bin/sh\n";
    script += "printf '%s\\n' \"$@\" > '" + argsPath.toUtf8() + "'\n";
    script += "for a in \"$@\"; do case \"$a\" in /D=*) d=\"${a#/D=}\";; esac; done\n";
    script += "mkdir -p \"$d\"\n";
    script += "touch \"$d/vcxsrv.exe\"\n";
    return script;
}

QString sha256Hex(const QByteArray &data)
{
    return QString::fromLatin1(QCryptographicHash::hash(data, QCryptographicHash::Sha256).toHex());
}

} // namespace

/**
 * @brief ZzXServerDownloader 单元测试：下载、SHA256 校验、静默安装参数、幂等与平台分支。
 */
class tst_ZzXServerDownloader : public QObject
{
    Q_OBJECT
private:
    QTemporaryDir m_dir;   ///< 每用例独立临时目录
    QString m_root;        ///< 安装根目录覆盖
    QString m_argsFile;    ///< 桩安装包参数记录文件

private slots:
    void init()
    {
        QVERIFY(m_dir.isValid());
        // QtTest 全用例共享同一测试对象实例，须逐用例清理安装根，防串扰
        QDir(m_dir.filePath(QStringLiteral("xserver"))).removeRecursively();
        QFile::remove(m_dir.filePath(QStringLiteral("installer-args.txt")));
        m_root = m_dir.filePath(QStringLiteral("xserver"));
        m_argsFile = m_dir.filePath(QStringLiteral("installer-args.txt"));
    }

    /** @brief 桩返回合法包体 → 校验通过 → 以 /S /D= 正确参数发起静默安装 → ready。 */
    void downloadsAndInstallsWithCorrectArgs()
    {
        const QByteArray pkg = makeStubInstaller(m_argsFile);
        ZzStubHttpServer server;
        server.body = pkg;
        QVERIFY(server.start());

        QNetworkAccessManager nam;
        ZzXServerDownloader dl;
        dl.setSimulateWindows(true);
        dl.setNetworkAccessManager(&nam);
        dl.setInstallRoot(m_root);
        dl.setReleaseSource(server.url(), sha256Hex(pkg));

        QSignalSpy readySpy(&dl, &ZzXServerDownloader::ready);
        QSignalSpy failSpy(&dl, &ZzXServerDownloader::downloadFailed);
        QSignalSpy progSpy(&dl, &ZzXServerDownloader::progressChanged);
        dl.ensureAvailable();

        QVERIFY(readySpy.wait(15000) || failSpy.count() > 0);
        QVERIFY2(failSpy.count() == 0,
                 failSpy.count() ? qPrintable(failSpy.first().at(0).toString()) : "ready 超时");
        QCOMPARE(server.requestCount, 1);

        const QString exe = m_root + QStringLiteral("/vcxsrv.exe");
        QCOMPARE(readySpy.takeFirst().at(0).toString(), exe);
        QVERIFY(QFile::exists(exe));

        // 静默安装必须以 /S /D=<根目录> 参数发起
        QFile af(m_argsFile);
        QVERIFY(af.open(QIODevice::ReadOnly));
        QCOMPARE(QString::fromUtf8(af.readAll()),
                 QStringLiteral("/S\n/D=%1\n").arg(m_root));

        // 版本标记幂等：VERSION 写入官方版本号
        QCOMPARE(dl.installedVersion(), QString::fromLatin1(ZzXServerRelease::kVersion));
        QCOMPARE(dl.serverExecutablePath(), exe);

        // 进度信号最终到达 100%
        QVERIFY(progSpy.count() >= 1);
        QCOMPARE(progSpy.last().at(0).toInt(), 100);
    }

    /** @brief 篡改一字节 → downloadFailed，安装目录不落盘。 */
    void rejectsOnSha256Mismatch()
    {
        const QByteArray pkg = makeStubInstaller(m_argsFile);
        ZzStubHttpServer server;
        server.body = pkg;
        QVERIFY(server.start());

        QNetworkAccessManager nam;
        ZzXServerDownloader dl;
        dl.setSimulateWindows(true);
        dl.setNetworkAccessManager(&nam);
        dl.setInstallRoot(m_root);
        // 期望哈希故意给官方常量（与桩包体不符），等价于「下载内容被篡改」
        dl.setReleaseSource(server.url(), QString::fromLatin1(ZzXServerRelease::kSha256));

        QSignalSpy readySpy(&dl, &ZzXServerDownloader::ready);
        QSignalSpy failSpy(&dl, &ZzXServerDownloader::downloadFailed);
        dl.ensureAvailable();

        QVERIFY(failSpy.wait(15000));
        QCOMPARE(readySpy.count(), 0);
        QVERIFY(failSpy.first().at(0).toString().contains(QStringLiteral("SHA256")));
        QVERIFY(!QDir(m_root).exists());          // 半成品已清理
        QVERIFY(!QFile::exists(m_argsFile));      // 安装从未发起
    }

    /** @brief 404 → downloadFailed 含状态码。 */
    void rejectsOnHttpError()
    {
        ZzStubHttpServer server;
        server.status = 404;
        server.body = "not found";
        QVERIFY(server.start());

        QNetworkAccessManager nam;
        ZzXServerDownloader dl;
        dl.setSimulateWindows(true);
        dl.setNetworkAccessManager(&nam);
        dl.setInstallRoot(m_root);
        dl.setReleaseSource(server.url(), sha256Hex(server.body));

        QSignalSpy readySpy(&dl, &ZzXServerDownloader::ready);
        QSignalSpy failSpy(&dl, &ZzXServerDownloader::downloadFailed);
        dl.ensureAvailable();

        QVERIFY(failSpy.wait(15000));
        QCOMPARE(readySpy.count(), 0);
        QVERIFY(failSpy.first().at(0).toString().contains(QStringLiteral("404")));
        QVERIFY(!QDir(m_root).exists());
    }

    /** @brief 本地已有同版本且可执行文件存在 → 直接 ready 不发请求。 */
    void resumesExistingInstall()
    {
        QVERIFY(QDir().mkpath(m_root));
        QFile vf(m_root + QStringLiteral("/VERSION"));
        QVERIFY(vf.open(QIODevice::WriteOnly));
        vf.write(QByteArray(ZzXServerRelease::kVersion) + '\n');
        vf.close();
        QFile ef(m_root + QStringLiteral("/vcxsrv.exe"));
        QVERIFY(ef.open(QIODevice::WriteOnly));
        ef.write("pe");
        ef.close();

        ZzStubHttpServer server;
        QVERIFY(server.start());
        QNetworkAccessManager nam;
        ZzXServerDownloader dl;
        dl.setSimulateWindows(true);
        dl.setNetworkAccessManager(&nam);
        dl.setInstallRoot(m_root);
        dl.setReleaseSource(server.url(), sha256Hex("irrelevant"));

        QSignalSpy readySpy(&dl, &ZzXServerDownloader::ready);
        dl.ensureAvailable();

        QCOMPARE(readySpy.count(), 1); // 同步直发，无需等待
        QCOMPARE(readySpy.first().at(0).toString(), m_root + QStringLiteral("/vcxsrv.exe"));
        QCOMPARE(server.requestCount, 0);
    }

    /** @brief 非 Windows 平台编译期分支：ensureAvailable 直接 ready(QString())。 */
    void nonWindowsEmitsReadyEmpty()
    {
#ifdef Q_OS_WIN
        QSKIP("本用例仅验证非 Windows 编译期直通分支");
#else
        ZzStubHttpServer server;
        QVERIFY(server.start());
        QNetworkAccessManager nam;
        ZzXServerDownloader dl; // 不开 simulate：走平台直通分支
        dl.setNetworkAccessManager(&nam);
        dl.setInstallRoot(m_root);
        dl.setReleaseSource(server.url(), sha256Hex("irrelevant"));

        QSignalSpy readySpy(&dl, &ZzXServerDownloader::ready);
        QSignalSpy failSpy(&dl, &ZzXServerDownloader::downloadFailed);
        dl.ensureAvailable();

        QCOMPARE(readySpy.count(), 1);
        QCOMPARE(readySpy.first().at(0).toString(), QString());
        QCOMPARE(failSpy.count(), 0);
        QVERIFY(dl.serverExecutablePath().isEmpty());
        QCOMPARE(server.requestCount, 0);
#endif
    }
};

QTEST_MAIN(tst_ZzXServerDownloader)
#include "tst_ZzXServerDownloader.moc"
