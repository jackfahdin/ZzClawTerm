/**
 * @file ZzPtyWindows.h
 * @brief Windows 伪终端实现, 基于 ConPTY (CreatePseudoConsole)。
 */

#ifndef ZZ_PTY_WINDOWS_H
#define ZZ_PTY_WINDOWS_H

#include <memory>

#include "platform/ZzPtyInterface.h"

namespace ZzPlatform {

class ZzPtyWindowsPrivate;

/**
 * @brief 基于 ConPTY 的 Windows 伪终端。
 *
 * 需要 Windows 10 1809 (build 17763) 及以上。输出经独立读取线程
 * 读取后以队列连接转发到主线程。
 */
class ZzPtyWindows : public ZzPtyInterface
{
    Q_OBJECT

public:
    explicit ZzPtyWindows(QObject* parent = nullptr);
    ~ZzPtyWindows() override;

    bool start(const QString& program, const QStringList& arguments,
               const QSize& initialSize) override;
    qint64 write(const QByteArray& data) override;
    void resize(const QSize& size) override;
    void shutdown() override;
    bool isRunning() const override;

private:
    std::unique_ptr<ZzPtyWindowsPrivate> d_ptr;
};

}  // namespace ZzPlatform

#endif  // ZZ_PTY_WINDOWS_H
