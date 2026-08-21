#include "ZzCredentialStore.h"

#include <QFile>
#include <QFileInfo>

#include "ZzAesFileCredentialBackend.h"
#include "ZzKeyringCredentialBackend.h"

ZzCredentialStore::ZzCredentialStore(const QString &filePath, QObject *parent)
    : ZzCredentialStore(BackendMode::AesFile, filePath, parent)
{
}

ZzCredentialStore::ZzCredentialStore(BackendMode mode, const QString &filePath,
                                     QObject *parent)
    : ZzCredentialBackend(parent)
    , m_filePath(filePath)
{
    // Auto：系统密钥环可用则用，否则回退 AES 文件（优雅降级，后端报告不可用）
    if (mode == BackendMode::Auto) {
        mode = ZzKeyringCredentialBackend::probeAvailability()
            ? BackendMode::SystemKeyring
            : BackendMode::AesFile;
    }
    m_activeMode = mode;
    if (m_activeMode == BackendMode::SystemKeyring) {
        m_backend = new ZzKeyringCredentialBackend(this);
    } else {
        m_backend = new ZzAesFileCredentialBackend(m_filePath, this);
    }
}

ZzCredentialStore::~ZzCredentialStore()
{
    lock();
}

QString ZzCredentialStore::defaultFilePath()
{
    return ZzAesFileCredentialBackend::defaultFilePath();
}

QString ZzCredentialStore::backendModeToString(BackendMode mode)
{
    switch (mode) {
    case BackendMode::AesFile:       return QStringLiteral("aes-file");
    case BackendMode::SystemKeyring: return QStringLiteral("system-keyring");
    case BackendMode::Auto:          return QStringLiteral("auto");
    }
    return QStringLiteral("auto");
}

ZzCredentialStore::BackendMode ZzCredentialStore::backendModeFromString(const QString &text)
{
    if (text == QLatin1String("aes-file"))
        return BackendMode::AesFile;
    if (text == QLatin1String("system-keyring"))
        return BackendMode::SystemKeyring;
    return BackendMode::Auto;
}

ZzCredentialStore::BackendMode ZzCredentialStore::activeBackendMode() const
{
    return m_activeMode;
}

bool ZzCredentialStore::migrateLegacyAesFileToKeyring(const QString &masterPassword)
{
    m_errorString.clear();
    if (m_activeMode != BackendMode::SystemKeyring) {
        m_errorString = QStringLiteral("当前后端不是系统密钥环，无迁移目标");
        return false;
    }
    if (!QFileInfo::exists(m_filePath)) {
        m_errorString = QStringLiteral("旧凭据文件不存在，无可迁移数据");
        return false; // 幂等：无旧文件
    }
    if (!isUnlocked()) {
        m_errorString = QStringLiteral("凭据存储未解锁");
        return false; // 目标后端未解锁
    }

    ZzAesFileCredentialBackend legacy(m_filePath);
    if (!legacy.unlock(masterPassword)) {
        m_errorString = legacy.errorString(); // 主密码错误或文件损坏
        return false;
    }
    const QList<ZzCredentialEntry> entries = legacy.allCredentials();
    for (const ZzCredentialEntry &entry : entries) {
        // 保留原 id：ZzSessionProfile 的 credentialId 引用迁移后仍有效
        if (!m_backend->putCredential(entry.id, entry.name, entry.secret)) {
            m_errorString = m_backend->errorString();
            return false;
        }
    }
    legacy.lock();

    // 旧文件改名留档而非删除（至少不丢数据）；改名失败不影响迁移结果
    const QString backupPath = m_filePath + QStringLiteral(".migrated");
    QFile::remove(backupPath);
    QFile::rename(m_filePath, backupPath);
    return true;
}

QString ZzCredentialStore::backendId() const
{
    return m_backend->backendId();
}

bool ZzCredentialStore::isAvailable() const
{
    return m_backend->isAvailable();
}

bool ZzCredentialStore::requiresMasterPassword() const
{
    return m_backend->requiresMasterPassword();
}

bool ZzCredentialStore::hasMasterPassword() const
{
    m_errorString.clear();
    return m_backend->hasMasterPassword();
}

bool ZzCredentialStore::initialize(const QString &masterPassword)
{
    m_errorString.clear();
    return m_backend->initialize(masterPassword);
}

bool ZzCredentialStore::unlock(const QString &masterPassword)
{
    m_errorString.clear();
    return m_backend->unlock(masterPassword);
}

void ZzCredentialStore::lock()
{
    m_errorString.clear();
    m_backend->lock();
}

bool ZzCredentialStore::isUnlocked() const
{
    return m_backend->isUnlocked();
}

QUuid ZzCredentialStore::addCredential(const QString &name, const QString &secret)
{
    m_errorString.clear();
    return m_backend->addCredential(name, secret);
}

bool ZzCredentialStore::putCredential(const QUuid &credentialId, const QString &name,
                                      const QString &secret)
{
    m_errorString.clear();
    return m_backend->putCredential(credentialId, name, secret);
}

bool ZzCredentialStore::updateCredential(const QUuid &credentialId, const QString &secret)
{
    m_errorString.clear();
    return m_backend->updateCredential(credentialId, secret);
}

std::optional<QString> ZzCredentialStore::credential(const QUuid &credentialId) const
{
    m_errorString.clear();
    return m_backend->credential(credentialId);
}

bool ZzCredentialStore::removeCredential(const QUuid &credentialId)
{
    m_errorString.clear();
    return m_backend->removeCredential(credentialId);
}

QList<ZzCredentialEntry> ZzCredentialStore::allCredentials() const
{
    m_errorString.clear();
    return m_backend->allCredentials();
}

QString ZzCredentialStore::errorString() const
{
    return m_errorString.isEmpty() ? m_backend->errorString() : m_errorString;
}
