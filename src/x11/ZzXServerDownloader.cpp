#include "ZzXServerDownloader.h"

#include <QCryptographicHash>
#include <QDir>
#include <QFile>
#include <QNetworkAccessManager>
#include <QNetworkReply>
#include <QNetworkRequest>
#include <QProcess>
#include <QSaveFile>
#include <QStandardPaths>
#include <QTimer>
#include <QUuid>

namespace {

/// 安装根目录下的 X server 可执行文件名（NSIS 直接装到 INSTDIR 根）。
const QString kExeName = QStringLiteral("vcxsrv.exe");

/// 版本标记文件名（内容为官方版本号，供幂等判断）。
const QString kVersionFileName = QStringLiteral("VERSION");

/// 静默安装最长等待时间（5 分钟），超时按失败处理。
constexpr int kInstallTimeoutMs = 5 * 60 * 1000;

} // namespace

ZzXServerDownloader::ZzXServerDownloader(QObject *parent)
    : QObject(parent)
{
}

ZzXServerDownloader::~ZzXServerDownloader()
{
    if (m_reply) {
        m_reply->disconnect(this);
        m_reply->abort();
        m_reply->deleteLater();
    }
    if (m_installer) {
        m_installer->disconnect(this);
        m_installer->kill();
        m_installer->deleteLater();
    }
    removeTempFile();
    delete m_hash;
}

void ZzXServerDownloader::removeTempFile()
{
    if (!m_tempFile)
        return;
    const QString path = m_tempFile->fileName();
    delete m_tempFile;
    m_tempFile = nullptr;
    if (!path.isEmpty())
        QFile::remove(path);
}

QString ZzXServerDownloader::installedVersion() const
{
    QFile f(QDir(installRoot()).filePath(kVersionFileName));
    if (!f.open(QIODevice::ReadOnly))
        return QString();
    return QString::fromUtf8(f.readAll()).trimmed();
}

void ZzXServerDownloader::ensureAvailable()
{
    if (!windowsFlowEnabled()) {
        // 非 Windows 平台编译期直通：X server 由系统提供，无需下载
        emit ready(QString());
        return;
    }
    if (m_running)
        return; // 下载/安装进行中，忽略重复调用
    if (installedVersion() == QString::fromLatin1(ZzXServerRelease::kVersion)
        && QFile::exists(serverExecutablePath())) {
        emit ready(serverExecutablePath()); // 幂等直通：已装同版本，不发请求
        return;
    }
    startDownload();
}

QString ZzXServerDownloader::serverExecutablePath() const
{
    if (!windowsFlowEnabled())
        return QString();
    return QDir(installRoot()).filePath(kExeName);
}

void ZzXServerDownloader::setNetworkAccessManager(QNetworkAccessManager *nam)
{
    m_nam = nam;
}

void ZzXServerDownloader::setInstallRoot(const QString &root)
{
    m_installRoot = root;
}

void ZzXServerDownloader::setReleaseSource(const QUrl &url, const QString &sha256Hex)
{
    m_url = url;
    m_expectedSha256 = sha256Hex;
}

void ZzXServerDownloader::setSimulateWindows(bool on)
{
    m_simulateWindows = on;
}

bool ZzXServerDownloader::windowsFlowEnabled() const
{
#ifdef Q_OS_WIN
    return true;
#else
    return m_simulateWindows;
#endif
}

QString ZzXServerDownloader::installRoot() const
{
    if (!m_installRoot.isEmpty())
        return m_installRoot;
    return QStandardPaths::writableLocation(QStandardPaths::AppDataLocation)
           + QStringLiteral("/xserver");
}

QString ZzXServerDownloader::stagingRoot() const
{
    // 静默安装先落到安装根旁的 staging 目录，成功后才换入，失败只清它
    return installRoot() + QStringLiteral(".staging");
}

QString ZzXServerDownloader::backupRoot() const
{
    // swap 期间旧版安装的临时名，替换成功后删除
    return installRoot() + QStringLiteral(".old");
}

void ZzXServerDownloader::startDownload()
{
    if (!m_nam)
        m_nam = new QNetworkAccessManager(this);
    m_running = true;

    if (!m_hash)
        m_hash = new QCryptographicHash(QCryptographicHash::Sha256);
    m_hash->reset();

    delete m_tempFile;
    // 不用 QTemporaryFile：Qt 6.11 用 O_TMPFILE 懒物化，close() 后写 fd 仍持有，
    // 导致 QProcess execve 该文件返回 ETXTBSY（Text file busy）；普通 QFile 无此问题
    m_tempFile = new QFile(QDir::temp().filePath(QStringLiteral("zz-vcxsrv-%1.installer")
        .arg(QUuid::createUuid().toString(QUuid::WithoutBraces))));
    if (!m_tempFile->open(QIODevice::WriteOnly)) {
        fail(tr("无法创建下载临时文件：%1").arg(m_tempFile->errorString()));
        return;
    }

    QNetworkRequest req(m_url);
    // GitHub releases 下载会 302 到 CDN，需允许 https→https 重定向
    req.setAttribute(QNetworkRequest::RedirectPolicyAttribute,
                     QNetworkRequest::NoLessSafeRedirectPolicy);
    m_reply = m_nam->get(req);
    connect(m_reply, &QNetworkReply::readyRead, this, [this] {
        const QByteArray chunk = m_reply->readAll();
        m_hash->addData(chunk); // 边下边算，免二次读盘
        m_tempFile->write(chunk);
    });
    connect(m_reply, &QNetworkReply::downloadProgress, this,
            [this](qint64 received, qint64 total) {
        if (total > 0)
            emit progressChanged(int(received * 100 / total));
    });
    connect(m_reply, &QNetworkReply::finished, this,
            &ZzXServerDownloader::handleReplyFinished);
}

void ZzXServerDownloader::handleReplyFinished()
{
    const QNetworkReply::NetworkError err = m_reply->error();
    const QString errString = m_reply->errorString();
    const int status = m_reply->attribute(QNetworkRequest::HttpStatusCodeAttribute).toInt();
    m_reply->deleteLater();
    m_reply = nullptr;

    if (err != QNetworkReply::NoError) {
        fail(status > 0 ? tr("安装包下载失败（HTTP %1）").arg(status)
                        : tr("安装包下载失败：%1").arg(errString));
        return;
    }

    m_tempFile->flush();
    m_tempFile->close(); // 执行前关闭写句柄；临时文件由 removeTempFile() 清理
    const QString actual = QString::fromLatin1(m_hash->result().toHex());
    if (actual.compare(m_expectedSha256, Qt::CaseInsensitive) != 0) {
        fail(tr("SHA256 校验失败：期望 %1，实际 %2").arg(m_expectedSha256, actual));
        return;
    }
    startInstall();
}

void ZzXServerDownloader::startInstall()
{
    // 安装目标是 staging 目录而非安装根本身：全程不触碰既有可用旧版，
    // 成功核验后才换入（见 handleInstallFinished）
    const QString staging = stagingRoot();
    QDir(staging).removeRecursively(); // 清掉上次可能残留的 staging
    QDir().mkpath(staging);

    m_installer = new QProcess(this);
    // 超时保护：安装包损坏卡死时不至于永久悬挂
    auto *timeout = new QTimer(m_installer);
    timeout->setSingleShot(true);
    connect(timeout, &QTimer::timeout, this, [this] {
        m_installer->kill();
        fail(tr("静默安装超时（%1 分钟）").arg(kInstallTimeoutMs / 60000));
    });
    connect(m_installer, &QProcess::errorOccurred, this, [this](QProcess::ProcessError e) {
        if (e == QProcess::FailedToStart)
            fail(tr("安装程序启动失败：%1").arg(m_installer->errorString()));
    });
    connect(m_installer, qOverload<int, QProcess::ExitStatus>(&QProcess::finished), this,
            [this](int code, QProcess::ExitStatus st) {
        handleInstallFinished(code, int(st));
    });
    timeout->start(kInstallTimeoutMs);

#ifndef Q_OS_WIN
    // 测试钩子路径：桩安装包为脚本，需补可执行权限
    QFile::setPermissions(m_tempFile->fileName(),
                          QFile::permissions(m_tempFile->fileName()) |
                              QFileDevice::ExeOwner | QFileDevice::ExeUser);
    m_installer->start(m_tempFile->fileName(),
                       {QStringLiteral("/S"), QStringLiteral("/D=%1").arg(staging)});
#else
    // NSIS 要求 /D= 为命令行最后一个参数且即使含空格也不加引号，
    // QStringList 参数会被 QProcess 自动加引号，故用原始命令行拼接
    m_installer->setNativeArguments(QStringLiteral("/S /D=%1").arg(staging));
    m_installer->start(m_tempFile->fileName());
#endif
}

void ZzXServerDownloader::handleInstallFinished(int exitCode, int exitStatus)
{
    if (!m_running)
        return; // 已被超时/错误路径终结
    if (QProcess::ExitStatus(exitStatus) != QProcess::NormalExit || exitCode != 0) {
        fail(tr("静默安装失败（exit=%1）").arg(exitCode));
        return;
    }
    // 安装成功：先在 staging 内核验可执行文件落盘、写版本标记
    const QString staging = stagingRoot();
    if (!QFile::exists(QDir(staging).filePath(kExeName))) {
        fail(tr("安装完成但未找到 %1").arg(QDir(staging).filePath(kExeName)));
        return;
    }
    QSaveFile versionFile(QDir(staging).filePath(kVersionFileName));
    if (!versionFile.open(QIODevice::WriteOnly)
        || versionFile.write(QByteArray(ZzXServerRelease::kVersion) + '\n') < 0
        || !versionFile.commit()) {
        fail(tr("版本标记写入失败：%1").arg(versionFile.errorString()));
        return;
    }
    // staging 核验通过，换入安装根：旧版改名 .old → staging 上位 → 删 .old。
    // rename 同卷为原子操作；staging 上位失败时回滚旧版，保证根目录始终可用
    const QString root = installRoot();
    const QString backup = backupRoot();
    QDir(backup).removeRecursively();
    if (QDir(root).exists() && !QDir().rename(root, backup)) {
        fail(tr("旧版安装改名失败，无法换入新版本：%1").arg(root));
        return;
    }
    if (!QDir().rename(staging, root)) {
        if (QDir(backup).exists())
            QDir().rename(backup, root); // 回滚旧版
        fail(tr("新版本换入安装根失败：%1").arg(root));
        return;
    }
    QDir(backup).removeRecursively();
    m_installer->deleteLater();
    m_installer = nullptr;
    m_running = false;
    removeTempFile();
    emit progressChanged(100);
    emit ready(serverExecutablePath());
}

void ZzXServerDownloader::fail(const QString &message)
{
    if (!m_running)
        return; // 防超时/错误/完成信号重复触发
    m_running = false;
    if (m_reply) {
        m_reply->disconnect(this);
        m_reply->abort();
        m_reply->deleteLater();
        m_reply = nullptr;
    }
    if (m_installer) {
        m_installer->disconnect(this);
        m_installer->kill();
        m_installer->deleteLater();
        m_installer = nullptr;
    }
    removeTempFile(); // 清理临时安装包
    // 只清本次流程的半成品（staging 目录）；既有可用旧版安装必须原样保留
    QDir(stagingRoot()).removeRecursively();
    emit downloadFailed(message);
}
