#pragma once

#include <functional>
#include <memory>

#include "ZzTransportInterface.h"

class ZzSshConnection;
class ZzSshShellChannel;
class ZzTunnelManager;
class ZzSshTunnelFactory;

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

    explicit ZzSshTransportAdapter(QObject *parent = nullptr);
    ~ZzSshTransportAdapter() override;

    void open(const ZzTransportEndpoint &endpoint) override;
    void write(const QByteArray &data) override;
    void resize(int cols, int rows) override;
    void close() override;

    void setPasswordProvider(ZzPasswordProvider provider);
    void setHostKeyConfirmer(ZzHostKeyConfirmer confirmer);

private:
    void wireConnection();
    void onConnected();
    /** @brief 销毁隧道管理器（先 stopAll 释放监听，再 deleteLater）。 */
    void destroyTunnelManager();
    /** @brief connected 后按 endpoint.portForwards 创建并启动隧道管理器。 */
    void startTunnels();

    ZzSshConnection *m_conn = nullptr;      ///< 本对象为父，随适配器销毁
    ZzSshShellChannel *m_channel = nullptr; ///< 观察指针，连接断开即失效
    ZzTransportEndpoint m_endpoint;
    ZzPasswordProvider m_passwordProvider;
    ZzHostKeyConfirmer m_hostKeyConfirmer;
    std::unique_ptr<ZzSshTunnelFactory> m_tunnelFactory; ///< 随 m_conn 重建
    ZzTunnelManager *m_tunnelManager = nullptr;          ///< 本对象为父；随 m_conn 重建
    ///< 主动 close 期间压制底层 disconnected/closed 上报（接口契约：主动关闭不报断线）
    bool m_suppressDisconnect = false;
};

/** @brief 对外契约名（简报/ZzTabManager 消费形态），实现类见上注。 */
using ZzSshTransport = ZzSshTransportAdapter;
