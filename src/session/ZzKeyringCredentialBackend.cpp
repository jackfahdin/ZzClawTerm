#include "ZzKeyringCredentialBackend.h"

#include <QtGlobal>
#include <QStringList>
#include <atomic>

#if defined(Q_OS_LINUX) && defined(ZZ_HAVE_LIBSECRET)
#define ZZ_KEYRING_HAS_PLATFORM 1
#include <libsecret/secret.h>
#elif defined(Q_OS_WIN)
#define ZZ_KEYRING_HAS_PLATFORM 1
#include <QJsonDocument>
#include <QJsonObject>
#ifndef NOMINMAX
#define NOMINMAX
#endif
#include <windows.h>
#include <wincred.h>
#elif defined(Q_OS_MACOS)
#define ZZ_KEYRING_HAS_PLATFORM 1
#include <QByteArray>
#include <Security/Security.h>
#include <CoreFoundation/CoreFoundation.h>
#else
#define ZZ_KEYRING_HAS_PLATFORM 0
#endif

namespace {

/** @brief 密钥环条目归属标记（libsecret 属性 / Windows 目标名前缀）。 */
const QString kAppTag = QStringLiteral("ZzClawTerm");

#if ZZ_KEYRING_HAS_PLATFORM && defined(Q_OS_LINUX)
// ---------------------------------------------------------------------------
// Linux：libsecret（Secret Service API）
// ---------------------------------------------------------------------------

/**
 * @brief 凭据 schema：按 id + app 两个属性定位条目。
 */
const SecretSchema *zzSecretSchema()
{
    static const SecretSchema schema = {
        "com.zzclawterm.Credential", SECRET_SCHEMA_NONE,
        {
            {"id", SECRET_SCHEMA_ATTRIBUTE_STRING},
            {"app", SECRET_SCHEMA_ATTRIBUTE_STRING},
            {nullptr, SECRET_SCHEMA_ATTRIBUTE_STRING},
        },
    };
    return &schema;
}

/**
 * @brief GError 转中文错误信息并释放。
 */
QString zzGlibError(GError *error, const QString &prefix)
{
    const QString detail = error ? QString::fromUtf8(error->message) : QString();
    if (error) {
        g_error_free(error);
    }
    return detail.isEmpty() ? prefix : QStringLiteral("%1：%2").arg(prefix, detail);
}

/** @brief 密钥环中的显示标签（用户可在系统密码管理器中看到）。 */
QString zzLabel(const QString &name)
{
    return QStringLiteral("ZzClawTerm: %1").arg(name);
}

bool platformProbe()
{
    GError *error = nullptr;
    SecretService *service = secret_service_get_sync(SECRET_SERVICE_NONE, nullptr, &error);
    if (error) {
        g_error_free(error);
    }
    if (!service) {
        return false;
    }
    g_object_unref(service);
    return true;
}

bool platformStoreSecret(const QUuid &id, const QString &name, const QString &secret,
                         QString &errOut)
{
    const QByteArray idUtf8 = id.toString(QUuid::WithoutBraces).toUtf8();
    const QByteArray labelUtf8 = zzLabel(name).toUtf8();
    const QByteArray secretUtf8 = secret.toUtf8();
    GError *error = nullptr;
    const gboolean ok = secret_password_store_sync(
        zzSecretSchema(), SECRET_COLLECTION_DEFAULT,
        labelUtf8.constData(), secretUtf8.constData(), nullptr, &error,
        "id", idUtf8.constData(), "app", "ZzClawTerm", nullptr);
    if (!ok) {
        errOut = zzGlibError(error, QStringLiteral("写入系统密钥环失败"));
        return false;
    }
    return true;
}

bool platformLookupSecret(const QUuid &id, std::optional<QString> &secretOut,
                          QString &nameOut, QString &errOut)
{
    // 名称随标签存储，lookup 只回秘密；用 search 一并取回标签
    const QByteArray idUtf8 = id.toString(QUuid::WithoutBraces).toUtf8();
    GError *error = nullptr;
    GList *items = secret_password_search_sync(
        zzSecretSchema(),
        static_cast<SecretSearchFlags>(SECRET_SEARCH_UNLOCK
                                       | SECRET_SEARCH_LOAD_SECRETS),
        nullptr, &error,
        "id", idUtf8.constData(), "app", "ZzClawTerm", nullptr);
    if (error) {
        errOut = zzGlibError(error, QStringLiteral("读取系统密钥环失败"));
        return false;
    }
    if (!items) {
        secretOut = std::nullopt; // 未找到不是错误
        return true;
    }
    auto *item = static_cast<SecretItem *>(items->data);
    SecretValue *value = secret_item_get_secret(item);
    secretOut = value ? QString::fromUtf8(secret_value_get_text(value)) : QString();
    if (value) {
        secret_value_unref(value);
    }
    gchar *label = secret_item_get_label(item);
    QString name = QString::fromUtf8(label ? label : "");
    g_free(label);
    const QString prefix = QStringLiteral("ZzClawTerm: ");
    if (name.startsWith(prefix)) {
        name = name.mid(prefix.size());
    }
    nameOut = name;
    g_list_free_full(items, g_object_unref);
    return true;
}

bool platformDeleteSecret(const QUuid &id, bool &foundOut, QString &errOut)
{
    const QByteArray idUtf8 = id.toString(QUuid::WithoutBraces).toUtf8();
    GError *error = nullptr;
    const gboolean removed = secret_password_clear_sync(
        zzSecretSchema(), nullptr, &error,
        "id", idUtf8.constData(), "app", "ZzClawTerm", nullptr);
    if (error) {
        errOut = zzGlibError(error, QStringLiteral("删除系统密钥环条目失败"));
        return false;
    }
    foundOut = removed == TRUE;
    return true;
}

bool platformListSecrets(QList<ZzCredentialEntry> &out, QString &errOut)
{
    GError *error = nullptr;
    GList *items = secret_password_search_sync(
        zzSecretSchema(),
        static_cast<SecretSearchFlags>(SECRET_SEARCH_ALL | SECRET_SEARCH_UNLOCK
                                       | SECRET_SEARCH_LOAD_SECRETS),
        nullptr, &error, "app", "ZzClawTerm", nullptr);
    if (error) {
        errOut = zzGlibError(error, QStringLiteral("枚举系统密钥环失败"));
        return false;
    }
    for (GList *l = items; l != nullptr; l = l->next) {
        auto *item = static_cast<SecretItem *>(l->data);
        GHashTable *attrs = secret_item_get_attributes(item);
        const gchar *idText = static_cast<const gchar *>(
            g_hash_table_lookup(attrs, "id"));
        if (!idText) {
            continue;
        }
        ZzCredentialEntry entry;
        entry.id = QUuid::fromString(QString::fromUtf8(idText));
        gchar *label = secret_item_get_label(item);
        QString name = QString::fromUtf8(label ? label : "");
        g_free(label);
        const QString prefix = QStringLiteral("ZzClawTerm: ");
        if (name.startsWith(prefix)) {
            name = name.mid(prefix.size());
        }
        entry.name = name;
        SecretValue *value = secret_item_get_secret(item);
        entry.secret = value ? QString::fromUtf8(secret_value_get_text(value)) : QString();
        if (value) {
            secret_value_unref(value);
        }
        out.append(entry);
    }
    g_list_free_full(items, g_object_unref);
    return true;
}

#elif ZZ_KEYRING_HAS_PLATFORM && defined(Q_OS_WIN)
// ---------------------------------------------------------------------------
// Windows：Credential Manager（wincred，未在本机实测，仅保证代码合理）
// ---------------------------------------------------------------------------

/** @brief 目标名："ZzClawTerm/<id>"，也是 CredEnumerate 的过滤前缀。 */
std::wstring zzTargetName(const QUuid &id)
{
    return (kAppTag + QStringLiteral("/") + id.toString(QUuid::WithoutBraces))
        .toStdWString();
}

bool platformProbe()
{
    return true; // Credential Manager 为系统内置服务
}

bool platformStoreSecret(const QUuid &id, const QString &name, const QString &secret,
                         QString &errOut)
{
    // 名称与明文一起以 JSON 存入 blob（target 仅容纳 id）
    QJsonObject obj;
    obj.insert(QStringLiteral("name"), name);
    obj.insert(QStringLiteral("secret"), secret);
    const QByteArray blob = QJsonDocument(obj).toJson(QJsonDocument::Compact);
    if (blob.size() > CRED_MAX_CREDENTIAL_BLOB_SIZE) {
        errOut = QStringLiteral("凭据过长，超出 Windows 凭据管理器容量限制");
        return false;
    }

    const std::wstring target = zzTargetName(id);
    std::wstring userName = kAppTag.toStdWString();
    CREDENTIALW cred = {};
    cred.Type = CRED_TYPE_GENERIC;
    cred.TargetName = const_cast<LPWSTR>(target.c_str());
    cred.CredentialBlobSize = static_cast<DWORD>(blob.size());
    cred.CredentialBlob = reinterpret_cast<LPBYTE>(const_cast<char *>(blob.constData()));
    cred.Persist = CRED_PERSIST_LOCAL_MACHINE;
    cred.UserName = const_cast<LPWSTR>(userName.c_str());
    if (!CredWriteW(&cred, 0)) {
        errOut = QStringLiteral("写入 Windows 凭据管理器失败：错误码 %1")
                     .arg(GetLastError());
        return false;
    }
    return true;
}

bool platformLookupSecret(const QUuid &id, std::optional<QString> &secretOut,
                          QString &nameOut, QString &errOut)
{
    const std::wstring target = zzTargetName(id);
    PCREDENTIALW cred = nullptr;
    if (!CredReadW(target.c_str(), CRED_TYPE_GENERIC, 0, &cred)) {
        const DWORD code = GetLastError();
        if (code == ERROR_NOT_FOUND) {
            secretOut = std::nullopt; // 未找到不是错误
            return true;
        }
        errOut = QStringLiteral("读取 Windows 凭据管理器失败：错误码 %1").arg(code);
        return false;
    }
    const QByteArray blob(reinterpret_cast<const char *>(cred->CredentialBlob),
                          cred->CredentialBlobSize);
    CredFree(cred);
    const QJsonObject obj = QJsonDocument::fromJson(blob).object();
    secretOut = obj.value(QStringLiteral("secret")).toString();
    nameOut = obj.value(QStringLiteral("name")).toString();
    return true;
}

bool platformDeleteSecret(const QUuid &id, bool &foundOut, QString &errOut)
{
    const std::wstring target = zzTargetName(id);
    if (!CredDeleteW(target.c_str(), CRED_TYPE_GENERIC, 0)) {
        const DWORD code = GetLastError();
        if (code == ERROR_NOT_FOUND) {
            foundOut = false;
            return true;
        }
        errOut = QStringLiteral("删除 Windows 凭据管理器条目失败：错误码 %1").arg(code);
        return false;
    }
    foundOut = true;
    return true;
}

bool platformListSecrets(QList<ZzCredentialEntry> &out, QString &errOut)
{
    const std::wstring filter = (kAppTag + QStringLiteral("/*")).toStdWString();
    DWORD count = 0;
    PCREDENTIALW *creds = nullptr;
    if (!CredEnumerateW(filter.c_str(), 0, &count, &creds)) {
        const DWORD code = GetLastError();
        if (code == ERROR_NOT_FOUND) {
            return true; // 空列表
        }
        errOut = QStringLiteral("枚举 Windows 凭据管理器失败：错误码 %1").arg(code);
        return false;
    }
    // CredEnumerateW 只回元数据不回 CredentialBlob（MSDN）：先收目标名，再逐个 CredReadW
    QStringList targets;
    for (DWORD i = 0; i < count; ++i) {
        targets.append(QString::fromStdWString(creds[i]->TargetName));
    }
    CredFree(creds);
    for (const QString &target : targets) {
        const QUuid id = QUuid::fromString(target.mid(kAppTag.size() + 1));
        if (id.isNull()) {
            continue;
        }
        PCREDENTIALW cred = nullptr;
        const std::wstring targetW = target.toStdWString();
        if (!CredReadW(targetW.c_str(), CRED_TYPE_GENERIC, 0, &cred)) {
            continue; // 枚举间隙被并发删除等：跳过该条，不算整体失败
        }
        ZzCredentialEntry entry;
        entry.id = id;
        const QByteArray blob(reinterpret_cast<const char *>(cred->CredentialBlob),
                              cred->CredentialBlobSize);
        CredFree(cred);
        const QJsonObject obj = QJsonDocument::fromJson(blob).object();
        entry.name = obj.value(QStringLiteral("name")).toString();
        entry.secret = obj.value(QStringLiteral("secret")).toString();
        out.append(entry);
    }
    return true;
}

#elif ZZ_KEYRING_HAS_PLATFORM && defined(Q_OS_MACOS)
// ---------------------------------------------------------------------------
// macOS：Keychain（generic password，未在本机实测，仅保证代码合理）
// ---------------------------------------------------------------------------

const QByteArray kServiceName = QByteArrayLiteral("com.zzclawterm.credential");

/** @brief QString → CFStringRef（调用方负责 CFRelease）。 */
CFStringRef zzCfString(const QString &text)
{
    return CFStringCreateWithCharacters(
        nullptr, reinterpret_cast<const UniChar *>(text.utf16()),
        CFIndex(text.size()));
}

/** @brief 构造定位单条凭据的查询字典（调用方负责 CFRelease）。
 *  @note kSecAttrService/kSecAttrAccount 要求 CFStringRef，传 CFDataRef 会 errSecParam(-50)。
 */
CFMutableDictionaryRef zzKeychainQuery(const QUuid &id)
{
    CFMutableDictionaryRef query = CFDictionaryCreateMutable(
        nullptr, 0, &kCFTypeDictionaryKeyCallBacks, &kCFTypeDictionaryValueCallBacks);
    CFDictionarySetValue(query, kSecClass, kSecClassGenericPassword);
    CFStringRef service = zzCfString(QString::fromUtf8(kServiceName));
    CFDictionarySetValue(query, kSecAttrService, service);
    CFRelease(service);
    CFStringRef account = zzCfString(id.toString(QUuid::WithoutBraces));
    CFDictionarySetValue(query, kSecAttrAccount, account);
    CFRelease(account);
    return query;
}

// platformStoreSecret 需要先读后写，前向声明
bool platformLookupSecret(const QUuid &id, std::optional<QString> &secretOut,
                          QString &nameOut, QString &errOut);

bool platformProbe()
{
    return true; // Keychain 为系统内置服务
}

bool platformStoreSecret(const QUuid &id, const QString &name, const QString &secret,
                         QString &errOut)
{
    const QByteArray secretUtf8 = secret.toUtf8();
    CFDataRef data = CFDataCreate(nullptr,
        reinterpret_cast<const UInt8 *>(secretUtf8.constData()), secretUtf8.size());
    CFStringRef label = zzCfString(QStringLiteral("ZzClawTerm: %1").arg(name));

    // kSecAttrLabel 在部分 macOS 版本不可经 SecItemUpdate 更新：
    // 名称未变只 update kSecValueData；名称变化（或新增）走 delete+add
    std::optional<QString> existingSecret;
    QString existingName;
    if (!platformLookupSecret(id, existingSecret, existingName, errOut)) {
        CFRelease(label);
        CFRelease(data);
        return false;
    }
    OSStatus status = errSecSuccess;
    if (existingSecret.has_value() && existingName == name) {
        CFMutableDictionaryRef query = zzKeychainQuery(id);
        CFMutableDictionaryRef attrs = CFDictionaryCreateMutable(
            nullptr, 0, &kCFTypeDictionaryKeyCallBacks, &kCFTypeDictionaryValueCallBacks);
        CFDictionarySetValue(attrs, kSecValueData, data);
        status = SecItemUpdate(query, attrs);
        CFRelease(attrs);
        CFRelease(query);
    } else {
        if (existingSecret.has_value()) {
            CFMutableDictionaryRef delQuery = zzKeychainQuery(id);
            SecItemDelete(delQuery); // 名称变化：先删后加
            CFRelease(delQuery);
        }
        CFMutableDictionaryRef query = zzKeychainQuery(id);
        CFDictionarySetValue(query, kSecValueData, data);
        CFDictionarySetValue(query, kSecAttrLabel, label);
        status = SecItemAdd(query, nullptr);
        CFRelease(query);
    }
    CFRelease(label);
    CFRelease(data);
    if (status != errSecSuccess) {
        errOut = QStringLiteral("写入 macOS 钥匙串失败：状态码 %1").arg(int(status));
        return false;
    }
    return true;
}

bool platformLookupSecret(const QUuid &id, std::optional<QString> &secretOut,
                          QString &nameOut, QString &errOut)
{
    CFMutableDictionaryRef query = zzKeychainQuery(id);
    CFDictionarySetValue(query, kSecReturnData, kCFBooleanTrue);
    CFDictionarySetValue(query, kSecReturnAttributes, kCFBooleanTrue);
    CFDictionarySetValue(query, kSecMatchLimit, kSecMatchLimitOne);
    CFTypeRef result = nullptr;
    const OSStatus status = SecItemCopyMatching(query, &result);
    CFRelease(query);
    if (status == errSecItemNotFound) {
        secretOut = std::nullopt; // 未找到不是错误
        return true;
    }
    if (status != errSecSuccess) {
        errOut = QStringLiteral("读取 macOS 钥匙串失败：状态码 %1").arg(int(status));
        return false;
    }
    CFDictionaryRef item = static_cast<CFDictionaryRef>(result);
    CFDataRef data = static_cast<CFDataRef>(CFDictionaryGetValue(item, kSecValueData));
    if (data) {
        secretOut = QString::fromUtf8(
            reinterpret_cast<const char *>(CFDataGetBytePtr(data)),
            CFDataGetLength(data));
    } else {
        secretOut = QString();
    }
    CFStringRef label = static_cast<CFStringRef>(CFDictionaryGetValue(item, kSecAttrLabel));
    QString name = label ? QString::fromCFString(label) : QString();
    const QString prefix = QStringLiteral("ZzClawTerm: ");
    if (name.startsWith(prefix)) {
        name = name.mid(prefix.size());
    }
    nameOut = name;
    CFRelease(result);
    return true;
}

bool platformDeleteSecret(const QUuid &id, bool &foundOut, QString &errOut)
{
    CFMutableDictionaryRef query = zzKeychainQuery(id);
    const OSStatus status = SecItemDelete(query);
    CFRelease(query);
    if (status == errSecItemNotFound) {
        foundOut = false;
        return true;
    }
    if (status != errSecSuccess) {
        errOut = QStringLiteral("删除 macOS 钥匙串条目失败：状态码 %1").arg(int(status));
        return false;
    }
    foundOut = true;
    return true;
}

bool platformListSecrets(QList<ZzCredentialEntry> &out, QString &errOut)
{
    CFMutableDictionaryRef query = CFDictionaryCreateMutable(
        nullptr, 0, &kCFTypeDictionaryKeyCallBacks, &kCFTypeDictionaryValueCallBacks);
    CFDictionarySetValue(query, kSecClass, kSecClassGenericPassword);
    CFStringRef service = zzCfString(QString::fromUtf8(kServiceName));
    CFDictionarySetValue(query, kSecAttrService, service);
    CFRelease(service);
    CFDictionarySetValue(query, kSecReturnAttributes, kCFBooleanTrue);
    CFDictionarySetValue(query, kSecReturnData, kCFBooleanTrue);
    CFDictionarySetValue(query, kSecMatchLimit, kSecMatchLimitAll);
    CFTypeRef result = nullptr;
    const OSStatus status = SecItemCopyMatching(query, &result);
    CFRelease(query);
    if (status == errSecItemNotFound) {
        return true; // 空列表
    }
    if (status != errSecSuccess) {
        errOut = QStringLiteral("枚举 macOS 钥匙串失败：状态码 %1").arg(int(status));
        return false;
    }
    CFArrayRef items = static_cast<CFArrayRef>(result);
    const CFIndex total = CFArrayGetCount(items);
    const QString prefix = QStringLiteral("ZzClawTerm: ");
    for (CFIndex i = 0; i < total; ++i) {
        CFDictionaryRef item = static_cast<CFDictionaryRef>(CFArrayGetValueAtIndex(items, i));
        // kSecAttrAccount 写入时为 CFString（见 zzKeychainQuery），按 CFStringRef 读回
        CFStringRef account = static_cast<CFStringRef>(CFDictionaryGetValue(item, kSecAttrAccount));
        if (!account) {
            continue;
        }
        ZzCredentialEntry entry;
        entry.id = QUuid::fromString(QString::fromCFString(account));
        if (entry.id.isNull()) {
            continue;
        }
        CFStringRef label = static_cast<CFStringRef>(CFDictionaryGetValue(item, kSecAttrLabel));
        QString name = label ? QString::fromCFString(label) : QString();
        if (name.startsWith(prefix)) {
            name = name.mid(prefix.size());
        }
        entry.name = name;
        CFDataRef data = static_cast<CFDataRef>(CFDictionaryGetValue(item, kSecValueData));
        entry.secret = data ? QString::fromUtf8(
            reinterpret_cast<const char *>(CFDataGetBytePtr(data)),
            CFDataGetLength(data)) : QString();
        out.append(entry);
    }
    CFRelease(result);
    return true;
}

#else
// ---------------------------------------------------------------------------
// 无平台支持（如 Linux 未找到 libsecret 开发包）：优雅降级桩
// ---------------------------------------------------------------------------

bool platformProbe()
{
    return false;
}

bool platformStoreSecret(const QUuid &, const QString &, const QString &, QString &errOut)
{
    errOut = QStringLiteral("当前平台未编译系统密钥环支持");
    return false;
}

bool platformLookupSecret(const QUuid &, std::optional<QString> &, QString &,
                          QString &errOut)
{
    errOut = QStringLiteral("当前平台未编译系统密钥环支持");
    return false;
}

bool platformDeleteSecret(const QUuid &, bool &, QString &errOut)
{
    errOut = QStringLiteral("当前平台未编译系统密钥环支持");
    return false;
}

bool platformListSecrets(QList<ZzCredentialEntry> &, QString &errOut)
{
    errOut = QStringLiteral("当前平台未编译系统密钥环支持");
    return false;
}

#endif

} // namespace

ZzKeyringCredentialBackend::ZzKeyringCredentialBackend(QObject *parent)
    : ZzCredentialBackend(parent)
{
}

bool ZzKeyringCredentialBackend::probeAvailability()
{
    // 环境变量强制禁用：测试降级路径与应急回退 AES 文件后端用（每次读取，便于测试切换）
    if (qEnvironmentVariableIsSet("ZZCLAWTERM_KEYRING_DISABLE")) {
        return false;
    }
    // Linux 下 secret_service_get_sync 每次新建代理、完整 D-Bus 握手，开销不小；
    // 密钥环服务由 OS 托管、存活期一般长于本进程，故首次成功后进程内缓存。
    // 失败不缓存：服务可能随后启动，下次调用重新探测。
    static std::atomic<bool> probeSucceeded{false};
    if (probeSucceeded.load(std::memory_order_relaxed)) {
        return true;
    }
    const bool ok = platformProbe();
    if (ok) {
        probeSucceeded.store(true, std::memory_order_relaxed);
    }
    return ok;
}

QString ZzKeyringCredentialBackend::backendId() const
{
    return QStringLiteral("system-keyring");
}

bool ZzKeyringCredentialBackend::isAvailable() const
{
    return probeAvailability();
}

bool ZzKeyringCredentialBackend::requiresMasterPassword() const
{
    return false; // 密钥环由操作系统托管解锁
}

bool ZzKeyringCredentialBackend::hasMasterPassword() const
{
    return isAvailable(); // 无需初始化，服务可用即视为已就绪
}

bool ZzKeyringCredentialBackend::initialize(const QString & /*masterPassword*/)
{
    if (!isAvailable()) {
        m_errorString = QStringLiteral("系统密钥环不可用");
        return false;
    }
    m_unlocked = true;
    return true;
}

bool ZzKeyringCredentialBackend::unlock(const QString & /*masterPassword*/)
{
    if (m_unlocked) {
        m_errorString = QStringLiteral("凭据存储已处于解锁状态");
        return false;
    }
    if (!isAvailable()) {
        m_errorString = QStringLiteral("系统密钥环不可用");
        return false;
    }
    m_unlocked = true;
    return true;
}

void ZzKeyringCredentialBackend::lock()
{
    m_unlocked = false; // 仅翻转标记，密钥环内容不受影响
}

bool ZzKeyringCredentialBackend::isUnlocked() const
{
    return m_unlocked;
}

bool ZzKeyringCredentialBackend::ensureReady() const
{
    if (!m_unlocked) {
        m_errorString = QStringLiteral("凭据存储未解锁");
        return false;
    }
    if (!isAvailable()) {
        m_errorString = QStringLiteral("系统密钥环不可用");
        return false;
    }
    return true;
}

QUuid ZzKeyringCredentialBackend::addCredential(const QString &name, const QString &secret)
{
    if (!ensureReady()) {
        return QUuid();
    }
    const QUuid id = QUuid::createUuid();
    if (!platformStoreSecret(id, name, secret, m_errorString)) {
        return QUuid();
    }
    return id;
}

bool ZzKeyringCredentialBackend::putCredential(const QUuid &credentialId,
                                               const QString &name,
                                               const QString &secret)
{
    if (!ensureReady()) {
        return false;
    }
    if (credentialId.isNull()) {
        m_errorString = QStringLiteral("凭据 id 不能为空");
        return false;
    }
    return platformStoreSecret(credentialId, name, secret, m_errorString);
}

bool ZzKeyringCredentialBackend::updateCredential(const QUuid &credentialId,
                                                  const QString &secret)
{
    if (!ensureReady()) {
        return false;
    }
    std::optional<QString> oldSecret;
    QString name;
    if (!platformLookupSecret(credentialId, oldSecret, name, m_errorString)) {
        return false;
    }
    if (!oldSecret.has_value()) {
        m_errorString = QStringLiteral("凭据不存在：%1")
                            .arg(credentialId.toString(QUuid::WithoutBraces));
        return false;
    }
    // 原标签（含名称）保留：name 由平台层随条目一并读出
    return platformStoreSecret(credentialId, name, secret, m_errorString);
}

std::optional<QString> ZzKeyringCredentialBackend::credential(const QUuid &credentialId) const
{
    if (!ensureReady()) {
        return std::nullopt;
    }
    std::optional<QString> secret;
    QString name;
    if (!platformLookupSecret(credentialId, secret, name, m_errorString)) {
        return std::nullopt;
    }
    return secret;
}

bool ZzKeyringCredentialBackend::removeCredential(const QUuid &credentialId)
{
    if (!ensureReady()) {
        return false;
    }
    bool found = false;
    if (!platformDeleteSecret(credentialId, found, m_errorString)) {
        return false;
    }
    if (!found) {
        m_errorString = QStringLiteral("凭据不存在：%1")
                            .arg(credentialId.toString(QUuid::WithoutBraces));
        return false;
    }
    return true;
}

QList<ZzCredentialEntry> ZzKeyringCredentialBackend::allCredentials() const
{
    if (!ensureReady()) {
        return {};
    }
    QList<ZzCredentialEntry> out;
    if (!platformListSecrets(out, m_errorString)) {
        return {};
    }
    return out;
}

QString ZzKeyringCredentialBackend::errorString() const
{
    return m_errorString;
}
