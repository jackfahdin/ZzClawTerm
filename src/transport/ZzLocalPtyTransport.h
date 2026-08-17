#pragma once

#include <memory>

#include "ZzTransportInterface.h"

class IPtyProcess;

/**
 * @brief 本地 shell 传输：包装 ZzTermWidget 内置 ptyqt（规格 §七）。
 *
 * 不走网络、不经 SSH，验证终端层与协议层解耦——对 ZzTerminalView 而言
 * 与 ZzSshTransport 完全同形。
 */
class ZzLocalPtyTransport : public ZzTransportInterface
{
    Q_OBJECT
public:
    explicit ZzLocalPtyTransport(QObject *parent = nullptr);
    ~ZzLocalPtyTransport() override;

    void open(const ZzTransportEndpoint &endpoint) override;
    void write(const QByteArray &data) override;
    void resize(int cols, int rows) override;
    void close() override;

private:
    std::unique_ptr<IPtyProcess> m_pty; ///< PTY 进程（RAII 持有）
    bool m_closing = false;             ///< 主动关闭标记：抑制重复 disconnected
};
