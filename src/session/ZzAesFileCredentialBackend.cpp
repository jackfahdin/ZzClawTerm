#include "ZzAesFileCredentialBackend.h"

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
constexpr quint32 kMaxKdfIterations = 10000000; // 文件头迭代次数上限，防恶意文件 DoS
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

ZzAesFileCredentialBackend::ZzAesFileCredentialBackend(const QString &filePath, QObject *parent)
    : ZzCredentialBackend(parent)
    , m_filePath(filePath)
{
}

ZzAesFileCredentialBackend::~ZzAesFileCredentialBackend()
{
    lock();
}

QString ZzAesFileCredentialBackend::defaultFilePath()
{
    const QString dir = QStandardPaths::writableLocation(QStandardPaths::AppConfigLocation);
    return dir + QStringLiteral("/credentials.dat");
}

QString ZzAesFileCredentialBackend::backendId() const
{
    return QStringLiteral("aes-file");
}

bool ZzAesFileCredentialBackend::isAvailable() const
{
    return true; // 本地文件后端无外部依赖
}

bool ZzAesFileCredentialBackend::requiresMasterPassword() const
{
    return true;
}

bool ZzAesFileCredentialBackend::hasMasterPassword() const
{
    return QFileInfo::exists(m_filePath);
}

bool ZzAesFileCredentialBackend::initialize(const QString &masterPassword)
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
        m_key.clear(); // deriveKey 失败可能已分配非空缓冲区，不能让 isUnlocked() 误报 true
        return false;
    }

    m_credentials.clear();
    if (!persist()) {
        lock(); // 落盘失败不能留下驻留密钥
        return false;
    }
    return true;
}

bool ZzAesFileCredentialBackend::unlock(const QString &masterPassword)
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
    if (iterations < 1 || iterations > kMaxKdfIterations) {
        m_errorString = QStringLiteral("凭据文件格式非法");
        return false;
    }
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

    QList<ZzCredentialEntry> loaded;
    const QJsonArray array = root.value(kCredentialsKey).toArray();
    loaded.reserve(array.size());
    for (const QJsonValue &value : array) {
        const QJsonObject obj = value.toObject();
        loaded.append(ZzCredentialEntry{QUuid::fromString(obj.value(kIdKey).toString()),
                                        obj.value(kNameKey).toString(),
                                        obj.value(kSecretKey).toString()});
    }

    m_key = key;
    m_salt = salt;
    m_kdfIterations = iterations;
    m_credentials = loaded;
    return true;
}

void ZzAesFileCredentialBackend::lock()
{
    if (!m_key.isEmpty()) {
        OPENSSL_cleanse(m_key.data(), m_key.size());
        m_key.clear();
    }
    m_credentials.clear();
}

bool ZzAesFileCredentialBackend::isUnlocked() const
{
    return !m_key.isEmpty();
}

bool ZzAesFileCredentialBackend::persist() const
{
    QJsonArray array;
    for (const ZzCredentialEntry &cred : m_credentials) {
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
    // 密文文件也必须最小权限 0600（仅属主可读写），防止同机其他用户读取。
    // Windows 无 POSIX 权限语义，QFile::setPermissions 近似 no-op，属已知平台限制
    if (!QFile::setPermissions(m_filePath,
                               QFileDevice::ReadOwner | QFileDevice::WriteOwner)) {
        m_errorString = QStringLiteral("无法收紧凭据文件权限：%1").arg(m_filePath);
        return false;
    }
    return true;
}

QUuid ZzAesFileCredentialBackend::addCredential(const QString &name, const QString &secret)
{
    if (!isUnlocked()) {
        m_errorString = QStringLiteral("凭据存储未解锁");
        return QUuid();
    }

    const ZzCredentialEntry cred{QUuid::createUuid(), name, secret};
    m_credentials.append(cred);
    if (!persist()) {
        m_credentials.removeLast(); // 落盘失败回滚内存状态
        return QUuid();
    }
    return cred.id;
}

bool ZzAesFileCredentialBackend::putCredential(const QUuid &credentialId,
                                               const QString &name,
                                               const QString &secret)
{
    if (!isUnlocked()) {
        m_errorString = QStringLiteral("凭据存储未解锁");
        return false;
    }
    if (credentialId.isNull()) {
        m_errorString = QStringLiteral("凭据 id 不能为空");
        return false;
    }

    for (ZzCredentialEntry &cred : m_credentials) {
        if (cred.id == credentialId) {
            const ZzCredentialEntry backup = cred;
            cred.name = name;
            cred.secret = secret;
            if (!persist()) {
                cred = backup; // 落盘失败回滚内存状态
                return false;
            }
            return true;
        }
    }
    m_credentials.append(ZzCredentialEntry{credentialId, name, secret});
    if (!persist()) {
        m_credentials.removeLast(); // 落盘失败回滚内存状态
        return false;
    }
    return true;
}

bool ZzAesFileCredentialBackend::updateCredential(const QUuid &credentialId, const QString &secret)
{
    if (!isUnlocked()) {
        m_errorString = QStringLiteral("凭据存储未解锁");
        return false;
    }

    for (ZzCredentialEntry &cred : m_credentials) {
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

std::optional<QString> ZzAesFileCredentialBackend::credential(const QUuid &credentialId) const
{
    if (!isUnlocked())
        return std::nullopt;
    for (const ZzCredentialEntry &cred : m_credentials) {
        if (cred.id == credentialId)
            return cred.secret;
    }
    return std::nullopt;
}

bool ZzAesFileCredentialBackend::removeCredential(const QUuid &credentialId)
{
    if (!isUnlocked()) {
        m_errorString = QStringLiteral("凭据存储未解锁");
        return false;
    }

    for (qsizetype i = 0; i < m_credentials.size(); ++i) {
        if (m_credentials[i].id == credentialId) {
            const ZzCredentialEntry backup = m_credentials[i];
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

QList<ZzCredentialEntry> ZzAesFileCredentialBackend::allCredentials() const
{
    if (!isUnlocked())
        return {};
    return m_credentials;
}

QString ZzAesFileCredentialBackend::errorString() const
{
    return m_errorString;
}
