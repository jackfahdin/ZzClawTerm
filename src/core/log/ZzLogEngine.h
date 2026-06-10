/**
 * @file ZzLogEngine.h
 * @brief 无限日志引擎: 编排热/温/冷三层存储。
 *
 * 热层 (ZzRingBuffer) 提供最近行的 O(1) 访问; 冷层 (ZzColdStorage)
 * 提供无限持久化与全文搜索; 温层行偏移索引 (ZzLineIndex) 为后续
 * mmap+LZ4 优化预留。对外提供统一的按行号读取与搜索接口。
 */

#ifndef ZZ_LOG_ENGINE_H
#define ZZ_LOG_ENGINE_H

#include <QObject>
#include <QString>
#include <QStringList>
#include <QVector>

#include <cstdint>
#include <memory>

namespace ZzCore {
namespace Log {

class ZzLogEnginePrivate;

/**
 * @brief 日志引擎。
 */
class ZzLogEngine : public QObject
{
    Q_OBJECT

public:
    explicit ZzLogEngine(QObject* parent = nullptr);
    ~ZzLogEngine() override;

    /**
     * @brief 打开存储 (创建冷层数据库)。
     * @param storagePath 数据库文件路径。
     * @return 成功返回 true。
     */
    bool open(const QString& storagePath);

    /** @brief 设置热层容量 (行数)。 */
    void setHotCapacity(std::size_t lines);

    /** @brief 追加一行 (滚出屏幕的行)。 */
    void appendLine(const QString& text);

    /** @brief 全部历史行数。 */
    std::uint64_t totalLines() const;

    /** @brief 按全局行号读取一行 (热层优先, 否则冷层)。 */
    QString getLine(std::uint64_t lineNumber) const;

    /** @brief 读取 [start, start+count) 行。 */
    QStringList getLines(std::uint64_t start, std::uint64_t count) const;

    /** @brief 搜索, 返回匹配行号。 */
    QVector<std::uint64_t> search(const QString& pattern, int limit = 1000) const;

signals:
    /** @brief 追加一行后发出, 携带最新总行数。 */
    void lineAppended(std::uint64_t totalLines);

private:
    std::unique_ptr<ZzLogEnginePrivate> d_ptr;
};

}  // namespace Log
}  // namespace ZzCore

#endif  // ZZ_LOG_ENGINE_H
