/**
 * @file ZzSearchEnginePrivate.h
 * @brief ZzSearchEngine 私有实现声明 (Pimpl)。
 */

#ifndef ZZ_SEARCH_ENGINE_PRIVATE_H
#define ZZ_SEARCH_ENGINE_PRIVATE_H

namespace ZzCore {
namespace Log {
class ZzLogEngine;
}

namespace Search {

/**
 * @brief 搜索引擎内部状态。
 */
class ZzSearchEnginePrivate
{
public:
    Log::ZzLogEngine* logEngine = nullptr;  ///< 日志引擎 (弱引用)
};

}  // namespace Search
}  // namespace ZzCore

#endif  // ZZ_SEARCH_ENGINE_PRIVATE_H
