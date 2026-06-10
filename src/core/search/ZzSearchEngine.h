/**
 * @file ZzSearchEngine.h
 * @brief 跨层搜索引擎。
 *
 * MVP 阶段委托日志引擎的冷层 (FTS5/LIKE) 完成搜索, 以信号返回
 * 匹配行号。后续可扩展为热/温/冷三层并行 + 增量回调。
 */

#ifndef ZZ_SEARCH_ENGINE_H
#define ZZ_SEARCH_ENGINE_H

#include <QObject>
#include <QString>
#include <QVector>

#include <cstdint>
#include <memory>

namespace ZzCore {
namespace Log {
class ZzLogEngine;
}

namespace Search {

class ZzSearchEnginePrivate;

/**
 * @brief 搜索引擎。
 */
class ZzSearchEngine : public QObject
{
    Q_OBJECT

public:
    /**
     * @brief 构造搜索引擎。
     * @param logEngine 日志引擎 (不取得所有权)。
     * @param parent 父对象。
     */
    explicit ZzSearchEngine(Log::ZzLogEngine* logEngine, QObject* parent = nullptr);
    ~ZzSearchEngine() override;

    /**
     * @brief 执行搜索。
     * @param pattern 关键词。
     * @param limit 最大结果数。
     */
    void search(const QString& pattern, int limit = 1000);

signals:
    /** @brief 搜索完成, 返回匹配行号 (升序)。 */
    void resultsReady(const QVector<std::uint64_t>& lineNumbers);

private:
    std::unique_ptr<ZzSearchEnginePrivate> d_ptr;
};

}  // namespace Search
}  // namespace ZzCore

#endif  // ZZ_SEARCH_ENGINE_H
