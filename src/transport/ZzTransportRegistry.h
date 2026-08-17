#pragma once

#include <functional>

#include <QtCore/QHash>
#include <QtCore/QString>
#include <QtCore/QStringList>

class ZzTransportInterface;

/** @brief 传输工厂：按给定父对象创建一个未 open 的传输实例。 */
using ZzTransportFactory = std::function<ZzTransportInterface *(QObject *parent)>;

/**
 * @brief 传输协议注册表（规格 §2.3：内部模块与未来插件同一条注册路径）。
 *
 * 进程级单例；内置协议（ssh/local）在 main() 启动时注册，测试可用 clear() 隔离。
 */
class ZzTransportRegistry final
{
public:
    /** @brief 进程级唯一注册表。 */
    static ZzTransportRegistry &instance();

    /**
     * @brief 注册协议工厂。
     * @param scheme 协议名（如 "ssh"、"local"），非空。
     * @param factory 创建函数。
     * @return 注册成功；scheme 为空或已存在返回 false。
     */
    bool registerTransport(const QString &scheme, ZzTransportFactory factory);

    /**
     * @brief 按协议名创建传输实例。
     * @return 未注册协议返回 nullptr。
     */
    [[nodiscard]] ZzTransportInterface *create(const QString &scheme,
                                               QObject *parent = nullptr);

    /** @brief 已注册协议名列表。 */
    [[nodiscard]] QStringList schemes() const;

    /** @brief 清空全部注册（仅测试使用）。 */
    void clear();

private:
    ZzTransportRegistry() = default;
    QHash<QString, ZzTransportFactory> m_factories;
};
