#pragma once

#include <QtCore/QPointer>

#include "ZzSftpOps.h"

class ZzSftpSession;

/**
 * @brief 生产适配器：把 ZzSshCore 的 ZzSftpSession 包装成 ZzSftpOps。
 *
 * 不获得会话所有权（会话 parent 为 ZzSshConnection，随连接断开/重连销毁）；
 * 全部调用经 QPointer 守卫，会话已销毁时按"未打开"语义返回 0/空操作。
 */
class ZzSftpSessionOps : public ZzSftpOps
{
    Q_OBJECT
public:
    /**
     * @brief 包装一个已创建的 SFTP 会话。
     * @param session 由 ZzSshConnection::createSftpSession() 创建；可为空（全部调用退化为 0）。
     * @param parent QObject 父对象（面板持有适配器生命周期）。
     */
    explicit ZzSftpSessionOps(ZzSftpSession *session, QObject *parent = nullptr);

    [[nodiscard]] bool isOpen() const override;
    quint64 listDir(const QString &path) override;
    quint64 makeDir(const QString &path, long mode = 0755) override;
    quint64 removeFile(const QString &path) override;
    quint64 removeDir(const QString &path) override;
    quint64 rename(const QString &oldPath, const QString &newPath) override;
    quint64 upload(const QString &localPath, const QString &remotePath) override;
    quint64 download(const QString &remotePath, const QString &localPath) override;
    void cancelTransfer(quint64 requestId) override;
    void closeSession() override;

private:
    QPointer<ZzSftpSession> m_session; ///< 观察指针：会话随连接销毁自动置空
};
