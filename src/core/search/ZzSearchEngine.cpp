/**
 * @file ZzSearchEngine.cpp
 * @brief ZzSearchEngine 公开接口实现。
 */

#include "core/search/ZzSearchEngine.h"

#include "core/log/ZzLogEngine.h"
#include "core/search/ZzSearchEnginePrivate.h"

namespace ZzCore {
namespace Search {

ZzSearchEngine::ZzSearchEngine(Log::ZzLogEngine* logEngine, QObject* parent)
    : QObject(parent), d_ptr(std::make_unique<ZzSearchEnginePrivate>())
{
    d_ptr->logEngine = logEngine;
}

ZzSearchEngine::~ZzSearchEngine() = default;

void ZzSearchEngine::search(const QString& pattern, int limit)
{
    if (!d_ptr->logEngine || pattern.isEmpty()) {
        emit resultsReady({});
        return;
    }
    emit resultsReady(d_ptr->logEngine->search(pattern, limit));
}

}  // namespace Search
}  // namespace ZzCore
