#pragma once

#include <QObject>
#include <QString>
#include <QUrl>

/**
 * @brief vcxsrv 官方发布物事实常量（X11 forwarding 应用侧，按需下载器前置交付）。
 *
 * 本头文件只固化常量，不含实现；后续 Windows 按需下载器（ZzXServerDownloader）
 * 直接引用本命名空间。以下信息均经真实网络调研核实，核实日期 2026-08-22。
 *
 * 调研证据（命令 + 关键结果）：
 *
 * 1. 发布渠道：SourceForge 旧主页已停更，最新停留在 1.20.14.0：
 *    @code
 *    curl -sIL "https://sourceforge.net/projects/vcxsrv/files/latest/download"
 *    → 302 → downloads.sourceforge.net/project/vcxsrv/vcxsrv/1.20.14.0/...
 *    @endcode
 *    官方现行发布渠道为 GitHub marchaesen/vcxsrv releases。
 *
 * 2. 最新版本与资产形式：
 *    @code
 *    curl -s "https://api.github.com/repos/marchaesen/vcxsrv/releases/latest"
 *    → tag_name 21.1.16.1（published_at 2025-03-10）
 *    @endcode
 *    资产仅有 NSIS 安装包四种（64 位 release/debug × admin/noadmin），无 zip/7z。
 *    release 页面未公布 SHA256；下列哈希为 2026-08-22 实际下载后本地 sha256sum
 *    计算所得。
 *
 * 3. 解压方案验证（NSIS 为 solid LZMA 压缩，7-Zip 报告 Type=Nsis, Method=LZMA:23,
 *    Solid=+）：
 *    - bsdtar 3.8.5（libarchive）实测无法识别：
 *      "bsdtar: Error opening archive: Unrecognized archive format"。
 *      Windows 10+ 自带 tar.exe 同为 bsdtar/libarchive，故「tar -xf 解 NSIS」
 *      对本资产不可行。
 *    - 7-Zip 26.00 实测可完整解包：解出 vcxsrv.exe、xkbcomp.exe、xkbdata/、
 *      xauth.exe、plink.exe、xhost.exe 等完整运行文件集（exit=0）。
 *
 * 决策：首选 NSIS 静默安装——下载校验后用 QProcess 运行
 * "vcxsrv-*.installer.noadmin.exe /S /D=<目标目录>"（/D 必须是最后一个参数；
 * noadmin 变体免 UAC 提权，适合应用内静默部署），不依赖任何第三方解压工具。
 * 备选方案为随应用分发 7za.exe 解包（已实测可行）。
 *
 * @note 常量固定为免管理员变体（noadmin）。管理员变体
 * vcxsrv-64.21.1.16.1.installer.exe（42,993,397 字节）内容等价但安装需 UAC，
 * 其 SHA256 为 df7fed8f49665d0592528ab6be9d07111ea73c6848283d128b77690e05b8f90b，
 * 仅作记录，默认不采用。
 */
namespace ZzXServerRelease {

/// 官方最新版本号（GitHub marchaesen/vcxsrv releases，2026-08-22 核实）。
inline constexpr char kVersion[] = "21.1.16.1";

/// 64 位免管理员 NSIS 安装包的稳定下载 URL。
inline constexpr char kUrl[] =
    "https://github.com/marchaesen/vcxsrv/releases/download/21.1.16.1/"
    "vcxsrv-64.21.1.16.1.installer.noadmin.exe";

/// kUrl 所指安装包的 SHA256（官方未公布，2026-08-22 实际下载后本地计算；
/// 文件大小 42,998,523 字节）。
inline constexpr char kSha256[] =
    "dea6c7d67d3d15b4ed45c87b63a83c88f4aceaaef5425630f0e97a0bad70d620";

} // namespace ZzXServerRelease

class QNetworkAccessManager;
class QNetworkReply;
class QProcess;
class QFile;
class QCryptographicHash;

/**
 * @brief vcxsrv 按需下载器：官方发布物下载 → SHA256 流式校验 → NSIS 静默安装。
 *
 * 仅 Windows 真用：其他平台 ensureAvailable() 编译期直通，直接发 ready(QString())。
 * 流程：下载到临时文件（边下边算 SHA256）→ 校验通过才以
 * "installer /S /D=<staging 目录>" 静默安装（免 UAC noadmin 变体）→ 核验
 * vcxsrv.exe 落盘并写版本标记 xserver/VERSION → 成功后才把 staging 换入安装根
 * （旧版先改名 .old 再替换，替换失败回滚）；任何失败只清理本次半成品
 * （临时包与 staging 目录），不误伤既有可用的旧版安装。
 * 已装同版本且可执行文件存在时幂等直通，不发网络请求。
 */
class ZzXServerDownloader : public QObject
{
    Q_OBJECT
public:
    explicit ZzXServerDownloader(QObject *parent = nullptr);
    ~ZzXServerDownloader() override;

    /** @brief 本地已装版本（无则空串）。安装根目录 = AppDataLocation/xserver/。 */
    QString installedVersion() const;

    /** @brief 确保可用：已装且校验通过直接发 ready；否则下载+校验+静默安装。 */
    void ensureAvailable();

    /** @brief 安装后 vcxsrv.exe 的完整路径（Windows）；其他平台返回空。 */
    QString serverExecutablePath() const;

    // —— 以下为测试注入接口，须在 ensureAvailable() 之前调用 ——

    /** @brief 注入网络层（不接管所有权）；未注入时首次使用自建实例。 */
    void setNetworkAccessManager(QNetworkAccessManager *nam);

    /** @brief 覆盖安装根目录（默认 QStandardPaths::AppDataLocation/xserver）。 */
    void setInstallRoot(const QString &root);

    /** @brief 覆盖下载 URL 与期望 SHA256（默认 ZzXServerRelease 官方常量）。 */
    void setReleaseSource(const QUrl &url, const QString &sha256Hex);

    /**
     * @brief 非 Windows 平台模拟完整下载安装流程（测试钩子）。
     *
     * Windows 下流程恒启用，本开关无效；非 Windows 默认直通 ready(QString())，
     * 打开后可配合桩安装包在 Linux/macOS 单测中走完整流程。
     */
    void setSimulateWindows(bool on);

signals:
    /** @brief 流程成功：executablePath 为 vcxsrv.exe 完整路径（非 Windows 直通为空串）。 */
    void ready(const QString &executablePath);
    /** @brief 任一步骤失败：message 含原因（HTTP 错误含状态码），半成品已清理。 */
    void downloadFailed(const QString &message);
    /** @brief 下载进度百分比（0-100），供状态栏显示。 */
    void progressChanged(int percent);

private:
    bool windowsFlowEnabled() const;
    QString installRoot() const;
    QString stagingRoot() const;
    QString backupRoot() const;
    void startDownload();
    void handleReplyFinished();
    void startInstall();
    void handleInstallFinished(int exitCode, int exitStatus);
    void removeTempFile();
    void fail(const QString &message);

    QNetworkAccessManager *m_nam = nullptr;      ///< 注入或自建（自建时以 this 为父）
    QUrl m_url = QUrl(QString::fromLatin1(ZzXServerRelease::kUrl));
    QString m_expectedSha256 = QString::fromLatin1(ZzXServerRelease::kSha256);
    QString m_installRoot;                       ///< 空 = 默认 AppDataLocation/xserver
    bool m_simulateWindows = false;
    bool m_running = false;                      ///< 下载/安装进行中，忽略重复调用

    QNetworkReply *m_reply = nullptr;
    QProcess *m_installer = nullptr;
    QFile *m_tempFile = nullptr;   ///< 下载中的临时安装包（不用 QTemporaryFile，见 cpp）
    QCryptographicHash *m_hash = nullptr;   ///< 下载期间增量计算
};
