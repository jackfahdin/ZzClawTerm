#include "ZzScrollbackBridge.h"

#include "qtermwidget.h"
#include "log/ZzLogEngine.h"
#include "log/ZzLogLine.h"

ZzScrollbackBridge::ZzScrollbackBridge(QTermWidget *term, ZzLogEngine *engine,
                                       QObject *parent)
    : QObject(parent)
    , m_term(term)
    , m_engine(engine)
{
    // 追加路径：dupDisplayOutput 逐行吐 UTF-8，去掉行尾换行后入热层
    connect(m_term, &QTermWidget::dupDisplayOutput, this,
            [this](const char *data, int len) {
                QString line = QString::fromUtf8(data, len);
                while (line.endsWith(QLatin1Char('\n'))
                       || line.endsWith(QLatin1Char('\r'))) {
                    line.chop(1);
                }
                m_engine->appendLine({line, QByteArray()});
            });
    // 降级路径（规格 §八）：引擎 I/O 失败 → 状态栏提示，不打断终端
    connect(m_engine, &ZzLogEngine::degradedToMemoryOnly, this,
            [this](const QString &reason) { emit degraded(reason); });
    // 读回路径（规格 §5.4）：滚动越顶时由计划 05 的注入机制回调索取更老的行
    m_term->setHistoryProvider([this](qint64 beforeLine, int maxLines) {
        return readOlderLines(beforeLine, maxLines);
    });
}

QStringList ZzScrollbackBridge::readOlderLines(qint64 beforeLine,
                                               int maxLines) const
{
    if (maxLines <= 0 || beforeLine <= 0) {
        return {};
    }
    const qint64 first = qint64(m_engine->firstLineNo());
    qint64 start = beforeLine - maxLines;
    if (start < first) {
        start = first;
    }
    if (start < 0 || start >= beforeLine) {
        return {};
    }
    const QVector<ZzLogLine> lines =
        m_engine->getLines(quint64(start), quint64(beforeLine - start));
    QStringList out;
    out.reserve(lines.size());
    for (const ZzLogLine &line : lines) {
        out.append(line.text);
    }
    return out;
}

qint64 ZzScrollbackBridge::totalLines() const
{
    return qint64(m_engine->totalLines());
}

void ZzScrollbackBridge::simulateDegradationForTest(const QString &reason)
{
    emit degraded(reason);
}
