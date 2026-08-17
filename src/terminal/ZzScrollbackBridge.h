#pragma once

#include <QtCore/QObject>
#include <QtCore/QStringList>

class QTermWidget;
class ZzLogEngine;

/**
 * @brief 滚动历史桥（规格 §5.4）：QTermWidget 滚出的行进 ZzLogEngine，
 *        向上滚动超出内存历史时从引擎读回。
 *
 * 追加路径：监听 QTermWidget::dupDisplayOutput（逐行 UTF-8）→ appendLine。
 * 读回路径：readOlderLines 委托引擎 getLines（夹取到当前可读窗口），并经
 * QTermWidget::setHistoryProvider 完成注入接线：滚动越顶时由显示层回调索取
 * 绝对行号 [beforeLine - maxLines, beforeLine) 的行文本（旧→新顺序）。
 * 降级路径：引擎 degradedToMemoryOnly → degraded 信号 → 状态栏提示（规格 §八）。
 */
class ZzScrollbackBridge : public QObject
{
    Q_OBJECT
public:
    /**
     * @brief 绑定终端与日志引擎（均不拥有，调用方保证存活期覆盖本桥）。
     */
    ZzScrollbackBridge(QTermWidget *term, ZzLogEngine *engine,
                       QObject *parent = nullptr);

    /**
     * @brief 读回绝对行号 [beforeLine - maxLines, beforeLine) 内的行文本
     *        （旧→新顺序，夹取到引擎当前可读窗口；供滚动加载与测试）。
     */
    [[nodiscard]] QStringList readOlderLines(qint64 beforeLine, int maxLines) const;

    /** @brief 引擎当前总行数。 */
    [[nodiscard]] qint64 totalLines() const;

    /** @brief 测试辅助：直接转发一次降级事件（等价引擎 I/O 失败）。 */
    void simulateDegradationForTest(const QString &reason);

signals:
    /** @brief 引擎降级为纯内存模式（规格 §八：提示用户，不影响终端交互）。 */
    void degraded(const QString &reason);

private:
    QTermWidget *m_term;
    ZzLogEngine *m_engine;
};
