#pragma once

#include <functional>
#include <memory>

#include <QtCore/QPointer>

#include "ZzTransportInterface.h"
#include "x11/ZzX11Service.h"
#include "x11/ZzXAuthority.h"

class ZzSshConnection;
class ZzSshShellChannel;
class ZzSshX11Bridge;
class ZzTunnelManager;
class ZzSshTunnelFactory;
class ZzXServerDownloader;
class ZzXServerManager;

/**
 * @brief SSH 传输适配器：把 ZzSshConnection/ZzSshShellChannel 包装成
 *        ZzTransportInterface（规格 §2.3/§4.2）。
 *
 * 认证链（agent→公钥→密码）在 ZzSshCore 内部；密码经 m_passwordProvider
 * 向上层索取，主机密钥确认经 m_hostKeyConfirmer 交给 UI（规格 §八安全底线）。
 *
 * @note 实现类名为 ZzSshTransportAdapter：ZzSshCore 内部已占用全局名
 *       ZzSshTransport（src/ZzSshTransport.h 的 socket 抽象接口），同名全局类
 *       会导致链接期 ODR 冲突。下方别名保持对外 API 形态不变；
 *       任何翻译单元不得同时包含本头文件与 ZzSshCore 的 ZzSshTransport.h。
 */
class ZzSshTransportAdapter : public ZzTransportInterface
{
    Q_OBJECT
public:
    /** @brief 密码索取回调：返回明文密码，空串表示用户取消。 */
    using ZzPasswordProvider = std::function<QString()>;
    /** @brief 主机密钥确认回调：host/fingerprint/oldFingerprint/changed → 是否接受并记住。
     *         changed=false（首次连接）时 oldFingerprint 为空串。 */
    using ZzHostKeyConfirmer =
        std::function<bool(const QString &host, const QString &fingerprint,
                           const QString &oldFingerprint, bool changed)>;
    /** @brief 私钥口令解析回调：按连接参数返回私钥口令（空串表示无口令）。 */
    using ZzKeyPassphraseResolver =
        std::function<QString(const ZzTransportEndpoint &endpoint)>;

    explicit ZzSshTransportAdapter(QObject *parent = nullptr);
    ~ZzSshTransportAdapter() override;

    void open(const ZzTransportEndpoint &endpoint) override;
    void write(const QByteArray &data) override;
    void resize(int cols, int rows) override;
    void close() override;

    void setPasswordProvider(ZzPasswordProvider provider);
    void setHostKeyConfirmer(ZzHostKeyConfirmer confirmer);

    /** @brief 注入应用级共享 X server 门面（ZzTabManager 装配；观察指针不拥有）。 */
    void setX11Service(ZzX11Service *service) { m_x11Service = service; }
    /** @brief 测试观察口：当前注入的共享 X server 门面。 */
    [[nodiscard]] ZzX11Service *x11Service() const { return m_x11Service; }

    /**
     * @brief 底层 SSH 连接只读访问器（SFTP 面板创建会话用）。
     * @return 当前连接对象（非拥有，随 open/close 重建）；未 open 时为 nullptr。
     */
    [[nodiscard]] ZzSshConnection *sshConnection() const { return m_conn; }

    /**
     * @brief 全局默认 known_hosts.json 路径（AppConfigLocation 下）。
     * @return 绝对路径；writableLocation 取不到目录时返回空串。
     * @note 纯计算不建目录、不触碰文件，供 open() 与单测共用。
     */
    static QString defaultKnownHostsFilePath();

    /**
     * @brief 进程级安装私钥口令解析器（由组合根 ZzAppShell 装配一次）。
     *
     * 传输实例由终端视图深处按需创建，口令引用（ZzSessionProfile::
     * keyPassphraseCredentialId）不经过 ZzTransportEndpoint 逐层下传，
     * 而是由解析器按 endpoint（host/port/user/keyPath）反查会话模型后
     * 从凭据后端取明文。空解析器或返回空串均视为私钥无口令。
     *
     * @note 线程契约：解析器仅在 GUI 线程安装、调用与卸载（open() 在 GUI
     *       线程执行）；ZzAppShell 析构时必须置空，避免悬挂捕获。
     */
    static void setKeyPassphraseResolver(ZzKeyPassphraseResolver resolver);

private:
    void wireConnection();
    void onConnected();
    /** @brief 销毁隧道管理器（先 stopAll 释放监听，再 deleteLater）。 */
    void destroyTunnelManager();
    /** @brief connected 后按 endpoint.portForwards 创建并启动隧道管理器。 */
    void startTunnels();

    /** @brief 向 shell channel 发起 openShell（异步；shellOpened 后迁移 Connected）。 */
    void openShellChannel();
    /**
     * @brief X11 装配（openShell 之前调用）：备妥本地端点（嵌入=会话自带 server，
     *        非嵌入=注入的共享服务）、先发 x11-req 再开 shell（OpenSSH 仅 LARVAL 态
     *        受理 x11-req）。
     * @note 契约：无论成败都以 openShellChannel() 收尾（同步或经共享服务/downloader
     *       信号异步续接）；任何失败只 statusNotice 瞬时提示，不阻断会话。
     */
    void startX11Forwarding();
    /** @brief 嵌入实验路径（仅 Windows）：会话自带独立 server（M4b 原流程）。 */
    void startX11ForwardingEmbedded();
    /** @brief Windows：vcxsrv 就绪后拉起 server、发 x11-req 并补 openShell。 */
    void onX11ServerReady(const QString &executablePath);
    /** @brief 创建 X11 桥接器并向 shell channel 发起 x11-req（两平台共用收尾）。 */
    void requestX11Forwarding();
    /** @brief 销毁 X11 桥与 server 管理器（随 close/重连回收）。 */
    void destroyX11();

    ZzSshConnection *m_conn = nullptr;      ///< 本对象为父，随适配器销毁
    ZzSshShellChannel *m_channel = nullptr; ///< 观察指针，连接断开即失效
    ZzTransportEndpoint m_endpoint;
    ZzPasswordProvider m_passwordProvider;
    ZzHostKeyConfirmer m_hostKeyConfirmer;
    static ZzKeyPassphraseResolver s_keyPassphraseResolver; ///< 进程级私钥口令解析器
    std::unique_ptr<ZzSshTunnelFactory> m_tunnelFactory; ///< 随 m_conn 重建
    ZzTunnelManager *m_tunnelManager = nullptr;          ///< 本对象为父；随 m_conn 重建
    ZzXAuthority m_x11Authority;             ///< 值成员：无状态 cookie/xauth 工具（规格 §5.3）
    ZzXServerManager *m_x11Manager = nullptr;    ///< 本对象为父；仅嵌入实验路径创建（会话自带独立 server）
    QPointer<ZzX11Service> m_x11Service;        ///< 观察指针（QPointer 防悬垂）：应用级共享 server（ZzAppShell 持有，M5）
    ZzSshX11Bridge *m_x11Bridge = nullptr;       ///< 本对象为父；观察 m_conn，随会话重建
    ZzXServerDownloader *m_x11Downloader = nullptr; ///< 仅 Windows 嵌入路径：按需下载 ZzXsrv
    QString m_x11Cookie;                     ///< 嵌入路径的 MIT-MAGIC-COOKIE-1（hex）
    ///< 主动 close 期间压制底层 disconnected/closed 上报（接口契约：主动关闭不报断线）
    bool m_suppressDisconnect = false;
};

/** @brief 对外契约名（简报/ZzTabManager 消费形态），实现类见上注。 */
using ZzSshTransport = ZzSshTransportAdapter;
