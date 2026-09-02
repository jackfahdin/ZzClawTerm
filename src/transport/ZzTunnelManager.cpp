#include "ZzTunnelManager.h"

#include <utility>

#include "ZzTunnelHandle.h"

ZzTunnelManager::ZzTunnelManager(ZzTunnelFactory *factory,
                                 QVector<ZzForwardRule> rules, QObject *parent)
    : QObject(parent)
    , m_factory(factory)
    , m_rules(std::move(rules))
{
}

ZzTunnelManager::~ZzTunnelManager()
{
    stopAll();
}

void ZzTunnelManager::startAll()
{
    if (m_started) {
        return;
    }
    m_started = true;

    for (const ZzForwardRule &rule : m_rules) {
        ZzTunnelHandle *handle = m_factory->createHandle(rule, this);
        if (!handle) {
            m_failed.append(rule);
            emit ruleFailed(rule, tr("创建隧道失败（连接未就绪）"));
            continue;
        }

        Entry entry;
        entry.rule = rule;
        entry.handle = handle;
        m_entries.append(entry);

        // 信号接线必须先于 start()：失败可能同步发射（如 QTcpServer 绑定占用）
        connect(handle, &ZzTunnelHandle::listening, this,
                [this, handle](quint16 /*boundPort*/) {
                    for (Entry &e : m_entries) {
                        if (e.handle == handle) {
                            e.listening = true;
                            break;
                        }
                    }
                    emit tunnelsChanged();
                });
        connect(handle, &ZzTunnelHandle::failed, this,
                [this, handle](int /*code*/, const QString &message) {
                    const auto rule = dropEntry(handle);
                    if (!rule) {
                        return;
                    }
                    m_failed.append(*rule);
                    emit ruleFailed(*rule, message);
                    emit tunnelsChanged();
                });
        connect(handle, &ZzTunnelHandle::connectionError, this,
                &ZzTunnelManager::tunnelConnectionError);
        connect(handle, &ZzTunnelHandle::invalidated, this, [this, handle]() {
            if (dropEntry(handle)) {
                emit tunnelsChanged();
            }
        });

        handle->start();
    }
    emit tunnelsChanged();
}

void ZzTunnelManager::stopAll()
{
    m_started = false;
    m_failed.clear();
    // exchange 快照移出再逐个销毁：防 stop/delete 触发的信号重入修改 m_entries
    const QList<Entry> entries = std::exchange(m_entries, {});
    for (const Entry &e : entries) {
        e.handle->stop();
        delete e.handle; // 同线程、非事件处理中，直接 delete 安全
    }
}

int ZzTunnelManager::activeTunnelCount() const
{
    int count = 0;
    for (const Entry &e : m_entries) {
        if (e.listening) {
            ++count;
        }
    }
    return count;
}

QVector<ZzForwardRule> ZzTunnelManager::failedRules() const
{
    return m_failed;
}

std::optional<ZzForwardRule> ZzTunnelManager::dropEntry(ZzTunnelHandle *handle)
{
    for (qsizetype i = 0; i < m_entries.size(); ++i) {
        if (m_entries[i].handle == handle) {
            const ZzForwardRule rule = m_entries[i].rule;
            m_entries.removeAt(i);
            handle->deleteLater(); // 信号发射途中，延迟销毁
            return rule;
        }
    }
    return std::nullopt;
}
