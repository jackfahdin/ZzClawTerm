#pragma once

#include <ZzCore/ZzQtLogBridge.h>
#include <ZzPureTools/ZzApplicationModule.h>

/**
 * @brief ZzClawTerm 应用模块：承载进程级日志运行时（ZzLog 文件 sink +
 *        Qt 消息桥接），使 Windows 无控制台场景下诊断日志可落地取用。
 */
class ZzClawTermModule final : public ZzPureTools::ZzApplicationModule
{
public:
    /** @brief 模块身份：稳定 id、版本、空依赖集。 */
    [[nodiscard]] ZzPureTools::ZzModuleDescriptor descriptor() const override;

    /**
     * @brief 启动：初始化 ZzLog（文件 sink，滚动 + 大小上限）并安装 Qt 消息桥。
     * @return 日志初始化失败时返回错误，阻止应用构建（照框架 example 契约）。
     */
    [[nodiscard]] ZzCore::ZzResult<void> start() override;

    /** @brief 协作停止请求（幂等）。 */
    void requestStop() noexcept override;

    /** @brief 最终资源清理：卸载桥、关闭日志运行时（幂等，不可抛）。 */
    void stop() noexcept override;

private:
    bool m_started = false;
    bool m_ownsLogRuntime = false;   ///< 本模块初始化了 ZzLog 运行时（stop 时负责关闭）
    ZzCore::ZzQtLogBridge m_logBridge; ///< Qt 消息 → ZzLog 桥（析构自动卸载）
};
