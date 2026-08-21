#pragma once

#include <optional>

#include <QObject>
#include <QVector>

#include "session/ZzForwardRule.h"

class ZzTunnelHandle;

/**
 * @brief 隧道工厂：按规则创建隧道句柄。
 *
 * 生产实现 ZzSshTunnelFactory 包装 ZzSshConnection；测试注入 fake。
 * 创建失败（连接未就绪等）返回 nullptr，由 ZzTunnelManager 记为规则失败。
 */
class ZzTunnelFactory
{
public:
    virtual ~ZzTunnelFactory() = default;

    /**
     * @brief 按规则创建隧道句柄。
     * @param rule 转发规则。
     * @param parent 句柄的 QObject 父对象（manager 传入自身）。
     * @return 句柄；无法创建返回 nullptr。
     */
    virtual ZzTunnelHandle *createHandle(const ZzForwardRule &rule, QObject *parent) = 0;
};

/**
 * @brief 单个活动会话的隧道集合（规格 §三）：连接后建隧道、断线销毁、重连重建。
 *
 * startAll 按规则逐一经工厂创建句柄并启动；单规则失败只记入 failedRules
 * 并发射 ruleFailed，不影响其余规则（规格 §六错误矩阵）。
 * 断线时各句柄自下而上 invalidated，manager 将其移出活动集；
 * 重连重建由外部重建 manager 完成（ZzSshTransportAdapter 随新连接驱动）。
 */
class ZzTunnelManager : public QObject
{
    Q_OBJECT
public:
    /**
     * @brief 构造隧道管理器。
     * @param factory 隧道工厂（非拥有，须比 manager 活得久）。
     * @param rules 本会话的转发规则列表。
     */
    ZzTunnelManager(ZzTunnelFactory *factory, QVector<ZzForwardRule> rules,
                    QObject *parent = nullptr);

    /** @brief 析构即 stopAll（会话关闭释放全部监听）。 */
    ~ZzTunnelManager() override;

    /** @brief 启动全部规则（幂等）。 */
    void startAll();

    /**
     * @brief 停止并销毁全部隧道（幂等；之后可再次 startAll 重建）。
     * @warning 内部会直接 delete 句柄，因此不得在任何句柄信号
     *          （listening/failed/connectionError/invalidated）或 manager 自身信号
     *          （tunnelsChanged/ruleFailed/tunnelConnectionError）的同步调用链内调用，
     *          否则会销毁正在发射信号的对象。正常调用点为适配器的
     *          open()/close()/析构路径（重连前确定性释放监听端口）。
     */
    void stopAll();

    /** @brief 当前活动（listening）隧道数。 */
    int activeTunnelCount() const;

    /** @brief 启动失败的规则列表。 */
    QVector<ZzForwardRule> failedRules() const;

signals:
    /** @brief 活动隧道数或失败集合变化（状态栏刷新用）。 */
    void tunnelsChanged();

    /** @brief 单条规则启动失败（规格 §六：单规则失败隔离）。 */
    void ruleFailed(const ZzForwardRule &rule, const QString &message);

    /** @brief 隧道内单连接级错误提示（透传自句柄）。 */
    void tunnelConnectionError(const QString &message);

private:
    /** @brief 一条规则的运行时条目。 */
    struct Entry {
        ZzForwardRule rule;
        ZzTunnelHandle *handle = nullptr;
        bool listening = false;   ///< 是否已收到 listening 信号
    };

    /** @brief 按句柄移除条目并销毁句柄；返回被移除的规则，未找到返回 std::nullopt。 */
    std::optional<ZzForwardRule> dropEntry(ZzTunnelHandle *handle);

    ZzTunnelFactory *m_factory;          ///< 非拥有
    QVector<ZzForwardRule> m_rules;      ///< 全部规则
    QVector<ZzForwardRule> m_failed;     ///< 启动失败的规则
    QList<Entry> m_entries;              ///< 活动条目
    bool m_started = false;
};
