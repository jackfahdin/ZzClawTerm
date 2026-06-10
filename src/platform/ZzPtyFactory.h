/**
 * @file ZzPtyFactory.h
 * @brief PTY 工厂, 按当前平台创建合适的 ZzPtyInterface 实现。
 */

#ifndef ZZ_PTY_FACTORY_H
#define ZZ_PTY_FACTORY_H

#include <QObject>
#include <QString>

#include <memory>

#include "platform/ZzPtyInterface.h"

namespace ZzPlatform {

/**
 * @brief PTY 工厂。
 */
class ZzPtyFactory
{
public:
    /**
     * @brief 创建当前平台的 PTY 实例。
     * @param parent 父对象。
     * @return PTY 实例的独占指针。
     */
    static std::unique_ptr<ZzPtyInterface> create(QObject* parent = nullptr);

    /**
     * @brief 返回当前平台默认的 Shell 可执行路径。
     */
    static QString defaultShell();
};

}  // namespace ZzPlatform

#endif  // ZZ_PTY_FACTORY_H
