#pragma once

#include <ZzPureTools/ZzApplicationModule.h>

/**
 * @brief ZzClawTerm 应用模块：v0.1 无跨模块依赖，仅满足框架生命周期协议。
 */
class ZzClawTermModule final : public ZzPureTools::ZzApplicationModule
{
public:
    /** @brief 模块身份：稳定 id、版本、空依赖集。 */
    [[nodiscard]] ZzPureTools::ZzModuleDescriptor descriptor() const override;

    /** @brief 启动：装配在窗口回调与页面工厂中完成，此处直接成功。 */
    [[nodiscard]] ZzCore::ZzResult<void> start() override;

    /** @brief 协作停止请求（幂等）。 */
    void requestStop() noexcept override;

    /** @brief 最终资源清理（幂等，不可抛）。 */
    void stop() noexcept override;

private:
    bool m_started = false;
};
