/**
 * @file ZzPtyUnix.h
 * @brief Unix (Linux/macOS) 伪终端实现, 基于 forkpty。
 */

#ifndef ZZ_PTY_UNIX_H
#define ZZ_PTY_UNIX_H

#include <memory>

#include "platform/ZzPtyInterface.h"

namespace ZzPlatform {

class ZzPtyUnixPrivate;

/**
 * @brief 基于 forkpty 的 Unix 伪终端。
 *
 * 使用 QSocketNotifier 监听 master fd 实现异步读取。
 * Linux 与 macOS 共用此实现。
 */
class ZzPtyUnix : public ZzPtyInterface
{
    Q_OBJECT

public:
    explicit ZzPtyUnix(QObject* parent = nullptr);
    ~ZzPtyUnix() override;

    bool start(const QString& program, const QStringList& arguments,
               const QSize& initialSize) override;
    qint64 write(const QByteArray& data) override;
    void resize(const QSize& size) override;
    void shutdown() override;
    bool isRunning() const override;

private slots:
    /** @brief master fd 可读时被调用, 读取并上报数据。 */
    void onReadyRead();

private:
    std::unique_ptr<ZzPtyUnixPrivate> d_ptr;
};

}  // namespace ZzPlatform

#endif  // ZZ_PTY_UNIX_H
