#pragma once

#include "transport/ZzTransportInterface.h"

/**
 * @brief 测试用 mock 传输：脚本化成功/失败，记录写入与尺寸，可手动注入事件。
 *
 * open() 默认在事件循环下一拍进入 Connected；endpoint.host == "fail" 时
 * 以错误码 1001 失败。echoEnabled 时 write 的内容原样经 dataReceived 回显。
 */
class ZzMockTransport : public ZzTransportInterface
{
    Q_OBJECT
public:
    explicit ZzMockTransport(QObject *parent = nullptr);

    void open(const ZzTransportEndpoint &endpoint) override;
    void write(const QByteArray &data) override;
    void resize(int cols, int rows) override;
    void close() override;

    /** @brief 注入一次被动断开。 */
    void simulateDisconnect(const QString &reason);
    /** @brief 注入一次错误。 */
    void simulateError(int code, const QString &message);
    /** @brief 注入一段远端输出。 */
    void simulateData(const QByteArray &data);
    /** @brief 注入一次活动隧道数变化。 */
    void simulateTunnelCount(int count) { emit tunnelCountChanged(count); }
    /** @brief 注入一条瞬时提示消息。 */
    void simulateStatusNotice(const QString &message) { emit statusNotice(message); }

    bool echoEnabled = true;       ///< write 回显开关
    QByteArray writtenData;        ///< 累计写入内容
    int lastCols = 0;              ///< 最近一次 resize 列数
    int lastRows = 0;              ///< 最近一次 resize 行数
    int openCallCount = 0;         ///< open 调用次数（重连断言用）
    int closeCallCount = 0;        ///< close 调用次数
    ZzTransportEndpoint lastEndpoint; ///< 最近一次 open 参数
};
