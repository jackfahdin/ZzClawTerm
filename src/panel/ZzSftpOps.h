#pragma once

#include <QtCore/QObject>
#include <QtCore/QString>

#include <ZzSftpTypes.h>

/**
 * @brief SFTP 操作抽象：ZzSftpPanel 消费的最小接口（规格 §2.3 面板与后端解耦）。
 *
 * 信号与请求 ID 配对约定同 ZzSftpSession：调用返回请求 ID（0=会话未打开），
 * 结局为对应结果信号或 operationError/transferError。生产实现为
 * ZzSftpSessionOps（包 ZzSftpSession），测试用 ZzMockSftpOps 隔离网络。
 * 所有方法只在 GUI 线程调用。
 */
class ZzSftpOps : public QObject
{
    Q_OBJECT
public:
    using QObject::QObject;

    /** @brief 会话是否已打开（opened() 之后为 true）。 */
    [[nodiscard]] virtual bool isOpen() const = 0;

    /** @brief 列举远端目录。@return 请求 ID；未打开返回 0 且不产生信号。 */
    virtual quint64 listDir(const QString &path) = 0;
    /** @brief 创建远端目录（mode 默认 0755）。@return 请求 ID；未打开返回 0。 */
    virtual quint64 makeDir(const QString &path, long mode = 0755) = 0;
    /** @brief 删除远端文件。@return 请求 ID；未打开返回 0。 */
    virtual quint64 removeFile(const QString &path) = 0;
    /** @brief 删除远端空目录。@return 请求 ID；未打开返回 0。 */
    virtual quint64 removeDir(const QString &path) = 0;
    /** @brief 重命名/移动远端路径。@return 请求 ID；未打开返回 0。 */
    virtual quint64 rename(const QString &oldPath, const QString &newPath) = 0;
    /** @brief 上传本地文件到远端（覆盖同名文件）。@return 请求 ID；未打开返回 0。 */
    virtual quint64 upload(const QString &localPath, const QString &remotePath) = 0;
    /** @brief 下载远端文件到本地（覆盖同名文件）。@return 请求 ID；未打开返回 0。 */
    virtual quint64 download(const QString &remotePath, const QString &localPath) = 0;
    /** @brief 取消进行中的传输（结局为 transferError）。 */
    virtual void cancelTransfer(quint64 requestId) = 0;
    /** @brief 关闭会话（异步、幂等），完成后发射 closed()。 */
    virtual void closeSession() = 0;

signals:
    /** @brief 会话已打开。 */
    void opened();
    /** @brief 会话级失败（打开失败/连接级错误）。 */
    void errorOccurred(int code, const QString &message);
    /** @brief 会话已关闭（主动关闭或连接断开）。 */
    void closed();
    /** @brief listDir 成功。 */
    void dirListed(quint64 requestId, const QList<ZzSftpFileInfo> &entries);
    /** @brief makeDir/removeFile/removeDir/rename 成功。 */
    void operationFinished(quint64 requestId);
    /** @brief 简单操作失败。 */
    void operationError(quint64 requestId, int code, const QString &message);
    /** @brief 传输进度（库侧已节流：≥1MB 或 ≥100ms 一次，面板勿再额外刷新）。 */
    void transferProgress(quint64 requestId, qint64 done, qint64 total);
    /** @brief 传输完成。 */
    void transferFinished(quint64 requestId);
    /** @brief 传输失败或被取消。 */
    void transferError(quint64 requestId, int code, const QString &message);
};
